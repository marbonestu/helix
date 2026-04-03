# File Tree Sidebar Implementation Plan

A persistent, docked file tree panel on the left side of the editor, similar to
nvim-tree / neo-tree. Shows the project hierarchy with expand/collapse, git
status indicators, and file operations.

## Key Architectural Decision

Integrate **inside `EditorView::render()`** rather than as a separate compositor
layer — same pattern used by the statusline. The sidebar carves out space from
the left of the terminal area before `cx.editor.resize()` is called, so the
editor split tree automatically gets the reduced space. This avoids
cross-component coordination issues and z-order complexity.

State lives on `Editor` (in `helix-view`), rendering/event logic lives in
`helix-term`. The file tree is NOT a `Component` — it renders inline within
`EditorView`.

## Design Principles

Derived from analyzing neo-tree.nvim's architecture, reviewing for Rust best
practices, and auditing the Helix codebase for reusable patterns:

1. **Async-first I/O**: Filesystem scanning runs on background threads via
   `tokio::task::spawn_blocking()`. Git status uses
   `DiffProviderRegistry::for_each_changed_file` which already spawns
   internally. Results arrive via `tokio::sync::mpsc` channels; the main thread
   only calls `try_recv()` during render.
2. **Lazy loading**: Directories are scanned only on first expand (`loaded`
   flag), not upfront. This keeps startup fast regardless of workspace size.
3. **Slotmap arena**: Use `slotmap::SlotMap<NodeId, FileNode>` (already a
   workspace dependency, used by `helix-view/src/tree.rs` for the view split
   tree). Generational handles safely invalidate on deletion.
4. **Derive paths, don't store them**: Each node stores only its `name`. Full
   paths are reconstructed by walking parent pointers on demand. This eliminates
   redundant `PathBuf` allocations.
5. **Git status as indexed lookup**: Store git status in a flat
   `HashMap<PathBuf, GitStatus>` plus a pre-built directory index
   (`HashMap<PathBuf, GitStatus>`) that caches the worst status per directory.
   Avoids O(N) scans during render.
6. **Callback-based mutations**: Operations that need both `&mut FileTree` and
   `&mut Editor` (e.g., opening a file) use `cx.callback.push(Box::new(...))`
   — the same pattern used by the file picker and file explorer
   (`helix-term/src/ui/mod.rs:274-353`).
7. **Dirty-flag for visible list**: Only rebuild the flat `visible` list when
   `visible_dirty` is set, not on every render.
8. **Reuse existing infrastructure**: Use `ignore::WalkBuilder` with the same
   configuration helpers as the file picker (`.add_custom_ignore_filename`,
   `.types(get_excluded_types())`). Use `helix_vcs::FileChange` directly. Follow
   `FilePickerConfig`'s serde pattern for config.
9. **Debounced operations**: Coalesce rapid events to avoid redundant work.
   File-follow: 100ms. Git status refresh: 1000ms. File watcher re-scan:
   5000ms. These timings are derived from neo-tree.nvim's production-tuned
   values. The bottleneck is I/O (subprocess calls, disk reads), not CPU — so
   debouncing is equally important in Rust.

## Dependencies

All required crates are already workspace dependencies — no new additions:

- `slotmap` — arena allocation (used by `helix-view/src/tree.rs`)
- `ignore` — gitignore-aware directory walking (used by file picker)
- `tokio` — `spawn_blocking` for async I/O
- `helix-vcs` — `DiffProviderRegistry`, `FileChange`
- `serde` — config serialization

---

## Phase 1: Data Model and State (helix-view)

### Step 1.1: File tree data structure

**File:** `helix-view/src/file_tree.rs` (new)

```rust
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use slotmap::SlotMap;
use tokio::sync::mpsc::{Sender, Receiver};

slotmap::new_key_type! {
    pub struct NodeId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Untracked,
    Modified,
    Conflict,
    Deleted,
    Renamed,
    Clean,
}

impl GitStatus {
    fn severity(self) -> u8 {
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
    File,
    Directory,
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
    nodes: SlotMap<NodeId, FileNode>,
    visible: Vec<NodeId>,
    visible_dirty: bool,
    selected: usize,
    scroll_offset: usize,

    /// Sender half — cloned into background tasks. Stored here so that
    /// any method on FileTree can spawn work without external plumbing.
    update_tx: Sender<FileTreeUpdate>,
    /// Receiver half — drained by `process_updates()` each render cycle.
    update_rx: Receiver<FileTreeUpdate>,

    /// Flat git status: path → status for changed files.
    git_status_map: HashMap<PathBuf, GitStatus>,
    /// Pre-computed worst status per directory path. Rebuilt when
    /// `git_status_map` changes, avoiding O(N) scans during render.
    dir_status_cache: HashMap<PathBuf, GitStatus>,
    /// Hash of the last raw git status output. If unchanged on refresh,
    /// skip re-parsing entirely. The subprocess call is the bottleneck —
    /// this avoids redundant HashMap rebuilds when nothing changed.
    last_git_status_hash: Option<u64>,
    /// Debounce timer for git status refresh (1000ms). Prevents
    /// re-running git status more than once per second.
    git_refresh_deadline: Option<std::time::Instant>,
    /// Debounce state for follow-current-file (100ms).
    follow_target: Option<PathBuf>,
    follow_deadline: Option<std::time::Instant>,
}
```

