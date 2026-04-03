mod session_steps;

#[path = "test/helpers.rs"]
pub mod helpers;

use cucumber::World as _;
use session_steps::SessionWorld;

#[tokio::main]
async fn main() {
    let features_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../specs/resizable-splits"
    );

    SessionWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run(features_dir)
        .await;
}
