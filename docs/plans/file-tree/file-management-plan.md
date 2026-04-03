# File Management Commands for the Helix File Tree — Implementation Plan

## Architecture Summary

**`helix-view/src/file_tree.rs`** — Self-contained data model. Has:
- `FileTree` struct with slotmap of `FileNode`s, visible flat list, search state
- `FileTreeUpdate` enum sent over an `mpsc` channel from `tokio::task::spawn_blocking` tasks
- `process_updates()` drains the channel each render cycle
- `search_active: bool` + `search_query: String` as a simple two-field state machine

**`helix-term/src/ui/editor.rs`** — `handle_file_tree_key()` dispatches all key events. When `search_active` is true, characters go to search; otherwise they go to navigation commands. Uses `cx.callback.push()` to defer operations needing both `&mut FileTree` and `&mut Editor`.

**`helix-term/src/ui/file_tree.rs`** — Pure rendering. When `search_active` is true, reserves the bottom row and draws `/<query>`. The pattern for adding any new prompt is: test a flag on `FileTree`, reserve the bottom row, and render the prompt there.

**`helix-view/src/editor.rs`** — `set_doc_path(doc_id, &Path)` properly handles LSP notification + language redetection. `close_document(doc_id, force)` handles view cleanup. `document_by_path()` finds a doc by filesystem path.

---

## Keybindings

| Key | Operation |
|-----|-----------|
| `a` | Create new file |
| `A` | Create new directory |
| `r` | Rename selected item |
| `R` | Refresh tree from disk (previously `r`) |
| `d` | Delete selected item (with y/n confirmation) |
| `y` | Yank (copy) selected item to clipboard |
| `x` | Cut selected item to clipboard |
| `p` | Paste clipboard item into selected location |
| `D` | Duplicate selected file (prompt for new name) |

---

## Phase 1: Data Model (`helix-view/src/file_tree.rs`)

### 1.1 — Prompt mode state machine

Replace the current two booleans (`search_active`, `search_query`) with a proper mode enum so search and new prompts don't collide.

```rust
pub enum PromptMode {
    /// No prompt active — normal navigation.
    None,
    /// Incremental filename search (triggered by `/`).
    Search,
    /// New file name input (triggered by `a`). Stores the resolved target directory.
    NewFile { parent_dir: PathBuf },
    /// New directory name input (triggered by `A`).
    NewDir { parent_dir: PathBuf },
    /// Rename input. Carries the NodeId being renamed.
    Rename(NodeId),
    /// Duplicate name input. Carries the source NodeId.
    Duplicate(NodeId),
    /// Delete y/n confirmation. Carries the NodeId being deleted.
    DeleteConfirm(NodeId),
}
```

`FileTree` gets a single `prompt_mode: PromptMode` field and a `prompt_input: String` field in place of `search_active` / `search_query`. The existing search methods become thin wrappers that pattern-match on `PromptMode::Search` (Decision 1).

### 1.2 — Clipboard state

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOp {
    Copy,
    Cut,
}

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub path: PathBuf,
    pub op: ClipboardOp,
}
```

`FileTree` gets `clipboard: Option<ClipboardEntry>`. Public methods:

```rust
pub fn clipboard(&self) -> Option<&ClipboardEntry>
pub fn yank(&mut self, id: NodeId)       // op = Copy
pub fn cut(&mut self, id: NodeId)        // op = Cut
pub fn clear_clipboard(&mut self)
```

### 1.3 — Status/error display

Add `status_message: Option<String>` to `FileTree`. Shown in the bottom row when no prompt is active.

```rust
pub fn set_status(&mut self, msg: impl Into<String>)
pub fn clear_status(&mut self)
pub fn status_message(&self) -> Option<&str>
```

### 1.4 — New `FileTreeUpdate` variants

```rust
pub enum FileTreeUpdate {
    // existing:
    ChildrenLoaded { parent: NodeId, entries: Vec<(String, NodeKind)> },
    GitStatus(Vec<(PathBuf, GitStatus)>),
    ScanError { path: PathBuf, reason: String },

