mod given_steps;
mod when_steps;
mod then_steps;

use std::path::PathBuf;

use cucumber::World;
use helix_term::{application::Application, config::Config};
use helix_view::editor::LspConfig;
use tempfile::TempDir;

/// Shared state threaded through every resize BDD scenario.
///
/// Each scenario gets a fresh instance via [`ResizeWorld::init`]. Steps that
/// exercise split resize operate on the live [`Application`] event loop via
/// [`crate::helpers::test_key_sequence`]. Captured width snapshots allow
/// before/after assertions without inspecting internal tree weights directly.
#[derive(World)]
#[world(init = Self::init)]
pub struct ResizeWorld {
    /// Isolated temporary directory; dropped (and deleted) at scenario end.
    pub workspace_dir: TempDir,
    /// Running editor instance. `None` until a Given step builds it.
    pub app: Option<Application>,
    /// Files staged by Given steps to be opened when the Application is built.
    pub pending_files: Vec<PathBuf>,
    /// Helix configuration staged by Given steps.
    pub helix_config: Config,
    /// Split direction requested by a Given step ("vertical", "horizontal", "nested").
    pub split_direction: Option<String>,
    /// View widths captured before an action for before/after comparison.
    pub before_widths: Vec<u16>,
    /// View widths captured after an action for before/after comparison.
    pub after_widths: Vec<u16>,
    /// View heights captured before an action for before/after comparison.
    pub before_heights: Vec<u16>,
    /// View heights captured after an action for before/after comparison.
    pub after_heights: Vec<u16>,
    /// Sidebar width captured before an action.
    pub sidebar_width_before: u16,
    /// Sidebar width captured after an action.
    pub sidebar_width_after: u16,
}

impl std::fmt::Debug for ResizeWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResizeWorld")
            .field("workspace_dir", &self.workspace_dir.path())
            .field("pending_files", &self.pending_files)
            .field("split_direction", &self.split_direction)
            .field("before_widths", &self.before_widths)
            .field("after_widths", &self.after_widths)
            .field("before_heights", &self.before_heights)
            .field("after_heights", &self.after_heights)
            .field("sidebar_width_before", &self.sidebar_width_before)
            .field("sidebar_width_after", &self.sidebar_width_after)
            .finish()
    }
}

impl ResizeWorld {
    async fn init() -> Result<Self, anyhow::Error> {
        let workspace_dir = tempfile::Builder::new()
            .prefix("helix-resize-test-")
            .tempdir()?;
        std::fs::create_dir_all(workspace_dir.path().join(".helix"))?;

        // Create a scratch file so there is always at least one buffer open.
        let scratch = workspace_dir.path().join("scratch.txt");
        std::fs::write(&scratch, "")?;

        Ok(Self {
            workspace_dir,
            app: None,
            pending_files: Vec::new(),
            helix_config: test_config_no_lsp(),
            split_direction: None,
            before_widths: Vec::new(),
            after_widths: Vec::new(),
            before_heights: Vec::new(),
            after_heights: Vec::new(),
            sidebar_width_before: 0,
            sidebar_width_after: 0,
        })
    }

    /// Construct a live [`Application`] using the staged configuration.
    ///
    /// Sets `HELIX_WORKSPACE` so all session I/O is isolated to the scenario's
    /// temp directory. Files staged in `pending_files` are opened before the
    /// application is stored in `self.app`.
    pub fn build_app(&mut self) -> anyhow::Result<()> {
        std::env::set_var("HELIX_WORKSPACE", self.workspace_dir.path());

        let config = self.helix_config.clone();
        let mut builder = crate::helpers::AppBuilder::new().with_config(config);

        for path in self.pending_files.drain(..) {
            builder = builder.with_file(path, None);
        }

        self.app = Some(builder.build()?);
        Ok(())
    }

    /// Collect the pixel width of every view in document order.
    pub fn collect_view_widths(&self) -> Vec<u16> {
        self.app
            .as_ref()
            .map(|app| {
                app.editor
                    .tree
                    .views()
                    .map(|(v, _)| v.area.width)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Collect the pixel height of every view in document order.
    pub fn collect_view_heights(&self) -> Vec<u16> {
        self.app
            .as_ref()
            .map(|app| {
                app.editor
                    .tree
                    .views()
                    .map(|(v, _)| v.area.height)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Return the index of the currently focused view inside the views iterator.
    pub fn focused_view_index(&self) -> usize {
        self.app
            .as_ref()
            .map(|app| {
                let focus = app.editor.tree.focus;
                app.editor
                    .tree
                    .views()
                    .position(|(v, _)| v.id == focus)
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }
}

/// Build a [`Config`] with LSP disabled, suitable for integration tests.
pub(crate) fn test_config_no_lsp() -> Config {
    Config {
        editor: helix_view::editor::Config {
            lsp: LspConfig {
                enable: false,
                ..Default::default()
            },
            ..Default::default()
        },
        keys: helix_term::keymap::default(),
        ..Default::default()
    }
}
