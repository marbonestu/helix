# File Tree Sidebar Testing Plan

Tests are organized in two layers: unit tests for the `FileTree` data structure in
`helix-view`, and notes on integration-level coverage (manual/visual) since the
file tree renders inline inside `EditorView` rather than as a separate `Component`.

---

## Layer 1: Unit Tests — `helix-view/src/file_tree.rs`

All tests live in a `#[cfg(test)] mod tests` block at the bottom of the file.
Tests that need async (tokio runtime) use `#[tokio::test]`.
Tests that need a real filesystem use `tempfile::tempdir()`.

### Already implemented

| Test | What it verifies |
|------|-----------------|
| `test_new_nonexistent_root_returns_error` | `FileTree::new` returns `Err` for missing path |
| `test_new_creates_root_node` | Root node is a loaded, expanded `Directory` at depth 0 |
| `test_node_path_root` | `node_path` for root returns the workspace path |
| `test_node_path_depth_1` | `node_path` for direct child concatenates name correctly |
| `test_node_path_depth_2` | `node_path` at depth 2 is fully resolved |
| `test_selected_node_default` | Default selection (index 0) returns the root node |
| `test_git_status_severity_ordering` | `GitStatus::severity()` total ordering is correct |
| `test_node_kind_ordering` | `Directory < File` so sort puts dirs first |
| `test_toggle_expand_collapses_expanded_dir` | Toggling expanded dir sets `expanded = false` and marks visible dirty |
| `test_toggle_expand_expands_collapsed_dir` | Toggling collapsed dir sets `expanded = true` |
| `test_toggle_expand_on_file_is_noop` | Files are not expandable; visible_dirty stays false |
| `test_toggle_expand_unloaded_dir_stays_expanded` | Unloaded dir goes expanded while async load is in-flight |
| `test_spawn_load_children_sends_entries` | Background task sends `ChildrenLoaded` with dirs-first sorted entries |
| `test_rebuild_visible_all_expanded` | DFS traversal order with all nodes expanded |
| `test_rebuild_visible_collapsed_dir` | Collapsed directory hides its subtree from `visible` |
| `test_rebuild_visible_clamps_selection` | `selected` is clamped to `visible.len() - 1` after rebuild |
| `test_process_updates_children_loaded` | `ChildrenLoaded` update inserts child nodes with correct depth/parent |
| `test_process_updates_replaces_old_children` | Re-loading a directory removes old child nodes from the arena |
| `test_git_status_file_lookup` | Direct path hit in `git_status_map` returns correct status |
| `test_git_status_directory_uses_cache` | Directory status is the worst status among all descendants |
| `test_git_status_dir_worst_of_descendants` | Conflict beats Untracked for severity |
| `test_git_status_clean_file` | Missing entry returns `GitStatus::Clean` |
| `test_git_status_untracked_inherits_to_children` | Child inherits Untracked when parent dir is Untracked |
| `test_process_updates_git_status` | `GitStatus` update populates `git_status_map` and rebuilds dir cache |
| `test_move_down` | `move_down()` increments `selected` |
| `test_move_down_clamps_at_end` | `move_down()` stops at last visible item |
| `test_move_up` | `move_up()` decrements `selected` |
| `test_move_up_clamps_at_top` | `move_up()` stops at 0 |
| `test_jump_to_top_and_bottom` | `jump_to_top` resets scroll; `jump_to_bottom` selects last item |
| `test_clamp_scroll` | `clamp_scroll` scrolls down when selected is below viewport |
| `test_clamp_scroll_selected_above_viewport` | `clamp_scroll` scrolls up when selected is above viewport |
| `test_reveal_path_already_loaded` | `reveal_path` expands parent dirs and selects file when already loaded |
| `test_reveal_path_outside_root_is_noop` | Path outside root does not change selection |
| `test_reveal_path_with_async_load` | `reveal_path` sets `pending_reveal`, async load completes, file is selected |
| `test_refresh_marks_dirs_unloaded` | Refreshing clears `loaded` on expanded directories |
| `test_request_git_refresh_sets_deadline` | `request_git_refresh` sets a future deadline |
| `test_request_follow_sets_target` | `request_follow` stores target path and deadline |
| `test_request_follow_does_not_reset_deadline` | Second `request_follow` before deadline fires updates target but preserves deadline |

