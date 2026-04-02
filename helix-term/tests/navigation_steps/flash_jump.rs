use cucumber::{then, when};

use super::NavigationWorld;

// ---------------------------------------------------------------------------
// When steps — flash-jump-specific sequences
// ---------------------------------------------------------------------------

#[when(regex = r#"Alex presses "([^"]+)", types "([^"]+)", then presses Backspace"#)]
async fn when_press_type_backspace(
    world: &mut NavigationWorld,
    binding: String,
    chars: String,
) -> anyhow::Result<()> {
    world
        .send_keys(&format!("{binding}{chars}<backspace>"))
        .await?;
    world.capture_state();
    Ok(())
}

#[when(regex = r#"Alex enters select mode, presses "([^"]+)", and types "([^"]+)""#)]
async fn when_select_mode_press_type(
    world: &mut NavigationWorld,
    binding: String,
    chars: String,
) -> anyhow::Result<()> {
    // Enter select mode with 'v', then use the binding and type the chars.
    world.send_keys(&format!("v{binding}{chars}")).await?;
    world.capture_state();
    Ok(())
}

// ---------------------------------------------------------------------------
// Then steps — flash-jump-specific assertions
// ---------------------------------------------------------------------------

#[then("the jumplist has grown by one entry")]
fn then_jumplist_grew(world: &mut NavigationWorld) {
    let before = world
        .jumplist_len_before
        .expect("jumplist_len_before not captured — was build_app called?");
    let after = world
        .jumplist_len_after
        .expect("jumplist_len_after not captured — did a When step run?");
    assert_eq!(
        after,
        before + 1,
        "expected jumplist to grow from {before} to {}, got {after}",
        before + 1
    );
}

#[then("the flash prompt is still active")]
fn then_flash_prompt_active(world: &mut NavigationWorld) {
    let cursor = world
        .result_cursor
        .expect("no cursor captured — did a When step run?");
    assert_eq!(
        cursor, 0,
        "expected flash prompt to still be active (cursor at 0), but cursor moved to {cursor}"
    );
}

#[then(regex = r"the selection anchor is at position (\d+)")]
fn then_anchor_at_position(world: &mut NavigationWorld, pos: usize) {
    let anchor = world
        .result_anchor
        .expect("no anchor captured — did a When step run?");
    assert_eq!(anchor, pos, "expected selection anchor at {pos}, got {anchor}");
}
