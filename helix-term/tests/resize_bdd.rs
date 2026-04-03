mod resize_steps;

#[path = "test/helpers.rs"]
pub mod helpers;

use cucumber::World as _;
use resize_steps::ResizeWorld;

#[tokio::main]
async fn main() {
    let features_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../specs/resizable-splits"
    );

    ResizeWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run(features_dir)
        .await;
}