    // new:
    /// Filesystem operation completed. Triggers a re-scan of the parent directory
    /// and optionally reveals/selects a path.
    FsOpComplete {
        refresh_parent: PathBuf,
        select_path: Option<PathBuf>,
    },
    /// Filesystem op that moved a file — also needs a buffer path update.
    FsOpMoved {
        old_path: PathBuf,
        new_path: PathBuf,
        refresh_parent: PathBuf,
    },
    /// Filesystem operation failed.
    FsOpError { message: String },
}
```

### 1.5 — New `FileTree` public methods

```rust
// Prompt activation
pub fn start_new_file(&mut self)           // resolves selected_dir_path internally
pub fn start_new_dir(&mut self)
pub fn start_rename(&mut self, id: NodeId) // pre-fills prompt_input with current name
pub fn start_duplicate(&mut self, id: NodeId) // pre-fills with "<stem>.copy.<ext>"
pub fn start_delete_confirm(&mut self, id: NodeId)

// Input — shared with search
pub fn prompt_push(&mut self, ch: char)
pub fn prompt_pop(&mut self)
pub fn prompt_cancel(&mut self)

// Commit
pub enum PromptCommit {
    Search,
    NewFile { parent_dir: PathBuf, name: String },
    NewDir  { parent_dir: PathBuf, name: String },
    Rename  { old_path: PathBuf, new_name: String },
    Duplicate { src_path: PathBuf, new_name: String },
    DeleteConfirmed(PathBuf),
    DeleteCancelled,
}
pub fn prompt_confirm(&mut self) -> Option<PromptCommit>
pub fn prompt_input(&self) -> &str
pub fn prompt_mode(&self) -> &PromptMode

// Paste target
/// Returns the path of the selected node if it is a directory, or its parent
/// if it is a file. This is the natural paste/create destination.
pub fn selected_dir_path(&self) -> Option<PathBuf>

// Buffer move drain (see Phase 5)
pub fn take_pending_buffer_renames(&mut self) -> Vec<(PathBuf, PathBuf)>
```

---

## Phase 2: Async Filesystem Operations

Create `helix-view/src/file_tree_ops.rs` with standalone functions — no UI dependency.

Each function takes `Sender<FileTreeUpdate>` and `PathBuf` arguments, runs inside `tokio::task::spawn_blocking`, and sends `FsOpComplete` or `FsOpError` back. Calls `helix_event::request_redraw()` when done.

| Function | Mechanism |
|----------|-----------|
| `spawn_create_file` | `std::fs::File::create` |
| `spawn_create_dir` | `std::fs::create_dir_all` |
| `spawn_rename` | `std::fs::rename(old, parent.join(new_name))` |
| `spawn_delete` | `std::fs::remove_file` or `remove_dir_all` based on node kind |
| `spawn_copy` | `std::fs::copy` (file) or recursive walk using `ignore::WalkBuilder` (dir) |
| `spawn_move` | Try `std::fs::rename` first; on `EXDEV` error fall back to copy+delete. Sends `FsOpMoved`. |

When `FsOpComplete` / `FsOpMoved` is processed in `process_updates()`:
1. Find the node whose path equals `refresh_parent`.
2. If loaded, mark `loaded = false` and call `spawn_load_children` to re-scan.
3. After `ChildrenLoaded` fires, call `reveal_path(select_path)` using the existing `pending_reveal` mechanism.
4. For `FsOpMoved`: push `(old_path, new_path)` to a `pending_buffer_renames: Vec<(PathBuf, PathBuf)>` vec on `FileTree` (drained by the key handler — see Phase 5).

---

## Phase 3: Key Handler (`helix-term/src/ui/editor.rs`)

### 3.1 — Replace `in_search` guard with `prompt_active`

```rust
let prompt_active = !matches!(tree.prompt_mode(), PromptMode::None);

