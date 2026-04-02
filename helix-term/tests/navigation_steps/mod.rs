pub mod flash_jump;
pub mod flash_search;

use cucumber::{given, then, when, World};
use helix_term::application::Application;
use helix_view::{current_ref, input::parse_macro};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(windows)]
use crossterm::event::{Event, KeyEvent};
#[cfg(not(windows))]
use termina::event::{Event, KeyEvent};

/// Shared state threaded through every BDD navigation scenario.
///
/// Each scenario gets a fresh instance via [`NavigationWorld::init`]. Given
/// steps set [`buffer_text`] and optionally build the [`Application`]. When
/// steps run key sequences against the live app, and Then steps inspect the
/// captured outcome fields.
#[derive(World)]
#[world(init = Self::init)]
pub struct NavigationWorld {
    /// Buffer text (using `#[|x]#` notation) staged by Given steps.
    pub buffer_text: String,

    /// Running editor instance, kept alive between When and Then steps.
    pub app: Option<Application>,

    /// Char-offset cursor position captured after a When step.
    pub result_cursor: Option<usize>,

    /// Selection anchor offset captured after a When step.
    pub result_anchor: Option<usize>,

    /// Content of the "/" search register captured after a When step.
    pub result_register: Option<String>,

    /// Jumplist length captured when the app is first built (before any action).
    pub jumplist_len_before: Option<usize>,

    /// Jumplist length captured after a When step completes.
    pub jumplist_len_after: Option<usize>,

    /// Status message captured after a When step, or `None` if no status was set.
    pub result_status: Option<String>,
}

// Application does not implement Debug, so we provide a manual impl.
impl std::fmt::Debug for NavigationWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavigationWorld")
            .field("buffer_text", &self.buffer_text)
            .field("result_cursor", &self.result_cursor)
            .field("result_anchor", &self.result_anchor)
            .field("result_register", &self.result_register)
            .field("result_status", &self.result_status)
            .finish()
    }
}

impl NavigationWorld {
    async fn init() -> Result<Self, anyhow::Error> {
        Ok(Self {
            buffer_text: String::new(),
            app: None,
            result_cursor: None,
            result_anchor: None,
            result_register: None,
            jumplist_len_before: None,
            jumplist_len_after: None,
            result_status: None,
        })
    }

    /// Close the app if it is still running. Called from the cucumber `after` hook.
    pub async fn close_app(&mut self) {
        if let Some(mut app) = self.app.take() {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let mut rx_stream = UnboundedReceiverStream::new(rx);

            if let Ok(events) = parse_macro("<esc>:q!<ret>") {
                for key_event in events {
                    let _ = tx.send(Ok::<Event, std::io::Error>(Event::Key(KeyEvent::from(
                        key_event,
                    ))));
                }
            }

            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                app.event_loop(&mut rx_stream),
            )
            .await;

            let _ = app.close().await;
        }
    }

    /// Build a live [`Application`] using `self.buffer_text` as the initial
    /// buffer content. Captures the initial jumplist length, then stores the
    /// application in `self.app`.
    pub fn build_app(&mut self) -> anyhow::Result<()> {
        let app = crate::helpers::AppBuilder::new()
            .with_input_text(self.buffer_text.clone())
            .with_config(crate::helpers::test_config())
            .build()?;

        self.app = Some(app);

        // Capture jumplist size before any action so Then steps can measure growth.
        let app = self.app.as_ref().unwrap();
        let (view, _doc) = current_ref!(app.editor);
        self.jumplist_len_before = Some(view.jumps.iter().count());

        Ok(())
    }

    /// Send `keys` to the live application and wait until idle.
    ///
    /// Builds the app first if it hasn't been built yet.
    pub async fn send_keys(&mut self, keys: &str) -> anyhow::Result<()> {
        if self.app.is_none() {
            self.build_app()?;
        }

        let app = self.app.as_mut().expect("app must be present");
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut rx_stream = UnboundedReceiverStream::new(rx);

        for key_event in parse_macro(keys)? {
            tx.send(Ok::<Event, std::io::Error>(Event::Key(KeyEvent::from(
                key_event,
            ))))?;
        }

        app.event_loop_until_idle(&mut rx_stream).await;
        Ok(())
    }

    /// Capture cursor position, anchor, search register, jumplist length, and
    /// status message from the live app into `self.result_*` fields.
    pub fn capture_state(&mut self) {
        let app = match &self.app {
            Some(a) => a,
            None => return,
        };

        let (view, doc) = current_ref!(app.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).primary();

        self.result_cursor = Some(selection.cursor(text));
        self.result_anchor = Some(selection.anchor);
        self.result_register = app
            .editor
            .registers
            .first('/', &app.editor)
            .map(|s| s.to_string());
        self.jumplist_len_after = Some(view.jumps.iter().count());
        self.result_status = app
            .editor
            .get_status()
            .map(|(msg, _sev)| msg.to_string());
    }
}

// ---------------------------------------------------------------------------
// Shared step definitions used across both flash_jump and flash_search
// ---------------------------------------------------------------------------

#[given(regex = r#"the buffer contains "(.+)""#)]
fn given_buffer_contains(world: &mut NavigationWorld, content: String) {
    // Replace literal \n escapes with actual newlines; place cursor at char 0.
    let text = content.replace("\\n", "\n");
    let first_char = text.chars().next().unwrap_or(' ');
    world.buffer_text = format!("#[|{first_char}]#{}", &text[first_char.len_utf8()..]);
}

#[when(regex = r#"Alex presses "([^"]+)" and types "([^"]+)""#)]
async fn when_press_and_type(
    world: &mut NavigationWorld,
    binding: String,
    chars: String,
) -> anyhow::Result<()> {
    world.send_keys(&format!("{binding}{chars}")).await?;
    world.capture_state();
    Ok(())
}

#[when(regex = r#"Alex presses "([^"]+)", types "([^"]+)", then types "([^"]+)""#)]
async fn when_press_type_type(
    world: &mut NavigationWorld,
    binding: String,
    first: String,
    second: String,
) -> anyhow::Result<()> {
    world.send_keys(&format!("{binding}{first}{second}")).await?;
    world.capture_state();
    Ok(())
}

#[when(regex = r#"Alex presses "([^"]+)", types "([^"]+)", then presses Escape"#)]
async fn when_press_type_escape(
    world: &mut NavigationWorld,
    binding: String,
    chars: String,
) -> anyhow::Result<()> {
    world.send_keys(&format!("{binding}{chars}<esc>")).await?;
    world.capture_state();
    Ok(())
}

#[then(regex = r"the cursor is at position (\d+)")]
fn then_cursor_at_position(world: &mut NavigationWorld, pos: usize) {
    let cursor = world
        .result_cursor
        .expect("no cursor captured — did a When step run?");
    assert_eq!(cursor, pos, "expected cursor at {pos}, got {cursor}");
}

#[then("the cursor is at the start of the buffer")]
fn then_cursor_at_start(world: &mut NavigationWorld) {
    let cursor = world
        .result_cursor
        .expect("no cursor captured — did a When step run?");
    assert_eq!(cursor, 0, "expected cursor at 0, got {cursor}");
}

#[then("the cursor has not moved from the start of the buffer")]
fn then_cursor_not_moved(world: &mut NavigationWorld) {
    let cursor = world
        .result_cursor
        .expect("no cursor captured — did a When step run?");
    assert_eq!(
        cursor, 0,
        "expected cursor to remain at 0, but it moved to {cursor}"
    );
}
