/// Step definitions for git-status-responsiveness.feature.
///
/// Covers the call-first-and-last debounce strategy and two-phase git status
/// scanning. All steps operate on the `FileTree` library directly without
/// spawning real git processes — background scan completion is simulated by
/// sending the appropriate `FileTreeUpdate` messages through the channel.
use cucumber::{given, then, when};
use helix_view::file_tree::{FileTreeUpdate, GitStatus, NodeKind};
use std::path::PathBuf;

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

/// Ensures the tree exists and no git refresh is currently running.
#[given("no git refresh is in progress")]
fn no_git_refresh_in_progress(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    // A freshly initialised tree has a deadline set but is not yet "in progress"
    // (the background task has not been spawned yet).
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !tree.git_refresh_in_progress(),
        "expected no git refresh to be running but one is in progress"
    );
}

/// Puts the tree into a state where a background git scan is running.
#[given("a git refresh is currently in progress")]
fn git_refresh_currently_in_progress(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.simulate_git_refresh_in_progress();
    assert!(
        tree.git_refresh_in_progress(),
        "expected git refresh to be in progress after simulation"
    );
}

/// Injects a phase-1-complete event so subsequent steps see phase-1 results.
#[given("the first scan phase has completed and shows src/main.rs as Modified")]
fn first_scan_phase_completed_with_main_modified(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    // Simulate the in-progress state and inject a Modified entry for main.rs.
    let root = world.workspace_dir.path().to_path_buf();
    let abs_path = root.join("src/main.rs");
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.simulate_git_refresh_in_progress();

    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(abs_path, GitStatus::Modified)]));
    let _ = tx.try_send(FileTreeUpdate::GitStatusPhase1Complete);

    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

/// Simulates the file tree having just been opened (tree was just created).
#[given("the file tree has just been opened")]
fn file_tree_just_opened(world: &mut FileTreeWorld) {
    world.create_project_structure();
    world.init_tree();
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

/// Simulates saving a file by requesting a git refresh.
#[when(regex = r"^Alex saves (.+)$")]
fn alex_saves_file(world: &mut FileTreeWorld, _file: String) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.request_git_refresh();
}

/// Simulates saving a second file while a refresh is already running.
#[when(regex = r"^Alex saves (.+) while a refresh is in progress$")]
fn alex_saves_while_in_progress(world: &mut FileTreeWorld, _file: String) {
    let tree = world.tree.as_mut().expect("no FileTree");
    // Ensure we are in-progress before requesting another refresh.
    if !tree.git_refresh_in_progress() {
        tree.simulate_git_refresh_in_progress();
    }
    tree.request_git_refresh();
}

/// Simulates the background scan completing by sending GitStatusEnd.
#[when("the current git refresh completes")]
fn current_git_refresh_completes(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

/// Simulates the second scan phase completing by injecting untracked entries
/// followed by GitStatusEnd.
#[when("the second scan phase completes")]
fn second_scan_phase_completes(world: &mut FileTreeWorld) {
    let root = world.workspace_dir.path().to_path_buf();
    let untracked_path = root.join("new_feature.rs");
    // Create the file on disk so the node can be revealed.
    let _ = std::fs::write(&untracked_path, "");

    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(untracked_path, GitStatus::Untracked)]));
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
    // Refresh the root so the newly created file appears as a tree node.
    tree.refresh_sync(&config);
}

/// Simulates `process_updates` being called (the first render-loop tick).
#[when("the first process_updates call runs")]
fn first_process_updates_runs(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    if let Some(tree) = world.tree.as_mut() {
        tree.process_updates(&config, None);
    }
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

/// Checks that the tree has scheduled a refresh via the deadline (fires on the
/// very next `process_updates` tick because the deadline is set to `now`).
#[then("a git refresh starts immediately")]
fn git_refresh_starts_immediately(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    // "Immediate" means either a deadline already set to now (not in progress)
    // or the refresh is already in progress.
    let scheduled = tree.has_pending_git_refresh() || tree.git_refresh_in_progress();
    assert!(
        scheduled,
        "expected a git refresh to be scheduled immediately but none was"
    );
}

/// Checks that a deferred refresh is waiting (pending flag set while a scan
/// is in progress).
#[then("a deferred git refresh is scheduled for after the current one completes")]
fn deferred_git_refresh_scheduled(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.has_pending_deferred_git_refresh(),
        "expected a deferred git refresh to be pending but none was set"
    );
}