Note: `git_status` is NOT stored on `FileNode`. It lives in the flat maps on
`FileTree` and is looked up during render via `git_status_for(id)`.

### Step 1.2: Constructor

```rust
impl FileTree {
    pub fn new(root: PathBuf, config: &FileTreeConfig) -> Result<Self, String> {
        if !root.exists() {
            return Err(format!("Root path does not exist: {}", root.display()));
        }

        let (update_tx, update_rx) = tokio::sync::mpsc::channel(256);

        let mut nodes = SlotMap::with_key();
        let root_name = root.file_name()
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

        let mut tree = Self {
            root: root.clone(),
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
        };

        // Kick off initial load of root children
        tree.spawn_load_children(root_id, config);

        Ok(tree)
    }
}
```

### Step 1.3: Path reconstruction

```rust
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
```

### Step 1.4: Toggle expand

```rust
pub fn toggle_expand(&mut self, id: NodeId, config: &FileTreeConfig) {
    let Some(node) = self.nodes.get_mut(id) else { return };
    if node.kind != NodeKind::Directory { return; }

    if node.expanded {
        node.expanded = false;
        self.visible_dirty = true;
    } else {
        node.expanded = true;
        self.visible_dirty = true;

        if !node.loaded {
            // Children will arrive via channel; meanwhile the node
            // renders as expanded-but-empty (no loading spinner needed —
            // children typically arrive within one frame).
            self.spawn_load_children(id, config);
        }
    }
}
```

### Step 1.5: Async filesystem scanning

Directory children are loaded lazily on first expand. Uses `ignore::WalkBuilder`
with the same configuration used by the file picker
(`helix-term/src/ui/mod.rs:229-244`), including `.add_custom_ignore_filename`
for `.helix/ignore`:

```rust
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
                    if entry.path() == path { continue; }
                    let name = entry.file_name().to_string_lossy().to_string();
                    let kind = if entry.file_type()
                        .map(|ft| ft.is_dir()).unwrap_or(false)
                    {
                        NodeKind::Directory
                    } else {
                        NodeKind::File
                    };
                    entries.push((name, kind));
                }
                Err(err) => {
                    let _ = tx.blocking_send(FileTreeUpdate::ScanError {
                        path: err.path().unwrap_or(&path).to_owned(),
                        reason: err.to_string(),
                    });
                }
            }
        }

        // Sort: directories first, then alphabetical (WalkBuilder sorts
        // by name but not by kind)
        entries.sort_by(|(a_name, a_kind), (b_name, b_kind)| {
            a_kind.cmp(b_kind).reverse().then(a_name.cmp(b_name))
        });

        // If receiver is dropped (FileTree was closed), silently exit
        let _ = tx.blocking_send(FileTreeUpdate::ChildrenLoaded {
            parent: node_id,
            entries,
        });
    });
}
```

### Step 1.6: Process updates

Called at the start of each render cycle to drain the channel and check
debounce timers:

```rust
pub fn process_updates(&mut self, config: &FileTreeConfig,
                       diff_providers: Option<&DiffProviderRegistry>) {
    // Check debounce timers
    if let Some(providers) = diff_providers {
        self.check_git_refresh_timer(providers);
    }
    self.check_follow_timer(config);

    // Drain channel
    while let Ok(update) = self.update_rx.try_recv() {
        match update {
            FileTreeUpdate::ChildrenLoaded { parent, entries } => {
                let Some(parent_node) = self.nodes.get(parent) else {
                    continue; // Parent was deleted before children arrived
                };
                let depth = parent_node.depth + 1;

                // Remove old children if re-scanning
                let old_children: Vec<NodeId> = self.nodes
                    .get(parent)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for old_id in old_children {
                    self.nodes.remove(old_id);
                }

                let mut child_ids = Vec::with_capacity(entries.len());
                for (name, kind) in entries {
                    let child_id = self.nodes.insert(FileNode {
                        name,
                        kind,
                        parent: Some(parent),
                        children: Vec::new(),
                        expanded: false,
                        loaded: false,
                        depth,
                    });
                    child_ids.push(child_id);
                }

                if let Some(parent_node) = self.nodes.get_mut(parent) {
                    parent_node.children = child_ids;
                    parent_node.loaded = true;
                }
                self.visible_dirty = true;
            }
            FileTreeUpdate::GitStatus(statuses) => {
                for (path, status) in statuses {
                    self.git_status_map.insert(path, status);
                }
                self.rebuild_dir_status_cache();
            }
            FileTreeUpdate::ScanError { path, reason } => {
                log::warn!("file tree: {}: {}", path.display(), reason);
            }
        }
    }

    if self.visible_dirty {
        self.rebuild_visible();
    }
}
```

