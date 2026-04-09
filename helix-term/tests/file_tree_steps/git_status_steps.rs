/// Step definitions for git-status.feature.
///
/// Tests the git status refresh lifecycle at the library level:
///   - initial refresh is scheduled on tree creation
///   - eager map clear before each refresh cycle
///   - Untracked vs Added distinction
///   - refresh triggered by filesystem operations
use std::time::{Duration, Instant};

use cucumber::{given, then, when};
use helix_view::file_tree::{FileTreeUpdate, GitStatus, NodeKind};

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given("Alex opens the file tree for the first time")]
fn alex_opens_file_tree(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
    }
    // Re-init so the tree is freshly created — request_git_refresh() runs in new().
    world.init_tree();
}


#[given("staged.rs has been staged for addition")]
fn staged_rs_added(world: &mut FileTreeWorld) {
    let root = world.workspace_dir.path().to_path_buf();
    std::fs::write(root.join("staged.rs"), "pub fn staged() {}").unwrap();
    if let Some(tree) = world.tree.as_mut() {
        let config = world.tree_config.clone();
        tree.refresh_sync(&config);
    }
    inject_git_status(world, "staged.rs", GitStatus::Added);
}

#[given("src/main.rs has been staged for commit")]
fn main_rs_staged(world: &mut FileTreeWorld) {
    inject_git_status(world, "src/main.rs", GitStatus::Staged);
}

#[given("src/main.rs was previously Modified")]
fn src_main_previously_modified(world: &mut FileTreeWorld) {
    inject_git_status(world, "src/main.rs", GitStatus::Modified);
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("the git refresh deadline has elapsed")]
fn git_deadline_elapsed(world: &mut FileTreeWorld) {
    // Force the deadline into the past so check_git_refresh_timer fires.
    if let Some(tree) = world.tree.as_mut() {
        tree.force_git_refresh_deadline_past();
    }
}

#[when("process_updates runs with diff providers")]
fn process_updates_with_providers(world: &mut FileTreeWorld) {
    // Inject a synthetic GitStatus message to simulate what the background
    // task sends. Real DiffProviderRegistry is not available in unit tests.
    inject_git_status(world, "src/main.rs", GitStatus::Modified);
}

#[when("a new file is created via the tree")]
fn new_file_created_via_tree(world: &mut FileTreeWorld) {
    let root = world.workspace_dir.path().to_path_buf();
    let new_path = root.join("created_by_tree.rs");
    std::fs::write(&new_path, "").unwrap();
    if let Some(tree) = world.tree.as_mut() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::FsOpCreatedFile {
            path: new_path,
            refresh_parent: root,
        });
        let config = world.tree_config.clone();
        tree.process_updates(&config, None);
    }
}

#[when("an external file change is detected in the project")]
fn external_change_detected(world: &mut FileTreeWorld) {
    let root = world.workspace_dir.path().to_path_buf();
    if let Some(tree) = world.tree.as_mut() {
        let tx = tree.update_tx();
        let _ = tx.try_send(FileTreeUpdate::ExternalChange { dir: root });
        let config = world.tree_config.clone();
        tree.process_updates(&config, None);
    }
}

#[when("a new git refresh cycle starts")]
fn new_git_refresh_cycle(world: &mut FileTreeWorld) {
    if let Some(tree) = world.tree.as_mut() {
        // Simulate spawn_git_status: clear the map eagerly, then inject
        // no new status for src/main.rs (it is now clean this cycle).
        tree.clear_git_status_map_for_test();
        let config = world.tree_config.clone();
        tree.process_updates(&config, None);
    }
}

#[when("src/main.rs is not reported by git this cycle")]
fn main_rs_not_reported(_world: &mut FileTreeWorld) {
    // No-op: the map was cleared in the previous step and no entry
    // for src/main.rs will be injected, leaving it Clean.
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("a git refresh is pending")]
fn git_refresh_is_pending(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.has_pending_git_refresh(),
        "expected a git refresh to be pending but none was scheduled"
    );
}

#[then("the git status map is populated")]
fn git_status_map_populated(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !tree.git_status_map_is_empty(),
        "expected git status map to be populated but it is empty"
    );
}

#[then("the row for new_file.rs has Untracked status")]
fn new_file_has_untracked(world: &mut FileTreeWorld) {
    assert_node_status(world, "new_file.rs", GitStatus::Untracked);
}

#[then("the row for staged.rs has Added status")]
fn staged_rs_has_added(world: &mut FileTreeWorld) {
    assert_node_status(world, "staged.rs", GitStatus::Added);
}

#[then("the row for src/main.rs has Staged status")]
fn main_rs_has_staged(world: &mut FileTreeWorld) {
    assert_node_status(world, "main.rs", GitStatus::Staged);
}

#[then("src/ has Conflict status")]
fn src_has_conflict(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    if let Some(tree) = world.tree.as_mut() {
        tree.process_updates(&config, None);
    }
    let tree = world.tree.as_ref().expect("no FileTree");
    let id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == "src" && n.kind == NodeKind::Directory)
        .map(|(id, _)| id);
    let Some(id) = id else {
        panic!("src/ directory not found in tree");
    };
    let status = tree.git_status_for(id);
    assert_eq!(
        status,
        GitStatus::Conflict,
        "expected src/ to have Conflict status but got {:?}",
        status
    );
}

#[then("src/main.rs has Clean status after the refresh")]
fn main_rs_clean_after_refresh(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    if let Some(tree) = world.tree.as_mut() {
        // Expand src/ so main.rs is loaded.
        let src_id = tree
            .nodes()
            .iter()
            .find(|(_, n)| n.name == "src")
            .map(|(id, _)| id);
        if let Some(id) = src_id {
            tree.expand_sync(id, &config);
        }
        tree.process_updates(&config, None);
    }
    assert_node_status(world, "main.rs", GitStatus::Clean);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn inject_git_status(world: &mut FileTreeWorld, relative_path: &str, status: GitStatus) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let root = world.workspace_dir.path().to_path_buf();
    let abs_path = root.join(relative_path);
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(abs_path, status)]));
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

fn assert_node_status(world: &mut FileTreeWorld, node_name: &str, expected: GitStatus) {
    let config = world.tree_config.clone();

    let tree = world.tree.as_mut().expect("no FileTree");

    // Expand all top-level directories so their children are loaded.
    let dir_ids: Vec<_> = tree
        .nodes()
        .iter()
        .filter(|(_, n)| n.kind == NodeKind::Directory && n.parent.is_some())
        .map(|(id, _)| id)
        .collect();
    for id in dir_ids {
        tree.expand_sync(id, &config);
    }
    tree.process_updates(&config, None);

    let id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == node_name)
        .map(|(id, _)| id);

    let Some(id) = id else {
        if expected == GitStatus::Clean {
            return;
        }
        panic!("node '{node_name}' not found in tree");
    };

    let actual = tree.git_status_for(id);
    assert_eq!(
        actual, expected,
        "expected '{node_name}' to have git status {:?} but got {:?}",
        expected, actual
    );
}