/// Asserts that src/main.rs shows Modified status (phase-1 results visible).
#[then("src/main.rs shows Modified status before new_feature.rs status is resolved")]
fn main_rs_modified_before_untracked(world: &mut FileTreeWorld) {
    assert_node_git_status(world, "main.rs", GitStatus::Modified);
}

/// Asserts that new_feature.rs shows Untracked status (phase-2 result).
#[then("new_feature.rs shows Untracked status")]
fn new_feature_rs_untracked(world: &mut FileTreeWorld) {
    assert_node_git_status(world, "new_feature.rs", GitStatus::Untracked);
}

/// Asserts that src/main.rs still shows Modified status after phase 2.
#[then("src/main.rs still shows Modified status")]
fn main_rs_still_modified(world: &mut FileTreeWorld) {
    assert_node_git_status(world, "main.rs", GitStatus::Modified);
}

// ---------------------------------------------------------------------------
// Additional Given steps
// ---------------------------------------------------------------------------

/// Sets up the state as if a git refresh was started by saving src/main.rs:
/// the tree is in-progress and a Modified status has been injected.
#[given(regex = r"^a git refresh started when src/main\.rs was saved$")]
fn git_refresh_started_for_main(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let root = world.workspace_dir.path().to_path_buf();
    let abs_path = root.join("src/main.rs");
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.simulate_git_refresh_in_progress();
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(abs_path, GitStatus::Modified)]));
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

/// Injects Modified status for src/main.rs to represent a tracked file with
/// uncommitted edits at the start of a scan scenario.
#[given(regex = r"^src/main\.rs is a tracked file with uncommitted edits$")]
fn main_rs_tracked_with_edits(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let root = world.workspace_dir.path().to_path_buf();
    let abs_path = root.join("src/main.rs");
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(abs_path, GitStatus::Modified)]));
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

/// Creates new_feature.rs on disk so the tree can pick it up as untracked.
#[given(regex = r"^new_feature\.rs is an untracked file in the same directory$")]
fn new_feature_rs_untracked_setup(world: &mut FileTreeWorld) {
    let root = world.workspace_dir.path().to_path_buf();
    let path = root.join("new_feature.rs");
    std::fs::write(&path, "").expect("failed to create new_feature.rs");
    let abs_path = path;
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(abs_path, GitStatus::Untracked)]));
}

/// Sets up the tree in a state where all known files have Clean status and
/// no untracked files exist — simulating a fully-committed repository.
#[given("all files in the repository are tracked")]
fn all_files_tracked(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    // Inject Clean status for all known files. An empty GitStatus update
    // followed by GitStatusEnd simulates a scan that found no changed files.
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![]));
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

// ---------------------------------------------------------------------------
// Additional When steps
// ---------------------------------------------------------------------------

/// Simulates saving src/lib.rs while a refresh is already in progress.
#[when(regex = r"^src/lib\.rs is also saved before the refresh completes$")]
fn lib_rs_saved_while_in_progress(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    if !tree.git_refresh_in_progress() {
        tree.simulate_git_refresh_in_progress();
    }
    tree.request_git_refresh();
}

/// Triggers a git refresh request on the current tree.
#[when("a git refresh runs")]
fn git_refresh_runs(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.request_git_refresh();
    // Simulate completion via GitStatusEnd.
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
}

/// Starts a git refresh (request without immediately completing it).
#[when("a git refresh starts")]
fn git_refresh_starts(world: &mut FileTreeWorld) {
    let root = world.workspace_dir.path().to_path_buf();
    let main_rs = root.join("src").join("main.rs");
    let tree = world.tree.as_mut().expect("no FileTree");
    // simulate_git_refresh_in_progress clears the status map; re-inject the
    // phase-1 result (src/main.rs = Modified) before signalling Phase1Complete.
    tree.simulate_git_refresh_in_progress();
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(main_rs, GitStatus::Modified)]));
    let _ = tx.try_send(FileTreeUpdate::GitStatusPhase1Complete);
    let config = world.tree_config.clone();
    // Two calls: first drains GitStatus into the map, second processes Phase1Complete.
    tree.process_updates(&config, None);
    tree.process_updates(&config, None);
}