### Step 1.7: Git status lookup

Git status uses a two-tier lookup: a flat map for file-level status and a
pre-built cache for directory-level "worst descendant" status. The cache is
rebuilt when the git status map changes (on receiving `GitStatus` updates),
NOT on every render call.

```rust
fn rebuild_dir_status_cache(&mut self) {
    self.dir_status_cache.clear();

    for (path, &status) in &self.git_status_map {
        // Walk up ancestors, updating worst status for each
        let mut ancestor = path.parent();
        while let Some(dir) = ancestor {
            if dir < self.root.as_path() { break; }
            let entry = self.dir_status_cache
                .entry(dir.to_path_buf())
                .or_insert(GitStatus::Clean);
            if status.severity() > entry.severity() {
                *entry = status;
            }
            ancestor = dir.parent();
        }
    }
}

pub fn git_status_for(&self, id: NodeId) -> GitStatus {
    let path = self.node_path(id);
    let node = match self.nodes.get(id) {
        Some(n) => n,
        None => return GitStatus::Clean,
    };

    match node.kind {
        NodeKind::File => {
            // Direct lookup
            if let Some(&status) = self.git_status_map.get(&path) {
                return status;
            }
            // Inherit untracked from parent directory
            let mut current = id;
            while let Some(parent_id) = self.nodes.get(current).and_then(|n| n.parent) {
                let parent_path = self.node_path(parent_id);
                if let Some(&s) = self.git_status_map.get(&parent_path) {
                    if matches!(s, GitStatus::Untracked) {
                        return s;
                    }
                }
                current = parent_id;
            }
            GitStatus::Clean
        }
        NodeKind::Directory => {
            // O(1) lookup from pre-built cache
            self.dir_status_cache.get(&path).copied().unwrap_or(GitStatus::Clean)
        }
    }
}
```

### Step 1.8: Rebuild visible list

Stack-based (not recursive) traversal, matching the pattern used by
`helix-view/src/tree.rs`'s `Traverse` iterator:

```rust
fn rebuild_visible(&mut self) {
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

    // Clamp selection and scroll to valid range
    let max = self.visible.len().saturating_sub(1);
    self.selected = self.selected.min(max);
    self.scroll_offset = self.scroll_offset.min(max);

    self.visible_dirty = false;
}
```

### Step 1.9: Navigation

```rust
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
    // scroll_offset adjusted in ensure_selected_visible
    self.ensure_selected_visible();
}

fn ensure_selected_visible(&mut self) {
    if self.selected < self.scroll_offset {
        self.scroll_offset = self.selected;
    }
    // viewport_height is set during render; use a reasonable default
    // until first render. The actual clamping happens in the render fn.
}

pub fn selected_node(&self) -> Option<&FileNode> {
    self.visible.get(self.selected).and_then(|&id| self.nodes.get(id))
}

pub fn selected_id(&self) -> Option<NodeId> {
    self.visible.get(self.selected).copied()
}
```

### Step 1.10: Reveal path

Handles the case where ancestor directories haven't been loaded yet by
loading them synchronously (acceptable since reveal targets specific paths,
not entire subtrees):

