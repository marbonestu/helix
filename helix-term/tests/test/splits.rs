use super::*;

use helix_stdx::path;
use std::cell::Cell;

#[tokio::test(flavor = "multi_thread")]
async fn test_split_write_quit_all() -> anyhow::Result<()> {
    let mut file1 = tempfile::NamedTempFile::new()?;
    let mut file2 = tempfile::NamedTempFile::new()?;
    let mut file3 = tempfile::NamedTempFile::new()?;

    let mut app = helpers::AppBuilder::new()
        .with_file(file1.path(), None)
        .build()?;

    test_key_sequences(
        &mut app,
        vec![
            (
                Some(&format!(
                    "ihello1<esc>:sp<ret>:o {}<ret>ihello2<esc>:sp<ret>:o {}<ret>ihello3<esc>",
                    file2.path().to_string_lossy(),
                    file3.path().to_string_lossy()
                )),
                Some(&|app| {
                    let docs: Vec<_> = app.editor.documents().collect();
                    assert_eq!(3, docs.len());

                    let doc1 = docs
                        .iter()
                        .find(|doc| doc.path().unwrap() == &path::normalize(file1.path()))
                        .unwrap();

                    assert_eq!("hello1", doc1.text().to_string());

                    let doc2 = docs
                        .iter()
                        .find(|doc| doc.path().unwrap() == &path::normalize(file2.path()))
                        .unwrap();

                    assert_eq!("hello2", doc2.text().to_string());

                    let doc3 = docs
                        .iter()
                        .find(|doc| doc.path().unwrap() == &path::normalize(file3.path()))
                        .unwrap();

                    assert_eq!("hello3", doc3.text().to_string());

                    helpers::assert_status_not_error(&app.editor);
                    assert_eq!(3, app.editor.tree.views().count());
                }),
            ),
            (
                Some(":wqa<ret>"),
                Some(&|app| {
                    helpers::assert_status_not_error(&app.editor);
                    assert_eq!(0, app.editor.tree.views().count());
                }),
            ),
        ],
        true,
    )
    .await?;

    helpers::assert_file_has_content(&mut file1, &LineFeedHandling::Native.apply("hello1"))?;
    helpers::assert_file_has_content(&mut file2, &LineFeedHandling::Native.apply("hello2"))?;
    helpers::assert_file_has_content(&mut file3, &LineFeedHandling::Native.apply("hello3"))?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_split_write_quit_same_file() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    let mut app = helpers::AppBuilder::new()
        .with_file(file.path(), None)
        .build()?;

    test_key_sequences(
        &mut app,
        vec![
            (
                Some("O<esc>ihello<esc>:sp<ret>ogoodbye<esc>"),
                Some(&|app| {
                    assert_eq!(2, app.editor.tree.views().count());
                    helpers::assert_status_not_error(&app.editor);

                    let mut docs: Vec<_> = app.editor.documents().collect();
                    assert_eq!(1, docs.len());

                    let doc = docs.pop().unwrap();

                    assert_eq!(
                        LineFeedHandling::Native.apply("hello\ngoodbye"),
                        doc.text().to_string()
                    );

                    assert!(doc.is_modified());
                }),
            ),
            (
                Some(":wq<ret>"),
                Some(&|app| {
                    helpers::assert_status_not_error(&app.editor);
                    assert_eq!(1, app.editor.tree.views().count());

                    let mut docs: Vec<_> = app.editor.documents().collect();
                    assert_eq!(1, docs.len());

                    let doc = docs.pop().unwrap();

                    assert_eq!(
                        LineFeedHandling::Native.apply("hello\ngoodbye"),
                        doc.text().to_string()
                    );

                    assert!(!doc.is_modified());
                }),
            ),
        ],
        false,
    )
    .await?;

    helpers::assert_file_has_content(&mut file, &LineFeedHandling::Native.apply("hello\ngoodbye"))?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_changes_in_splits_apply_to_all_views() -> anyhow::Result<()> {
    // See <https://github.com/helix-editor/helix/issues/4732>.
    // Transactions must be applied to any view that has the changed document open.
    // This sequence would panic since the jumplist entry would be modified in one
    // window but not the other. Attempting to update the changelist in the other
    // window would cause a panic since it would point outside of the document.

    // The key sequence here:
    // * <C-w>v       Create a vertical split of the current buffer.
    //                Both views look at the same doc.
    // * [<space>     Add a line ending to the beginning of the document.
    //                The cursor is now at line 2 in window 2.
    // * <C-s>        Save that selection to the jumplist in window 2.
    // * <C-w>w       Switch to window 1.
    // * kd           Delete line 1 in window 1.
    // * <C-w>q       Close window 1, focusing window 2.
    // * d            Delete line 1 in window 2.
    //
    // This panicked in the past because the jumplist entry on line 2 of window 2
    // was not updated and after the `kd` step, pointed outside of the document.
    test((
        "#[|]#",
        "<C-w>v[<space><C-s><C-w>wkd<C-w>qd",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    // Transactions are applied to the views for windows lazily when they are focused.
    // This case panics if the transactions and inversions are not applied in the
    // correct order as we switch between windows.
    test((
        "#[|]#",
        "[<space>[<space>[<space><C-w>vuuu<C-w>wUUU<C-w>quuu",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    // See <https://github.com/helix-editor/helix/issues/4957>.
    // This sequence undoes part of the history and then adds new changes, creating a
    // new branch in the history tree. `View::sync_changes` applies transactions down
    // and up to the lowest common ancestor in the path between old and new revision
    // numbers. If we apply these up/down transactions in the wrong order, this case
    // panics.
    // The key sequence:
    // * 3[<space>    Create three empty lines so we are at the end of the document.
    // * <C-w>v<C-s>  Create a split and save that point at the end of the document
    //                in the jumplist.
    // * <C-w>w       Switch back to the first window.
    // * uu           Undo twice (not three times which would bring us back to the
    //                root of the tree).
    // * 3[<space>    Create three empty lines. Now the end of the document is past
    //                where it was on step 1.
    // * <C-w>q       Close window 1, focusing window 2 and causing a sync. This step
    //                panics if we don't apply in the right order.
    // * %d           Clean up the buffer.
    test((
        "#[|]#",
        "3[<space><C-w>v<C-s><C-w>wuu3[<space><C-w>q%d",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_changes_in_splits_jumplist_sync() -> anyhow::Result<()> {
    // See <https://github.com/helix-editor/helix/issues/9833>
    // When jumping backwards (<C-o>) switches between two documents, we need to
    // ensure that the current view has been synced with all changes to the
    // document that occurred since the last time the view focused this document.
    // If the view isn't synced then this case panics since we try to form a
    // selection on "test" (which was deleted in the other view).
    test((
        "#[test|]#",
        "<C-w>sgf<C-w>wd<C-w>w<C-o><C-w>qd",
        "#[|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;

    Ok(())
}

// ── Resize keybindings ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_split_grow_width() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    // <C-w>v creates right split (focused). <C-w>h focuses left. <C-w><gt> grows left.
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w><gt>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len());
                    let focused_width = views.iter().find(|(_, f)| *f).unwrap().0.area.width;
                    let other_width = views.iter().find(|(_, f)| !f).unwrap().0.area.width;
                    assert!(
                        focused_width > other_width,
                        "focused ({focused_width}) should be wider than other ({other_width})"
                    );
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_split_shrink_width() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    // <C-w>v creates right split focused at pos=1 (has left sibling). <C-w><lt> shrinks it.
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w><lt>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len());
                    let focused_width = views.iter().find(|(_, f)| *f).unwrap().0.area.width;
                    let other_width = views.iter().find(|(_, f)| !f).unwrap().0.area.width;
                    assert!(
                        focused_width < other_width,
                        "focused ({focused_width}) should be narrower after shrink"
                    );
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_split_resize_count() -> anyhow::Result<()> {
    // A count of 3 should produce a larger width change than a count of 1.
    let single_step_diff: Cell<i32> = Cell::new(0);
    let three_step_diff: Cell<i32> = Cell::new(0);

    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w><gt>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    let fw = views.iter().find(|(_, f)| *f).unwrap().0.area.width as i32;
                    let ow = views.iter().find(|(_, f)| !f).unwrap().0.area.width as i32;
                    single_step_diff.set(fw - ow);
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;

    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h3<C-w><gt>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    let fw = views.iter().find(|(_, f)| *f).unwrap().0.area.width as i32;
                    let ow = views.iter().find(|(_, f)| !f).unwrap().0.area.width as i32;
                    three_step_diff.set(fw - ow);
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;

    let s = single_step_diff.get();
    let t = three_step_diff.get();
    assert!(t > s, "3-step resize ({t}) should give larger diff than 1-step ({s})");
    Ok(())
}

// ── Equalize ──────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_equalize_splits_keybinding() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w><gt><C-w><gt><C-w>="),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len());
                    let w0 = views[0].0.area.width;
                    let w1 = views[1].0.area.width;
                    let diff = w0.abs_diff(w1);
                    assert!(diff <= 1, "widths should be equal after equalize (±1): {w0} vs {w1}");
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_equalize_splits_typed_command() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w><gt><C-w><gt>:equalize-splits<ret>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len());
                    let diff = views[0].0.area.width.abs_diff(views[1].0.area.width);
                    assert!(diff <= 1, "widths should be equal after :equalize-splits");
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_equalize_splits_alias() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w><gt><C-w><gt>:equal<ret>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len());
                    let diff = views[0].0.area.width.abs_diff(views[1].0.area.width);
                    assert!(diff <= 1, "widths should be equal after :equal alias");
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

// ── Zoom ──────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_zoom_toggle_keybinding() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w>z"),
                Some(&|app| {
                    assert!(app.editor.tree.is_zoomed());
                    let views: Vec<_> = app.editor.tree.views().collect();
                    assert_eq!(2, views.len(), "both views still exist while zoomed");
                    let focused_w = views.iter().find(|(_, f)| *f).unwrap().0.area.width;
                    let other_w = views.iter().find(|(_, f)| !f).unwrap().0.area.width;
                    assert!(
                        focused_w > other_w * 3,
                        "zoomed view ({focused_w}) should dominate other ({other_w})"
                    );
                }),
            ),
            (
                Some("<C-w>z"),
                Some(&|app| {
                    assert!(!app.editor.tree.is_zoomed());
                    let views: Vec<_> = app.editor.tree.views().collect();
                    let diff = views[0].0.area.width.abs_diff(views[1].0.area.width);
                    assert!(diff <= 1, "widths should be equal after unzoom");
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zoom_toggle_typed_command() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h:toggle-zoom<ret>"),
                Some(&|app| {
                    assert!(app.editor.tree.is_zoomed());
                }),
            ),
            (
                Some(":zoom<ret>"),
                Some(&|app| {
                    assert!(!app.editor.tree.is_zoomed());
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zoom_then_close_split() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequence(
        &mut app,
        Some("<C-w>v<C-w>h<C-w>z<C-w>q"),
        Some(&|app| {
            assert!(!app.editor.tree.is_zoomed(), "zoom should clear after closing a split");
            assert_eq!(1, app.editor.tree.views().count());
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zoom_then_split() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w>z<C-w>v"),
                Some(&|app| {
                    assert!(!app.editor.tree.is_zoomed(), "zoom should clear after splitting");
                    assert_eq!(3, app.editor.tree.views().count());
                }),
            ),
            (Some("<C-w>q<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}

// ── Resize + zoom interaction ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_resize_then_zoom_then_unzoom_restores() -> anyhow::Result<()> {
    let saved_w0: Cell<u16> = Cell::new(0);
    let saved_w1: Cell<u16> = Cell::new(0);

    let mut app = helpers::AppBuilder::new().build()?;
    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<C-w>v<C-w>h<C-w><gt><C-w><gt><C-w><gt>"),
                Some(&|app| {
                    let views: Vec<_> = app.editor.tree.views().collect();
                    saved_w0.set(views[0].0.area.width);
                    saved_w1.set(views[1].0.area.width);
                }),
            ),
            (
                Some("<C-w>z"),
                Some(&|app| {
                    assert!(app.editor.tree.is_zoomed());
                }),
            ),
            (
                Some("<C-w>z"),
                Some(&|app| {
                    assert!(!app.editor.tree.is_zoomed());
                    let views: Vec<_> = app.editor.tree.views().collect();
                    let w0 = views[0].0.area.width;
                    let w1 = views[1].0.area.width;
                    assert!(
                        w0.abs_diff(saved_w0.get()) <= 1 && w1.abs_diff(saved_w1.get()) <= 1,
                        "pre-zoom widths ({}, {}) should be restored (got {w0}, {w1})",
                        saved_w0.get(),
                        saved_w1.get()
                    );
                }),
            ),
            (Some("<C-w>q"), None),
        ],
        false,
    )
    .await?;
    Ok(())
}
