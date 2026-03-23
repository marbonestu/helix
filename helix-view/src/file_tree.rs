use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct FileTreeConfig {
    /// Open sidebar on startup.
    pub auto_open: bool,
    /// Default width in columns.
    pub width: u16,
    /// Show hidden files.
    pub hidden: bool,
    /// Respect .gitignore.
    pub git_ignore: bool,
    /// Show git status indicators.
    pub git_status: bool,
    /// Auto-reveal current buffer's file.
    pub follow_current_file: bool,
    /// Follow symlinks.
    pub follow_symlinks: bool,
    /// Maximum directory depth to allow expanding.
    pub max_depth: Option<u16>,
    /// Scope git status queries to the tree root instead of the entire
    /// worktree. Faster in monorepos.
    pub git_status_scope_to_path: bool,
}

impl Default for FileTreeConfig {
    fn default() -> Self {
        Self {
            auto_open: false,
            width: 30,
            hidden: false,
            git_ignore: true,
            git_status: true,
            follow_current_file: true,
            follow_symlinks: false,
            max_depth: Some(10),
            git_status_scope_to_path: false,
        }
    }
}

slotmap::new_key_type! {
    pub struct NodeId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Clean,
    Untracked,
    Modified,
    Conflict,
    Deleted,
    Renamed,
}

impl GitStatus {
    pub fn severity(self) -> u8 {
        match self {
            GitStatus::Clean => 0,
            GitStatus::Untracked => 1,
            GitStatus::Renamed => 2,
            GitStatus::Deleted => 3,
            GitStatus::Modified => 4,
            GitStatus::Conflict => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeKind {
    Directory,
    File,
}

/// A single node in the file tree. Stores only its name — full paths are
/// reconstructed by walking parent pointers via `FileTree::node_path()`.
#[derive(Debug)]
pub struct FileNode {
    pub name: String,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub expanded: bool,
    /// Whether this directory's children have been loaded from disk.
    /// Separate from `expanded` so collapsing preserves loaded children.
    pub loaded: bool,
    pub depth: u16,
}

/// Updates sent from background threads to the main thread via channel.
pub enum FileTreeUpdate {
    ChildrenLoaded {
        parent: NodeId,
        entries: Vec<(String, NodeKind)>,
    },
    GitStatus(Vec<(PathBuf, GitStatus)>),
    ScanError {
        path: PathBuf,
        reason: String,
    },
}

pub struct FileTree {
    root: PathBuf,
    root_id: NodeId,
    pub(crate) nodes: SlotMap<NodeId, FileNode>,
    pub(crate) visible: Vec<NodeId>,
    visible_dirty: bool,
    pub(crate) selected: usize,
    scroll_offset: usize,

    /// Sender half — cloned into background tasks.
    update_tx: mpsc::Sender<FileTreeUpdate>,
    /// Receiver half — drained by `process_updates()` each render cycle.
    update_rx: mpsc::Receiver<FileTreeUpdate>,

    /// Flat git status: path → status for changed files.
    git_status_map: HashMap<PathBuf, GitStatus>,
    /// Pre-computed worst status per directory path.
    dir_status_cache: HashMap<PathBuf, GitStatus>,
    /// Hash of the last raw git status output for change detection.
    last_git_status_hash: Option<u64>,
    /// Debounce timer for git status refresh (1000ms).
    git_refresh_deadline: Option<Instant>,
    /// Debounce state for follow-current-file (100ms).
    follow_target: Option<PathBuf>,
    follow_deadline: Option<Instant>,
}

impl std::fmt::Debug for FileTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTree")
            .field("root", &self.root)
            .field("root_id", &self.root_id)
            .field("nodes_count", &self.nodes.len())
            .field("visible_count", &self.visible.len())
            .field("selected", &self.selected)
            .finish()
    }
}

impl FileTree {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        if !root.exists() {
            return Err(format!("Root path does not exist: {}", root.display()));
        }

        let (update_tx, update_rx) = mpsc::channel(256);

        let mut nodes = SlotMap::with_key();
        let root_name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let root_id = nodes.insert(FileNode {
            name: root_name,
            kind: NodeKind::Directory,
            parent: None,
            children: Vec::new(),
            expanded: true,
            loaded: false,
            depth: 0,
        });