if prompt_active {
    match key.code {
        KeyCode::Esc   => { tree.prompt_cancel(); }
        KeyCode::Enter => {
            if let Some(commit) = tree.prompt_confirm() {
                dispatch_prompt_commit(commit, tree.update_tx(), cx);
            }
        }
        KeyCode::Backspace => { tree.prompt_pop(); }
        KeyCode::Char(ch) if !ctrl => {
            match tree.prompt_mode() {
                PromptMode::DeleteConfirm(_) => {
                    if ch == 'y' {
                        if let Some(commit) = tree.prompt_confirm() {
                            dispatch_prompt_commit(commit, tree.update_tx(), cx);
                        }
                    } else {
                        tree.prompt_cancel();
                    }
                }
                _ => { tree.prompt_push(ch); }
            }
        }
        _ => {}
    }
    return EventResult::Consumed(None);
}
```

### 3.2 — `dispatch_prompt_commit` helper

```rust
fn dispatch_prompt_commit(
    commit: PromptCommit,
    tx: Sender<FileTreeUpdate>,
    cx: &mut Context,
) {
    match commit {
        PromptCommit::NewFile { parent_dir, name } =>
            spawn_create_file(tx, parent_dir, name),
        PromptCommit::NewDir { parent_dir, name } =>
            spawn_create_dir(tx, parent_dir, name),
        PromptCommit::Rename { old_path, new_name } => {
            // editor.move_path() handles the full LSP rename cycle in one call:
            //   1. willRenameFiles → LSP returns workspace edits → apply edits
            //      (this is what updates `mod main` → `mod app` in other files)
            //   2. fs::rename on disk
            //   3. set_doc_path → did_close + language redetection + did_open
            //   4. didRenameFiles notification to all LSP clients
            //   5. file_event_handler notifications for watchers
            // No separate spawn needed — move_path does blocking I/O synchronously
            // inside the callback (same thread as the compositor), which is acceptable
            // for a single-file rename. For large directory renames, revisit with
            // spawn_blocking.
            let new_path = old_path.parent().unwrap().join(&new_name);
            cx.callback.push(Box::new(move |_compositor, cx| {
                if let Err(err) = cx.editor.move_path(&old_path, &new_path) {
                    cx.editor.set_error(format!("rename failed: {err}"));
                } else if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.refresh_dir(new_path.parent().unwrap(), &config);
                }
            }));
        }
        PromptCommit::DeleteConfirmed(path) => {
            cx.callback.push(Box::new(move |_compositor, cx| {
                close_buffers_for_path(cx.editor, &path);
                spawn_delete(tx, path);
            }));
        }
        PromptCommit::Duplicate { src_path, new_name } =>
            spawn_copy(tx, src_path, new_name),
        PromptCommit::Search | PromptCommit::DeleteCancelled => {}
    }
}
```

### 3.3 — New key bindings (in the normal navigation arm)

```rust
KeyCode::Char('a') => { tree.start_new_file(); }
KeyCode::Char('A') => { tree.start_new_dir(); }
KeyCode::Char('r') => {
    if let Some(id) = tree.selected_id() { tree.start_rename(id); }
}
KeyCode::Char('R') => {
    tree.refresh(&config);  // previously bound to 'r'
}
KeyCode::Char('d') if !ctrl => {
    if let Some(id) = tree.selected_id() { tree.start_delete_confirm(id); }
}
KeyCode::Char('y') if !ctrl => {
    if let Some(id) = tree.selected_id() { tree.yank(id); }
}
KeyCode::Char('x') => {
    if let Some(id) = tree.selected_id() { tree.cut(id); }
}
KeyCode::Char('p') => {
    cx.callback.push(Box::new(|_compositor, cx| {
        let Some(ref mut tree) = cx.editor.file_tree else { return };
        let Some(clip) = tree.clipboard().cloned() else { return };
        let Some(dest_dir) = tree.selected_dir_path() else { return };
        let tx = tree.update_tx();
        match clip.op {
            ClipboardOp::Copy => spawn_copy(tx, clip.path, dest_dir),
            ClipboardOp::Cut  => {
                // Best-effort buffer sync before async op (see Decision 3)
                let new_path = dest_dir.join(clip.path.file_name().unwrap());
                if let Some(doc) = cx.editor.document_by_path(&clip.path) {
                    let id = doc.id();
                    cx.editor.set_doc_path(id, &new_path);
                }
                tree.clear_clipboard();
                spawn_move(tx, clip.path, dest_dir);
            }
        }
    }));
}
KeyCode::Char('D') => {
    if let Some(id) = tree.selected_id() { tree.start_duplicate(id); }
}
```

### 3.4 — Drain pending buffer renames (after process_updates)

At the top of `handle_file_tree_key`, after `process_updates()` is called:

```rust
let renames = tree.take_pending_buffer_renames();
for (old, new) in renames {
    cx.callback.push(Box::new(move |_compositor, cx| {
        if let Some(doc) = cx.editor.document_by_path(&old) {
            let id = doc.id();
            cx.editor.set_doc_path(id, &new);
        }
    }));
}
```

---

## Phase 4: Rendering (`helix-term/src/ui/file_tree.rs`)

### 4.1 — Generalize the prompt row

```rust
let prompt_row_needed = !matches!(tree.prompt_mode(), PromptMode::None)
    || tree.status_message().is_some();
