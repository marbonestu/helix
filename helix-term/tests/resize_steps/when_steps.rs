/// When step definitions for resize BDD scenarios.
///
/// Steps send key sequences to the live editor and capture before/after state
/// so Then steps can make relative assertions.
use cucumber::when;

use super::ResizeWorld;

// ---------------------------------------------------------------------------
// Width resize — named variants (for readable Gherkin)
// ---------------------------------------------------------------------------

#[when(expr = "Alex presses {string} to grow the focused split's width")]
async fn press_to_grow_width(world: &mut ResizeWorld, keys: String) {
    capture_before(world);
    send_keys(world, &keys).await;
    capture_after(world);
}

#[when(expr = "Alex presses {string} to shrink the focused split's width")]
async fn press_to_shrink_width(world: &mut ResizeWorld, keys: String) {
    capture_before(world);
    send_keys(world, &keys).await;
    capture_after(world);
}

#[when(expr = "Alex presses {string} to grow the focused split's height")]
async fn press_to_grow_height(world: &mut ResizeWorld, keys: String) {
    capture_before(world);
    send_keys(world, &keys).await;
    capture_after(world);
}

#[when(expr = "Alex presses {string} to shrink the focused split's height")]
async fn press_to_shrink_height(world: &mut ResizeWorld, keys: String) {
    capture_before(world);
    send_keys(world, &keys).await;
    capture_after(world);
}

// ---------------------------------------------------------------------------
// Generic key dispatch
// ---------------------------------------------------------------------------

#[when(expr = "Alex presses {string}")]
async fn alex_presses(world: &mut ResizeWorld, keys: String) {
    // Capture sidebar width before the action so sidebar Then steps can compare.
    world.sidebar_width_before = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
    world.before_widths = world.collect_view_widths();
    world.before_heights = world.collect_view_heights();

    send_keys(world, &keys).await;

    world.sidebar_width_after = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
    world.after_widths = world.collect_view_widths();
    world.after_heights = world.collect_view_heights();
}

// ---------------------------------------------------------------------------
// Explicit capture step
// ---------------------------------------------------------------------------

#[when("Alex captures the split widths")]
fn capture_split_widths(world: &mut ResizeWorld) {
    world.before_widths = world.collect_view_widths();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn capture_before(world: &mut ResizeWorld) {
    world.before_widths = world.collect_view_widths();
    world.before_heights = world.collect_view_heights();
    world.sidebar_width_before = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
}

fn capture_after(world: &mut ResizeWorld) {
    world.after_widths = world.collect_view_widths();
    world.after_heights = world.collect_view_heights();
    world.sidebar_width_after = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.width)
        .unwrap_or(0);
}

async fn send_keys(world: &mut ResizeWorld, keys: &str) {
    if let Some(app) = world.app.as_mut() {
        crate::helpers::test_key_sequence(app, Some(keys), None, false)
            .await
            .unwrap_or_else(|e| panic!("key sequence {keys:?} failed: {e}"));
    }
}
