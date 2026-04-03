# Resizable Splits — Testing Plan

Tests are split across two layers matching helix's existing test architecture:

1. **Unit tests** in `helix-view/src/tree.rs` `#[cfg(test)]` — fast, direct
   assertions on tree structure, weights, and areas.
2. **Integration tests** in `helix-term/tests/test/splits.rs` — full editor
   simulation via key sequences, gated behind `#[cfg(feature = "integration")]`.

---

## Layer 1: Unit Tests (`helix-view/src/tree.rs`)

All tests use the existing pattern: `Tree::new(Rect::new(0, 0, W, H))`,
create `View::new(DocumentId::default(), GutterConfig::default())`, and
split with `tree.split(view, Layout::*)`.

### 1.1 Weight Invariants

**`test_weights_initialized_to_equal`**
- Create tree, split 3 times vertically.
- Assert each container's `weights` == `vec![1.0; children.len()]`.
- Assert `weights.len() == children.len()` at every container.

**`test_weights_sync_on_insert`**
- Insert a view into an existing container.
- Assert `weights.len() == children.len()` and new weight is `1.0`.

**`test_weights_sync_on_remove`**
- Create 3 vertical splits (views A, B, C). Remove B.
- Assert container has 2 children and 2 weights.
- Assert A and C retain their original weights (not reset).

**`test_weights_sync_on_remove_collapse`**
- Create vertical split, then split one child horizontally (nested container).
- Remove the horizontal sibling so the container collapses.
- Assert parent weights are consistent after merge-up.

**`test_weights_sync_on_swap_same_parent`**
- Create 3 vertical splits. Set weights to `[2.0, 1.0, 1.0]`.
- Swap children 0 and 1 via `swap_split_in_direction`.
- Assert weights are now `[1.0, 2.0, 1.0]` (weights follow their views).

**`test_weights_sync_on_swap_cross_parent`**
- Create a layout with two containers (e.g., vertical split where one child is
  a horizontal sub-container).
- Set different weights in each container.
- Swap a view from one container to the other.
- Assert weights transferred correctly between parents.

### 1.2 Proportional Layout (`recalculate`)

**`test_equal_weights_match_equal_distribution`**
- Create 3 vertical splits in 180x80 area with all weights 1.0.
- Call `recalculate()`.
- Assert view widths match the existing `all_vertical_views_have_same_width`
  behavior (backwards compatibility).

**`test_weighted_vertical_distribution`**
- Create 2 vertical splits in 181x80 area. Set weights to `[2.0, 1.0]`.
- Call `recalculate()`.
- Assert first view gets ~2/3 of usable width, second gets ~1/3.
- Account for 1px gap between splits.

**`test_weighted_horizontal_distribution`**
- Create 2 horizontal splits in 180x90 area. Set weights to `[1.0, 2.0]`.
- Call `recalculate()`.
- Assert first view gets ~1/3 of height, second gets ~2/3.

**`test_last_child_absorbs_rounding`**
- Create 3 vertical splits in 100x80 area. Set weights to `[1.0, 1.0, 1.0]`.
- Assert last child's width == `container.x + container.width - child.x`
  (no pixel gaps or overlaps).

**`test_nested_weighted_layout`**
- Create vertical split [A, sub-container]. Sub-container has horizontal
  split [B, C].
- Set outer weights to `[3.0, 1.0]`, inner weights to `[1.0, 2.0]`.
- Assert A gets 3/4 width, sub-container gets 1/4 width.
- Within sub-container, B gets 1/3 height, C gets 2/3 height.

**`test_vertical_gap_count_fix`**
- Create 3 vertical splits in 100x80 area with equal weights.
- Assert total of all view widths + gaps == container width.
- Specifically validates the `saturating_sub(1)` fix (n children → n-1 gaps).

### 1.3 Minimum Size Enforcement

**`test_min_width_enforced`**
- Create 2 vertical splits in 30x80 area. Set weights to `[10.0, 0.1]`.
- Assert second view width >= `MIN_SPLIT_WIDTH` (10).

**`test_min_height_enforced`**
- Create 2 horizontal splits in 180x10 area. Set weights to `[10.0, 0.1]`.
- Assert second view height >= `MIN_SPLIT_HEIGHT` (3).

### 1.4 Resize API

