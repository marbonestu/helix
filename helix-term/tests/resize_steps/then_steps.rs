/// Then step definitions for resize BDD scenarios.
///
/// Steps assert on view widths, heights, and sidebar dimensions captured by
/// the When steps. Relative comparisons (wider/narrower/taller/shorter) use
/// the before/after snapshots stored in [`ResizeWorld`].
use cucumber::then;

use super::ResizeWorld;

// ---------------------------------------------------------------------------
// Split width assertions
// ---------------------------------------------------------------------------

#[then("the focused split should be wider than before")]
fn focused_split_wider(world: &mut ResizeWorld) {
    let idx = world.focused_view_index();
    let before = world.before_widths.get(idx).copied().unwrap_or(0);
    let after = world.after_widths.get(idx).copied().unwrap_or(0);
    assert!(
        after > before,
        "expected focused split (index {idx}) to be wider than before \
         (before={before}, after={after})"
    );
}

#[then("the focused split should be narrower than before")]
fn focused_split_narrower(world: &mut ResizeWorld) {
    let idx = world.focused_view_index();
    let before = world.before_widths.get(idx).copied().unwrap_or(0);
    let after = world.after_widths.get(idx).copied().unwrap_or(0);
    assert!(
        after < before,
        "expected focused split (index {idx}) to be narrower than before \
         (before={before}, after={after})"
    );
}

#[then("the adjacent sibling split should be narrower than before")]
fn sibling_narrower(world: &mut ResizeWorld) {
    let focused = world.focused_view_index();
    // Pick the first view index that is not the focused one.
    let sibling = if focused == 0 { 1 } else { 0 };
    let before = world.before_widths.get(sibling).copied().unwrap_or(0);
    let after = world.after_widths.get(sibling).copied().unwrap_or(0);
    assert!(
        after < before,
        "expected sibling split (index {sibling}) to be narrower than before \
         (before={before}, after={after})"
    );
}

#[then("the adjacent sibling split should be wider than before")]
fn sibling_wider(world: &mut ResizeWorld) {
    let focused = world.focused_view_index();
    let sibling = if focused == 0 { 1 } else { 0 };
    let before = world.before_widths.get(sibling).copied().unwrap_or(0);
    let after = world.after_widths.get(sibling).copied().unwrap_or(0);
    assert!(
        after > before,
        "expected sibling split (index {sibling}) to be wider than before \
         (before={before}, after={after})"
    );
}

// ---------------------------------------------------------------------------
// Split height assertions
// ---------------------------------------------------------------------------

#[then("the focused split should be taller than before")]
fn focused_split_taller(world: &mut ResizeWorld) {
    let idx = world.focused_view_index();
    let before = world.before_heights.get(idx).copied().unwrap_or(0);
    let after = world.after_heights.get(idx).copied().unwrap_or(0);
    assert!(
        after > before,
        "expected focused split (index {idx}) to be taller than before \
         (before={before}, after={after})"
    );
}

#[then("the focused split should be shorter than before")]
fn focused_split_shorter(world: &mut ResizeWorld) {
    let idx = world.focused_view_index();
    let before = world.before_heights.get(idx).copied().unwrap_or(0);
    let after = world.after_heights.get(idx).copied().unwrap_or(0);
    assert!(
        after < before,
        "expected focused split (index {idx}) to be shorter than before \
         (before={before}, after={after})"
    );
}

#[then("the sibling split below should be shorter than before")]
fn sibling_below_shorter(world: &mut ResizeWorld) {
    // The focused split grew, so the other split must have shrunk.
    let focused = world.focused_view_index();
    let sibling = if focused == 0 { 1 } else { 0 };
    let before = world.before_heights.get(sibling).copied().unwrap_or(0);
    let after = world.after_heights.get(sibling).copied().unwrap_or(0);
    assert!(
        after < before,
        "expected sibling split (index {sibling}) to be shorter than before \
         (before={before}, after={after})"
    );
}

#[then("the sibling split below should be taller than before")]
fn sibling_below_taller(world: &mut ResizeWorld) {
    let focused = world.focused_view_index();
    let sibling = if focused == 0 { 1 } else { 0 };
    let before = world.before_heights.get(sibling).copied().unwrap_or(0);
    let after = world.after_heights.get(sibling).copied().unwrap_or(0);
    assert!(
        after > before,
        "expected sibling split (index {sibling}) to be taller than before \
         (before={before}, after={after})"
    );
}

// ---------------------------------------------------------------------------
// No-change assertions
// ---------------------------------------------------------------------------

#[then("the split sizes remain unchanged")]
fn split_sizes_unchanged(world: &mut ResizeWorld) {
    assert_eq!(
        world.before_widths, world.after_widths,
        "expected split widths to be unchanged (before={:?}, after={:?})",
        world.before_widths, world.after_widths
    );
}

#[then("the split size remains unchanged and no error occurs")]
fn single_split_unchanged(world: &mut ResizeWorld) {
    // For a single split the width vector has one element; it must be unchanged.
    assert_eq!(
        world.before_widths, world.after_widths,
        "expected single split width to be unchanged (before={:?}, after={:?})",
        world.before_widths, world.after_widths
    );
}

// ---------------------------------------------------------------------------
// Sidebar width assertions
// ---------------------------------------------------------------------------

#[then(expr = "the sidebar should be {int} columns wide")]
fn sidebar_exact_width(world: &mut ResizeWorld, expected: u16) {
    let actual = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
    assert_eq!(
        actual, expected,
        "expected sidebar width {expected} but got {actual}"
    );
}

#[then(expr = "the sidebar should be wider than {int} columns")]
fn sidebar_wider_than(world: &mut ResizeWorld, threshold: u16) {
    let actual = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
    assert!(
        actual > threshold,
        "expected sidebar width > {threshold} but got {actual}"
    );
}

#[then(expr = "the sidebar should be narrower than {int} columns")]
fn sidebar_narrower_than(world: &mut ResizeWorld, threshold: u16) {
    let actual = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
    assert!(
        actual < threshold,
        "expected sidebar width < {threshold} but got {actual}"
    );
}

// ---------------------------------------------------------------------------
// Sidebar expand/collapse assertions
// ---------------------------------------------------------------------------

#[then("the sidebar should be expanded")]
fn sidebar_expanded(world: &mut ResizeWorld) {
    let expanded = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.expanded)
        .unwrap_or(false);
    assert!(expanded, "expected sidebar to be in expanded mode");
}

#[then("the sidebar should be collapsed")]
fn sidebar_collapsed(world: &mut ResizeWorld) {
    let expanded = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.expanded)
        .unwrap_or(true);
    assert!(!expanded, "expected sidebar to be collapsed (not expanded)");
}
