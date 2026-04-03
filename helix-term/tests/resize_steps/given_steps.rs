/// Given step definitions for resize BDD scenarios.
///
/// Steps set up the editor state (single split, vertical splits, horizontal
/// splits, sidebar open/focused) before the When action runs.
use cucumber::given;

use super::ResizeWorld;

// ---------------------------------------------------------------------------
// Editor launch
// ---------------------------------------------------------------------------

#[given("Alex has opened Helix editor")]
fn alex_opens_helix(world: &mut ResizeWorld) {
    if world.app.is_none() {
        let scratch = world.workspace_dir.path().join("scratch.txt");
        world.pending_files.push(scratch);
        world.build_app().expect("failed to build Application");
    }
}

// ---------------------------------------------------------------------------
// Split layouts
// ---------------------------------------------------------------------------

#[given("Alex has two vertical splits open side by side with equal widths")]
async fn two_vertical_splits(world: &mut ResizeWorld) {
    if world.app.is_none() {
        let scratch = world.workspace_dir.path().join("scratch.txt");
        world.pending_files.push(scratch);
        world.build_app().expect("failed to build Application");
    }
    let app = world.app.as_mut().unwrap();
    crate::helpers::test_key_sequence(app, Some(":vsplit<ret>"), None, false)
        .await
        .expect("failed to open vertical split");
}

#[given("Alex has two vertical splits with equal widths")]
async fn two_vertical_splits_alias(world: &mut ResizeWorld) {
    two_vertical_splits(world).await;
}

#[given("Alex has two horizontal splits stacked vertically with equal heights")]
async fn two_horizontal_splits(world: &mut ResizeWorld) {
    if world.app.is_none() {
        let scratch = world.workspace_dir.path().join("scratch.txt");
        world.pending_files.push(scratch);
        world.build_app().expect("failed to build Application");
    }
    let app = world.app.as_mut().unwrap();
    crate::helpers::test_key_sequence(app, Some(":hsplit<ret>"), None, false)
        .await
        .expect("failed to open horizontal split");
}

#[given("Alex has only one split open")]
fn single_split(world: &mut ResizeWorld) {
    if world.app.is_none() {
        let scratch = world.workspace_dir.path().join("scratch.txt");
        world.pending_files.push(scratch);
        world.build_app().expect("failed to build Application");
    }
}

#[given("Alex has two vertical splits and focus is on the rightmost split")]
async fn two_splits_focus_right(world: &mut ResizeWorld) {
    two_vertical_splits(world).await;
    // After :vsplit the new split is on the right and already focused.
}

#[given("Alex has two vertical splits and focus is on the leftmost split")]
async fn two_splits_focus_left(world: &mut ResizeWorld) {
    two_vertical_splits(world).await;
    // Move focus left so the leftmost split is active.
    let app = world.app.as_mut().unwrap();
    crate::helpers::test_key_sequence(app, Some("<C-w>h"), None, false)
        .await
        .expect("failed to move focus left");
}

// ---------------------------------------------------------------------------
// Sidebar state
// ---------------------------------------------------------------------------

#[given("the sidebar is open and focused")]
async fn sidebar_open_and_focused(world: &mut ResizeWorld) {
    if world.app.is_none() {
        let scratch = world.workspace_dir.path().join("scratch.txt");
        world.pending_files.push(scratch);
        world.build_app().expect("failed to build Application");
    }
    let is_visible = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.visible)
        .unwrap_or(false);
    if !is_visible {
        let app = world.app.as_mut().unwrap();
        crate::helpers::test_key_sequence(app, Some("<space>e"), None, false)
            .await
            .expect("failed to open sidebar");
    }
    if let Some(app) = world.app.as_mut() {
        app.editor.left_sidebar.focused = true;
    }
}

#[given("the sidebar is open but not focused")]
async fn sidebar_open_not_focused(world: &mut ResizeWorld) {
    if world.app.is_none() {
        let scratch = world.workspace_dir.path().join("scratch.txt");
        world.pending_files.push(scratch);
        world.build_app().expect("failed to build Application");
    }
    let is_visible = world
        .app
        .as_ref()
        .map(|a| a.editor.left_sidebar.visible)
        .unwrap_or(false);
    if !is_visible {
        let app = world.app.as_mut().unwrap();
        crate::helpers::test_key_sequence(app, Some("<space>e"), None, false)
            .await
            .expect("failed to open sidebar");
    }
    if let Some(app) = world.app.as_mut() {
        app.editor.left_sidebar.focused = false;
    }
}

#[given(expr = "the sidebar width is {int} columns")]
fn sidebar_width_is(world: &mut ResizeWorld, width: u16) {
    if let Some(app) = world.app.as_mut() {
        app.editor.left_sidebar.width = width;
    }
}
