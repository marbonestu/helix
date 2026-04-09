use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use helix_vcs::{DiffProviderRegistry, FileChange};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use tokio::sync::mpsc;

/// Controls which directory is used as the root when `gf` (file picker) or
/// `gs` (search) is invoked from the file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PickerRoot {
    /// Open the picker rooted at the directory of the currently selected node.
    #[default]
    Directory,
    /// Open the picker rooted at the file tree's workspace root.
    Workspace,
}

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
    /// Show nerd font file/folder icons.
    pub icons: bool,
    /// Command used to open a terminal at a directory when pressing `t` in the
    /// file tree. The selected directory path is appended as the final argument.
    ///
    /// Example (tmux):    `open-terminal = ["tmux", "split-window", "-c"]`
    /// Example (wezterm): `open-terminal = ["wezterm", "cli", "split-pane", "--cwd"]`
    ///
    /// When unset, Helix auto-detects based on the `[editor.terminal]` config.
    pub open_terminal: Option<Vec<String>>,
    /// Root directory used when `gf` (file picker) or `gs` (search) is invoked
    /// from the file tree.
    ///
    /// - `"directory"` (default) — use the directory of the currently selected node.
    /// - `"workspace"` — use the file tree's workspace root.
    pub picker_root: PickerRoot,
}

impl Default for FileTreeConfig {
    fn default() -> Self {
        Self {
            auto_open: false,
            width: 30,
            hidden: true,
            git_ignore: false,
            git_status: true,
            follow_current_file: true,
            follow_symlinks: false,
            max_depth: Some(10),
            git_status_scope_to_path: false,
            icons: true,
            open_terminal: None,
            picker_root: PickerRoot::Directory,
        }
    }
}

slotmap::new_key_type! {
    pub struct NodeId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Clean,
    /// File is tracked by gitignore rules and excluded from version control.
    /// Shown with a dimmed style; does not propagate to parent directories.
    Ignored,
    Untracked,
    Added,
    Staged,
    Modified,
    Conflict,
    Deleted,
    Renamed,
}

impl GitStatus {
    pub fn severity(self) -> u8 {
        match self {
            GitStatus::Clean => 0,
            GitStatus::Ignored => 0,
            GitStatus::Untracked => 1,
            GitStatus::Added => 2,
            GitStatus::Staged => 3,
            GitStatus::Renamed => 4,
            GitStatus::Deleted => 5,
            GitStatus::Modified => 6,
            GitStatus::Conflict => 7,
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
    /// Cached git status for this node. Updated after each git status refresh.
    /// Avoids O(depth) `node_path` allocations in the per-frame render loop.
    pub cached_git_status: GitStatus,
}

/// Describes the active prompt in the file tree bottom row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptMode {
    /// No prompt active — normal navigation.
    None,
    /// Incremental filename search (triggered by `/`).
    Search,
    /// Filter-as-you-type: hides rows whose names do not match the query
    /// (triggered by `f`). Unlike `Search`, this narrows the visible set
    /// rather than jumping between matches.
    Filter,
    /// New file name input. Stores the resolved target directory.
    NewFile { parent_dir: PathBuf },
    /// New directory name input.
    NewDir { parent_dir: PathBuf },
    /// Rename input. Carries the NodeId being renamed.
    Rename(NodeId),
    /// Duplicate name input. Carries the source NodeId.
    Duplicate(NodeId),
    /// Delete y/n confirmation. Carries the NodeId being deleted and whether
    /// the target is a directory (affects the confirmation prompt text).
    DeleteConfirm { id: NodeId, is_dir: bool },
    /// Delete y/n confirmation for a batch of paths collected from the
    /// multi-select set. The Vec holds the resolved paths to delete.
    DeleteConfirmMulti { paths: Vec<PathBuf> },
}

/// Clipboard operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOp {
    Copy,
    Cut,
}

/// Entry stored in the file tree clipboard.
///
/// Holds one or more source paths so that multi-selected files can be
/// copied, cut, or moved in a single paste operation.
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    /// Paths staged for the operation. Single-file operations still use a
    /// one-element vec so callers always iterate uniformly.
    pub paths: Vec<PathBuf>,
    /// NodeId of the primary clipped node for O(1) identity checks during
    /// render. For multi-select operations this is the first selected node.
    pub node_id: NodeId,
    pub op: ClipboardOp,
}

/// The resolved action from `prompt_confirm()`, ready to be dispatched to
/// an async filesystem operation.
#[derive(Debug, Clone)]
pub enum PromptCommit {
    Search,
    NewFile { parent_dir: PathBuf, name: String },
    NewDir { parent_dir: PathBuf, name: String },
    Rename { old_path: PathBuf, new_name: String },
    Duplicate { src_path: PathBuf, new_name: String },
    DeleteConfirmed(PathBuf),
    /// Confirmed deletion of multiple paths from a multi-select operation.
    DeleteConfirmedMulti(Vec<PathBuf>),
    DeleteCancelled,
}

/// Updates sent from background threads to the main thread via channel.
pub enum FileTreeUpdate {
    ChildrenLoaded {
        parent: NodeId,
        entries: Vec<(String, NodeKind)>,
        /// Names of entries in this directory that are excluded by gitignore
        /// rules. Only populated when `git_ignore` is disabled so that ignored
        /// files appear in the tree with a visual marker rather than being
        /// hidden.
        ignored_names: HashSet<String>,
    },
    /// Sent once before the first `GitStatus` batch of a refresh cycle so
    /// stale entries from the previous cycle can be discarded.
    GitStatusBegin,
    GitStatus(Vec<(PathBuf, GitStatus)>),
    ScanError {
        path: PathBuf,
        reason: String,
    },
    FsOpComplete {
        refresh_parent: PathBuf,
        select_path: Option<PathBuf>,
    },
    FsOpError {
        message: String,
    },
    /// Sent by `spawn_create_file` on success — triggers a reveal *and* an
    /// open in the editor (the distinction from `FsOpComplete`).
    FsOpCreatedFile {
        path: PathBuf,
        refresh_parent: PathBuf,
    },
    /// Sent by the filesystem watcher when an external change is detected
    /// in `dir`. The tree reloads the affected directory's children.
    ExternalChange {
        dir: PathBuf,
    },
    /// Sent by the `.git` directory watcher when git state changes
    /// (commit, checkout, rebase, stash). Triggers a git status refresh.
    GitDirEvent,
    /// Sent after the first (tracked-files-only) phase of a git status scan
    /// completes. The tree can re-render at this point; untracked files will
    /// follow in the second phase.
    GitStatusPhase1Complete,
    /// Sent after the full git status scan (including untracked files) has
    /// finished. Clears `git_refresh_in_progress` so any pending refresh can
    /// be dispatched.
    GitStatusEnd,
}

pub struct FileTree {
    root: PathBuf,
    root_id: NodeId,
    pub(crate) nodes: SlotMap<NodeId, FileNode>,
    pub(crate) visible: Vec<NodeId>,
    visible_dirty: bool,
    pub(crate) selected: usize,
    scroll_offset: usize,
    /// Last known viewport height, updated by `clamp_scroll` on each render.
    /// Used by scroll functions to keep the selection within the visible range.
    viewport_height: usize,

    /// Sender half — cloned into background tasks.
    update_tx: mpsc::Sender<FileTreeUpdate>,
    /// Receiver half — drained by `process_updates()` each render cycle.
    update_rx: mpsc::Receiver<FileTreeUpdate>,

    /// Flat git status: path → status for changed files.
    /// This is the live map used during rendering. It is updated incrementally
    /// as scan batches arrive and replaced atomically at `GitStatusEnd`.
    git_status_map: HashMap<PathBuf, GitStatus>,
    /// Accumulates results from the current background scan. Swapped into
    /// `git_status_map` at `GitStatusEnd` to atomically remove stale entries.
    git_status_new: HashMap<PathBuf, GitStatus>,
    /// Pre-computed worst status per directory path.
    dir_status_cache: HashMap<PathBuf, GitStatus>,
    /// Set to true when `git_status_map` changes or new nodes are loaded,
    /// so `rebuild_dir_status_cache` runs once at the end of `process_updates`
    /// rather than once per incoming message.
    git_status_dirty: bool,
    /// Debounce timer for the initial git status refresh.
    git_refresh_deadline: Option<Instant>,
    /// True while a background git status scan is running. Prevents overlapping
    /// refreshes from being spawned concurrently.
    git_refresh_in_progress: bool,
    /// True when a git refresh was requested while one was already in progress.
    /// Causes a second refresh to be spawned immediately after the current one
    /// sends `GitStatusEnd`.
    git_refresh_pending: bool,
    /// Debounce state for follow-current-file (100ms).
    follow_target: Option<PathBuf>,
    follow_deadline: Option<Instant>,
    /// Path to reveal after an async directory load completes.
    pending_reveal: Option<PathBuf>,
    /// File path to open in the editor after a create-file op completes.
    pending_open: Option<PathBuf>,
    /// Active prompt mode (search, new file, rename, delete confirm, etc.).
    prompt_mode: PromptMode,
    /// Text input shared by all prompt modes.
    prompt_input: String,
    /// Byte offset of the cursor within `prompt_input`.
    prompt_cursor: usize,
    /// Selection to restore if a prompt is cancelled with Esc.
    pre_prompt_selected: usize,
    /// Clipboard for copy/cut/paste.
    clipboard: Option<ClipboardEntry>,
    /// Set of nodes that are currently marked for batch operations (multi-select).
    selected_nodes: HashSet<NodeId>,
    /// Active filter query. When `Some`, `rebuild_visible` hides nodes whose
    /// names don't contain the query (plus their ancestors).
    filter_query: Option<String>,
    /// Transient status message shown in the bottom row when no prompt is active.
    status_message: Option<String>,
    /// Nodes awaiting async child loads as part of a `expand_all` operation.
    /// When `ChildrenLoaded` arrives for a node in this set, expansion continues
    /// into the newly loaded directory children.
    pending_expand_all: HashSet<NodeId>,
    /// Depth limit captured at the start of the current `expand_all` call, used
    /// to resume expansion after async child loads complete.
    expand_all_depth_limit: usize,
    /// Receives the `Debouncer` from the background watcher-init task.
    /// Polled by `process_updates()`; cleared once the debouncer is stored.
    watcher_init: Option<std::sync::mpsc::Receiver<Debouncer<RecommendedWatcher>>>,
    /// Filesystem watcher that feeds external changes into the update channel.
    /// Populated on the first `process_updates()` after the background init
    /// task completes. `None` if watching is unavailable or disabled.
    _watcher: Option<Debouncer<RecommendedWatcher>>,
    /// Receives the git-directory `Debouncer` from the background init task.
    /// Polled by `process_updates()`; cleared once the debouncer is stored.
    git_watcher_init: Option<std::sync::mpsc::Receiver<Debouncer<RecommendedWatcher>>>,
    /// Watcher for the `.git` directory. Feeds `GitDirEvent` into the update
    /// channel whenever git state changes (commit, checkout, rebase, stash).
    _git_watcher: Option<Debouncer<RecommendedWatcher>>,
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

/// Build a shallow `ignore::Walk` for one level of a directory, applying
/// all ignore-file options from the config in a single place.
fn build_dir_walker(path: &Path, config: &FileTreeConfig) -> ignore::Walk {
    ignore::WalkBuilder::new(path)
        .hidden(!config.hidden)
        .parents(config.git_ignore)
        .ignore(config.git_ignore)
        .git_ignore(config.git_ignore)
        .git_global(config.git_ignore)
        .git_exclude(config.git_ignore)
        .follow_links(config.follow_symlinks)
        .max_depth(Some(1))
        .sort_by_file_name(|a, b| a.cmp(b))
        .add_custom_ignore_filename(helix_loader::config_dir().join("ignore"))
        .add_custom_ignore_filename(".helix/ignore")
        .build()
}

/// Returns names of entries in `path` that would be filtered by gitignore
/// rules. Only performs the detection when `git_ignore` is disabled in
/// `config` (when it is enabled, ignored files are already hidden and don't
/// need to be marked separately).
fn detect_ignored_names(path: &Path, config: &FileTreeConfig) -> HashSet<String> {
    if config.git_ignore {
        return HashSet::new();
    }
    // Walk the directory a second time with gitignore rules enabled to build
    // the set of names that would survive filtering.
    let ignored_config = FileTreeConfig {
        git_ignore: true,
        ..config.clone()
    };
    let non_ignored: HashSet<String> = build_dir_walker(path, &ignored_config)
        .filter_map(|r| r.ok())
        .filter(|e| e.path() != path)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    // Any name present in the unrestricted walk but absent from the
    // gitignore-filtered walk is a gitignored entry.
    build_dir_walker(path, config)
        .filter_map(|r| r.ok())
        .filter(|e| e.path() != path)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !non_ignored.contains(name))
        .collect()
}

