mod full_scope;
mod live_editor;
mod named_sessions;
mod resilience;
mod session_io;

use std::collections::HashMap;
use std::path::PathBuf;

use cucumber::World;
use helix_term::{application::Application, config::Config};
use helix_view::session::{SessionDocument, SessionLayout, SessionSnapshot, SessionView};
use tempfile::TempDir;

/// Shared state threaded through every BDD scenario.
///
/// Each scenario gets a fresh instance via [`SessionWorld::init`]. Library-level
/// steps use `temp_dir` and `session_file` for isolated I/O without a live
/// editor. Live-editor steps use `workspace_dir` (which contains `.helix/`) and
/// build a real [`Application`] via [`SessionWorld::build_app`].
#[derive(World)]
#[world(init = Self::init)]
pub struct SessionWorld {
    // -----------------------------------------------------------------------
    // Library-level I/O isolation (used by existing step modules)
    // -----------------------------------------------------------------------

    /// Per-scenario scratch directory; dropped (and deleted) at scenario end.
    pub temp_dir: TempDir,
    /// Named test file fixtures: label → absolute path on disk.
    pub test_files: HashMap<String, PathBuf>,
    /// Explicit override for where session I/O reads/writes the session file
    /// in library-level steps.
    pub session_file: Option<PathBuf>,
    /// A directory to use for named session files in library-level steps.
    pub named_session_dir: Option<PathBuf>,
    /// Snapshot produced or loaded by the most recent session operation.
    pub snapshot: Option<SessionSnapshot>,
    /// Error string from the most recent failing session operation.
    pub last_error: Option<String>,
    /// Status message from the most recent successful session operation.
    pub last_status: Option<String>,
    /// Registers staged for a "full scope" capture test.
    pub staged_registers: HashMap<char, Vec<String>>,
    /// Cursor position (anchor char offset) staged by a Given step.
    pub staged_cursor: Option<usize>,
    /// When true the scope is "full" so registers are captured.
    pub full_scope: bool,

    // -----------------------------------------------------------------------
    // Live-editor infrastructure (used by live_editor step module)
    // -----------------------------------------------------------------------

    /// Isolated workspace directory containing a `.helix/` subdirectory.
    /// `HELIX_WORKSPACE` is set to this path before building the Application.
    pub workspace_dir: TempDir,
    /// Running editor instance. `Option` so it can be taken for key sequences.
    pub app: Option<Application>,
    /// Files staged by Given steps to be opened when the Application is built.
    pub pending_files: Vec<PathBuf>,
    /// Helix config staged by Given steps, applied when the Application is built.
    pub helix_config: Config,
    /// Split direction requested by Given steps (`"vertical"`, `"horizontal"`,
    /// or `"nested"`).
    pub split_direction: Option<String>,
}

// Application does not implement Debug, so we implement it manually.
impl std::fmt::Debug for SessionWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionWorld")
            .field("temp_dir", &self.temp_dir.path())
            .field("workspace_dir", &self.workspace_dir.path())
            .field("test_files", &self.test_files)
            .field("session_file", &self.session_file)
            .field("pending_files", &self.pending_files)
            .field("last_error", &self.last_error)
            .field("last_status", &self.last_status)
            .finish()
    }
}

impl SessionWorld {
    /// Factory called by the `#[derive(World)]`-generated code at the start of
    /// each scenario.
    async fn init() -> Result<Self, anyhow::Error> {
        let temp_dir = tempfile::tempdir()?;
        let session_file = Some(temp_dir.path().join("session.json"));

        let workspace_dir = tempfile::tempdir()?;
        // Create the .helix directory so find_workspace() recognises the
        // workspace root and stores the session file inside it.
        std::fs::create_dir_all(workspace_dir.path().join(".helix"))?;

        Ok(Self {
            temp_dir,
            test_files: HashMap::new(),
            session_file,
            named_session_dir: None,
            snapshot: None,
            last_error: None,
            last_status: None,
            staged_registers: HashMap::new(),
            staged_cursor: None,
            full_scope: false,

            workspace_dir,
            app: None,
            pending_files: Vec::new(),
            helix_config: test_config_no_lsp(),
            split_direction: None,
        })
    }

    // -----------------------------------------------------------------------
    // Library-level helpers
    // -----------------------------------------------------------------------

    /// Return the default session file path used by library-level steps.
    pub fn session_path(&self) -> PathBuf {
        self.session_file
            .clone()
            .expect("session_file not initialised")
    }

    /// Return the directory used for named session files in library-level
    /// steps. Creates the directory on first call.
    pub fn named_dir(&mut self) -> PathBuf {
        if let Some(ref d) = self.named_session_dir {
            return d.clone();
        }
        let dir = self.temp_dir.path().join("named_sessions");
        std::fs::create_dir_all(&dir).expect("failed to create named session dir");
        self.named_session_dir = Some(dir.clone());
        dir
    }

