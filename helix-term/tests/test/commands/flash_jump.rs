use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_escape_cancels() -> anyhow::Result<()> {
    // 'e' appears multiple times so labels are shown (no auto-jump).
    // Escape cancels and restores the original selection.
    test((
        "#[|h]#ello here end\n",
        "gSe<esc>",
        "#[|h]#ello here end\n",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_single_match_autojump() -> anyhow::Result<()> {
    // Only one 'w' in the viewport -> single match -> auto-jump
    test((
        "#[|h]#ello world\n",
        "gSw",
        "hello #[w|]#orld\n",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_narrow_then_autojump() -> anyhow::Result<()> {
    // 'h' matches at two positions, but "ha" narrows to one -> auto-jump
    test((
        "xyz #[|h]#ello hab end\n",
        "gSha",
        "xyz hello #[h|]#ab end\n",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_no_matches_closes() -> anyhow::Result<()> {
    // First 'z' has no matches -> "No matches" and cursor stays
    test((
        "#[|h]#ello world\n",
        "gSz",
        "#[|h]#ello world\n",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_narrow_to_zero() -> anyhow::Result<()> {
    // 'h' has matches, but "hz" has zero -> closes
    test((
        "xyz #[|h]#ello hab end\n",
        "gShz",
        "xyz #[|h]#ello hab end\n",
    ))
    .await?;
    Ok(())
}