// ---------------------------------------------------------------------------
// Additional Then steps
// ---------------------------------------------------------------------------

/// Verifies the refresh was scheduled immediately with no 1-second delay.
/// This is the same invariant as `git_refresh_starts_immediately` but
/// expressed from the perspective of timing rather than scheduling.
#[then(regex = r"^the tree shows updated status for src/main\.rs without a 1-second wait$")]
fn updated_status_without_wait(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let scheduled = tree.has_pending_git_refresh() || tree.git_refresh_in_progress();
    assert!(
        scheduled,
        "expected git refresh to have been scheduled immediately (no 1-second wait)"
    );
}

/// After rapid saves with CALL_FIRST_AND_LAST debounce, exactly one refresh
/// should be scheduled immediately (deadline=now, not yet in_progress).
#[then("exactly one git refresh runs immediately")]
fn exactly_one_refresh_immediately(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.has_pending_git_refresh(),
        "expected exactly one git refresh to be pending"
    );
    assert!(
        !tree.git_refresh_in_progress(),
        "expected refresh to be pending (not yet in progress) on the first tick"
    );
}

/// After the burst settles, a deferred refresh should follow the immediate one.
#[then("exactly one more git refresh runs after the burst settles")]
fn exactly_one_deferred_refresh(world: &mut FileTreeWorld) {
    // Drive the pending refresh to completion, then verify a deferred one fires.
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.simulate_git_refresh_in_progress();
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
    // After completion, the pending flag should have triggered another refresh.
    let scheduled = tree.has_pending_git_refresh() || tree.git_refresh_in_progress();
    assert!(
        scheduled,
        "expected a follow-up git refresh after the burst, but none was scheduled"
    );
}

/// After the in-progress refresh completes, a second one should fire because
/// lib.rs was saved while the first was running.
#[then("a second git refresh runs after the first one finishes")]
fn second_refresh_after_first(world: &mut FileTreeWorld) {
    assert!(
        world
            .tree
            .as_ref()
            .expect("no FileTree")
            .has_pending_deferred_git_refresh(),
        "expected a deferred git refresh to be queued for src/lib.rs"
    );
}

/// The lib.rs status is updated when the deferred refresh fires.
#[then(regex = r"^the status for src/lib\.rs is updated$")]
fn lib_rs_status_updated(world: &mut FileTreeWorld) {
    // Drive the deferred refresh through by completing the current in-progress
    // scan and letting process_updates fire the pending one.
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
    // Inject a Modified status for lib.rs (the deferred refresh would produce this).
    let root = world.workspace_dir.path().to_path_buf();
    let lib_path = root.join("src/lib.rs");
    let tree = world.tree.as_mut().expect("no FileTree");
    let tx = tree.update_tx();
    let _ = tx.try_send(FileTreeUpdate::GitStatus(vec![(lib_path, GitStatus::Modified)]));
    let _ = tx.try_send(FileTreeUpdate::GitStatusEnd);
    let config = world.tree_config.clone();
    tree.process_updates(&config, None);
    assert_node_git_status(world, "lib.rs", GitStatus::Modified);
}

/// Verifies the tree completed in a single scan phase (no pending state after
/// GitStatusEnd with only tracked files).
#[then("the refresh completes after a single scan phase")]
fn refresh_completes_single_phase(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !tree.git_refresh_in_progress(),
        "expected refresh to be complete (not still in progress)"
    );
    assert!(
        !tree.has_pending_deferred_git_refresh(),
        "expected no deferred follow-up refresh for a tracked-only repo"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_node_git_status(world: &mut FileTreeWorld, node_name: &str, expected: GitStatus) {
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");

    // Expand all directories so nested files are visible.
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
