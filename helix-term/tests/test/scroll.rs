use super::*;

use helix_term::config::Config;
use helix_view::current_ref;

/// Build a config with a known scrolloff and optional soft-wrap so the
/// horizontal-scroll math is deterministic regardless of the test terminal size.
fn scroll_config(scrolloff: usize, soft_wrap: bool) -> Config {
    let mut config = helpers::test_config();
    config.editor.scrolloff = scrolloff;
    config.editor.soft_wrap.enable = Some(soft_wrap);
    config
}

/// A single line that is comfortably wider than any gutter so horizontal
/// offsets are meaningful, with the cursor on the first column.
const LONG_LINE: &str = "#[a|]#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// `scroll_right` (bound to `l` in view mode) advances the horizontal offset and
/// drags the cursor along once it would leave the viewport. With `scrolloff = 0`
/// the cursor sits exactly on the left edge, so after N steps both the offset and
/// the cursor column are N.
#[tokio::test(flavor = "multi_thread")]
async fn scroll_right_advances_offset_and_drags_cursor() -> anyhow::Result<()> {
    test_key_sequence(
        &mut AppBuilder::new()
            .with_config(scroll_config(0, false))
            .with_input_text(LONG_LINE)
            .build()?,
        // Sticky view mode, then three right-scrolls.
        Some("Zlll"),
        Some(&|app| {
            let (view, doc) = current_ref!(app.editor);
            let offset = doc.view_offset(view.id);
            assert_eq!(offset.horizontal_offset, 3, "offset should advance by 3");
            let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
            assert_eq!(cursor, 3, "cursor should be dragged to the left edge");
        }),
        false,
    )
    .await?;

    Ok(())
}

/// `scroll_left` (bound to `h` in view mode) walks the horizontal offset back
/// towards zero, saturating at zero rather than underflowing.
#[tokio::test(flavor = "multi_thread")]
async fn scroll_left_reduces_offset_and_saturates_at_zero() -> anyhow::Result<()> {
    test_key_sequence(
        &mut AppBuilder::new()
            .with_config(scroll_config(0, false))
            .with_input_text(LONG_LINE)
            // Scroll right five, then left two: offset should land on 3.
            .build()?,
        Some("Zlllllhh"),
        Some(&|app| {
            let (view, doc) = current_ref!(app.editor);
            assert_eq!(doc.view_offset(view.id).horizontal_offset, 3);
        }),
        false,
    )
    .await?;

    // Scrolling left past the start clamps at zero (no underflow panic).
    test_key_sequence(
        &mut AppBuilder::new()
            .with_config(scroll_config(0, false))
            .with_input_text(LONG_LINE)
            .build()?,
        Some("Zlhhh"),
        Some(&|app| {
            let (view, doc) = current_ref!(app.editor);
            assert_eq!(doc.view_offset(view.id).horizontal_offset, 0);
        }),
        false,
    )
    .await?;

    Ok(())
}

/// Horizontal scrolling is a no-op while soft-wrap is enabled, since wrapped text
/// has no horizontal offset.
#[tokio::test(flavor = "multi_thread")]
async fn horizontal_scroll_is_noop_with_soft_wrap() -> anyhow::Result<()> {
    test_key_sequence(
        &mut AppBuilder::new()
            .with_config(scroll_config(0, true))
            .with_input_text(LONG_LINE)
            .build()?,
        Some("Zllll"),
        Some(&|app| {
            let (view, doc) = current_ref!(app.editor);
            assert_eq!(doc.view_offset(view.id).horizontal_offset, 0);
            let cursor = doc.selection(view.id).primary().cursor(doc.text().slice(..));
            assert_eq!(cursor, 0, "cursor must not move under soft-wrap");
        }),
        false,
    )
    .await?;

    Ok(())
}
