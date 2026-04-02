use cucumber::{then, when};

use super::NavigationWorld;

// ---------------------------------------------------------------------------
// When steps — flash-search-specific sequences
// ---------------------------------------------------------------------------

#[when(regex = r#"Alex presses "([^"]+)""#)]
async fn when_press(world: &mut NavigationWorld, binding: String) -> anyhow::Result<()> {
    world.send_keys(&binding).await?;
    world.capture_state();
    Ok(())
}

/// Handles: `Alex presses "/", types "fn", types "b", then presses "n"`
#[when(
    regex = r#"Alex presses "([^"]+)", types "([^"]+)", types "([^"]+)", then presses "([^"]+)""#
)]
async fn when_press_type_type_press(
    world: &mut NavigationWorld,
    binding: String,
    first: String,
    second: String,
    last: String,
) -> anyhow::Result<()> {
    world
        .send_keys(&format!("{binding}{first}{second}{last}"))
        .await?;
    world.capture_state();
    Ok(())
}

// ---------------------------------------------------------------------------
// Then steps — flash-search-specific assertions
// ---------------------------------------------------------------------------

#[then(regex = r#"the status line shows "([^"]+)""#)]
fn then_status_shows(world: &mut NavigationWorld, expected: String) {
    let status = world.result_status.as_deref().unwrap_or("");
    assert!(
        status.contains(&expected),
        "expected status to contain \"{expected}\", got \"{status}\""
    );
}

#[then(regex = r#"the search register contains "([^"]+)""#)]
fn then_register_contains(world: &mut NavigationWorld, expected: String) {
    let reg = world.result_register.as_deref().unwrap_or("");
    assert_eq!(
        reg, expected,
        "expected search register \"{expected}\", got \"{reg}\""
    );
}