let tree_height = if prompt_row_needed {
    content_area.height.saturating_sub(1)
} else {
    content_area.height
};
```

Bottom row display based on mode:

| Mode | Display |
|------|---------|
| `Search` | `/<query>█` |
| `NewFile` | `New file: <input>█` |
| `NewDir` | `New dir: <input>█` |
| `Rename` | `Rename to: <input>█` |
| `Duplicate` | `Duplicate as: <input>█` |
| `DeleteConfirm` | `Delete '<name>'? [y/n]` |
| `None` + status | `<status message>` (dimmed) |

Use `ui.sidebar.search` (existing) as the style for all input prompts. Use a dimmed version for the status message.

### 4.2 — Clipboard indicator tags

After rendering the filename, check if this node is the clipboard item:

```rust
if let Some(clip) = tree.clipboard() {
    if tree.node_path(node_id) == clip.path {
        let tag = match clip.op {
            ClipboardOp::Copy => " (C)",
            ClipboardOp::Cut  => " (X)",
        };
        let tag_style = bg_style.add_modifier(Modifier::DIM);
        // render at current x position if space permits
        surface.set_stringn(current_x, y, tag, remaining_width, tag_style);
    }
}
```

Only renders if there is remaining column space — no layout changes required.

---

## Phase 5: Buffer Sync

### Rename — via `editor.move_path()`

`editor.move_path(old, new)` already exists in `helix-view/src/editor.rs:1611` and handles the complete rename lifecycle in one call:

1. **`willRenameFiles`** — sent to every initialized LSP client that registered interest via `FileOperationsInterest::will_rename`. Each client may return a `WorkspaceEdit`; these are applied immediately via `apply_workspace_edit`, updating references in open buffers (e.g. `mod main` → `mod app`).
2. **`fs::rename`** — the actual filesystem move.
3. **`set_doc_path`** — if the file was open, sends `did_close`, clears language servers, updates the path, re-detects language, and sends `did_open` on the new path.
4. **`didRenameFiles`** — notification sent to all initialized clients.
5. **File event handler** — notifies watchers for both old and new paths.

The callback in `dispatch_prompt_commit` calls `cx.editor.move_path(&old_path, &new_path)` directly. No `spawn_blocking` needed for single-file renames. For directory renames (which are also supported by `move_path`), the blocking I/O is acceptable for typical project sizes — revisit with `spawn_blocking` if needed.

### Delete
`close_buffers_for_path` collects all doc IDs whose path `starts_with(deleted_path)` (handles directory deletion) and calls `close_document(id, force: true)`.

### Move (cut+paste)
Best-effort: update buffer path in the paste callback before spawning the fs task. If the task fails, the buffer holds a stale path — flagged as a v1 known limitation.

---

## Phase 6: Tests

Unit tests in `helix-view/src/file_tree.rs`:
- `test_prompt_mode_transitions`
- `test_prompt_input_accumulates`
- `test_prompt_confirm_variants`
- `test_yank_sets_clipboard` / `test_cut_sets_clipboard`
- `test_selected_dir_path_on_file` / `test_selected_dir_path_on_directory`
- `test_process_updates_fs_op_complete_triggers_rescan`

Integration tests in `helix-view/src/file_tree_ops.rs` using `#[tokio::test]` + `tempfile::tempdir()`:
- create file/dir, rename, delete, copy, move

---

## Phased Delivery Order

| Phase | Deliverable | Can parallelize with |
|-------|-------------|---------------------|
| 1 | Data model: PromptMode, ClipboardEntry, new variants, new methods | — |
| 2 | Async fs ops module | Phase 3 |
| 3 | Rendering changes (prompt row, clipboard tags) | Phase 2 |
| 4 | Key handler additions | After 1, 2 |
| 5 | Buffer sync callbacks | After 4 |
| 6 | Tests | After each phase |

---

## Open Decisions

**Decision 1 — search API rename.** Keep `search_active()` / `search_query()` as wrapper methods over `PromptMode` to avoid breaking existing callsites. Refactor in a follow-up.

**Decision 2 — warn on dirty buffer delete.** For v1, always show plain `Delete '<name>'? [y/n]` and close with `force=true`. For v2, pass a dirty flag to `start_delete_confirm` and prefix the prompt with `(unsaved!)`.

**Decision 3 — clipboard on failed paste.** For v1, clear the cut clipboard immediately on paste attempt. If the fs op fails, the user must re-cut. A restore-on-failure mechanism is v2.

**Decision 4 — overwrite on paste collision.** For v1, let the OS behavior apply (Unix: overwrite file, fail on dir). Surface any OS error via `FsOpError` → status message. A user-facing overwrite confirmation prompt is v2.
