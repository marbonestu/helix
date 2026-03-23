use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slotmap::SlotMap;
use tokio::sync::mpsc;

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
}
