use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn block_select_enter_exit() -> anyhow::Result<()> {
    // C-v enters block select, C-v again exits to normal
    test((
        "#[h|]#ello\nworld\n",
        "<C-v><C-v>",
        "#[h|]#ello\nworld\n",
    ))
    .await?;

    // C-v then Esc also exits
    test((
        "#[h|]#ello\nworld\n",
        "<C-v><esc>",
        "#[h|]#ello\nworld\n",
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn block_select_delete_single_column() -> anyhow::Result<()> {
    let app = helpers::AppBuilder::new().build()?;

    helpers::test_key_sequence_with_input_text(
        Some(app),
        ("#[h|]#ello\nworld\n", "<C-v>jd", "#[h|]#\n"),
        &|app| {
            let doc = helix_view::doc!(app.editor);
            assert_eq!(doc.text().to_string(), "ello\norld\n");
        },
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn block_select_delete_multi_column() -> anyhow::Result<()> {
    let app = helpers::AppBuilder::new().build()?;

    helpers::test_key_sequence_with_input_text(
        Some(app),
        ("#[h|]#ello\nworld\n", "<C-v>jlld", "#[h|]#\n"),
        &|app| {
            let doc = helix_view::doc!(app.editor);
            assert_eq!(doc.text().to_string(), "lo\nld\n");
        },
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn block_select_delete_middle() -> anyhow::Result<()> {
    let app = helpers::AppBuilder::new().build()?;

    helpers::test_key_sequence_with_input_text(
        Some(app),
        ("he#[l|]#lo\nworld\n", "<C-v>jld", "#[h|]#\n"),
        &|app| {
            let doc = helix_view::doc!(app.editor);
            assert_eq!(doc.text().to_string(), "heo\nwod\n");
        },
        false,
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn block_select_vertical_preserves_column() -> anyhow::Result<()> {
    let app = helpers::AppBuilder::new().build()?;

    // Move through a short line; column should be preserved.
    // Col 4 is deleted from lines 0 and 2. Line 1 ("hi") is too short,
    // so nothing is deleted from it.
    helpers::test_key_sequence_with_input_text(
        Some(app),
        ("hell#[o|]#\nhi\nworld\n", "<C-v>jjd", "#[h|]#\n"),
        &|app| {
            let doc = helix_view::doc!(app.editor);
            assert_eq!(doc.text().to_string(), "hell\nhi\nworl\n");
        },
        false,
    )
    .await?;

    Ok(())
}