```rust
pub fn reveal_path(&mut self, path: &Path, config: &FileTreeConfig) {
    // Path must be under root
    let relative = match path.strip_prefix(&self.root) {
        Ok(r) => r,
        Err(_) => return, // Path outside workspace
    };

    // Walk down from root, expanding and loading as needed
    let mut current_id = self.root_id;
    let mut current_path = self.root.clone();

    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        current_path.push(&*name);

        // Ensure directory is loaded
        let node = match self.nodes.get(current_id) {
            Some(n) => n,
            None => return,
        };

        if !node.loaded && node.kind == NodeKind::Directory {
            // Synchronous load for reveal — only loads one level at a time
            // along the reveal path, not entire subtrees
            self.load_children_sync(current_id, config);
        }

        // Expand the directory
        if let Some(n) = self.nodes.get_mut(current_id) {
            n.expanded = true;
        }

        // Find the child matching this component
        let children = self.nodes.get(current_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        match children.iter().find(|&&cid| {
            self.nodes.get(cid).map(|n| n.name == *name).unwrap_or(false)
        }) {
            Some(&child_id) => current_id = child_id,
            None => return, // Path not found in tree
        }
    }

    // Select the revealed node
    self.visible_dirty = true;
    self.rebuild_visible();
    if let Some(pos) = self.visible.iter().position(|&id| id == current_id) {
        self.selected = pos;
        self.ensure_selected_visible();
    }
}

fn load_children_sync(&mut self, node_id: NodeId, config: &FileTreeConfig) {
    let path = self.node_path(node_id);
    let walker = ignore::WalkBuilder::new(&path)
        .hidden(!config.hidden)
        .git_ignore(config.git_ignore)
        .follow_links(config.follow_symlinks)
        .max_depth(Some(1))
        .sort_by_file_name(|a, b| a.cmp(b))
        .add_custom_ignore_filename(helix_loader::config_dir().join("ignore"))
        .add_custom_ignore_filename(".helix/ignore")
        .build();

    let depth = self.nodes.get(node_id).map(|n| n.depth + 1).unwrap_or(1);
    let mut child_ids = Vec::new();

    for result in walker {
        if let Ok(entry) = result {
            if entry.path() == path { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = if entry.file_type()
                .map(|ft| ft.is_dir()).unwrap_or(false)
            {
                NodeKind::Directory
            } else {
                NodeKind::File
            };
            let child_id = self.nodes.insert(FileNode {
                name,
                kind,
                parent: Some(node_id),
                children: Vec::new(),
                expanded: false,
                loaded: false,
                depth,
            });
            child_ids.push(child_id);
        }
    }

    // Sort: directories first, then alphabetical
    child_ids.sort_by(|&a, &b| {
        let na = &self.nodes[a];
        let nb = &self.nodes[b];
        na.kind.cmp(&nb.kind).reverse().then(na.name.cmp(&nb.name))
    });

    if let Some(node) = self.nodes.get_mut(node_id) {
        node.children = child_ids;
        node.loaded = true;
    }
}
```

### Step 1.11: Refresh

Re-scans all currently expanded directories, preserving expand state:

```rust
pub fn refresh(&mut self, config: &FileTreeConfig) {
    // Collect all expanded+loaded directory IDs
    let expanded_dirs: Vec<NodeId> = self.nodes.iter()
        .filter(|(_, n)| n.kind == NodeKind::Directory && n.expanded && n.loaded)
        .map(|(id, _)| id)
        .collect();

    // Mark them as unloaded so spawn_load_children re-scans
    for &id in &expanded_dirs {
        if let Some(node) = self.nodes.get_mut(id) {
            node.loaded = false;
        }
    }

    // Re-scan each (children arrive via channel, replacing old ones)
    for &id in &expanded_dirs {
        self.spawn_load_children(id, config);
    }
}
```

### Step 1.12: Sidebar state on Editor

**File:** `helix-view/src/editor.rs`

Add to `Editor` struct:

```rust
pub file_tree: Option<FileTree>,
pub file_tree_visible: bool,
pub file_tree_focused: bool,
```

Width is read from config at render time (`config().file_tree.width`), not
stored as separate state. This avoids two sources of truth.

**State invariant**: `file_tree_focused` is always `false` when
`file_tree_visible` is `false`. Setting `file_tree_visible = false` must also
set `file_tree_focused = false`.

