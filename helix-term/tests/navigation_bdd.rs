mod navigation_steps;

// Include the shared integration-test helpers so navigation_steps can call
// AppBuilder, test_key_sequences, and run_event_loop_until_idle.
#[path = "test/helpers.rs"]
pub mod helpers;

use cucumber::World as _;
use navigation_steps::NavigationWorld;

#[tokio::main]
async fn main() {
    // CARGO_MANIFEST_DIR is helix-term; specs live one level up in helix root.
    let features_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../specs/navigation");

    NavigationWorld::cucumber()
        .max_concurrent_scenarios(1)
        .after(|_feature, _rule, _scenario, _ev, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.close_app().await;
                }
            })
        })
        .run(features_dir)
        .await;
}