    /// Path for a named session file in library-level steps.
    pub fn named_session_path(&mut self, name: &str) -> PathBuf {
        self.named_dir().join(format!("{name}.json"))
    }

    // -----------------------------------------------------------------------
    // Live-editor helpers
    // -----------------------------------------------------------------------

    /// Path to the default session file inside the isolated workspace.
    ///
    /// This is where the Application reads/writes the session because
    /// `HELIX_WORKSPACE` points to `workspace_dir` and `.helix/` exists there.
    pub fn session_file_path(&self) -> PathBuf {
        self.workspace_dir.path().join(".helix").join("session.json")
    }

    /// Path to a named session file inside the isolated workspace.
    pub fn named_session_file_path(&self, name: &str) -> PathBuf {
        self.workspace_dir
            .path()
            .join(".helix")
            .join(format!("{name}.json"))
    }

    /// Build a live [`Application`] using the scenario's staged configuration.
    ///
    /// Sets `HELIX_WORKSPACE` to the scenario workspace dir so that all session
    /// file I/O is isolated to this test. Any files in `pending_files` are
    /// opened before the Application is stored in `self.app`.
    pub fn build_app(&mut self) -> anyhow::Result<()> {
        // Must be set before Application::new() because that's where
        // find_workspace() — and therefore session restore — is called.
        std::env::set_var("HELIX_WORKSPACE", self.workspace_dir.path());

        let config = self.helix_config.clone();
        let mut builder = crate::helpers::AppBuilder::new().with_config(config);

        for path in self.pending_files.drain(..) {
            builder = builder.with_file(path, None);
        }

        let app = builder.build()?;
        self.app = Some(app);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Snapshot factories
    // -----------------------------------------------------------------------

    /// Build a minimal [`SessionSnapshot`] with a single view at `file_path`.
    pub fn make_snapshot(file_path: &str, anchor: usize, head: usize) -> SessionSnapshot {
        SessionSnapshot {
            version: 1,
            working_directory: PathBuf::from("/tmp/test-project"),
            layout: SessionLayout::View(SessionView {
                document: SessionDocument {
                    path: Some(PathBuf::from(file_path)),
                    selection: Some((anchor, head)),
                    view_position: Some((0, 0)),
                },
                docs_access_history: vec![],
                jumps: vec![],
            }),
            focused_index: 0,
            registers: None,
            breakpoints: None,
        }
    }

    /// Build a minimal scratch-buffer snapshot (no file path).
    pub fn make_scratch_snapshot() -> SessionSnapshot {
        SessionSnapshot {
            version: 1,
            working_directory: PathBuf::from("/tmp/test-project"),
            layout: SessionLayout::View(SessionView {
                document: SessionDocument {
                    path: None,
                    selection: Some((0, 0)),
                    view_position: None,
                },
                docs_access_history: vec![],
                jumps: vec![],
            }),
            focused_index: 0,
            registers: None,
            breakpoints: None,
        }
    }

    // -----------------------------------------------------------------------
    // Layout inspection helpers
    // -----------------------------------------------------------------------

    pub fn find_selection_in_snapshot(
        snapshot: &SessionSnapshot,
        path_suffix: &str,
    ) -> Option<(usize, usize)> {
        Self::search_layout(&snapshot.layout, path_suffix)
    }

    fn search_layout(layout: &SessionLayout, path_suffix: &str) -> Option<(usize, usize)> {
        match layout {
            SessionLayout::View(sv) => {
                if sv
                    .document
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().contains(path_suffix))
                    .unwrap_or(false)
                {
                    sv.document.selection
                } else {
                    None
                }
            }
            SessionLayout::Container { children, .. } => {
                children.iter().find_map(|c| Self::search_layout(c, path_suffix))
            }
        }
    }

    pub fn layout_contains_path(layout: &SessionLayout, path_suffix: &str) -> bool {
        match layout {
            SessionLayout::View(sv) => sv
                .document
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().contains(path_suffix))
                .unwrap_or(false),
            SessionLayout::Container { children, .. } => {
                children.iter().any(|c| Self::layout_contains_path(c, path_suffix))
            }
        }
    }

    #[allow(dead_code)]
    pub fn count_views(layout: &SessionLayout) -> usize {
        match layout {
            SessionLayout::View(_) => 1,
            SessionLayout::Container { children, .. } => {
                children.iter().map(Self::count_views).sum()
            }
        }
    }
}

// -----------------------------------------------------------------------
// Shared test infrastructure
// -----------------------------------------------------------------------

/// Build a [`helix_term::config::Config`] with LSP disabled, suitable for
/// integration tests.
pub(crate) fn test_config_no_lsp() -> Config {
    use helix_view::editor::LspConfig;
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
