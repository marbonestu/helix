/// Step definitions for file-watching.feature.
///
/// These steps work at the library level (using `world.tree` directly) to
/// avoid spinning up a full Application. File system events are injected
/// manually via the `update_tx` channel so tests are deterministic and do not
/// depend on inotify/kqueue delivery timing.
use cucumber::{given, then, when};
use helix_view::file_tree::FileTreeUpdate;

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given("the file tree shows the project root")]
fn tree_shows_project_root(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
}

#[given("the src/ directory is expanded in the tree")]
fn src_dir_is_expanded(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let config = world.tree_config.clone();
    if let Some(tree) = world.tree.as_mut() {
        let src_id = tree
            .nodes()
            .iter()
            .find(|(_, n)| n.name == "src")
            .map(|(id, _)| id);
        if let Some(id) = src_id {
            tree.expand_sync(id, &config);
        }
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

/// Create a new file externally and inject an ExternalChange event so
/// `process_updates` can pick it up on the next render cycle.
#[when(expr = "a new file {string} is created externally in the root directory")]
fn new_file_in_root(world: &mut FileTreeWorld, name: String) {
    let root = world.workspace_dir.path().to_path_buf();
    std::fs::write(root.join(&name), "").expect("failed to create test file");
    if let Some(tree) = world.tree.as_ref() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::ExternalChange { dir: root });
    }
}

/// Create a new file inside src/ and inject an ExternalChange for that dir.
#[when(expr = "a new file {string} is created externally in the src\\/ directory")]
fn new_file_in_src(world: &mut FileTreeWorld, name: String) {
    let src = world.workspace_dir.path().join("src");
    std::fs::write(src.join(&name), "").expect("failed to create test file in src");
    if let Some(tree) = world.tree.as_ref() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::ExternalChange { dir: src });
    }
}

/// Delete a file externally and inject an ExternalChange for the parent dir.
#[when(expr = "{string} is deleted externally")]
fn file_deleted_externally(world: &mut FileTreeWorld, name: String) {
    let root = world.workspace_dir.path().to_path_buf();
    let path = root.join(&name);
    if path.is_file() {
        std::fs::remove_file(&path).expect("failed to delete test file");
    }
    if let Some(tree) = world.tree.as_ref() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::ExternalChange { dir: root });
    }
}

/// Create a new directory externally and inject an ExternalChange for root.
#[when(expr = "a new directory {string} is created externally in the root directory")]
fn new_dir_in_root(world: &mut FileTreeWorld, name: String) {
    let root = world.workspace_dir.path().to_path_buf();
    let dir_name = name.trim_end_matches('/');
    std::fs::create_dir(root.join(dir_name)).expect("failed to create test directory");
    if let Some(tree) = world.tree.as_ref() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::ExternalChange { dir: root });
    }
}

/// Create multiple files rapidly — all events coalesce into one ExternalChange
/// because the watcher debounces them. We simulate this by sending a single
/// update message after all file system operations are complete.
#[when(expr = "{int} files are created externally in rapid succession in the root directory")]
fn many_files_in_root(world: &mut FileTreeWorld, count: usize) {
    let root = world.workspace_dir.path().to_path_buf();
    for i in 0..count {
        std::fs::write(root.join(format!("rapid_{i}.rs")), "")
            .expect("failed to create rapid test file");
    }
    // One coalesced change event, matching the debouncer behaviour.
    if let Some(tree) = world.tree.as_ref() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::ExternalChange { dir: root });
    }
}

/// Simulate the render-cycle drain.
///
/// `ExternalChange` handling calls `spawn_load_children`, which is
/// asynchronous: it schedules a blocking task that sends `ChildrenLoaded`
/// back on the channel. We call `process_updates` once to trigger the
/// spawn, sleep briefly to let the background task complete, then call
/// `process_updates` a second time to integrate the `ChildrenLoaded`
/// result — matching what the render loop does across two frames.
#[when("the file tree processes pending updates")]
fn process_pending_updates(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    if let Some(tree) = world.tree.as_mut() {
        tree.process_updates(&config, None);
        std::thread::sleep(std::time::Duration::from_millis(100));
        tree.process_updates(&config, None);
    }
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(expr = "{string} appears in the file tree")]
fn file_appears(world: &mut FileTreeWorld, name: String) {
    let clean = name.trim_end_matches('/');
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree
        .nodes()
        .values()
        .any(|n| n.name == clean);
    assert!(found, "expected '{clean}' to appear in the file tree");
}

#[then(expr = "{string} appears under src\\/ in the file tree")]
fn file_appears_under_src(world: &mut FileTreeWorld, name: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let src_id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == "src")
        .map(|(id, _)| id);
    let found = src_id
        .and_then(|sid| {
            tree.nodes()
                .iter()
                .find(|(_, n)| n.parent == Some(sid) && n.name == name)
        })
        .is_some();
    assert!(found, "expected '{name}' to appear under src/ in the file tree");
}

#[then(expr = "{string} no longer appears in the file tree")]
fn file_gone(world: &mut FileTreeWorld, name: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree.nodes().values().any(|n| n.name == name);
    assert!(!found, "expected '{name}' to be gone from the file tree, but it is still present");
}

#[then(expr = "all {int} new files appear in the file tree")]
fn all_rapid_files_appear(world: &mut FileTreeWorld, count: usize) {
    let tree = world.tree.as_ref().expect("no FileTree");
    for i in 0..count {
        let name = format!("rapid_{i}.rs");
        let found = tree.nodes().values().any(|n| n.name == name);
        assert!(found, "expected '{name}' to appear in the file tree after rapid creation");
    }
}