**`test_resize_view_grows_right`**
- Create 2 vertical splits, equal weights. Call `resize_view(Right, 0.4)`.
- Assert left view weight increased, right view weight decreased.
- Assert left view area is wider than right view area after `recalculate()`.

**`test_resize_view_shrinks_left`**
- Create 2 vertical splits, equal weights. Call `resize_view(Left, 0.4)`.
- Assert left view weight decreased, right view weight increased.

**`test_resize_view_grows_down`**
- Create 2 horizontal splits. Call `resize_view(Down, 0.4)`.
- Assert top view weight increased.

**`test_resize_view_no_op_at_edge`**
- Create 2 vertical splits. Focus the rightmost view.
- Call `resize_view(Right, 0.4)`.
- Assert weights unchanged (no sibling to the right).

**`test_resize_view_no_op_single_view`**
- Single view tree. Call `resize_view(Right, 0.4)`.
- Assert no panic, no change.

**`test_resize_view_respects_min_weight`**
- Create 2 vertical splits. Set weights to `[1.0, 0.2]`.
- Call `resize_view(Right, 5.0)` (tries to transfer more than available).
- Assert sibling weight doesn't go below 0.1.

**`test_resize_view_walks_up_tree`**
- Create a horizontal split [top, bottom]. Top has a vertical sub-split
  [A, B]. Focus A.
- Call `resize_view(Down, 0.4)` — should walk up past the vertical container
  to find the horizontal parent and grow the top container.
- Assert top container's weight increased relative to bottom.

**`test_resize_with_count`**
- Call `resize_view(Right, 0.6)` (simulating count=3 × 0.2 step).
- Assert weight transfer is 0.6, not 0.2.

### 1.5 Equalize

**`test_equalize_splits_resets_weights`**
- Create 3 vertical splits. Set weights to `[3.0, 0.5, 1.5]`.
- Call `equalize_splits()`.
- Assert all weights are `1.0`.

**`test_equalize_splits_recursive`**
- Create nested containers with varied weights at multiple levels.
- Call `equalize_splits()`.
- Assert all containers at all levels have uniform weights.

### 1.6 Zoom

**`test_zoom_inflates_focused_view`**
- Create 3 vertical splits. Focus the middle view. Call `toggle_zoom()`.
- Assert middle view's area is significantly larger than siblings.
- Assert `is_zoomed() == true`.

**`test_zoom_nested`**
- Create vertical split [A, sub-container[B, C]]. Focus B.
- Call `toggle_zoom()`.
- Assert B dominates area at both the inner (horizontal) and outer (vertical)
  container levels.

**`test_unzoom_restores_exact_weights`**
- Create 3 vertical splits. Set weights to `[2.0, 1.0, 3.0]`.
- Call `toggle_zoom()` then `toggle_zoom()` again.
- Assert weights restored to exactly `[2.0, 1.0, 3.0]`.

**`test_zoom_single_view_no_op`**
- Single view tree. Call `toggle_zoom()`.
- Assert no panic, `is_zoomed()` is true (or no-op — decide on behavior).

**`test_zoom_then_remove_unzooms_first`**
- Create 2 splits, zoom one. Remove a view.
- Assert zoom state is cleared and remaining view has correct area.

**`test_zoom_then_split_unzooms_first`**
- Create 2 splits, zoom one. Split the focused view.
- Assert zoom state is cleared and new split has equal weights.