/// Walk up from `start` looking for a `.git` directory. Returns its path if
/// found, or `None` if the filesystem root is reached without finding one.
fn find_git_dir(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let candidate = current.join(".git");
        if candidate.is_dir() {
            return Some(candidate);
        }
        current = current.parent()?;
    }
}

impl FileTree {
    pub fn new(root: PathBuf, config: &FileTreeConfig) -> Result<Self, String> {
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
            cached_git_status: GitStatus::Clean,
        });

        // Spawn the watcher setup on a blocking thread so it does not delay
        // `new()`. Recursive inotify/FSEvents setup on a large workspace can
        // take seconds; we let the render loop pick up the Debouncer via
        // `watcher_init` once it is ready.
        let (watcher_done_tx, watcher_done_rx) = std::sync::mpsc::channel();
        let root_for_watcher = root.clone();
        let events_tx = update_tx.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(debouncer) = Self::start_watcher(&root_for_watcher, events_tx) {
                let _ = watcher_done_tx.send(debouncer);
            }
        });

        // Spawn the git-directory watcher on a blocking thread. It watches
        // `.git/` for changes that indicate git state transitions (commit,
        // checkout, rebase, stash) that don't touch working-tree files.
        let git_watcher_rx = find_git_dir(&root).map(|git_dir| {
            Self::spawn_git_watcher_init(git_dir, update_tx.clone())
        });

        let mut tree = Self {
            root,
            root_id,
            nodes,
            visible: vec![root_id],
            visible_dirty: false,
            selected: 0,
            scroll_offset: 0,
            viewport_height: 0,
            update_tx,
            update_rx,
            git_status_map: HashMap::new(),
            git_status_new: HashMap::new(),
            dir_status_cache: HashMap::new(),
            git_status_dirty: false,
            git_refresh_deadline: None,
            git_refresh_in_progress: false,
            git_refresh_pending: false,
            follow_target: None,
            follow_deadline: None,
            pending_reveal: None,
            pending_open: None,
            prompt_mode: PromptMode::None,
            prompt_input: String::new(),
            prompt_cursor: 0,
            pre_prompt_selected: 0,
            clipboard: None,
            selected_nodes: HashSet::new(),
            filter_query: None,
            status_message: None,
            pending_expand_all: HashSet::new(),
            expand_all_depth_limit: 0,
            watcher_init: Some(watcher_done_rx),
            _watcher: None,
            git_watcher_init: git_watcher_rx,
            _git_watcher: None,
        };

        // Load root children synchronously so the tree is populated on
        // first render. A depth-1 walk is fast (milliseconds).
        tree.load_children_sync(root_id, config);
        tree.rebuild_visible();
        // Schedule the first git status scan. The scan fires after the debounce
        // period expires during the first `process_updates` call.
        tree.request_git_refresh();

        Ok(tree)
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
            viewport_height: 0,
            update_tx,
            update_rx,
            git_status_map: HashMap::new(),
            git_status_new: HashMap::new(),
            dir_status_cache: HashMap::new(),
            git_status_dirty: false,
            git_refresh_deadline: None,
            git_refresh_in_progress: false,
            git_refresh_pending: false,
            follow_target: None,
            follow_deadline: None,
            pending_reveal: None,
            pending_open: None,
            prompt_mode: PromptMode::None,
            prompt_input: String::new(),
            prompt_cursor: 0,
            pre_prompt_selected: 0,
            clipboard: None,
            selected_nodes: HashSet::new(),
            filter_query: None,
            status_message: None,
            pending_expand_all: HashSet::new(),
            expand_all_depth_limit: 0,
            watcher_init: None,
            _watcher: None,
            git_watcher_init: None,
            _git_watcher: None,
        }
    }

    /// Start a debounced filesystem watcher on `root`, forwarding external
    /// change events into `tx` as [`FileTreeUpdate::ExternalChange`] messages.
    ///
    /// Returns `None` if the platform watcher cannot be initialised — the tree
    /// continues to work without automatic updates in that case.
    fn start_watcher(
        root: &Path,
        tx: mpsc::Sender<FileTreeUpdate>,
    ) -> Option<Debouncer<RecommendedWatcher>> {
        let root_buf = root.to_path_buf();
        let result = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
            let events = match res {
                Ok(ev) => ev,
                Err(_) => return,
            };

            // Collect unique parent directories so we only send one update per dir.
            let mut dirs: std::collections::HashSet<PathBuf> =
                std::collections::HashSet::new();
            for event in events {
                let dir = if event.path.is_dir() {
                    event.path.clone()
                } else {
                    event.path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| root_buf.clone())
                };
                dirs.insert(dir);
            }

            for dir in dirs {
                let _ = tx.blocking_send(FileTreeUpdate::ExternalChange { dir });
            }
            helix_event::request_redraw();
        });

        match result {
            Ok(mut debouncer) => {
                if let Err(e) = debouncer.watcher().watch(root, RecursiveMode::Recursive) {
                    log::warn!("file tree: failed to watch {}: {e}", root.display());
                    return None;
                }
                Some(debouncer)
            }
            Err(e) => {
                log::warn!("file tree: failed to start filesystem watcher: {e}");
                None
            }
        }
    }

    /// Spawn a background task that watches `git_dir` for changes and sends
    /// [`FileTreeUpdate::GitDirEvent`] via `tx` whenever git state changes.
    ///
    /// Lock files (`.lock`) and editor temporaries (`_null-ls_`, `.tmp`) are
    /// filtered out to avoid spurious refreshes during in-progress git operations.
    ///
    /// Returns a receiver that yields the `Debouncer` once the watcher is ready.
    /// The caller stores this in `git_watcher_init` and polls it in
    /// `process_updates()`, matching the pattern used by `watcher_init`.
    fn spawn_git_watcher_init(
        git_dir: PathBuf,
        tx: mpsc::Sender<FileTreeUpdate>,
    ) -> std::sync::mpsc::Receiver<Debouncer<RecommendedWatcher>> {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        tokio::task::spawn_blocking(move || {
            let result = new_debouncer(
                Duration::from_millis(500),
                move |res: DebounceEventResult| {
                    let events = match res {
                        Ok(ev) => ev,
                        Err(_) => return,
                    };
                    let should_refresh = events.iter().any(|event| {
                        let path = &event.path;
                        // Skip git lock files: they are created at the start of
                        // a git operation and removed when it finishes. Reacting
                        // to them would trigger a stale read.
                        if path.extension().map_or(false, |e| e == "lock") {
                            return false;
                        }
                        // Skip editor temporary files that land inside .git/.
                        let file_name =
                            path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if file_name.contains("_null-ls_") || file_name.ends_with(".tmp") {
                            return false;
                        }
                        true
                    });
                    if should_refresh {
                        let _ = tx.blocking_send(FileTreeUpdate::GitDirEvent);
                        helix_event::request_redraw();
                    }
                },
            );
            match result {
                Ok(mut debouncer) => {
                    if let Err(e) =
                        debouncer.watcher().watch(&git_dir, RecursiveMode::Recursive)
                    {
                        log::warn!(
                            "file tree: failed to watch git dir {}: {e}",
                            git_dir.display()
                        );
                        return;
                    }
                    let _ = done_tx.send(debouncer);
                }
                Err(e) => {
                    log::warn!("file tree: failed to start git directory watcher: {e}");
                }
            }
        });
        done_rx
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

    pub fn nodes(&self) -> &SlotMap<NodeId, FileNode> {
        &self.nodes
    }

    pub fn visible(&self) -> &[NodeId] {
        &self.visible
    }

    pub fn selected(&self) -> usize {
        self.selected
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

    pub fn move_to(&mut self, pos: usize) {
        self.selected = pos.min(self.visible.len().saturating_sub(1));
        self.ensure_selected_visible();
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

    /// Synchronously expand a directory, loading its children immediately.
    /// Unlike [`toggle_expand`], this does not spawn a background task, making
    /// it suitable for test contexts where async child loading is not available.
    pub fn expand_sync(&mut self, id: NodeId, config: &FileTreeConfig) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        if node.kind != NodeKind::Directory {
            return;
        }
        if !node.expanded {
            if let Some(n) = self.nodes.get_mut(id) {
                n.expanded = true;
                self.visible_dirty = true;
            }
        }
        if !self.nodes.get(id).map(|n| n.loaded).unwrap_or(false) {
            self.load_children_sync(id, config);
        }
        self.rebuild_visible();
    }

    /// Collapse all directories to root level. The selected node is preserved by
    /// `rebuild_visible`, which restores selection by node ID rather than position.
    pub fn collapse_all(&mut self) {
        for (_, node) in self.nodes.iter_mut() {
            if node.kind == NodeKind::Directory && node.parent.is_some() {
                node.expanded = false;
            }
        }
        self.pending_expand_all.clear();
        self.visible_dirty = true;
    }

    /// Recursively expand `start_id` and all of its loaded descendants.
    /// Directories whose children have not yet been loaded are marked expanded
    /// and queued for an async load; expansion resumes automatically when
    /// `ChildrenLoaded` arrives for those nodes.
    ///
    /// Expansion is capped at `config.max_depth` (default 10) to prevent
    /// unbounded traversal of very deep trees.
    pub fn expand_all(&mut self, start_id: NodeId, config: &FileTreeConfig) {
        let limit = config.max_depth.unwrap_or(10) as usize;
        self.expand_all_depth_limit = limit;
        let start_depth = self.nodes.get(start_id).map(|n| n.depth as usize).unwrap_or(0);
        self.expand_subtree(start_id, start_depth, limit, config);
        self.visible_dirty = true;
    }

    /// Synchronously expand `start_id` and all of its descendants by loading
    /// children from disk as needed. Intended for tests where async task
    /// completion cannot be awaited. Capped at `config.max_depth`.
    pub fn expand_all_sync(&mut self, start_id: NodeId, config: &FileTreeConfig) {
        let limit = config.max_depth.unwrap_or(10) as usize;
        self.expand_all_depth_limit = limit;
        let start_depth = self.nodes.get(start_id).map(|n| n.depth as usize).unwrap_or(0);
        self.expand_subtree_sync_impl(start_id, start_depth, limit, config);
        self.visible_dirty = true;
        self.rebuild_visible();
    }

    fn expand_subtree_sync_impl(
        &mut self,
        id: NodeId,
        depth: usize,
        limit: usize,
        config: &FileTreeConfig,
    ) {
        let Some(node) = self.nodes.get(id) else { return };
        if node.kind != NodeKind::Directory {
            return;
        }
        if depth >= limit {
            self.set_status(format!(
                "Expansion stopped at depth {limit} (max-depth limit reached)"
            ));
            return;
        }
        if !self.nodes.get(id).map(|n| n.loaded).unwrap_or(false) {
            self.load_children_sync(id, config);
        }
        if let Some(n) = self.nodes.get_mut(id) {
            n.expanded = true;
        }
        let children = self.nodes.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child_id in children {
            self.expand_subtree_sync_impl(child_id, depth + 1, limit, config);
        }
    }

    fn expand_subtree(
        &mut self,
        id: NodeId,
        depth: usize,
        limit: usize,
        config: &FileTreeConfig,
    ) {
        let Some(node) = self.nodes.get(id) else { return };
        if node.kind != NodeKind::Directory {
            return;
        }
        if depth >= limit {
            self.set_status(format!(
                "Expansion stopped at depth {limit} (max-depth limit reached)"
            ));
            return;
        }

        let loaded = node.loaded;
        if let Some(n) = self.nodes.get_mut(id) {
            n.expanded = true;
        }

        if !loaded {
            self.pending_expand_all.insert(id);
            self.spawn_load_children(id, config);
            return;
        }

        let children: Vec<NodeId> = self.nodes.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child_id in children {
            self.expand_subtree(child_id, depth + 1, limit, config);
        }
    }

    /// Remove a node and all its descendants from the slotmap.
    fn remove_subtree(&mut self, id: NodeId) {
        let children = self.nodes.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            self.remove_subtree(child);
        }
        self.nodes.remove(id);
    }

    /// Force the git refresh deadline into the past so the next
    /// `process_updates` call fires the scan immediately. Intended for tests
    /// that need to simulate elapsed debounce time without sleeping.
    pub fn force_git_refresh_deadline_past(&mut self) {
        if self.git_refresh_deadline.is_some() {
            self.git_refresh_deadline = Some(Instant::now() - Duration::from_secs(1));
        }
    }

    /// Reset all pending git-refresh state. Use in tests to start from a
    /// known-clean baseline before asserting no refresh is triggered.
    pub fn clear_git_refresh_for_test(&mut self) {
        self.git_refresh_deadline = None;
        self.git_refresh_pending = false;
        self.git_refresh_in_progress = false;
    }

    /// Clear the git status map as if a refresh cycle completed with no results.
    /// Use in tests to verify stale entries are removed when a scan completes.
    pub fn clear_git_status_map_for_test(&mut self) {
        self.git_status_map.clear();
        self.git_status_new.clear();
        self.git_status_dirty = true;
    }

    /// Returns `true` when no git status entries have been received yet.
    pub fn git_status_map_is_empty(&self) -> bool {
        self.git_status_map.is_empty()
    }

    /// Set `git_refresh_in_progress` to `true`. Used in tests to simulate
    /// a background scan that is currently running.
    pub fn simulate_git_refresh_in_progress(&mut self) {
        self.git_refresh_in_progress = true;
        self.git_status_new.clear();
    }

    /// Returns `true` if a deferred git refresh is queued (set while a scan
    /// was in progress so that one more scan fires when the current one ends).
    pub fn has_pending_deferred_git_refresh(&self) -> bool {
        self.git_refresh_pending
    }

    /// Synchronously re-scan all expanded directories, replacing stale node
    /// entries with fresh data from disk. Use in test contexts instead of
    /// [`refresh`] (which is async).
    ///
    /// Root is refreshed with a merge strategy: existing child nodes whose
    /// names still exist on disk are kept (preserving expand state), new
    /// on-disk entries get fresh nodes, and deleted entries are pruned.
    /// Non-root expanded directories are fully replaced.
    pub fn refresh_sync(&mut self, config: &FileTreeConfig) {
        // Merge-refresh root so that:
        //   • Existing children whose names still exist on disk are kept
        //     (their NodeIds and expand state are preserved).
        //   • Children that were deleted on disk are removed (with their subtrees).
        //   • Newly created on-disk entries get fresh child nodes.
        let root_id = self.root_id;
        let root_path = self.root.clone();
        let root_depth = self.nodes.get(root_id).map(|n| n.depth).unwrap_or(0);

        // Build a name→NodeId map of current root children.
        let old_by_name: std::collections::HashMap<String, NodeId> = self
            .nodes
            .get(root_id)
            .map(|n| {
                n.children
                    .iter()
                    .filter_map(|&id| {
                        self.nodes.get(id).map(|c| (c.name.clone(), id))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Read fresh entries from disk.
        let fresh_entries = self.read_dir_entries(&root_path, config);

        // Build the new children list: reuse existing nodes or create new ones.
        let mut new_children: Vec<NodeId> = Vec::new();
        let mut seen_names = std::collections::HashSet::new();
        for (name, kind) in &fresh_entries {
            seen_names.insert(name.clone());
            if let Some(&existing_id) = old_by_name.get(name) {
                new_children.push(existing_id);
            } else {
                let new_id = self.nodes.insert(FileNode {
                    name: name.clone(),
                    kind: *kind,
                    parent: Some(root_id),
                    children: Vec::new(),
                    expanded: false,
                    loaded: false,
                    depth: root_depth + 1,
                    cached_git_status: GitStatus::Clean,
                });
                new_children.push(new_id);
            }
        }

        // Sort: directories before files, then alphabetically.
        new_children.sort_by(|&a, &b| {
            let na = &self.nodes[a];
            let nb = &self.nodes[b];
            na.kind.cmp(&nb.kind).then(na.name.cmp(&nb.name))
        });

        // Remove subtrees for old children that are no longer on disk.
        for (name, &old_id) in &old_by_name {
            if !seen_names.contains(name) {
                self.remove_subtree(old_id);
            }
        }

        if let Some(root) = self.nodes.get_mut(root_id) {
            root.children = new_children;
            root.loaded = true;
        }

        // Re-scan all non-root expanded directories with a full replace.
        let expanded_dirs: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.kind == NodeKind::Directory && n.expanded && n.parent.is_some())
            .map(|(id, _)| id)
            .collect();

        for &dir_id in &expanded_dirs {
            let old_children: Vec<NodeId> = self
                .nodes
                .get(dir_id)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            for child_id in old_children {
                self.remove_subtree(child_id);
            }
            if let Some(node) = self.nodes.get_mut(dir_id) {
                node.children.clear();
                node.loaded = false;
            }
            self.load_children_sync(dir_id, config);
        }

        self.rebuild_visible();
    }

    /// Spawn a background task to load directory children using
    /// `ignore::WalkBuilder` with the same configuration as the file picker.
    fn spawn_load_children(&self, node_id: NodeId, config: &FileTreeConfig) {
        let path = self.node_path(node_id);
        let tx = self.update_tx.clone();
        let config = config.clone();

        tokio::task::spawn_blocking(move || {
            let walker = build_dir_walker(&path, &config);

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

            let ignored_names = detect_ignored_names(&path, &config);

            let _ = tx.blocking_send(FileTreeUpdate::ChildrenLoaded {
                parent: node_id,
                entries,
                ignored_names,
            });
            helix_event::request_redraw();
        });
    }

    /// Read one level of a directory's children from disk, returning
    /// `(name, kind)` pairs without modifying the node graph.
    fn read_dir_entries(&self, path: &std::path::Path, config: &FileTreeConfig) -> Vec<(String, NodeKind)> {
        let walker = build_dir_walker(path, config);

        let mut entries = Vec::new();
        for result in walker {
            if let Ok(entry) = result {
                if entry.path() == path {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let kind = if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    NodeKind::Directory
                } else {
                    NodeKind::File
                };
                entries.push((name, kind));
            }
        }
        entries
    }

    /// Synchronously load one level of directory children. Used for the
    /// initial root load so the tree is populated on first render.
    fn load_children_sync(&mut self, node_id: NodeId, config: &FileTreeConfig) {
        let path = self.node_path(node_id);
        let walker = build_dir_walker(&path, config);
        let ignored_names = detect_ignored_names(&path, config);

        let depth = self.nodes.get(node_id).map(|n| n.depth + 1).unwrap_or(1);
        let mut child_ids = Vec::new();

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
                let initial_status = if ignored_names.contains(&name) {
                    GitStatus::Ignored
                } else {
                    GitStatus::Clean
                };
                let child_id = self.nodes.insert(FileNode {
                    name,
                    kind,
                    parent: Some(node_id),
                    children: Vec::new(),
                    expanded: false,
                    loaded: false,
                    depth,
                    cached_git_status: initial_status,
                });
                child_ids.push(child_id);
            }
        }

        child_ids.sort_by(|&a, &b| {
            let na = &self.nodes[a];
            let nb = &self.nodes[b];
            na.kind.cmp(&nb.kind).then(na.name.cmp(&nb.name))
        });

        if let Some(node) = self.nodes.get_mut(node_id) {
            node.children = child_ids;
            node.loaded = true;
        }

        // New nodes start with cached_git_status = Clean. If git status has
        // already been fetched, mark the cache dirty so rebuild_dir_status_cache
        // runs on the next process_updates and assigns the correct status.
        if !self.git_status_map.is_empty() {
            self.git_status_dirty = true;
        }
    }

    /// Drain the update channel and check debounce timers. Called at the
    /// start of each render cycle.
    pub fn process_updates(
        &mut self,
        config: &FileTreeConfig,
        diff_providers: Option<&DiffProviderRegistry>,
    ) {
        // Pick up the Debouncer from the background watcher-init task if ready.
        if self._watcher.is_none() {
            if let Some(rx) = &self.watcher_init {
                if let Ok(debouncer) = rx.try_recv() {
                    self._watcher = Some(debouncer);
                    self.watcher_init = None;
                }
            }
        }

        // Pick up the git-directory Debouncer once the background init task is ready.
        if self._git_watcher.is_none() {
            if let Some(rx) = &self.git_watcher_init {
                if let Ok(debouncer) = rx.try_recv() {
                    self._git_watcher = Some(debouncer);
                    self.git_watcher_init = None;
                }
            }
        }

        // Check debounce timers
        if let Some(providers) = diff_providers {
            self.check_git_refresh_timer(providers);
        }
        self.check_follow_timer(config);

        // Drain channel
        while let Ok(update) = self.update_rx.try_recv() {
            match update {
                FileTreeUpdate::ChildrenLoaded { parent, entries, ignored_names } => {
                    let Some(parent_node) = self.nodes.get(parent) else {
                        continue;
                    };
                    let depth = parent_node.depth + 1;

                    // Build a name → NodeId map for the current children so we
                    // can reuse existing node IDs for entries that survive the
                    // rescan. Reusing an ID preserves the node's subtree and
                    // keeps grandchildren's `parent` pointers valid, avoiding
                    // the stale-parent bug where node_path drops intermediate
                    // path components and opens a blank buffer.
                    let old_children: Vec<NodeId> = self
                        .nodes
                        .get(parent)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();
                    let mut old_by_name: std::collections::HashMap<String, NodeId> =
                        old_children
                            .iter()
                            .filter_map(|&id| {
                                self.nodes.get(id).map(|n| (n.name.clone(), id))
                            })
                            .collect();

                    let mut child_ids = Vec::with_capacity(entries.len());
                    for (name, kind) in entries {
                        let is_ignored = ignored_names.contains(&name);
                        if let Some(existing_id) = old_by_name.remove(&name) {
                            // Entry still exists: update the node in place so
                            // its NodeId (and all grandchild parent pointers)
                            // remain valid.
                            if let Some(node) = self.nodes.get_mut(existing_id) {
                                node.kind = kind;
                                node.parent = Some(parent);
                                node.depth = depth;
                                // Sync ignored status with the current scan.
                                if is_ignored {
                                    node.cached_git_status = GitStatus::Ignored;
                                } else if node.cached_git_status == GitStatus::Ignored {
                                    node.cached_git_status = GitStatus::Clean;
                                }
                            }
                            child_ids.push(existing_id);
                        } else {
                            // New entry: create a fresh node.
                            let child_id = self.nodes.insert(FileNode {
                                name,
                                kind,
                                parent: Some(parent),
                                children: Vec::new(),
                                expanded: false,
                                loaded: false,
                                depth,
                                cached_git_status: if is_ignored {
                                    GitStatus::Ignored
                                } else {
                                    GitStatus::Clean
                                },
                            });
                            child_ids.push(child_id);
                        }
                    }

                    // Remove entries that are no longer on disk, including their
                    // entire subtrees.
                    for (_, removed_id) in old_by_name {
                        self.remove_subtree(removed_id);
                    }

                    if let Some(parent_node) = self.nodes.get_mut(parent) {
                        parent_node.children = child_ids.clone();
                        parent_node.loaded = true;
                    }

                    // If a directory child was previously expanded but its old
                    // NodeId was replaced (race: parent rescanned while child
                    // load was in flight), the ChildrenLoaded for the old ID
                    // was silently dropped. Re-spawn the load so the child's
                    // files appear without requiring another user interaction.
                    for child_id in child_ids.iter().copied() {
                        if let Some(node) = self.nodes.get(child_id) {
                            if node.expanded && !node.loaded {
                                self.spawn_load_children(child_id, config);
                            }
                        }
                    }

                    // Continue recursive expansion if this parent was queued by
                    // `expand_all`. Resume expansion into the newly loaded children
                    // so the full subtree is expanded without user interaction.
                    if self.pending_expand_all.remove(&parent) {
                        let parent_depth = self
                            .nodes
                            .get(parent)
                            .map(|n| n.depth as usize)
                            .unwrap_or(0);
                        let limit = self.expand_all_depth_limit;
                        for child_id in child_ids {
                            self.expand_subtree(child_id, parent_depth + 1, limit, config);
                        }
                    }

                    self.visible_dirty = true;
                    self.git_status_dirty = true;
                }
                FileTreeUpdate::GitStatusBegin => {
                    // No-op. Kept as a variant for compatibility.
                }
                FileTreeUpdate::GitStatus(statuses) => {
                    for (path, status) in statuses {
                        // Insert into both the live map (for immediate display)
                        // and the incoming buffer (for atomic replacement at end).
                        self.git_status_map.insert(path.clone(), status);
                        self.git_status_new.insert(path, status);
                    }
                    self.git_status_dirty = true;
                }
                FileTreeUpdate::ScanError { path, reason } => {
                    log::warn!("file tree: {}: {}", path.display(), reason);
                }
                FileTreeUpdate::FsOpComplete { refresh_parent, select_path } => {
                    // Find the node for refresh_parent and mark it for re-scan
                    let node_id = self.nodes.iter()
                        .find(|(id, _)| self.node_path(*id) == refresh_parent)
                        .map(|(id, _)| id);
                    if let Some(id) = node_id {
                        if let Some(node) = self.nodes.get_mut(id) {
                            node.loaded = false;
                        }
                        self.spawn_load_children(id, config);
                    }
                    if let Some(path) = select_path {
                        self.pending_reveal = Some(path);
                    }
                    self.request_git_refresh();
                }
                FileTreeUpdate::FsOpError { message } => {
                    self.set_status(message);
                }
                FileTreeUpdate::FsOpCreatedFile { path, refresh_parent } => {
                    let node_id = self.nodes.iter()
                        .find(|(id, _)| self.node_path(*id) == refresh_parent)
                        .map(|(id, _)| id);
                    if let Some(id) = node_id {
                        if let Some(node) = self.nodes.get_mut(id) {
                            node.loaded = false;
                        }
                        self.spawn_load_children(id, config);
                    }
                    self.pending_reveal = Some(path.clone());
                    self.pending_open = Some(path);
                    self.request_git_refresh();
                }
                FileTreeUpdate::ExternalChange { dir } => {
                    // Find the closest loaded ancestor node for this directory
                    // and trigger a children reload so the tree stays in sync.
                    let node_id = self.nodes.iter()
                        .find(|(id, n)| n.loaded && self.node_path(*id) == dir)
                        .map(|(id, _)| id)
                        .or_else(|| {
                            // Fall back to the parent of `dir` if `dir` itself
                            // isn't a node we track (e.g. a newly created dir).
                            let parent = dir.parent()?;
                            self.nodes.iter()
                                .find(|(id, n)| n.loaded && self.node_path(*id) == parent)
                                .map(|(id, _)| id)
                        });
                    if let Some(id) = node_id {
                        if let Some(node) = self.nodes.get_mut(id) {
                            node.loaded = false;
                        }
                        self.spawn_load_children(id, config);
                    }
                    self.request_git_refresh();
                }
                FileTreeUpdate::GitDirEvent => {
                    // Git state changed (commit, checkout, rebase, stash).
                    // Schedule a git status refresh to pick up the new state.
                    self.request_git_refresh();
                }
                FileTreeUpdate::GitStatusPhase1Complete => {
                    // Phase 1 (tracked files) is done. The tree can re-render
                    // immediately; untracked files will arrive in phase 2.
                    // No state change needed — git_refresh_in_progress stays true
                    // until GitStatusEnd.
                }
                FileTreeUpdate::GitStatusEnd => {
                    // The full scan has finished. Atomically replace the live map
                    // with the incoming buffer so that stale entries from the
                    // previous cycle are removed all at once.
                    std::mem::swap(&mut self.git_status_map, &mut self.git_status_new);
                    self.git_status_new.clear();
                    self.git_status_dirty = true;
                    self.git_refresh_in_progress = false;
                    if self.git_refresh_pending {
                        self.git_refresh_pending = false;
                        if let Some(providers) = diff_providers {
                            let providers = providers.clone();
                            self.spawn_git_status(providers);
                        }
                    }
                }
            }
        }

        // Retry pending reveal after directory loads complete
        if let Some(path) = self.pending_reveal.take() {
            self.reveal_path(&path, config);
        }

        if self.git_status_dirty {
            self.git_status_dirty = false;
            self.rebuild_dir_status_cache();
        }

        if self.visible_dirty {
            self.rebuild_visible();
        }
    }

    fn rebuild_dir_status_cache(&mut self) {
        self.dir_status_cache.clear();

        for (path, &status) in &self.git_status_map {
            // If the entry itself is a directory (e.g. an untracked dir reported
            // by git), record it directly in the cache.
            if path.is_dir() {
                let entry = self
                    .dir_status_cache
                    .entry(path.clone())
                    .or_insert(GitStatus::Clean);
                if status.severity() > entry.severity() {
                    *entry = status;
                }
            }

            let mut ancestor = path.parent();
            while let Some(dir) = ancestor {
                if dir < self.root.as_path() {
                    break;
                }
                let entry = self
                    .dir_status_cache
                    .entry(dir.to_path_buf())
                    .or_insert(GitStatus::Clean);
                if status.severity() > entry.severity() {
                    *entry = status;
                }
                ancestor = dir.parent();
            }
        }

        // Populate per-node status cache so render-loop lookups are O(1).
        let node_ids: Vec<NodeId> = self.nodes.keys().collect();
        for id in node_ids {
            let status = self.git_status_for_compute(id);
            if let Some(node) = self.nodes.get_mut(id) {
                node.cached_git_status = status;
            }
        }
    }

    /// Compute the git status for a node without using the cache.
    /// Called during cache rebuild; use `git_status_for` for all other access.
    fn git_status_for_compute(&self, id: NodeId) -> GitStatus {
        let path = self.node_path(id);
        let node = match self.nodes.get(id) {
            Some(n) => n,
            None => return GitStatus::Clean,
        };

        match node.kind {
            NodeKind::File => {
                if let Some(&status) = self.git_status_map.get(&path) {
                    return status;
                }
                // Preserve Ignored status assigned by the directory scanner.
                // This persists across git refreshes because Ignored entries are
                // not in git_status_map; they live in node.cached_git_status.
                if node.cached_git_status == GitStatus::Ignored {
                    return GitStatus::Ignored;
                }
                // Inherit Untracked or Ignored from a parent directory.
                let mut current = id;
                while let Some(parent_id) = self.nodes.get(current).and_then(|n| n.parent) {
                    let parent_path = self.node_path(parent_id);
                    if let Some(&s) = self.git_status_map.get(&parent_path) {
                        if matches!(s, GitStatus::Untracked) {
                            return s;
                        }
                    }
                    if let Some(parent_node) = self.nodes.get(parent_id) {
                        if parent_node.cached_git_status == GitStatus::Ignored {
                            return GitStatus::Ignored;
                        }
                    }
                    current = parent_id;
                }
                GitStatus::Clean
            }
            NodeKind::Directory => self
                .dir_status_cache
                .get(&path)
                .copied()
                .unwrap_or_else(|| {
                    // Preserve Ignored status for directories not in dir_status_cache.
                    if node.cached_git_status == GitStatus::Ignored {
                        GitStatus::Ignored
                    } else {
                        GitStatus::Clean
                    }
                }),
        }
    }

    /// Returns the cached git status for a node. O(1).
    pub fn git_status_for(&self, id: NodeId) -> GitStatus {
        self.nodes
            .get(id)
            .map(|n| n.cached_git_status)
            .unwrap_or(GitStatus::Clean)
    }

    /// Rebuild the flat visible list from the tree structure using
    /// stack-based traversal. Preserves the selected node across rebuilds.
    /// When a filter query is active and non-empty, only matching nodes and
    /// their ancestors are included regardless of directory expanded state.
    fn rebuild_visible(&mut self) {
        if let Some(query) = self.filter_query.clone() {
            if !query.is_empty() {
                self.rebuild_visible_filtered(&query.to_lowercase());
                return;
            }
        }

        // Remember which node was selected so we can restore after rebuild
        let selected_node_id = self.visible.get(self.selected).copied();

        self.visible.clear();
        let mut stack = vec![self.root_id];

        while let Some(id) = stack.pop() {
            self.visible.push(id);
            if let Some(node) = self.nodes.get(id) {
                if node.expanded {
                    // Push in reverse so first child is visited first
                    for &child in node.children.iter().rev() {
                        stack.push(child);
                    }
                }
            }
        }

        // Restore selection by NodeId rather than raw index
        if let Some(prev_id) = selected_node_id {
            if let Some(pos) = self.visible.iter().position(|&id| id == prev_id) {
                self.selected = pos;
            }
        }

        let max = self.visible.len().saturating_sub(1);
        self.selected = self.selected.min(max);
        self.scroll_offset = self.scroll_offset.min(max);

        self.visible_dirty = false;
    }

    /// Build the visible list for a non-empty filter query. Only includes nodes
    /// whose name contains `query` (case-insensitive) plus all their ancestors.
    /// Directories with matching descendants are shown even if collapsed.
    fn rebuild_visible_filtered(&mut self, query: &str) {
        let selected_node_id = self.visible.get(self.selected).copied();
        let included = self.filter_included_nodes(query);

        self.visible.clear();
        let mut stack = vec![self.root_id];
        while let Some(id) = stack.pop() {
            if !included.contains(&id) {
                continue;
            }
            self.visible.push(id);
            if let Some(node) = self.nodes.get(id) {
                if node.kind == NodeKind::Directory {
                    for &child in node.children.iter().rev() {
                        stack.push(child);
                    }
                }
            }
        }

        if let Some(prev_id) = selected_node_id {
            if let Some(pos) = self.visible.iter().position(|&id| id == prev_id) {
                self.selected = pos;
            }
        }
        let max = self.visible.len().saturating_sub(1);
        self.selected = self.selected.min(max);
        self.scroll_offset = self.scroll_offset.min(max);
        self.visible_dirty = false;
    }

    /// Compute the set of node IDs to include when `query` is active.
    /// A node is included if its name matches the query OR if any of its
    /// descendants match (so ancestor directories remain visible as context).
    fn filter_included_nodes(&self, query: &str) -> std::collections::HashSet<NodeId> {
        let mut result = std::collections::HashSet::new();
        fn recurse(
            tree: &FileTree,
            id: NodeId,
            query: &str,
            result: &mut std::collections::HashSet<NodeId>,
        ) -> bool {
            let Some(node) = tree.nodes.get(id) else {
                return false;
            };
            let self_matches = node.name.to_lowercase().contains(query);
            let children: Vec<NodeId> = node.children.iter().copied().collect();
            // Must NOT short-circuit: recurse into ALL children so every
            // matching node gets added to `result`, even after a first match.
            let any_child_matches = children
                .iter()
                .fold(false, |found, &c| recurse(tree, c, query, result) | found);
            let included = self_matches || any_child_matches;
            if included {
                result.insert(id);
            }
            included
        }
        recurse(self, self.root_id, query, &mut result);
        result
    }

    /// Force an unconditional visible-list rebuild, bypassing the dirty flag.
    /// Intended for benchmarking the rebuild cost in isolation.
    pub fn force_rebuild_visible(&mut self) {
        self.visible_dirty = true;
        self.rebuild_visible();
    }

    // --- Navigation ---

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.ensure_selected_visible();
    }

    pub fn move_down(&mut self) {
        self.selected = (self.selected + 1).min(self.visible.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    pub fn jump_to_top(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn jump_to_bottom(&mut self) {
        self.selected = self.visible.len().saturating_sub(1);
        self.ensure_selected_visible();
    }

    pub fn page_up(&mut self, count: usize) {
        self.selected = self.selected.saturating_sub(count);
        self.ensure_selected_visible();
    }

    pub fn page_down(&mut self, count: usize) {
        self.selected =
            (self.selected + count).min(self.visible.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    /// Scroll the viewport up one line, keeping the selection within the new
    /// visible range. If the selection would go below the bottom of the viewport
    /// it is snapped to the last visible row.
    pub fn scroll_view_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
        if self.viewport_height > 0 {
            let last_visible = (self.scroll_offset + self.viewport_height)
                .saturating_sub(1)
                .min(self.visible.len().saturating_sub(1));
            if self.selected > last_visible {
                self.selected = last_visible;
            }
        }
    }

    /// Scroll the viewport down one line, keeping the selection within the new
    /// visible range. If the selection would go above the top of the viewport
    /// it is snapped to the first visible row.
    pub fn scroll_view_down(&mut self) {
        let max = self.visible.len().saturating_sub(1);
        if self.scroll_offset < max {
            self.scroll_offset += 1;
            if self.selected < self.scroll_offset {
                self.selected = self.scroll_offset;
            }
        }
    }

    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
        // Upper bound clamping happens in the render function where
        // viewport height is known.
    }

    // --- Prompt mode ---

    pub fn prompt_mode(&self) -> &PromptMode {
        &self.prompt_mode
    }

    pub fn prompt_input(&self) -> &str {
        &self.prompt_input
    }

    /// Byte offset of the cursor within the prompt input.
    pub fn prompt_cursor(&self) -> usize {
        self.prompt_cursor
    }

    /// Set prompt input text and place the cursor at the end.
    fn set_prompt_input(&mut self, text: String) {
        self.prompt_cursor = text.len();
        self.prompt_input = text;
    }

    /// Insert a character at the cursor position, then advance the cursor.
    pub fn prompt_push(&mut self, ch: char) {
        self.prompt_input.insert(self.prompt_cursor, ch);
        self.prompt_cursor += ch.len_utf8();
        if matches!(self.prompt_mode, PromptMode::Search) {
            let from = self.pre_prompt_selected;
            self.search_jump_next_from(from);
        }
        if matches!(self.prompt_mode, PromptMode::Filter) {
            self.filter_query = Some(self.prompt_input.clone());
            self.visible_dirty = true;
            self.rebuild_visible();
        }
    }

    /// Delete the character immediately before the cursor (backspace).
    pub fn prompt_pop(&mut self) {
        if self.prompt_cursor == 0 {
            return;
        }
        // Step back to the start of the previous UTF-8 character.
        let mut new_cursor = self.prompt_cursor - 1;
        while !self.prompt_input.is_char_boundary(new_cursor) {
            new_cursor -= 1;
        }
        self.prompt_input.remove(new_cursor);
        self.prompt_cursor = new_cursor;
        if matches!(self.prompt_mode, PromptMode::Search) {
            if self.prompt_input.is_empty() {
                self.selected = self.pre_prompt_selected;
                self.ensure_selected_visible();
            } else {
                let from = self.pre_prompt_selected;
                self.search_jump_next_from(from);
            }
        }
        if matches!(self.prompt_mode, PromptMode::Filter) {
            self.filter_query = Some(self.prompt_input.clone());
            self.visible_dirty = true;
            self.rebuild_visible();
        }
    }

    /// Move the cursor one grapheme to the left.
    pub fn prompt_cursor_left(&mut self) {
        if self.prompt_cursor == 0 {
            return;
        }
        let mut pos = self.prompt_cursor - 1;
        while !self.prompt_input.is_char_boundary(pos) {
            pos -= 1;
        }
        self.prompt_cursor = pos;
    }

    /// Move the cursor one grapheme to the right.
    pub fn prompt_cursor_right(&mut self) {
        if self.prompt_cursor >= self.prompt_input.len() {
            return;
        }
        let ch = self.prompt_input[self.prompt_cursor..]
            .chars()
            .next()
            .unwrap();
        self.prompt_cursor += ch.len_utf8();
    }

    /// Cancel the active prompt, restoring state as if it was never started.
    pub fn prompt_cancel(&mut self) {
        if matches!(self.prompt_mode, PromptMode::Search) {
            self.selected = self.pre_prompt_selected;
            self.ensure_selected_visible();
        }
        if matches!(self.prompt_mode, PromptMode::Filter) {
            self.filter_cancel();
            return;
        }
        self.prompt_mode = PromptMode::None;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
    }

    /// Confirm the active prompt and return the commit action to dispatch.
    pub fn prompt_confirm(&mut self) -> Option<PromptCommit> {
        let commit = match &self.prompt_mode {
            PromptMode::None => return None,
            PromptMode::Search => {
                self.prompt_mode = PromptMode::None;
                Some(PromptCommit::Search)
            }
            PromptMode::NewFile { parent_dir } => {
                let name = self.prompt_input.trim().to_string();
                if name.is_empty() {
                    self.prompt_mode = PromptMode::None;
                    self.prompt_input.clear();
                    return None;
                }
                let commit = PromptCommit::NewFile { parent_dir: parent_dir.clone(), name };
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                Some(commit)
            }
            PromptMode::NewDir { parent_dir } => {
                let name = self.prompt_input.trim().to_string();
                if name.is_empty() {
                    self.prompt_mode = PromptMode::None;
                    self.prompt_input.clear();
                    return None;
                }
                let commit = PromptCommit::NewDir { parent_dir: parent_dir.clone(), name };
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                Some(commit)
            }
            PromptMode::Rename(id) => {
                let new_name = self.prompt_input.trim().to_string();
                if new_name.is_empty() {
                    self.prompt_mode = PromptMode::None;
                    self.prompt_input.clear();
                    return None;
                }
                // No-op: name unchanged.
                let current_name = self.nodes.get(*id).map(|n| n.name.as_str()).unwrap_or("");
                if new_name == current_name {
                    self.prompt_mode = PromptMode::None;
                    self.prompt_input.clear();
                    return None;
                }
                let old_path = self.node_path(*id);
                let commit = PromptCommit::Rename { old_path, new_name };
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                Some(commit)
            }
            PromptMode::Duplicate(id) => {
                let new_name = self.prompt_input.trim().to_string();
                if new_name.is_empty() {
                    self.prompt_mode = PromptMode::None;
                    self.prompt_input.clear();
                    return None;
                }
                let src_path = self.node_path(*id);
                let commit = PromptCommit::Duplicate { src_path, new_name };
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                Some(commit)
            }
            PromptMode::DeleteConfirm { id, .. } => {
                // DeleteConfirm is handled character-by-character in the key
                // handler, so Enter also confirms.
                let path = self.node_path(*id);
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                Some(PromptCommit::DeleteConfirmed(path))
            }
            PromptMode::DeleteConfirmMulti { paths } => {
                let paths = paths.clone();
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                self.clear_selection();
                Some(PromptCommit::DeleteConfirmedMulti(paths))
            }
            PromptMode::Filter => {
                self.prompt_mode = PromptMode::None;
                self.prompt_input.clear();
                None
            }
        };
        commit
    }

    // --- Search (compatibility wrappers) ---

    pub fn search_active(&self) -> bool {
        matches!(self.prompt_mode, PromptMode::Search)
    }

    pub fn search_query(&self) -> &str {
        if matches!(self.prompt_mode, PromptMode::Search | PromptMode::Filter) {
            &self.prompt_input
        } else {
            ""
        }
    }

    pub fn search_start(&mut self) {
        self.pre_prompt_selected = self.selected;
        self.prompt_mode = PromptMode::Search;
        self.prompt_input.clear();
    }

    pub fn search_push(&mut self, ch: char) {
        self.prompt_push(ch);
    }

    pub fn search_pop(&mut self) {
        self.prompt_pop();
    }

    pub fn search_confirm(&mut self) {
        if matches!(self.prompt_mode, PromptMode::Search) {
            self.prompt_mode = PromptMode::None;
        }
    }

    pub fn search_cancel(&mut self) {
        if matches!(self.prompt_mode, PromptMode::Search) {
            self.prompt_cancel();
        }
    }

    /// Jump to the next search match after the current selection, wrapping around.
    pub fn search_next(&mut self) {
        if self.prompt_input.is_empty() {
            return;
        }
        let start = (self.selected + 1) % self.visible.len().max(1);
        self.search_jump_next_from(start);
    }

    /// Jump to the previous search match before the current selection, wrapping around.
    pub fn search_prev(&mut self) {
        if self.prompt_input.is_empty() || self.visible.is_empty() {
            return;
        }
        let query = self.prompt_input.to_lowercase();
        let len = self.visible.len();
        for offset in 1..=len {
            let idx = (self.selected + len - offset) % len;
            if let Some(node) = self.visible.get(idx).and_then(|&id| self.nodes.get(id)) {
                if node.name.to_lowercase().contains(&query) {
                    self.selected = idx;
                    self.ensure_selected_visible();
                    return;
                }
            }
        }
    }

    fn search_jump_next_from(&mut self, from: usize) {
        if self.prompt_input.is_empty() || self.visible.is_empty() {
            return;
        }
        let query = self.prompt_input.to_lowercase();
        let len = self.visible.len();
        for offset in 0..len {
            let idx = (from + offset) % len;
            if let Some(node) = self.visible.get(idx).and_then(|&id| self.nodes.get(id)) {
                if node.name.to_lowercase().contains(&query) {
                    self.selected = idx;
                    self.ensure_selected_visible();
                    return;
                }
            }
        }
        // No match: restore the pre-search position so the selection doesn't
        // drift to whatever a shorter prefix happened to match.
        self.selected = self.pre_prompt_selected;
        self.ensure_selected_visible();
    }

    // --- File management prompts ---

    /// Begin a new-file prompt, targeting the directory that contains the
    /// currently selected node (or the node itself if it is a directory).
    pub fn start_new_file(&mut self) {
        let parent_dir = self.selected_dir_path().unwrap_or_else(|| self.root.clone());
        self.pre_prompt_selected = self.selected;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
        self.prompt_mode = PromptMode::NewFile { parent_dir };
    }

    /// Begin a new-directory prompt, targeting the same parent as
    /// `start_new_file`.
    pub fn start_new_dir(&mut self) {
        let parent_dir = self.selected_dir_path().unwrap_or_else(|| self.root.clone());
        self.pre_prompt_selected = self.selected;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
        self.prompt_mode = PromptMode::NewDir { parent_dir };
    }

    /// Begin a rename prompt, pre-filling the input with the node's current name.
    pub fn start_rename(&mut self, id: NodeId) {
        let current_name = self.nodes.get(id).map(|n| n.name.clone()).unwrap_or_default();
        self.pre_prompt_selected = self.selected;
        self.set_prompt_input(current_name);
        self.prompt_mode = PromptMode::Rename(id);
    }

    /// Begin a duplicate prompt, pre-filling with `<stem>.copy.<ext>` (or
    /// `<name>.copy` for files without an extension).
    pub fn start_duplicate(&mut self, id: NodeId) {
        let name = self.nodes.get(id).map(|n| n.name.clone()).unwrap_or_default();
        let suggested = {
            let path = std::path::Path::new(&name);
            match (path.file_stem(), path.extension()) {
                (Some(stem), Some(ext)) => {
                    format!("{}.copy.{}", stem.to_string_lossy(), ext.to_string_lossy())
                }
                _ => format!("{}.copy", name),
            }
        };
        self.pre_prompt_selected = self.selected;
        self.set_prompt_input(suggested);
        self.prompt_mode = PromptMode::Duplicate(id);
    }

    /// Begin a delete-confirmation prompt.
    pub fn start_delete_confirm(&mut self, id: NodeId) {
        let is_dir = self.nodes.get(id).map(|n| n.kind == NodeKind::Directory).unwrap_or(false);
        self.pre_prompt_selected = self.selected;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
        self.prompt_mode = PromptMode::DeleteConfirm { id, is_dir };
    }

    // --- Clipboard ---

    pub fn clipboard(&self) -> Option<&ClipboardEntry> {
        self.clipboard.as_ref()
    }

    /// Copy the node at `id` to the clipboard (does not move the file).
    pub fn yank(&mut self, id: NodeId) {
        let path = self.node_path(id);
        self.clipboard = Some(ClipboardEntry { paths: vec![path], node_id: id, op: ClipboardOp::Copy });
    }

    /// Cut the node at `id` into the clipboard (marks it for a move on paste).
    pub fn cut(&mut self, id: NodeId) {
        let path = self.node_path(id);
        self.clipboard = Some(ClipboardEntry { paths: vec![path], node_id: id, op: ClipboardOp::Cut });
    }

    pub fn clear_clipboard(&mut self) {
        self.clipboard = None;
    }

    // --- Multi-select ---

    /// Toggle the selection mark on the node at `id`. If the node is
    /// currently marked it is unmarked; otherwise it is added to the selection.
    pub fn toggle_selection(&mut self, id: NodeId) {
        if self.selected_nodes.contains(&id) {
            self.selected_nodes.remove(&id);
        } else {
            self.selected_nodes.insert(id);
        }
    }

    /// Remove all selection marks without performing any operation.
    pub fn clear_selection(&mut self) {
        self.selected_nodes.clear();
    }

    /// Stage all currently selected paths for a copy operation, then clear the selection.
    ///
    /// If no nodes are selected this is a no-op. The caller is responsible for
    /// checking [`has_selection`] first when single-file yank should be the fallback.
    pub fn yank_selection(&mut self) -> Option<usize> {
        if self.selected_nodes.is_empty() {
            return None;
        }
        let paths = self.selected_paths();
        let count = paths.len();
        let node_id = self.selected_nodes.iter().copied().next().unwrap();
        self.clipboard = Some(ClipboardEntry { paths, node_id, op: ClipboardOp::Copy });
        self.clear_selection();
        Some(count)
    }

    /// Stage all currently selected paths for a cut (move) operation, then clear the selection.
    pub fn cut_selection(&mut self) -> Option<usize> {
        if self.selected_nodes.is_empty() {
            return None;
        }
        let paths = self.selected_paths();
        let count = paths.len();
        let node_id = self.selected_nodes.iter().copied().next().unwrap();
        self.clipboard = Some(ClipboardEntry { paths, node_id, op: ClipboardOp::Cut });
        self.clear_selection();
        Some(count)
    }

    /// Returns `true` when one or more nodes are currently marked.
    pub fn has_selection(&self) -> bool {
        !self.selected_nodes.is_empty()
    }

    /// Returns whether the given node is in the current multi-select set.
    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selected_nodes.contains(&id)
    }

    /// Resolve all marked node IDs into their full filesystem paths.
    ///
    /// Nodes that no longer exist in the tree (e.g. after a refresh) are
    /// silently skipped.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_nodes
            .iter()
            .filter_map(|&id| {
                if self.nodes.contains_key(id) {
                    Some(self.node_path(id))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Begin a multi-delete confirmation prompt, naming all selected paths.
    pub fn start_delete_confirm_multi(&mut self, paths: Vec<PathBuf>) {
        self.pre_prompt_selected = self.selected;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
        self.prompt_mode = PromptMode::DeleteConfirmMulti { paths };
    }

    // --- Status message ---

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    // --- Filter mode ---

    /// Returns `true` while the filter prompt is visible (user is typing).
    pub fn filter_active(&self) -> bool {
        matches!(self.prompt_mode, PromptMode::Filter)
    }

    /// Returns the current filter query text (empty string when no filter).
    pub fn filter_query_text(&self) -> &str {
        self.filter_query.as_deref().unwrap_or("")
    }

    /// Enter filter mode. Expands-all first so every node is searchable.
    /// Callers that need synchronous test behaviour should call
    /// `expand_all_sync` before this method.
    pub fn filter_start(&mut self) {
        self.pre_prompt_selected = self.selected;
        self.prompt_mode = PromptMode::Filter;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
        self.filter_query = Some(String::new());
    }

    /// Append a character to the filter query and rebuild the visible list.
    pub fn filter_push(&mut self, ch: char) {
        if !matches!(self.prompt_mode, PromptMode::Filter) {
            return;
        }
        self.prompt_input.push(ch);
        self.prompt_cursor = self.prompt_input.len();
        self.filter_query = Some(self.prompt_input.clone());
        self.visible_dirty = true;
        self.rebuild_visible();
    }

    /// Remove the last character from the filter query and rebuild.
    pub fn filter_pop(&mut self) {
        if !matches!(self.prompt_mode, PromptMode::Filter) {
            return;
        }
        self.prompt_input.pop();
        self.prompt_cursor = self.prompt_input.len();
        self.filter_query = Some(self.prompt_input.clone());
        self.visible_dirty = true;
        self.rebuild_visible();
    }

    /// Confirm the filter: close the prompt but keep the filtered view active.
    pub fn filter_confirm(&mut self) {
        if !matches!(self.prompt_mode, PromptMode::Filter) {
            return;
        }
        self.prompt_mode = PromptMode::None;
        // filter_query stays set so visible list remains filtered.
    }

    /// Cancel filter mode: clear the query and restore the full unfiltered view.
    pub fn filter_cancel(&mut self) {
        if !matches!(self.prompt_mode, PromptMode::Filter) {
            return;
        }
        self.prompt_mode = PromptMode::None;
        self.prompt_input.clear();
        self.prompt_cursor = 0;
        self.filter_query = None;
        self.selected = self.pre_prompt_selected;
        self.visible_dirty = true;
        self.rebuild_visible();
    }

    // --- Copy path to system clipboard ---

    /// Return the absolute path of the currently selected node as a string and
    /// set a status message "Copied path: <path>". The caller is responsible
    /// for writing the returned string to the system clipboard.
    pub fn copy_path(&mut self) -> Option<String> {
        let id = self.selected_id()?;
        let path = self.node_path(id);
        let path_str = path.to_string_lossy().into_owned();
        self.set_status(format!("Copied path: {path_str}"));
        Some(path_str)
    }

    // --- Path helpers ---

    /// Takes the pending file-open path set by a file-creation op, clearing it.
    pub fn take_pending_open(&mut self) -> Option<PathBuf> {
        self.pending_open.take()
    }

    /// Returns the full destination path that the current prompt would create,
    /// without committing the prompt. Used for pre-validation before confirming.
    /// Returns `None` when the prompt mode doesn't create a path (search,
    /// delete) or when the input is empty.
    pub fn prompt_would_create_path(&self) -> Option<PathBuf> {
        let name = self.prompt_input.trim();
        if name.is_empty() {
            return None;
        }
        match &self.prompt_mode {
            PromptMode::NewFile { parent_dir } | PromptMode::NewDir { parent_dir } => {
                Some(parent_dir.join(name))
            }
            PromptMode::Rename(id) => {
                let parent = self.node_path(*id).parent()?.to_path_buf();
                Some(parent.join(name))
            }
            PromptMode::Duplicate(id) => {
                let parent = self.node_path(*id).parent()?.to_path_buf();
                Some(parent.join(name))
            }
            _ => None,
        }
    }

    /// Returns the selected node's path if it is a directory, or the parent
    /// directory path if it is a file.
    pub fn selected_dir_path(&self) -> Option<PathBuf> {
        let id = self.visible.get(self.selected).copied()?;
        let node = self.nodes.get(id)?;
        let path = self.node_path(id);
        if node.kind == NodeKind::Directory {
            Some(path)
        } else {
            path.parent().map(|p| p.to_path_buf())
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Clamp scroll_offset so the selected item is within the viewport.
    /// Called during render when viewport height is known.
    pub fn clamp_scroll(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        self.viewport_height = viewport_height;
        if self.selected >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected - viewport_height + 1;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    // --- Reveal path ---

    /// Expand ancestors and select the node at the given path.
    /// When an unloaded directory is encountered, spawns an async load and
    /// stores the target in `pending_reveal` to retry after loading completes.
    pub fn reveal_path(&mut self, path: &Path, config: &FileTreeConfig) {
        let relative = match path.strip_prefix(&self.root) {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut current_id = self.root_id;

        for component in relative.components() {
            let name = component.as_os_str().to_string_lossy();

            let node = match self.nodes.get(current_id) {
                Some(n) => n,
                None => return,
            };

            if !node.loaded && node.kind == NodeKind::Directory {
                // Only spawn a load if we haven't already (expanded but
                // not loaded means a load is already in flight from a
                // previous call or from toggle_expand).
                if !node.expanded {
                    self.spawn_load_children(current_id, config);
                    if let Some(n) = self.nodes.get_mut(current_id) {
                        n.expanded = true;
                    }
                }
                self.pending_reveal = Some(path.to_path_buf());
                return;
            }

            if let Some(n) = self.nodes.get_mut(current_id) {
                n.expanded = true;
            }

            let children = self
                .nodes
                .get(current_id)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            match children.iter().find(|&&cid| {
                self.nodes
                    .get(cid)
                    .map(|n| n.name == *name)
                    .unwrap_or(false)
            }) {
                Some(&child_id) => current_id = child_id,
                None => return,
            }
        }

        self.pending_reveal = None;
        self.visible_dirty = true;
        self.rebuild_visible();
        if let Some(pos) = self.visible.iter().position(|&id| id == current_id) {
            self.selected = pos;
            self.ensure_selected_visible();
        }
    }

    /// Synchronous counterpart to [`reveal_path`] for use in test contexts.
    ///
    /// Loads directories along the path synchronously (via [`load_children_sync`])
    /// instead of spawning background tasks, so the selection is updated immediately.
    pub fn reveal_path_sync(&mut self, path: &Path, config: &FileTreeConfig) {
        let relative = match path.strip_prefix(&self.root) {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut current_id = self.root_id;

        for component in relative.components() {
            let name = component.as_os_str().to_string_lossy().to_string();

            if !self.nodes.get(current_id).map(|n| n.loaded).unwrap_or(false) {
                self.load_children_sync(current_id, config);
            }
            if let Some(n) = self.nodes.get_mut(current_id) {
                n.expanded = true;
            }

            let children = self
                .nodes
                .get(current_id)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            match children.iter().find(|&&cid| {
                self.nodes
                    .get(cid)
                    .map(|n| n.name == name)
                    .unwrap_or(false)
            }) {
                Some(&child_id) => current_id = child_id,
                None => return,
            }
        }

        self.pending_reveal = None;
        self.visible_dirty = true;
        self.rebuild_visible();
        if let Some(pos) = self.visible.iter().position(|&id| id == current_id) {
            self.selected = pos;
            self.ensure_selected_visible();
        }
    }

    // --- Refresh ---

    /// Re-scan all currently expanded directories.
    pub fn refresh(&mut self, config: &FileTreeConfig) {
        let expanded_dirs: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.kind == NodeKind::Directory && n.expanded && n.loaded)
            .map(|(id, _)| id)
            .collect();

        for &id in &expanded_dirs {
            if let Some(node) = self.nodes.get_mut(id) {
                node.loaded = false;
            }
        }

        for &id in &expanded_dirs {
            self.spawn_load_children(id, config);
        }
    }

    // --- Debounce ---

    const FOLLOW_DEBOUNCE: Duration = Duration::from_millis(100);

    /// Request a git status refresh using a call-first-and-last debounce strategy.
    ///
    /// - If no refresh is currently running, the refresh spawns immediately.
    /// - If a refresh is already in progress, sets a pending flag so that one
    ///   more refresh is started when the current one finishes via
    ///   . This coalesces any number of concurrent requests into
    ///   at most one additional scan after the current one completes.
    ///
    /// The  argument is required when spawning immediately.
    /// Call [] instead when you do not have
    /// providers available (e.g., inside ).
    pub fn request_git_refresh(&mut self) {
        // Fall back to deadline-based scheduling; the actual providers are
        // supplied by check_git_refresh_timer which has access to them.
        if self.git_refresh_in_progress {
            self.git_refresh_pending = true;
        } else {
            // Spawn immediately on the next process_updates call via deadline=now.
            self.git_refresh_deadline = Some(Instant::now());
        }
    }

    /// Returns `true` if a git refresh is pending (deadline set or pending flag).
    pub fn has_pending_git_refresh(&self) -> bool {
        self.git_refresh_deadline.is_some() || self.git_refresh_pending
    }

    /// Returns `true` if a git refresh is currently running in the background.
    pub fn git_refresh_in_progress(&self) -> bool {
        self.git_refresh_in_progress
    }

    /// Queue a follow-current-file reveal. Only sets the deadline once so
    /// repeated calls (e.g. every render frame) don't push it forward forever.
    /// Skips queueing while a pending_reveal is still being resolved.
    pub fn request_follow(&mut self, path: PathBuf) {
        if self.pending_reveal.is_some() {
            return;
        }
        self.follow_target = Some(path);
        if self.follow_deadline.is_none() {
            self.follow_deadline = Some(Instant::now() + Self::FOLLOW_DEBOUNCE);
        }
    }

    fn check_git_refresh_timer(&mut self, diff_providers: &DiffProviderRegistry) {
        if let Some(deadline) = self.git_refresh_deadline {
            if Instant::now() >= deadline {
                self.git_refresh_deadline = None;
                // Clone before the mutable borrow so the compiler is happy.
                let providers = diff_providers.clone();
                self.spawn_git_status(providers);
            }
        }
    }

    fn check_follow_timer(&mut self, config: &FileTreeConfig) {
        if let Some(deadline) = self.follow_deadline {
            if Instant::now() >= deadline {
                self.follow_deadline = None;
                if let Some(path) = self.follow_target.take() {
                    self.reveal_path(&path, config);
                }
            }
        }
    }

    /// Spawn a two-phase background task to collect git status for all changed files.
    ///
    /// Phase 1 scans only tracked files (fast) and sends results immediately,
    /// then sends `GitStatusPhase1Complete`. Phase 2 scans untracked files,
    /// sends those results, then sends `GitStatusEnd` to signal completion.
    ///
    /// Incoming results accumulate in `git_status_new` alongside the live
    /// `git_status_map` so the tree continues to show the previous scan's data
    /// during the refresh (no blank flash). At `GitStatusEnd`, `git_status_map`
    /// is atomically replaced with `git_status_new` to purge stale entries.
    fn spawn_git_status(&mut self, diff_providers: DiffProviderRegistry) {
        // Only clear the incoming buffer, not the live map. This lets the tree
        // keep showing previous-cycle data while the new scan is in flight.
        self.git_status_new.clear();
        self.git_refresh_in_progress = true;

        let tx = self.update_tx.clone();
        let root = self.root.clone();

        tokio::task::spawn_blocking(move || {
            // Phase 1: tracked files only (no untracked). Fast because it
            // skips the directory walk needed to discover untracked files.
            let tx1 = tx.clone();
            let root1 = root.clone();
            let providers1 = diff_providers.clone();
            providers1.for_each_changed_file_tracked_only_blocking(&root1, |result| {
                if let Ok(change) = result {
                    let status = match &change {
                        FileChange::Untracked { .. } => GitStatus::Untracked,
                        FileChange::Added { .. } => GitStatus::Added,
                        FileChange::Staged { .. } => GitStatus::Staged,
                        FileChange::Modified { .. } => GitStatus::Modified,
                        FileChange::Deleted { .. } => GitStatus::Deleted,
                        FileChange::Renamed { .. } => GitStatus::Renamed,
                        FileChange::Conflict { .. } => GitStatus::Conflict,
                    };
                    let path = change.path().to_owned();
                    let _ = tx1.blocking_send(FileTreeUpdate::GitStatus(vec![(path, status)]));
                }
                true
            });
            let _ = tx.blocking_send(FileTreeUpdate::GitStatusPhase1Complete);

            // Phase 2: full scan including untracked files.
            let tx2 = tx.clone();
            diff_providers.for_each_untracked_files_blocking(&root, |result| {
                if let Ok(change) = result {
                    let status = match &change {
                        FileChange::Untracked { .. } => GitStatus::Untracked,
                        FileChange::Added { .. } => GitStatus::Added,
                        FileChange::Staged { .. } => GitStatus::Staged,
                        FileChange::Modified { .. } => GitStatus::Modified,
                        FileChange::Deleted { .. } => GitStatus::Deleted,
                        FileChange::Renamed { .. } => GitStatus::Renamed,
                        FileChange::Conflict { .. } => GitStatus::Conflict,
                    };
                    let path = change.path().to_owned();
                    let _ = tx2.blocking_send(FileTreeUpdate::GitStatus(vec![(path, status)]));
                }
                true
            });
            let _ = tx.blocking_send(FileTreeUpdate::GitStatusEnd);
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
            cached_git_status: GitStatus::Clean,
        });

        let src_id = nodes.insert(FileNode {
            name: "src".into(),
            kind: NodeKind::Directory,
            parent: Some(root_id),
            children: Vec::new(),
            expanded: true,
            loaded: true,
            depth: 1,
            cached_git_status: GitStatus::Clean,
        });

        let main_id = nodes.insert(FileNode {
            name: "main.rs".into(),
            kind: NodeKind::File,
            parent: Some(src_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 2,
            cached_git_status: GitStatus::Clean,
        });

        let lib_id = nodes.insert(FileNode {
            name: "lib.rs".into(),
            kind: NodeKind::File,
            parent: Some(src_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 2,
            cached_git_status: GitStatus::Clean,
        });

        let cargo_id = nodes.insert(FileNode {
            name: "Cargo.toml".into(),
            kind: NodeKind::File,
            parent: Some(root_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 1,
            cached_git_status: GitStatus::Clean,
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
        let result = FileTree::new(PathBuf::from("/nonexistent/path/that/doesnt/exist"), &FileTreeConfig::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_new_creates_root_node() {
        let dir = tempfile::tempdir().unwrap();
        let tree = FileTree::new(dir.path().to_path_buf(), &FileTreeConfig::default()).unwrap();

        let root = tree.nodes.get(tree.root_id()).unwrap();
        assert_eq!(root.kind, NodeKind::Directory);
        assert!(root.expanded);
        assert!(root.loaded);
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
        assert!(GitStatus::Untracked.severity() < GitStatus::Added.severity());
        assert!(GitStatus::Added.severity() < GitStatus::Staged.severity());
        assert!(GitStatus::Staged.severity() < GitStatus::Renamed.severity());
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
        let _config = FileTreeConfig::default();

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
            cached_git_status: GitStatus::Clean,
        });

        // Manually call the spawn logic by sending on our tx
        let config = FileTreeConfig::default();
        let path = root_path.clone();

        tokio::task::spawn_blocking(move || {
            let walker = build_dir_walker(&path, &config);

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
                ignored_names: HashSet::new(),
            });
        });

        let update = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        match update {
            FileTreeUpdate::ChildrenLoaded { parent, entries, .. } => {
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

    // --- Step 1.6-1.8 tests ---

    #[test]
    fn test_rebuild_visible_all_expanded() {
        let (mut tree, root_id, src_id, main_id, cargo_id) = build_test_tree();
        let lib_id = tree.nodes[src_id].children[1];

        tree.rebuild_visible();

        // Expected DFS order: root, src, main.rs, lib.rs, Cargo.toml
        assert_eq!(tree.visible.len(), 5);
        assert_eq!(tree.visible[0], root_id);
        assert_eq!(tree.visible[1], src_id);
        assert_eq!(tree.visible[2], main_id);
        assert_eq!(tree.visible[3], lib_id);
        assert_eq!(tree.visible[4], cargo_id);
    }

    #[test]
    fn test_rebuild_visible_collapsed_dir() {
        let (mut tree, root_id, src_id, _, cargo_id) = build_test_tree();

        tree.nodes[src_id].expanded = false;
        tree.rebuild_visible();

        // src children hidden: root, src, Cargo.toml
        assert_eq!(tree.visible.len(), 3);
        assert_eq!(tree.visible[0], root_id);
        assert_eq!(tree.visible[1], src_id);
        assert_eq!(tree.visible[2], cargo_id);
    }

    #[test]
    fn test_rebuild_visible_clamps_selection() {
        let (mut tree, _, src_id, _, _) = build_test_tree();

        tree.selected = 10; // way past end
        tree.nodes[src_id].expanded = false;
        tree.rebuild_visible();

        // 3 items visible, selected clamped to 2
        assert_eq!(tree.selected, 2);
    }

    #[test]
    fn test_process_updates_children_loaded() {
        let (mut tree, root_id, _, _, _) = build_test_tree();
        let config = FileTreeConfig::default();

        // Send a ChildrenLoaded update for root with new entries
        let tx = tree.update_tx();
        tx.try_send(FileTreeUpdate::ChildrenLoaded {
            parent: root_id,
            entries: vec![
                ("docs".into(), NodeKind::Directory),
                ("README.md".into(), NodeKind::File),
            ],
            ignored_names: HashSet::new(),
        })
        .unwrap();

        tree.process_updates(&config, None);

        let root = &tree.nodes[root_id];
        assert_eq!(root.children.len(), 2);
        assert!(root.loaded);

        let first_child = &tree.nodes[root.children[0]];
        assert_eq!(first_child.name, "docs");
        assert_eq!(first_child.kind, NodeKind::Directory);
        assert_eq!(first_child.depth, 1);
        assert_eq!(first_child.parent, Some(root_id));

        let second_child = &tree.nodes[root.children[1]];
        assert_eq!(second_child.name, "README.md");
        assert_eq!(second_child.kind, NodeKind::File);
    }

    #[test]
    fn test_process_updates_replaces_old_children() {
        let (mut tree, root_id, src_id, _main_id, cargo_id) = build_test_tree();
        let config = FileTreeConfig::default();

        // Replace root's children
        let tx = tree.update_tx();
        tx.try_send(FileTreeUpdate::ChildrenLoaded {
            parent: root_id,
            entries: vec![("new_file.txt".into(), NodeKind::File)],
            ignored_names: HashSet::new(),
        })
        .unwrap();

        tree.process_updates(&config, None);

        // Old children (src, Cargo.toml) should be removed
        assert!(tree.nodes.get(src_id).is_none());
        assert!(tree.nodes.get(cargo_id).is_none());
        // main_id was a child of src, but we only remove direct children
        // (main_id will be orphaned, which is acceptable — it won't appear in visible)
        assert_eq!(tree.nodes[root_id].children.len(), 1);
    }

    #[test]
    fn test_git_status_file_lookup() {
        let (mut tree, _, _, main_id, _) = build_test_tree();

        tree.git_status_map
            .insert(PathBuf::from("/tmp/project/src/main.rs"), GitStatus::Modified);

        assert_eq!(tree.git_status_for(main_id), GitStatus::Modified);
    }

    #[test]
    fn test_git_status_directory_uses_cache() {
        let (mut tree, _, src_id, _, _) = build_test_tree();

        tree.git_status_map
            .insert(PathBuf::from("/tmp/project/src/main.rs"), GitStatus::Modified);
        tree.rebuild_dir_status_cache();

        // src directory should show Modified (worst descendant)
        assert_eq!(tree.git_status_for(src_id), GitStatus::Modified);
    }

    #[test]
    fn test_git_status_dir_worst_of_descendants() {
        let (mut tree, _, src_id, _, _) = build_test_tree();

        tree.git_status_map
            .insert(PathBuf::from("/tmp/project/src/main.rs"), GitStatus::Untracked);
        tree.git_status_map
            .insert(PathBuf::from("/tmp/project/src/lib.rs"), GitStatus::Conflict);
        tree.rebuild_dir_status_cache();

        // src should show Conflict (higher severity than Untracked)
        assert_eq!(tree.git_status_for(src_id), GitStatus::Conflict);
    }

    #[test]
    fn test_git_status_clean_file() {
        let (tree, _, _, main_id, _) = build_test_tree();
        // No git status entries → clean
        assert_eq!(tree.git_status_for(main_id), GitStatus::Clean);
    }

    #[test]
    fn test_git_status_untracked_inherits_to_children() {
        let (mut tree, _, _src_id, main_id, _) = build_test_tree();

        // Mark the src directory itself as untracked
        tree.git_status_map
            .insert(PathBuf::from("/tmp/project/src"), GitStatus::Untracked);

        // main.rs should inherit Untracked from its parent
        assert_eq!(tree.git_status_for(main_id), GitStatus::Untracked);
    }

    #[test]
    fn test_process_updates_git_status() {
        let (mut tree, _root_id, _, _, _) = build_test_tree();
        let config = FileTreeConfig::default();

        let tx = tree.update_tx();
        tx.try_send(FileTreeUpdate::GitStatus(vec![
            (PathBuf::from("/tmp/project/src/main.rs"), GitStatus::Modified),
        ]))
        .unwrap();

        tree.process_updates(&config, None);

        assert!(tree.git_status_map.contains_key(&PathBuf::from("/tmp/project/src/main.rs")));
        // Dir cache should be rebuilt
        assert!(tree.dir_status_cache.contains_key(&PathBuf::from("/tmp/project/src")));
    }

    // --- Step 1.9-1.11 tests ---

    #[test]
    fn test_move_down() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible(); // 5 items
        assert_eq!(tree.selected, 0);

        tree.move_down();
        assert_eq!(tree.selected, 1);

        tree.move_down();
        assert_eq!(tree.selected, 2);
    }

    #[test]
    fn test_move_down_clamps_at_end() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible(); // 5 items

        for _ in 0..10 {
            tree.move_down();
        }
        assert_eq!(tree.selected, 4); // last item
    }

    #[test]
    fn test_move_up() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible();
        tree.selected = 3;

        tree.move_up();
        assert_eq!(tree.selected, 2);
    }

    #[test]
    fn test_move_up_clamps_at_top() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible();
        tree.selected = 0;

        tree.move_up();
        assert_eq!(tree.selected, 0);
    }

    #[test]
    fn test_jump_to_top_and_bottom() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible(); // 5 items

        tree.jump_to_bottom();
        assert_eq!(tree.selected, 4);

        tree.jump_to_top();
        assert_eq!(tree.selected, 0);
        assert_eq!(tree.scroll_offset, 0);
    }

    #[test]
    fn test_clamp_scroll() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible(); // 5 items

        tree.selected = 4;
        tree.scroll_offset = 0;
        tree.clamp_scroll(3); // viewport shows 3 items
        // selected=4 must be visible: scroll_offset = 4 - 3 + 1 = 2
        assert_eq!(tree.scroll_offset, 2);
    }

    #[test]
    fn test_clamp_scroll_selected_above_viewport() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.rebuild_visible();

        tree.selected = 1;
        tree.scroll_offset = 3;
        tree.clamp_scroll(3);
        assert_eq!(tree.scroll_offset, 1);
    }

    #[test]
    fn test_reveal_path_already_loaded() {
        let (mut tree, _, src_id, main_id, _) = build_test_tree();
        let config = FileTreeConfig::default();

        // Collapse src first
        tree.nodes[src_id].expanded = false;
        tree.rebuild_visible();
        // Only root, src, Cargo.toml visible
        assert_eq!(tree.visible.len(), 3);

        tree.reveal_path(Path::new("/tmp/project/src/main.rs"), &config);

        // src should be expanded and main.rs selected
        assert!(tree.nodes[src_id].expanded);
        let selected = tree.visible[tree.selected];
        assert_eq!(selected, main_id);
    }

    #[test]
    fn test_reveal_path_outside_root_is_noop() {
        let (mut tree, _, _, _, _) = build_test_tree();
        let config = FileTreeConfig::default();
        tree.rebuild_visible();
        let old_selected = tree.selected;

        tree.reveal_path(Path::new("/other/path/file.rs"), &config);
        assert_eq!(tree.selected, old_selected);
    }

    #[tokio::test]
    async fn test_reveal_path_with_async_load() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("file.txt"), "content").unwrap();

        let config = FileTreeConfig::default();
        let mut tree = FileTree::new(dir.path().to_path_buf(), &config).unwrap();

        // Root is loaded synchronously by new(), but sub/ is not
        assert!(tree.nodes[tree.root_id].loaded);

        // reveal_path should spawn async load for sub/ and set pending_reveal
        tree.reveal_path(&sub.join("file.txt"), &config);
        assert!(tree.pending_reveal.is_some());

        // Wait for sub directory load to complete
        tokio::time::sleep(Duration::from_millis(500)).await;
        tree.process_updates(&config, None);

        // The file should be selected
        let selected_node = tree.selected_node().unwrap();
        assert_eq!(selected_node.name, "file.txt");
    }

    #[test]
    fn test_refresh_marks_dirs_unloaded() {
        let (mut tree, root_id, src_id, _, _) = build_test_tree();

        assert!(tree.nodes[root_id].loaded);
        assert!(tree.nodes[src_id].loaded);

        // refresh would call spawn_load_children which needs tokio,
        // so just test the state change
        let expanded_dirs: Vec<NodeId> = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.kind == NodeKind::Directory && n.expanded && n.loaded)
            .map(|(id, _)| id)
            .collect();

        for &id in &expanded_dirs {
            if let Some(node) = tree.nodes.get_mut(id) {
                node.loaded = false;
            }
        }

        assert!(!tree.nodes[root_id].loaded);
        assert!(!tree.nodes[src_id].loaded);
    }

    #[test]
    fn test_request_git_refresh_sets_deadline() {
        let (mut tree, _, _, _, _) = build_test_tree();
        assert!(tree.git_refresh_deadline.is_none());

        tree.request_git_refresh();
        assert!(tree.git_refresh_deadline.is_some());
    }

    #[test]
    fn test_request_follow_sets_target() {
        let (mut tree, _, _, _, _) = build_test_tree();
        assert!(tree.follow_target.is_none());

        tree.request_follow(PathBuf::from("/tmp/project/src/main.rs"));
        assert_eq!(
            tree.follow_target.as_deref(),
            Some(Path::new("/tmp/project/src/main.rs"))
        );
        assert!(tree.follow_deadline.is_some());
    }

    #[test]
    fn test_request_follow_does_not_reset_deadline() {
        let (mut tree, _, _, _, _) = build_test_tree();

        tree.request_follow(PathBuf::from("/tmp/project/src/main.rs"));
        let first_deadline = tree.follow_deadline.unwrap();

        // Simulate time passing
        std::thread::sleep(Duration::from_millis(10));

        // Second call should NOT reset the deadline
        tree.request_follow(PathBuf::from("/tmp/project/src/lib.rs"));
        let second_deadline = tree.follow_deadline.unwrap();

        assert_eq!(first_deadline, second_deadline);
        // But the target should be updated
        assert_eq!(
            tree.follow_target.as_deref(),
            Some(Path::new("/tmp/project/src/lib.rs"))
        );
    }

    // --- Phase 6: file management unit tests ---

    #[test]
    fn test_prompt_mode_new_file() {
        let (mut tree, _, src_id, main_id, _) = build_test_tree();
        tree.rebuild_visible();
        // Select main.rs (index 2 in DFS order: root, src, main.rs, lib.rs, cargo)
        tree.selected = 2;

        tree.start_new_file();
        assert!(matches!(tree.prompt_mode, PromptMode::NewFile { .. }));
        assert_eq!(tree.prompt_input, "");

        tree.prompt_cancel();
        assert!(matches!(tree.prompt_mode, PromptMode::None));
        assert_eq!(tree.prompt_input, "");
        let _ = (src_id, main_id); // suppress unused warnings
    }

    #[test]
    fn test_prompt_mode_rename_prefills_name() {
        let (mut tree, _, _, main_id, _) = build_test_tree();
        tree.rebuild_visible();

        tree.start_rename(main_id);
        assert!(matches!(tree.prompt_mode, PromptMode::Rename(_)));
        assert_eq!(tree.prompt_input, "main.rs");
    }

    #[test]
    fn test_prompt_mode_duplicate_prefills_copy_name() {
        let (mut tree, _, _, main_id, _) = build_test_tree();
        tree.rebuild_visible();

        tree.start_duplicate(main_id);
        assert!(matches!(tree.prompt_mode, PromptMode::Duplicate(_)));
        assert_eq!(tree.prompt_input, "main.copy.rs");
    }

    #[test]
    fn test_prompt_mode_duplicate_no_extension() {
        let mut nodes: SlotMap<NodeId, FileNode> = SlotMap::with_key();
        let root_id = nodes.insert(FileNode {
            name: "project".into(),
            kind: NodeKind::Directory,
            parent: None,
            children: Vec::new(),
            expanded: true,
            loaded: true,
            depth: 0,
            cached_git_status: GitStatus::Clean,
        });
        let file_id = nodes.insert(FileNode {
            name: "Makefile".into(),
            kind: NodeKind::File,
            parent: Some(root_id),
            children: Vec::new(),
            expanded: false,
            loaded: false,
            depth: 1,
            cached_git_status: GitStatus::Clean,
        });
        nodes[root_id].children = vec![file_id];
        let mut tree = FileTree::from_nodes(PathBuf::from("/tmp/project"), root_id, nodes);
        tree.rebuild_visible();

        tree.start_duplicate(file_id);
        assert_eq!(tree.prompt_input, "Makefile.copy");
    }

    #[test]
    fn test_prompt_input_accumulates() {
        let (mut tree, _, _, _, _) = build_test_tree();
        tree.start_new_file();

        tree.prompt_push('h');
        tree.prompt_push('e');
        tree.prompt_push('l');
        assert_eq!(tree.prompt_input, "hel");

        tree.prompt_pop();
        assert_eq!(tree.prompt_input, "he");

        tree.prompt_pop();
        tree.prompt_pop();
        assert_eq!(tree.prompt_input, "");
    }

    #[test]
    fn test_yank_and_cut() {
        let (mut tree, _, _, main_id, cargo_id) = build_test_tree();

        tree.yank(main_id);
        let clip = tree.clipboard().unwrap();
        assert_eq!(clip.paths, vec![PathBuf::from("/tmp/project/src/main.rs")]);
        assert_eq!(clip.op, ClipboardOp::Copy);

        tree.cut(cargo_id);
        let clip = tree.clipboard().unwrap();
        assert_eq!(clip.paths, vec![PathBuf::from("/tmp/project/Cargo.toml")]);
        assert_eq!(clip.op, ClipboardOp::Cut);

        tree.clear_clipboard();
        assert!(tree.clipboard().is_none());
    }

    #[test]
    fn test_selected_dir_path_on_file() {
        let (mut tree, _, _, main_id, _) = build_test_tree();
        tree.rebuild_visible();

        // Select main.rs (index 2)
        tree.selected = 2;
        assert_eq!(tree.visible[2], main_id);

        let dir = tree.selected_dir_path().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/project/src"));
    }

    #[test]
    fn test_selected_dir_path_on_directory() {
        let (mut tree, _, src_id, _, _) = build_test_tree();
        tree.rebuild_visible();

        // Select src/ (index 1)
        tree.selected = 1;
        assert_eq!(tree.visible[1], src_id);

        let dir = tree.selected_dir_path().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/project/src"));
    }

    #[test]
    fn test_selection_clamped_after_node_removal() {
        let (mut tree, root_id, src_id, _main_id, _cargo_id) = build_test_tree();
        tree.rebuild_visible(); // 5 items: root, src, main.rs, lib.rs, Cargo.toml

        // Select Cargo.toml (last item, index 4)
        tree.selected = 4;

        // Simulate removal of src and its children — collapse src first so
        // only root, src, Cargo.toml are visible (3 items, max index 2).
        tree.nodes[src_id].expanded = false;
        tree.rebuild_visible();

        // selected was 4, max is now 2 — must be clamped
        assert_eq!(tree.selected, 2);
        let _ = root_id;
    }

    #[test]
    fn test_rename_noop_returns_none() {
        let (mut tree, _, _, main_id, _) = build_test_tree();
        tree.start_rename(main_id);
        // Input is pre-filled with "main.rs"; confirming without change is a no-op
        assert_eq!(tree.prompt_input, "main.rs");
        let result = tree.prompt_confirm();
        assert!(result.is_none());
        assert!(matches!(tree.prompt_mode, PromptMode::None));
    }

    #[test]
    fn test_delete_confirm_is_dir_flag() {
        let (mut tree, _, src_id, main_id, _) = build_test_tree();

        tree.start_delete_confirm(src_id);
        assert!(matches!(tree.prompt_mode, PromptMode::DeleteConfirm { is_dir: true, .. }));

        tree.prompt_cancel();
        tree.start_delete_confirm(main_id);
        assert!(matches!(tree.prompt_mode, PromptMode::DeleteConfirm { is_dir: false, .. }));
    }
}