### Missing: page scroll

```rust
#[test]
fn test_page_up() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible(); // 5 items
    tree.selected = 4;

    tree.page_up(2);
    assert_eq!(tree.selected, 2);
}

#[test]
fn test_page_up_clamps_at_top() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.selected = 1;

    tree.page_up(10);
    assert_eq!(tree.selected, 0);
}

#[test]
fn test_page_down() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible(); // 5 items
    tree.selected = 1;

    tree.page_down(2);
    assert_eq!(tree.selected, 3);
}

#[test]
fn test_page_down_clamps_at_bottom() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible(); // 5 items
    tree.selected = 3;

    tree.page_down(10);
    assert_eq!(tree.selected, 4);
}

#[test]
fn test_scroll_view_up() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.scroll_offset = 3;

    tree.scroll_view_up();
    assert_eq!(tree.scroll_offset, 2);
}

#[test]
fn test_scroll_view_up_clamps_at_zero() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.scroll_offset = 0;

    tree.scroll_view_up();
    assert_eq!(tree.scroll_offset, 0);
}

#[test]
fn test_scroll_view_down() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible(); // 5 items
    tree.scroll_offset = 0;

    tree.scroll_view_down();
    assert_eq!(tree.scroll_offset, 1);
}

#[test]
fn test_scroll_view_down_clamps_at_last() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible(); // 5 items
    tree.scroll_offset = 4; // already at max

    tree.scroll_view_down();
    assert_eq!(tree.scroll_offset, 4);
}
```

### Missing: search

```rust
#[test]
fn test_search_start_activates_mode() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();

    assert!(!tree.search_active());
    tree.search_start();
    assert!(tree.search_active());
    assert_eq!(tree.search_query(), "");
}

#[test]
fn test_search_push_appends_chars() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();

    tree.search_push('s');
    tree.search_push('r');
    tree.search_push('c');
    assert_eq!(tree.search_query(), "src");
}

#[test]
fn test_search_push_jumps_to_first_match() {
    let (mut tree, _, src_id, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();

    // "src" matches the src/ directory node
    tree.search_push('s');
    tree.search_push('r');
    tree.search_push('c');

    let selected = tree.visible[tree.selected];
    assert_eq!(selected, src_id);
}

#[test]
fn test_search_pop_removes_last_char() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();

    tree.search_push('a');
    tree.search_push('b');
    tree.search_pop();
    assert_eq!(tree.search_query(), "a");
}

#[test]
fn test_search_pop_on_empty_is_noop() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();

    tree.search_pop(); // should not panic
    assert_eq!(tree.search_query(), "");
}

#[test]
fn test_search_confirm_deactivates_keeps_selection() {
    let (mut tree, _, src_id, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();

    tree.search_push('s');
    tree.search_push('r');
    tree.search_push('c');
    let selected_before = tree.selected;

    tree.search_confirm();

    assert!(!tree.search_active());
    assert_eq!(tree.selected, selected_before); // selection preserved
}

#[test]
fn test_search_cancel_restores_selection() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    let original_selected = 0;
    tree.selected = original_selected;

    tree.search_start();
    tree.search_push('s'); // moves selection to src
    assert_ne!(tree.selected, original_selected);

    tree.search_cancel();

    assert!(!tree.search_active());
    assert_eq!(tree.selected, original_selected); // restored
}

#[test]
fn test_search_case_insensitive() {
    let (mut tree, _, src_id, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();

    // Uppercase "SRC" should match lowercase "src"
    tree.search_push('S');
    tree.search_push('R');
    tree.search_push('C');

    let selected = tree.visible[tree.selected];
    assert_eq!(selected, src_id);
}

#[test]
fn test_search_no_match_preserves_selection() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    let original_selected = tree.selected;
    tree.search_start();

    tree.search_push('z'); // no node named "z"
    tree.search_push('z');
    tree.search_push('z');

    assert_eq!(tree.selected, original_selected);
}

#[test]
fn test_search_next_wraps_around() {
    let (mut tree, _, _, main_id, _) = build_test_tree();
    // build_test_tree has main.rs and lib.rs under src/
    // Add another ".rs" match to make wrapping testable
    tree.rebuild_visible();
    tree.search_start();
    tree.search_push('.'); // matches main.rs, lib.rs, Cargo.toml (the dot)

    let first_match = tree.selected;
    tree.search_next();
    let second_match = tree.selected;
    assert_ne!(first_match, second_match);

    // Keep pressing next until we wrap back to the first match
    for _ in 0..20 {
        tree.search_next();
        if tree.selected == first_match {
            return;
        }
    }
    panic!("search_next did not wrap back to first match");
}

#[test]
fn test_search_prev_wraps_around() {
    let (mut tree, _, _, _, _) = build_test_tree();
    tree.rebuild_visible();
    tree.search_start();
    tree.search_push('.');

    let first_match = tree.selected;
    // Go to previous match (should wrap to last match)
    tree.search_prev();
    assert_ne!(tree.selected, first_match);
}
```

