mod file_tree_steps;

// Include the shared integration-test helpers so file_tree_steps can call
// test_key_sequence, AppBuilder, and run_event_loop_until_idle.
#[path = "test/helpers.rs"]
pub mod helpers;

use cucumber::World as _;
use file_tree_steps::FileTreeWorld;

#[tokio::main]
async fn main() {
    // CARGO_MANIFEST_DIR is the helix-term crate root inside the worktree at:
    //   /home/marbonestu/projects/helix-file-tree/helix-term
    // The specs live in the main helix checkout:
    //   /home/marbonestu/projects/helix/specs/file-tree
    // Navigating: helix-term → helix-file-tree → projects → helix/specs/file-tree
    let features_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../helix/specs/file-tree"
    );

    // Run scenarios serially to avoid HELIX_WORKSPACE env-var conflicts between
    // concurrent live-editor scenarios.
    FileTreeWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run(features_dir)
        .await;
}
