mod file_clipboard_steps;
mod file_creation_steps;
mod file_deletion_steps;
mod file_rename_steps;
mod navigation_steps;
mod search_steps;
mod sidebar_steps;
mod visual_steps;

use std::path::PathBuf;

use cucumber::{given, World};
use helix_term::{application::Application, config::Config};
use helix_view::{
    editor::LspConfig,
    file_tree::{FileTree, FileTreeConfig},
};
use tempfile::TempDir;

/// Shared state threaded through every file-tree BDD scenario.
///
/// Each scenario gets a fresh instance via [`FileTreeWorld::init`].
/// Library-level steps (navigation, search, visual) operate on `tree`
/// directly. Live-editor steps (sidebar visibility) build a full
/// [`Application`] via [`FileTreeWorld::build_app`].
#[derive(World)]
#[world(init = Self::init)]
pub struct FileTreeWorld {
    /// Isolated temporary directory containing the project file structure.
    pub workspace_dir: TempDir,
    /// File tree for library-level steps (navigation, search, visual).
    pub tree: Option<FileTree>,
    /// Configuration applied when constructing the FileTree or Application.
    pub tree_config: FileTreeConfig,
    /// Running editor instance for live-editor steps.
    pub app: Option<Application>,
    /// Helix configuration staged by Given steps, applied when building the app.
    pub helix_config: Config,
    /// Files staged to be opened when the Application is built.
    pub pending_files: Vec<PathBuf>,
    /// Error from the most recent failing operation.
    pub last_error: Option<String>,
    /// Name of the selected node captured by a step for later assertions.
    pub captured_node_name: Option<String>,
}

impl std::fmt::Debug for FileTreeWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTreeWorld")
            .field("workspace_dir", &self.workspace_dir.path())
            .field("tree_config", &self.tree_config)
            .field("pending_files", &self.pending_files)
            .field("last_error", &self.last_error)
            .field("captured_node_name", &self.captured_node_name)
            .finish()
    }
}

impl FileTreeWorld {
    async fn init() -> Result<Self, anyhow::Error> {
        let workspace_dir = tempfile::Builder::new()
            .prefix("helix-filetree-test-")
            .tempdir()?;
        std::fs::create_dir_all(workspace_dir.path().join(".helix"))?;

        Ok(Self {
            workspace_dir,
            tree: None,
            tree_config: FileTreeConfig {
                // Hide dotfiles so the predictable project structure
                // (src/, tests/, archive/, README.md, Cargo.toml) is the only
                // thing visible in the tree during tests.
                hidden: false,
                ..FileTreeConfig::default()
            },
            app: None,
            helix_config: test_config_no_lsp(),
            pending_files: Vec::new(),
            last_error: None,
            captured_node_name: None,
        })
    }

    /// Build and write the standard project layout into `workspace_dir`.
    pub fn create_project_structure(&mut self) {
        let root = self.workspace_dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("tests")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::write(root.join("tests/integration.rs"), "").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"project\"").unwrap();
        std::fs::write(root.join("README.md"), "# project").unwrap();
    }

    /// Construct a `FileTree` rooted at `workspace_dir`.
    pub fn init_tree(&mut self) {
        let root = self.workspace_dir.path().to_path_buf();
        self.tree = Some(
            FileTree::new(root, &self.tree_config)
                .expect("failed to construct FileTree for test"),
        );
    }

    /// Build a live [`Application`] for sidebar / live-editor steps.
    ///
    /// Sets `HELIX_WORKSPACE` to the scenario's isolated directory and enables
    /// file tree visibility in the editor config before constructing the app.
    pub fn build_app(&mut self) -> anyhow::Result<()> {
        std::env::set_var("HELIX_WORKSPACE", self.workspace_dir.path());

        let mut config = self.helix_config.clone();
        config.editor.file_tree.auto_open = false;

        let mut builder = crate::helpers::AppBuilder::new().with_config(config);
        for path in self.pending_files.drain(..) {
            builder = builder.with_file(path, None);
        }

        self.app = Some(builder.build()?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared Given steps used by multiple feature clusters
// ---------------------------------------------------------------------------

/// Library-level: creates the project structure and inits the `FileTree`.
/// Live-app level: ensures the sidebar is visible via `<space>e`.
#[given("the file tree sidebar is visible")]
async fn given_sidebar_visible(world: &mut FileTreeWorld) {
    if world.app.is_some() {
        let is_visible = world
            .app
            .as_ref()
            .map(|a| a.editor.file_tree_visible)
            .unwrap_or(false);
        if !is_visible {
            let app = world.app.as_mut().unwrap();
            crate::helpers::test_key_sequence(app, Some("<space>e"), None, false)
                .await
                .unwrap();
        }
    } else {
        world.create_project_structure();
        world.init_tree();
    }
}

/// Library-level: creates project structure and inits the `FileTree`.
/// Live-app level: ensures sidebar is visible and sets `file_tree_focused`.
#[given("the file tree sidebar is visible and focused")]
async fn given_sidebar_visible_and_focused(world: &mut FileTreeWorld) {
    if world.app.is_some() {
        let is_visible = world
            .app
            .as_ref()
            .map(|a| a.editor.file_tree_visible)
            .unwrap_or(false);
        if !is_visible {
            let app = world.app.as_mut().unwrap();
            crate::helpers::test_key_sequence(app, Some("<space>e"), None, false)
                .await
                .unwrap();
        }
        if let Some(app) = world.app.as_mut() {
            app.editor.file_tree_focused = true;
        }
    } else {
        world.create_project_structure();
        world.init_tree();
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