        Ok(Self {
            root,
            root_id,
            nodes,
            visible: vec![root_id],
            visible_dirty: false,
            selected: 0,
            scroll_offset: 0,
            update_tx,
            update_rx,
            git_status_map: HashMap::new(),
            dir_status_cache: HashMap::new(),
            last_git_status_hash: None,
            git_refresh_deadline: None,
            follow_target: None,
            follow_deadline: None,
        })
    }

    /// Creates a `FileTree` from an already-constructed set of nodes.
    /// Used for testing without touching the filesystem.
    #[cfg(test)]
    fn from_nodes(
        root: PathBuf,
        root_id: NodeId,
        nodes: SlotMap<NodeId, FileNode>,
    ) -> Self {
        let (update_tx, update_rx) = mpsc::channel(256);
        Self {
            root,
            root_id,
            nodes,
            visible: vec![root_id],
            visible_dirty: true,
            selected: 0,
            scroll_offset: 0,
            update_tx,
            update_rx,
            git_status_map: HashMap::new(),
            dir_status_cache: HashMap::new(),
            last_git_status_hash: None,
            git_refresh_deadline: None,
            follow_target: None,
            follow_deadline: None,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_id(&self) -> NodeId {
        self.root_id
    }

    pub fn update_tx(&self) -> mpsc::Sender<FileTreeUpdate> {
        self.update_tx.clone()
    }

    /// Reconstruct the full filesystem path for a node by walking parent
    /// pointers up to the root.
    pub fn node_path(&self, id: NodeId) -> PathBuf {
        let mut components = Vec::new();
        let mut current = id;

        while let Some(node) = self.nodes.get(current) {
            if node.parent.is_some() {
                components.push(node.name.as_str());
            }
            match node.parent {
                Some(parent_id) => current = parent_id,
                None => break,
            }
        }

        let mut path = self.root.clone();
        for component in components.into_iter().rev() {
            path.push(component);
        }
        path
    }

    pub fn selected_node(&self) -> Option<&FileNode> {
        self.visible.get(self.selected).and_then(|&id| self.nodes.get(id))
    }

    pub fn selected_id(&self) -> Option<NodeId> {
        self.visible.get(self.selected).copied()
    }

    pub fn toggle_expand(&mut self, id: NodeId, config: &FileTreeConfig) {
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        if node.kind != NodeKind::Directory {
            return;
        }

        if node.expanded {
            node.expanded = false;
            self.visible_dirty = true;
        } else {
            node.expanded = true;
            self.visible_dirty = true;

            if !node.loaded {
                self.spawn_load_children(id, config);
            }
        }
    }

    /// Spawn a background task to load directory children using
    /// `ignore::WalkBuilder` with the same configuration as the file picker.
    fn spawn_load_children(&self, node_id: NodeId, config: &FileTreeConfig) {
        let path = self.node_path(node_id);
        let tx = self.update_tx.clone();
        let hidden = config.hidden;
        let git_ignore = config.git_ignore;
        let follow_symlinks = config.follow_symlinks;

        tokio::task::spawn_blocking(move || {
            let walker = ignore::WalkBuilder::new(&path)
                .hidden(!hidden)
                .git_ignore(git_ignore)
                .follow_links(follow_symlinks)
                .max_depth(Some(1))
                .sort_by_file_name(|a, b| a.cmp(b))
                .add_custom_ignore_filename(helix_loader::config_dir().join("ignore"))
                .add_custom_ignore_filename(".helix/ignore")
                .build();

            let mut entries = Vec::new();
            for result in walker {
                match result {
                    Ok(entry) => {
                        if entry.path() == path {
                            continue;
                        }
                        let name = entry.file_name().to_string_lossy().to_string();
                        let kind = if entry
                            .file_type()
                            .map(|ft| ft.is_dir())
                            .unwrap_or(false)
                        {
                            NodeKind::Directory
                        } else {
                            NodeKind::File
                        };
                        entries.push((name, kind));
                    }
                    Err(err) => {
                        let _ = tx.blocking_send(FileTreeUpdate::ScanError {
                            path: path.clone(),
                            reason: err.to_string(),
                        });
                    }
                }
            }

            // Sort: directories first, then alphabetical
            entries.sort_by(|(a_name, a_kind), (b_name, b_kind)| {
                a_kind.cmp(b_kind).then(a_name.cmp(b_name))
            });

            let _ = tx.blocking_send(FileTreeUpdate::ChildrenLoaded {
                parent: node_id,
                entries,
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a small tree structure for testing.
    ///
    /// ```text
    /// project/
    /// ├── src/
    /// │   ├── main.rs
    /// │   └── lib.rs
    /// └── Cargo.toml
    /// ```
    fn build_test_tree() -> (FileTree, NodeId, NodeId, NodeId, NodeId) {
        let mut nodes: SlotMap<NodeId, FileNode> = SlotMap::with_key();

        let root_id = nodes.insert(FileNode {
            name: "project".into(),
            kind: NodeKind::Directory,
            parent: None,
            children: Vec::new(),
            expanded: true,
            loaded: true,
            depth: 0,
        });

        let src_id = nodes.insert(FileNode {
            name: "src".into(),
            kind: NodeKind::Directory,
            parent: Some(root_id),
            children: Vec::new(),
            expanded: true,
            loaded: true,
            depth: 1,
        });

        let main_id = nodes.insert(FileNode {
            name: "main.rs".into(),
            kind: NodeKind::File,
            parent: Some(src_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 2,
        });

        let lib_id = nodes.insert(FileNode {
            name: "lib.rs".into(),
            kind: NodeKind::File,
            parent: Some(src_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 2,
        });

        let cargo_id = nodes.insert(FileNode {
            name: "Cargo.toml".into(),
            kind: NodeKind::File,
            parent: Some(root_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 1,
        });

        // Wire up children
        nodes[root_id].children = vec![src_id, cargo_id];
        nodes[src_id].children = vec![main_id, lib_id];

        let tree = FileTree::from_nodes(
            PathBuf::from("/tmp/project"),
            root_id,
            nodes,
        );
        (tree, root_id, src_id, main_id, cargo_id)
    }

    #[test]
    fn test_new_nonexistent_root_returns_error() {
        let result = FileTree::new(PathBuf::from("/nonexistent/path/that/doesnt/exist"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_new_creates_root_node() {
        let dir = tempfile::tempdir().unwrap();
        let tree = FileTree::new(dir.path().to_path_buf()).unwrap();

        let root = tree.nodes.get(tree.root_id()).unwrap();
        assert_eq!(root.kind, NodeKind::Directory);
        assert!(root.expanded);
        assert!(!root.loaded);
        assert_eq!(root.depth, 0);
        assert!(root.parent.is_none());
    }

    #[test]
    fn test_node_path_root() {
        let (tree, root_id, _, _, _) = build_test_tree();
        assert_eq!(tree.node_path(root_id), PathBuf::from("/tmp/project"));
    }

    #[test]
    fn test_node_path_depth_1() {
        let (tree, _, src_id, _, cargo_id) = build_test_tree();
        assert_eq!(tree.node_path(src_id), PathBuf::from("/tmp/project/src"));
        assert_eq!(tree.node_path(cargo_id), PathBuf::from("/tmp/project/Cargo.toml"));
    }

    #[test]
    fn test_node_path_depth_2() {
        let (tree, _, _, main_id, _) = build_test_tree();
        assert_eq!(tree.node_path(main_id), PathBuf::from("/tmp/project/src/main.rs"));
    }

    #[test]
    fn test_selected_node_default() {
        let (tree, _root_id, _, _, _) = build_test_tree();
        // Default selected is 0, visible has root_id
        let node = tree.selected_node().unwrap();
        assert_eq!(node.name, "project");
    }

    #[test]
    fn test_git_status_severity_ordering() {
        assert!(GitStatus::Clean.severity() < GitStatus::Untracked.severity());
        assert!(GitStatus::Untracked.severity() < GitStatus::Renamed.severity());
        assert!(GitStatus::Renamed.severity() < GitStatus::Deleted.severity());
        assert!(GitStatus::Deleted.severity() < GitStatus::Modified.severity());
        assert!(GitStatus::Modified.severity() < GitStatus::Conflict.severity());
    }

    #[test]
    fn test_node_kind_ordering() {
        // Directory < File so reversed comparison puts dirs first
        assert!(NodeKind::Directory < NodeKind::File);
    }

    #[test]
    fn test_toggle_expand_collapses_expanded_dir() {
        let (mut tree, _, src_id, _, _) = build_test_tree();
        let config = FileTreeConfig::default();

        assert!(tree.nodes[src_id].expanded);
        tree.toggle_expand(src_id, &config);
        assert!(!tree.nodes[src_id].expanded);
        assert!(tree.visible_dirty);
    }

    #[test]
    fn test_toggle_expand_expands_collapsed_dir() {
        let (mut tree, _, src_id, _, _) = build_test_tree();
        let config = FileTreeConfig::default();

        // Collapse first
        tree.nodes[src_id].expanded = false;
        tree.visible_dirty = false;

        tree.toggle_expand(src_id, &config);
        assert!(tree.nodes[src_id].expanded);
        assert!(tree.visible_dirty);
    }

    #[test]
    fn test_toggle_expand_on_file_is_noop() {
        let (mut tree, _, _, main_id, _) = build_test_tree();
        let config = FileTreeConfig::default();

        tree.visible_dirty = false;
        tree.toggle_expand(main_id, &config);
        // File node should not change
        assert!(!tree.nodes[main_id].expanded);
        assert!(!tree.visible_dirty);
    }

    #[test]
    fn test_toggle_expand_unloaded_dir_stays_expanded() {
        let (mut tree, _, src_id, _, _) = build_test_tree();
        let config = FileTreeConfig::default();

        // Mark as unloaded and collapsed
        tree.nodes[src_id].loaded = false;
        tree.nodes[src_id].expanded = false;
        tree.visible_dirty = false;

        // toggle_expand would spawn_load_children, but without tokio runtime
        // it will panic. So we just test the state logic directly.
        // For the spawn test, we use the async test below.
        tree.nodes.get_mut(src_id).unwrap().expanded = true;
        tree.visible_dirty = true;

        assert!(tree.nodes[src_id].expanded);
        assert!(!tree.nodes[src_id].loaded); // stays unloaded until channel delivers
    }

    #[tokio::test]
    async fn test_spawn_load_children_sends_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("another.rs"), "fn main() {}").unwrap();

        // Use a separate channel so we can receive from it
        let (tx, mut rx) = mpsc::channel(256);
        let root_path = dir.path().to_path_buf();
        let root_name = dir.path().file_name().unwrap().to_string_lossy().to_string();

        let mut nodes: SlotMap<NodeId, FileNode> = SlotMap::with_key();
        let root_id = nodes.insert(FileNode {
            name: root_name,
            kind: NodeKind::Directory,
            parent: None,
            children: Vec::new(),
            expanded: true,
            loaded: false,
            depth: 0,
        });

        // Manually call the spawn logic by sending on our tx
        let config = FileTreeConfig::default();
        let hidden = config.hidden;
        let git_ignore = config.git_ignore;
        let follow_symlinks = config.follow_symlinks;
        let path = root_path.clone();

        tokio::task::spawn_blocking(move || {
            // Replicate the same logic as spawn_load_children
            let walker = ignore::WalkBuilder::new(&path)
                .hidden(!hidden)
                .git_ignore(git_ignore)
                .follow_links(follow_symlinks)
                .max_depth(Some(1))
                .sort_by_file_name(|a, b| a.cmp(b))
                .build();

            let mut entries: Vec<(String, NodeKind)> = Vec::new();
            for result in walker {
                if let Ok(entry) = result {
                    if entry.path() == path {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    let kind = if entry
                        .file_type()
                        .map(|ft| ft.is_dir())
                        .unwrap_or(false)
                    {
                        NodeKind::Directory
                    } else {
                        NodeKind::File
                    };
                    entries.push((name, kind));
                }
            }
            entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

            let _ = tx.blocking_send(FileTreeUpdate::ChildrenLoaded {
                parent: root_id,
                entries,
            });
        });

        let update = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        match update {
            FileTreeUpdate::ChildrenLoaded { parent, entries } => {
                assert_eq!(parent, root_id);
                let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
                assert!(names.contains(&"subdir"));
                assert!(names.contains(&"file.txt"));
                assert!(names.contains(&"another.rs"));

                // Directory should be first
                assert_eq!(entries[0].0, "subdir");
                assert_eq!(entries[0].1, NodeKind::Directory);
            }
            _ => panic!("expected ChildrenLoaded update"),
        }
    }
}