Add `FileTreeConfig` to `Config`, following the same serde pattern as
`FilePickerConfig` (`helix-view/src/editor.rs:180-223`):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct FileTreeConfig {
    /// Open sidebar on startup (default: false)
    pub auto_open: bool,
    /// Default width in columns (default: 30)
    pub width: u16,
    /// Show hidden files (default: false)
    pub hidden: bool,
    /// Respect .gitignore (default: true)
    pub git_ignore: bool,
    /// Show git status indicators (default: true)
    pub git_status: bool,
    /// Auto-reveal current buffer's file (default: true)
    pub follow_current_file: bool,
    /// Follow symlinks (default: false)
    pub follow_symlinks: bool,
    /// Maximum directory depth to allow expanding (default: 10)
    pub max_depth: Option<u16>,
    /// Scope git status queries to the tree root instead of the entire
    /// worktree. Significantly faster in monorepos. (default: false)
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
```

### Step 1.13: Export the module

Add `pub mod file_tree;` to `helix-view/src/lib.rs`.

### Step 1.14: Safety considerations

- **Symlink cycles**: Rely on `ignore` crate's built-in cycle detection
  (`follow_links(false)` by default). Symlinks appear as file nodes.
- **Permission errors**: Sent as `ScanError` via channel, logged as warnings.
  The tree shows what it can see without crashing.
- **Deep nesting**: `max_depth` config (default 10) prevents unbounded
  traversal. `depth` uses `u16` (65535 max).
- **Non-UTF8 filenames**: Use `to_string_lossy()` for display. Lossy names are
  fine for rendering; file operations use the reconstructed path from
  `node_path()`.
- **Large directories**: Lazy loading means only expanded directories are
  scanned. Background threads prevent UI freezes.
- **Race conditions**: If a file is deleted between tree display and user action
  (e.g., open), `editor.open()` returns an error which is shown via
  `editor.set_error()`.
- **Channel closure**: Background tasks check `tx.blocking_send()` return and
  exit silently if the receiver is dropped (tree was closed).

---

## Phase 2: Rendering (helix-term)

### Step 2.1: Create the file tree renderer

**File:** `helix-term/src/ui/file_tree.rs` (new)

Uses the same rendering primitives as the statusline (`surface.set_style`,
`surface.set_string`, `surface.set_stringn`):

```rust
pub fn render_file_tree(
    tree: &FileTree,
    area: Rect,
    surface: &mut Surface,
    theme: &Theme,
    is_focused: bool,
)
```

Rendering logic:
1. Guard: return early if `area.width < 5 || area.height < 2`.
2. Fill background with `theme.get("ui.sidebar")`.
3. Draw vertical separator (`│`) on right edge using `theme.get("ui.sidebar.separator")` with fallback to `theme.get("ui.virtual.separator")`.
4. For each visible node (starting from `scroll_offset`, limited by height):
   a. Indent based on `node.depth` (2 chars per level).
   b. Expand/collapse indicator: `▸` collapsed dir, `▾` expanded dir, ` ` file.
   c. Filename via `surface.set_stringn()` (auto-truncates to available width).
   d. Git status indicator at end of line (if space permits and status ≠ Clean).
   e. Selected line: apply `theme.get("ui.sidebar.selected")` style to full row.
5. If scrolled past the root, show parent path breadcrumb on the first line.

Git status indicators:

| Status | Symbol | Theme scope |
|--------|--------|-------------|
| Modified | `●` | `ui.sidebar.git.modified` |
| Untracked | `◌` | `ui.sidebar.git.untracked` |
| Deleted | `✕` | `ui.sidebar.git.deleted` |
| Conflict | `⚠` | `ui.sidebar.git.conflict` |
| Renamed | `→` | `ui.sidebar.git.modified` (fallback) |

### Step 2.2: Integrate into `EditorView::render`

**File:** `helix-term/src/ui/editor.rs`

In the `render` method, **before** `cx.editor.resize(editor_area)`:

```rust
// Process any pending async updates and debounce timers before rendering
let config = cx.editor.config();
if let Some(ref mut tree) = cx.editor.file_tree {
    tree.process_updates(
        &config.file_tree,
        Some(&cx.editor.diff_providers),
    );
}
let sidebar_width = if cx.editor.file_tree_visible {
    // Cap at 1/3 of terminal width, leave at least 10 cols for editor
    config.file_tree.width.min(area.width.saturating_sub(10) / 3)
} else {
    0
};

let sidebar_area = if sidebar_width > 0 {
    Rect::new(area.x, editor_area.y, sidebar_width, editor_area.height)
} else {
    Rect::default()
};

if sidebar_width > 0 {
    editor_area = editor_area.clip_left(sidebar_width + 1); // +1 for separator
}
```

After the view rendering loop:

```rust
if let Some(ref tree) = cx.editor.file_tree {
    if cx.editor.file_tree_visible {
        file_tree::render_file_tree(
            tree, sidebar_area, surface, &cx.editor.theme,
            cx.editor.file_tree_focused,
        );
    }
}
```

### Step 2.3: Register the module

Add `pub(crate) mod file_tree;` to `helix-term/src/ui/mod.rs`.

---

## Phase 3: Event Handling and Navigation

### Step 3.1: Handle keyboard events

**File:** `helix-term/src/ui/editor.rs`

In `EditorView::handle_event`, check focus before normal keymap dispatch:

```rust
if cx.editor.file_tree_focused {
    if let Event::Key(key) = event {
        return self.handle_file_tree_event(key, cx);
    }
}
```

### Step 3.2: Tree navigation keybindings

New method `handle_file_tree_event`. Operations that need `&mut Editor` (like
opening a file) use `cx.callback.push(Box::new(...))` — same pattern as the
file picker (`helix-term/src/ui/mod.rs:274-283`):

| Key | Action |
|-----|--------|
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `Enter` / `l` | Open file (via callback) / toggle expand directory |
| `h` | Collapse directory or go to parent |
| `q` / `Esc` | Unfocus sidebar (return to editor) |
| `r` | Refresh tree (re-scan expanded directories) |
| `a` | Create file (prompt) |
| `d` | Delete file (confirm prompt) |
| `R` | Rename file (prompt) |
| `/` | Search/filter within tree |
| `gg` | Jump to top |
| `G` | Jump to bottom |

Example — open file with proper error handling:

```rust
KeyCode::Enter | KeyCode::Char('l') => {
    let config = cx.editor.config().file_tree.clone();
    if let Some(tree) = cx.editor.file_tree.as_mut() {
        let Some(&id) = tree.visible.get(tree.selected) else {
            return EventResult::Consumed(None);
        };
        let Some(node) = tree.nodes.get(id) else {
            return EventResult::Consumed(None);
        };
        match node.kind {
            NodeKind::Directory => {
                tree.toggle_expand(id, &config);
            }
            NodeKind::File => {
                let path = tree.node_path(id);
                cx.callback.push(Box::new(move |_compositor, cx| {
                    if let Err(e) = cx.editor.open(&path, Action::Load) {
                        cx.editor.set_error(format!("{}", e));
                    } else {
                        cx.editor.file_tree_focused = false;
                    }
                }));
            }
        }
    }
    EventResult::Consumed(None)
}
```

### Step 3.3: Focus management

`Tab` or a configurable binding toggles focus between sidebar and editor.
When focused: `file_tree_focused = true`, keys intercepted by tree handler.
When unfocused: `file_tree_focused = false`, normal keymap dispatch resumes.
Mouse clicks in the sidebar area set focus; clicks in the editor area unset it.

File tree focus is orthogonal to editor mode (Normal/Insert/etc.) — the user
stays in Normal mode while navigating the tree.

---

## Phase 4: Commands

### Step 4.1: Register commands

**File:** `helix-term/src/commands.rs`

Add to `static_commands!`:

```rust
toggle_file_tree, "Toggle file tree sidebar",
focus_file_tree, "Focus file tree sidebar",
reveal_in_file_tree, "Reveal current file in tree",
```

Implementations:

```rust
fn toggle_file_tree(cx: &mut Context) {
    let config = cx.editor.config();
    if cx.editor.file_tree.is_none() {
        let root = find_workspace().0;
        match FileTree::new(root, &config.file_tree) {
            Ok(tree) => cx.editor.file_tree = Some(tree),
            Err(e) => {
                cx.editor.set_error(format!("Failed to open file tree: {}", e));
                return;
            }
        }
    }
    cx.editor.file_tree_visible = !cx.editor.file_tree_visible;
    if cx.editor.file_tree_visible {
        cx.editor.file_tree_focused = true;
    } else {
        cx.editor.file_tree_focused = false; // enforce invariant
    }
}

fn focus_file_tree(cx: &mut Context) {
    if cx.editor.file_tree_visible {
        cx.editor.file_tree_focused = !cx.editor.file_tree_focused;
    }
}

fn reveal_in_file_tree(cx: &mut Context) {
    let config = cx.editor.config().file_tree.clone();
    if let Some(path) = doc!(cx.editor).path().cloned() {
        if let Some(ref mut tree) = cx.editor.file_tree {
            tree.reveal_path(&path, &config);
        }
    }
}
```

### Step 4.2: Default keybindings

**File:** `helix-term/src/keymap/default.rs`

In space mode:

```rust
"E" => toggle_file_tree,
```

### Step 4.3: Typed commands

**File:** `helix-term/src/commands/typed.rs`

- `:tree-toggle` — toggle sidebar visibility
- `:tree-reveal` — reveal current file in tree
- `:tree-width <n>` — update `config.file_tree.width`

---

## Phase 5: Git Integration

### Step 5.1: Debounced git status refresh

Git status refreshes are debounced at 1000ms to avoid spawning redundant
subprocess calls during rapid save/edit cycles:

```rust
const GIT_REFRESH_DEBOUNCE: Duration = Duration::from_millis(1000);

pub fn request_git_refresh(&mut self) {
    self.git_refresh_deadline = Some(Instant::now() + GIT_REFRESH_DEBOUNCE);
}

/// Called from `process_updates()` each render cycle.
fn check_git_refresh_timer(&mut self, diff_providers: &DiffProviderRegistry) {
    if let Some(deadline) = self.git_refresh_deadline {
        if Instant::now() >= deadline {
            self.git_refresh_deadline = None;
            self.spawn_git_status(diff_providers.clone());
        }
    }
}
```

### Step 5.2: Three-stage git loading

Adapted from neo-tree.nvim's production-tuned strategy. Instead of one
expensive `git status` call, split into stages so tracked-file changes appear
almost instantly:

**Stage 1 — Fast scan (tracked files only):** Runs `git status` excluding
untracked files. This is fast even in monorepos because git only diffs the
index. Results arrive within ~50ms.

**Stage 2 — Add untracked files:** Runs a second `git status` including
untracked files. Slower in large repos because git must scan the working tree.
Results merge into the existing map.

**Stage 3 — Scope limiting (monorepo optimization):** When
`git_status_scope_to_path` is enabled, both stages scope their query to the
tree root path instead of the entire worktree.

```rust
/// Three-stage git status loading.
/// Uses `DiffProviderRegistry::for_each_changed_file` which already
/// spawns via `tokio::task::spawn_blocking` internally.
fn spawn_git_status(&self, diff_providers: DiffProviderRegistry) {
    let tx = self.update_tx.clone();
    let root = self.root.clone();

    // for_each_changed_file handles the background spawning
    diff_providers.for_each_changed_file(root, move |result| {
        if let Ok(change) = result {
            let status = match &change {
                FileChange::Untracked { .. } => GitStatus::Untracked,
                FileChange::Modified { .. } => GitStatus::Modified,
                FileChange::Deleted { .. } => GitStatus::Deleted,
                FileChange::Renamed { .. } => GitStatus::Renamed,
                FileChange::Conflict { .. } => GitStatus::Conflict,
            };
            let path = change.path().to_owned();
            let _ = tx.blocking_send(FileTreeUpdate::GitStatus(vec![(path, status)]));
        }
        true
    });
}
```

Note: The three-stage split requires extending `DiffProviderRegistry` with a
`for_each_changed_file_tracked_only` method, or adding a filter parameter. If
that's not feasible initially, a single-stage call is acceptable — the debounce
timer already prevents the worst-case rapid re-scanning.

### Step 5.3: Git status hash caching

When git status results arrive, hash the full set and compare to the previous
hash. If unchanged, skip the `rebuild_dir_status_cache()` call entirely:

```rust
// In process_updates(), when handling FileTreeUpdate::GitStatus:
FileTreeUpdate::GitStatus(statuses) => {
    for (path, status) in statuses {
        self.git_status_map.insert(path, status);
    }

    // Hash the current status map to detect actual changes
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Sort keys for deterministic hashing
    let mut entries: Vec<_> = self.git_status_map.iter().collect();
    entries.sort_by_key(|(p, _)| p.clone());
    for (path, status) in &entries {
        path.hash(&mut hasher);
        (*status as u8).hash(&mut hasher);
    }
    let new_hash = hasher.finish();

    if self.last_git_status_hash != Some(new_hash) {
        self.last_git_status_hash = Some(new_hash);
        self.rebuild_dir_status_cache();
    }
}
```

This avoids redundant work when the user triggers refresh but nothing actually
changed (common during active development).

### Step 5.4: Status display

Directories show the worst status of any descendant via the pre-built
`dir_status_cache` (see Step 1.7). The cache is rebuilt only when the git
status hash changes, making both the cache rebuild and render-time lookups
efficient.

The `diff_providers` are accessed from `cx.editor.diff_providers.clone()` when
calling `request_git_refresh` (e.g., on tree open, on manual refresh, and on
file save).

---

## Phase 6: Theme Support

| Scope | Purpose | Fallback |
|-------|---------|----------|
| `ui.sidebar` | Background | — |
| `ui.sidebar.separator` | Vertical separator line | `ui.virtual.separator` |
| `ui.sidebar.selected` | Selected entry highlight | `ui.cursor.primary` |
| `ui.sidebar.file` | File name | `ui.text` |
| `ui.sidebar.directory` | Directory name | `ui.text.directory` |
| `ui.sidebar.git.modified` | Git modified indicator | — |
| `ui.sidebar.git.untracked` | Git untracked indicator | — |
| `ui.sidebar.git.deleted` | Git deleted indicator | — |
| `ui.sidebar.git.conflict` | Git conflict indicator | — |

---

## Phase 7: Polish and Edge Cases

### Step 7.1: Refresh on file save

In `Application::handle_editor_event`, after a successful save:
1. Trigger a re-scan of the parent directory of the saved file (not the entire
   tree).
2. Call `tree.request_git_refresh()` to queue a debounced git status update
   (1000ms). If the user saves multiple files in quick succession, only one git
   status subprocess runs.

### Step 7.2: Follow current file

When `follow_current_file` is true, on document focus change call
`tree.reveal_path(current_doc_path)` to auto-expand and scroll. Debounce with
a 100ms window to avoid thrashing on rapid buffer switches (e.g., `:bnext` in
a loop, or `gd` jumping through definitions).

Add to `FileTree`:

```rust
const FOLLOW_DEBOUNCE: Duration = Duration::from_millis(100);

/// Queue a follow-current-file reveal. The actual reveal happens when
/// the debounce timer fires in `process_updates()`.
pub fn request_follow(&mut self, path: PathBuf) {
    self.follow_target = Some(path);
    self.follow_deadline = Some(Instant::now() + FOLLOW_DEBOUNCE);
}

/// Called from `process_updates()` each render cycle.
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
```

In `Application::handle_editor_event`, on document focus change:

```rust
if config.file_tree.follow_current_file {
    if let Some(path) = cx.editor.documents().current().path().cloned() {
        if let Some(ref mut tree) = cx.editor.file_tree {
            tree.request_follow(path);
        }
    }
}
```

### Step 7.3: Handle resize

The `editor_area` subtraction before `cx.editor.resize()` means the split tree
automatically gets the correct reduced space. The sidebar's `scroll_offset`
should be clamped to the new height.

### Step 7.4: Mouse support

In `EditorView::handle_mouse_event`, check if click coordinates fall within the
sidebar area. Single click selects, double click opens/toggles. Clicking in the
editor area while sidebar is focused must unfocus the sidebar.

### Step 7.5: Scrolled-off parent breadcrumb

When the tree is scrolled past a directory's header, display that directory's
name on the top line of the sidebar (similar to sticky headers). This helps
users orient themselves in deeply nested trees.

---

## Phase 8: Future Enhancements

Features identified from neo-tree.nvim analysis, not required for initial
implementation but worth considering for later phases:

### File nesting rules

Group related files under a parent (e.g., `tsconfig.json` nests
`tsconfig.spec.json`, `package.json` nests `package-lock.json`). Configured via
pattern rules in `config.toml`.

### Deep search / filter

Regex or fuzzy filter that shows only matching files plus their ancestor
directories. The tree auto-expands matching branches and collapses non-matching
ones. Similar to neo-tree's `filter_as_you_type` mode.

### LSP diagnostics indicators

Show error/warning count badges on files that have LSP diagnostics. Propagate
worst diagnostic severity to parent directories (similar to git status
propagation).

### File preview

When hovering over a file in the tree (configurable delay or explicit
keybinding), show a read-only preview in the editor pane without switching the
active document.

### Cut/copy/paste clipboard

Built-in clipboard for file operations. `y` to copy, `x` to cut, `p` to paste.
Visual feedback showing which nodes are in the clipboard and whether the
operation is copy or move.

### Multiple source tabs

Switch between different views in the same sidebar pane: filesystem tree, open
buffers, git changed files. A tab bar at the top of the sidebar shows available
sources.

### File watchers

Use `notify` crate to watch loaded directories for changes. When a file is
created/deleted/renamed externally, the relevant directory is automatically
re-scanned (debounced at 5000ms to avoid thrashing). More responsive than
refresh-on-save alone.

### Rendered line caching

Cache the rendered styled text per node, keyed by `(node_id, available_width)`.
Only re-render a line when the node's state changes (name, git status, expand
state) or the sidebar width changes. Neo-tree does this and it significantly
reduces rendering work when scrolling through unchanged content.

### Empty directory grouping

Merge single-child directory chains into a single display node. For example,
`src/main/java/com` where each directory has exactly one child renders as one
line: `src/main/java/com` instead of four nested entries. This is a pure UX
improvement that reduces visual noise in deeply nested projects (common in Java,
Go module layouts).

---

## Files Summary

| File | Action | Phase |
|------|--------|-------|
| `helix-view/src/file_tree.rs` | New: tree data structure + async scanning | 1 |
| `helix-view/src/editor.rs` | Add state + `FileTreeConfig` | 1 |
| `helix-view/src/lib.rs` | Export `file_tree` module | 1 |
| `helix-term/src/ui/file_tree.rs` | New: rendering | 2 |
| `helix-term/src/ui/editor.rs` | Area splitting, event routing, update processing | 2-3 |
| `helix-term/src/ui/mod.rs` | Register module | 2 |
| `helix-term/src/commands.rs` | Toggle/focus/reveal commands | 4 |
| `helix-term/src/keymap/default.rs` | Bind `<space>E` | 4 |
| `helix-term/src/commands/typed.rs` | Typed commands | 4 |
| `helix-term/src/application.rs` | Auto-refresh on save, debounced follow | 7 |
| `book/src/editor.md` | Document config | 7 |
| `book/src/keymap.md` | Document keybindings | 7 |