---

## Layer 2: Integration Tests

The file tree renders inside `EditorView::render()` and receives keyboard events
through `EditorView::handle_event()`. It is not a standalone `Component`, so the
standard `test()` / `test_key_sequence` helpers cannot drive it directly without
running a full app.

### What to cover manually / with visual inspection

| Scenario | How to verify |
|----------|---------------|
| `space-e` opens sidebar | Sidebar appears on the left, root directory shown |
| Root loads on first open | No second toggle required; children appear immediately |
| `j`/`k` navigate the list | Selection highlight moves, no reset on re-render |
| `l`/`Enter` on directory | Directory expands; children appear |
| `h` on expanded directory | Directory collapses |
| `h` on file | Cursor moves to parent directory row |
| `Enter`/`l` on file | File opens in the editor view; tree focus dropped |
| `C-w h` from file editor | Focus moves into the file tree sidebar |
| `C-w l` from sidebar | Focus moves back to the editor view |
| `C-u` / `C-d` | Selection jumps by half the viewport height |
| `C-b` / `C-f` | Selection jumps by a full viewport height |
| `C-y` / `C-e` | Viewport scrolls without moving selection |
| `g` / `G` | Jump to first / last item |
| `/` enters search mode | Prompt `/<query>` appears at bottom of sidebar |
| Typing in search mode | Selection tracks first match live |
| `<esc>` in search mode | Mode exits, selection returns to pre-search position |
| `<enter>` in search mode | Mode exits, selection stays on current match |
| `n` / `N` outside search | Jump to next / previous match from last search |
| `C-n` / `C-p` in search | Jump to next / previous match while prompt is open |
| Follow current file | Switching buffers scrolls tree to the active file |
| Follow suppressed while focused | Navigating tree does not snap selection to buffer |
| Git status indicators | Modified/untracked/conflict symbols appear next to files |
| File icons | Nerd font icons shown when `file_tree.icons = true` |
| Theme scopes | `ui.sidebar`, `ui.sidebar.selected`, `ui.sidebar.directory` colors applied |

### Tests NOT needed (and why)

- **Render pixel-level tests**: Sidebar rendering uses the same `surface.set_stringn`
  path as the statusline. Coverage through visual inspection is sufficient.
- **`toggle_file_tree` command**: It calls `FileTree::new` which is tested by the
  unit suite. The command wiring is trivial.
- **Async channel exhaustion**: `process_updates` drains at most 64 messages per
  frame. Overflow in `try_recv` just stops early; no panic path exists.
- **Resize during search**: The `search_active` flag survives a resize because it
  lives on `FileTree` in `Editor`, not on a frame-local stack.