**`test_zoom_preserves_tree_traversal`**
- Create 3 splits, zoom the middle. Iterate `tree.views()`.
- Assert all 3 views are still yielded (zoom doesn't hide views).

### 1.7 Terminal Resize

**`test_terminal_resize_preserves_proportions`**
- Create 2 vertical splits with weights `[2.0, 1.0]` in 180x80.
- Call `tree.resize(Rect::new(0, 0, 120, 60))`.
- Assert weight ratio is unchanged and areas are proportionally scaled.

---

## Layer 2: Integration Tests (`helix-term/tests/test/splits.rs`)

These tests use `test_key_sequence` / `test_key_sequences` with full editor
simulation. They validate that commands, keybindings, and typed commands work
end-to-end.

### 2.1 Resize Keybindings

**`test_split_resize_width`**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_split_resize_width() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequence(
        &mut app,
        // Create vertical split, grow width of left pane
        Some("<C-w>v<C-w>h<C-w>>"),
        Some(&|app| {
            let views: Vec<_> = app.editor.tree.views().collect();
            assert_eq!(2, views.len());
            // Left view should be wider than right view
            assert!(views[0].0.area.width > views[1].0.area.width);
        }),
        false,
    ).await?;
    Ok(())
}
```

**`test_split_shrink_width`**
- Same setup, use `<C-w><` instead. Assert left view is narrower.

**`test_split_resize_height`**
- Create horizontal split with `:sp<ret>`, use `<C-w>+` and `<C-w>-`.
- Assert height changes accordingly.

**`test_split_resize_with_count`**
- Use `3<C-w>>` to grow by 3 steps. Assert larger change than single step.

### 2.2 Equalize

**`test_equalize_splits_keybinding`**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_equalize_splits_keybinding() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequence(
        &mut app,
        // Split, resize, then equalize
        Some("<C-w>v<C-w>>><C-w>="),
        Some(&|app| {
            let views: Vec<_> = app.editor.tree.views().collect();
            assert_eq!(2, views.len());
            // Views should have equal or near-equal widths (within 1px rounding)
            let diff = views[0].0.area.width.abs_diff(views[1].0.area.width);
            assert!(diff <= 1, "widths differ by {diff}");
        }),
        false,
    ).await?;
    Ok(())
}
```

**`test_equalize_splits_typed_command`**
- Same flow but use `:equalize-splits<ret>` instead of `<C-w>=`.

### 2.3 Zoom

**`test_zoom_toggle`**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_zoom_toggle() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>z"),
                Some(&|app| {
                    assert!(app.editor.tree.is_zoomed());
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len());
                    // Focused view should dominate the area
                    let (focused, _) = views.iter()
                        .find(|(_, is_focused)| *is_focused).unwrap();
                    let (other, _) = views.iter()
                        .find(|(_, is_focused)| !*is_focused).unwrap();
                    assert!(focused.area.width > other.area.width * 5);
                }),
            ),
            (
                Some("<C-w>z"),
                Some(&|app| {
                    assert!(!app.editor.tree.is_zoomed());
                    let views: Vec<_> = app.editor.tree.views().collect();
                    let diff = views[0].0.area.width.abs_diff(views[1].0.area.width);
                    assert!(diff <= 1, "widths should be equal after unzoom");
                }),
            ),
        ],
        false,
    ).await?;
    Ok(())
}
```

**`test_zoom_typed_command`**
- Use `:toggle-zoom<ret>` and `:zoom<ret>` alias.

**`test_zoom_then_close_split`**
- Zoom, then `:q<ret>`. Assert unzoomed and single view remains.

**`test_zoom_then_split`**
- Zoom, then `<C-w>v`. Assert unzoomed and 3 views with equal weights.

### 2.4 Resize + Zoom Interaction

**`test_resize_then_zoom_then_unzoom_restores`**
- Resize a split, then zoom, then unzoom.
- Assert the pre-zoom resize is preserved after unzoom.

### 2.5 Backwards Compatibility

**`test_existing_split_tests_still_pass`**
- No new test needed — the existing `test_split_write_quit_all`,
  `test_split_write_quit_same_file`, `test_changes_in_splits_apply_to_all_views`,
  and `test_changes_in_splits_jumplist_sync` must continue passing without
  modification. This validates that default weights (1.0) produce identical
  behavior to the old equal-distribution code.

---

## Test Execution

```bash
# Unit tests (fast, no feature gate)
cargo test -p helix-view tree::tests

# Integration tests (requires feature flag)
cargo test -p helix-term --features integration splits
```

---

## Coverage Matrix

| Feature | Unit | Integration |
|---------|------|-------------|
| Weight init / sync | 6 tests | — |
| Proportional layout | 6 tests | — |
| Minimum size | 2 tests | — |
| Resize API | 8 tests | 4 tests |
| Equalize | 2 tests | 2 tests |
| Zoom / Unzoom | 7 tests | 4 tests |
| Resize + Zoom interaction | — | 1 test |
| Terminal resize | 1 test | — |
| Backwards compat | existing tests | 4 existing tests |
| **Total** | **32 tests** | **11 tests** |
