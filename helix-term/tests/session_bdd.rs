mod session_steps;

// Include the shared integration-test helpers so session_steps::live_editor
// can call test_key_sequences, AppBuilder, and run_event_loop_until_idle.
#[path = "test/helpers.rs"]
pub mod helpers;

use cucumber::World as _;
use session_steps::SessionWorld;

#[tokio::main]
async fn main() {
    // CARGO_MANIFEST_DIR is the helix-term crate root, baked in at compile time.
    // From there, the feature files live at ../../../../specs/session-persistence
    // relative to the worktree layout (.claude/worktrees/<id>/helix-term/).
    let features_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../specs/session-persistence"
    );

    // Run scenarios serially to avoid HELIX_WORKSPACE env-var conflicts between
    // concurrent live-editor scenarios.
    SessionWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run(features_dir)
        .await;
}
