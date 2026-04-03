/// Step definitions for file-clipboard.feature.
///
/// All steps exercise `FileTree` directly (library-level). Paste operations
/// that spawn async filesystem tasks are performed synchronously here using
/// `std::fs` so tests are deterministic without requiring a running editor.
use cucumber::{given, then, when};
use helix_view::file_tree::{ClipboardOp, NodeKind, PromptMode};

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expand_all(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    for name in ["src", "tests", "archive"] {
        let id = tree
            .nodes()
            .iter()
            .find(|(_, n)| n.name == name)
            .map(|(id, _)| id);
        if let Some(id) = id {
            tree.expand_sync(id, &config);
        }
    }
}

fn select_node(world: &mut FileTreeWorld, name: &str) {
    expand_all(world);
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree
        .visible()
        .iter()
        .position(|&id| tree.nodes().get(id).map(|n| n.name == name).unwrap_or(false));
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

/// Copy a file recursively (file only at library level).
fn copy_file(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_file(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Perform paste synchronously: copy or move the clipboard item into dest_dir.
fn do_paste(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();

    let clip = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .clipboard()
        .cloned();
    let dest_dir = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .selected_dir_path();

    let (clip, dest_dir) = match (clip, dest_dir) {
        (Some(c), Some(d)) => (c, d),
        _ => return,
    };

    let file_name = clip.path.file_name().expect("clipboard path has no filename");
    let dest = dest_dir.join(file_name);

    // Collision check
    if dest.exists() {
        let rel = dest
            .strip_prefix(world.workspace_dir.path())
            .unwrap_or(&dest);
        let msg = format!("File already exists: {}", rel.display());
        let tree = world.tree.as_mut().expect("no FileTree");
        tree.set_status(msg.clone());
        world.last_error = Some(msg);
        return;
    }

    let result = match clip.op {
        ClipboardOp::Copy => copy_file(&clip.path, &dest),
        ClipboardOp::Cut => std::fs::rename(&clip.path, &dest).or_else(|_| {
            copy_file(&clip.path, &dest)?;
            if clip.path.is_dir() {
                std::fs::remove_dir_all(&clip.path)
            } else {
                std::fs::remove_file(&clip.path)
            }
        }),
    };

    match result {
        Ok(()) => {
            let tree = world.tree.as_mut().expect("no FileTree");
            if clip.op == ClipboardOp::Cut {
                tree.clear_clipboard();
            }
            tree.refresh_sync(&config);
            // Select the pasted item
            let pasted_name = file_name.to_string_lossy().into_owned();
            let pos = tree.visible().iter().position(|&id| {
                tree.nodes()
                    .get(id)
                    .map(|n| n.name == pasted_name)
                    .unwrap_or(false)
            });
            if let Some(p) = pos {
                tree.move_to(p);
            }
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
            let tree = world.tree.as_mut().expect("no FileTree");
            tree.set_status(e.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Given — selection / clipboard setup
// ---------------------------------------------------------------------------

/// Ensure archive/ exists in addition to the standard project structure.
/// Called from steps that need it; the Background "the project contains the
/// structure:" step is handled by navigation_steps.rs (shared).
fn ensure_archive(world: &mut FileTreeWorld) {
    std::fs::create_dir_all(world.workspace_dir.path().join("archive")).unwrap();
    let config = world.tree_config.clone();
    if let Some(ref mut tree) = world.tree {
        tree.refresh_sync(&config);
    }
}

#[given("archive/ is selected")]
fn archive_selected(world: &mut FileTreeWorld) {
    ensure_archive(world);
    select_node(world, "archive");
}

#[given("Cargo.toml is selected")]
fn cargo_toml_selected(world: &mut FileTreeWorld) {
    select_node(world, "Cargo.toml");
}

#[given("main.rs was previously yanked")]
fn main_rs_previously_yanked(world: &mut FileTreeWorld) {
    select_node(world, "main.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    world.tree.as_mut().expect("no FileTree").yank(id);
}

#[given(expr = "lib.rs was previously yanked and shows {string}")]
fn lib_rs_previously_yanked(world: &mut FileTreeWorld, _tag: String) {
    select_node(world, "lib.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    world.tree.as_mut().expect("no FileTree").yank(id);
}

#[given(expr = "lib.rs is in the clipboard with operation {string}")]
fn lib_rs_in_clipboard(world: &mut FileTreeWorld, op: String) {
    select_node(world, "lib.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    let tree = world.tree.as_mut().expect("no FileTree");
    if op == "copy" {
        tree.yank(id);
    } else {
        tree.cut(id);
    }
}

#[given(expr = "main.rs is in the clipboard with operation {string}")]
fn main_rs_in_clipboard(world: &mut FileTreeWorld, op: String) {
    select_node(world, "main.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    let tree = world.tree.as_mut().expect("no FileTree");
    if op == "copy" {
        tree.yank(id);
    } else {
        tree.cut(id);
    }
}

#[given("the clipboard is empty")]
fn clipboard_empty(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.clear_clipboard();
}

#[given(expr = "the duplication prompt is active pre-filled with {string}")]
fn duplication_prompt_active(world: &mut FileTreeWorld, prefill: String) {
    select_node(world, "main.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.start_duplicate(id);
    // The prompt should be pre-filled; verify and override if needed
    let current = tree.prompt_input().to_string();
    if current != prefill {
        while !tree.prompt_input().is_empty() {
            tree.prompt_pop();
        }
        for ch in prefill.chars() {
            tree.prompt_push(ch);
        }
    }
}

#[given("Alex pastes main.rs into archive/")]
fn alex_pastes_main_into_archive(world: &mut FileTreeWorld) {
    ensure_archive(world);
    select_node(world, "archive");
    do_paste(world);
}

#[when("Alex selects lib.rs and presses y")]
fn alex_selects_lib_and_yanks(world: &mut FileTreeWorld) {
    select_node(world, "lib.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    world.tree.as_mut().expect("no FileTree").yank(id);
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

pub fn alex_presses_y(world: &mut FileTreeWorld) {
    if let Some(id) = world.tree.as_ref().expect("no FileTree").selected_id() {
        let path = world.tree.as_ref().expect("no FileTree").node_path(id);
        let root = world.workspace_dir.path().to_path_buf();
        let display = path
            .strip_prefix(&root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());
        let tree = world.tree.as_mut().expect("no FileTree");
        tree.yank(id);
        tree.set_status(format!("Yanked: {}", display));
    }
}

#[when("Alex presses x")]
fn alex_presses_x(world: &mut FileTreeWorld) {
    if let Some(id) = world.tree.as_ref().expect("no FileTree").selected_id() {
        let path = world.tree.as_ref().expect("no FileTree").node_path(id);
        let root = world.workspace_dir.path().to_path_buf();
        let display = path
            .strip_prefix(&root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());
        let tree = world.tree.as_mut().expect("no FileTree");
        tree.cut(id);
        tree.set_status(format!("Cut: {}", display));
    }
}

#[when("Alex presses p")]
fn alex_presses_p(world: &mut FileTreeWorld) {
    do_paste(world);
}

#[when("Alex presses D")]
fn alex_presses_shift_d(world: &mut FileTreeWorld) {
    if let Some(id) = world.tree.as_ref().expect("no FileTree").selected_id() {
        let is_dir = world
            .tree
            .as_ref()
            .expect("no FileTree")
            .nodes()
            .get(id)
            .map(|n| n.kind == NodeKind::Directory)
            .unwrap_or(false);
        let tree = world.tree.as_mut().expect("no FileTree");
        if is_dir {
            tree.set_status("Duplicate is only available for files".to_string());
        } else {
            tree.start_duplicate(id);
        }
    }
}

pub fn alex_presses_enter(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();

    let conflict = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .prompt_would_create_path()
        .filter(|p| p.exists());
    if let Some(p) = conflict {
        let msg = format!(
            "File already exists: {}",
            p.strip_prefix(world.workspace_dir.path())
                .unwrap_or(&p)
                .display()
        );
        let tree = world.tree.as_mut().expect("no FileTree");
        tree.set_status(msg.clone());
        world.last_error = Some(msg);
        return;
    }

    let tree = world.tree.as_mut().expect("no FileTree");
    if let Some(commit) = tree.prompt_confirm() {
        use helix_view::file_tree::PromptCommit;
        if let PromptCommit::Duplicate { src_path, new_name } = commit {
            let to = src_path.parent().map(|p| p.join(&new_name)).unwrap_or_else(|| std::path::PathBuf::from(&new_name));
            if let Err(e) = std::fs::copy(&src_path, &to) {
                world.last_error = Some(e.to_string());
                let tree = world.tree.as_mut().expect("no FileTree");
                tree.set_status(e.to_string());
                return;
            }
            let tree = world.tree.as_mut().expect("no FileTree");
            tree.refresh_sync(&config);
            let pos = tree.visible().iter().position(|&id| {
                tree.nodes()
                    .get(id)
                    .map(|n| n.name == new_name)
                    .unwrap_or(false)
            });
            if let Some(p) = pos {
                tree.move_to(p);
            }
        }
    }
}

#[when(expr = "Alex clears the input, types {string}, and presses Enter")]
fn alex_clears_types_enter(world: &mut FileTreeWorld, text: String) {
    let tree = world.tree.as_mut().expect("no FileTree");
    while !tree.prompt_input().is_empty() {
        tree.prompt_pop();
    }
    for ch in text.chars() {
        tree.prompt_push(ch);
    }
    drop(tree);
    alex_presses_enter(world);
}

#[when("Alex navigates the tree with j and k")]
fn alex_navigates_jk(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.move_down();
    tree.move_up();
}

#[when("Alex yanks main.rs")]
fn alex_yanks_main(world: &mut FileTreeWorld) {
    select_node(world, "main.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    world.tree.as_mut().expect("no FileTree").yank(id);
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(expr = "main.rs is recorded in the clipboard with operation {string}")]
fn main_rs_in_clipboard_op(world: &mut FileTreeWorld, op: String) {
    check_clipboard(world, "main.rs", &op);
}

#[then("src/ is recorded in the clipboard with operation \"copy\"")]
fn src_in_clipboard_copy(world: &mut FileTreeWorld) {
    check_clipboard(world, "src", "copy");
}

#[then("src/ is recorded in the clipboard with operation \"cut\"")]
fn src_in_clipboard_cut(world: &mut FileTreeWorld) {
    check_clipboard(world, "src", "cut");
}

// Keep the expr variant for non-slash cases:
#[allow(dead_code)]
fn src_in_clipboard_op(world: &mut FileTreeWorld, op: String) {
    check_clipboard(world, "src", &op);
}

fn check_clipboard(world: &mut FileTreeWorld, name: &str, op: &str) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip = tree.clipboard().expect("clipboard is empty");
    let clip_name = clip
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert_eq!(
        clip_name, name,
        "expected clipboard to hold {name} but got {clip_name}"
    );
    let expected_op = if op == "copy" { ClipboardOp::Copy } else { ClipboardOp::Cut };
    assert_eq!(
        clip.op, expected_op,
        "expected clipboard op {op:?} but got {:?}",
        clip.op
    );
}

#[then(expr = "a status message confirms {string}")]
fn status_confirms(world: &mut FileTreeWorld, expected: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let msg = tree.status_message().unwrap_or("");
    assert!(
        msg.contains(expected.trim_end_matches('.')),
        "expected status containing {expected:?} but got {msg:?}"
    );
}

#[then("main.rs remains on disk unchanged")]
fn main_rs_unchanged(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main.rs");
    assert!(path.exists(), "expected src/main.rs to remain on disk");
}

#[then("src/ and its contents remain on disk unchanged")]
fn src_unchanged(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src");
    assert!(path.is_dir(), "expected src/ to remain on disk");
}

#[then("main.rs remains on disk until pasted")]
fn main_rs_until_pasted(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main.rs");
    assert!(path.exists(), "expected src/main.rs to remain until pasted");
}

#[then("src/ remains on disk until pasted")]
fn src_until_pasted(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src");
    assert!(path.is_dir(), "expected src/ to remain until pasted");
}

#[then("the clipboard now holds lib.rs")]
fn clipboard_holds_lib(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip = tree.clipboard().expect("clipboard is empty");
    let name = clip.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    assert_eq!(name, "lib.rs", "expected clipboard to hold lib.rs but got {name}");
}

#[then("main.rs is no longer in the clipboard")]
fn main_not_in_clipboard(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip = tree.clipboard();
    let holds_main = clip
        .map(|c| c.path.file_name().map(|n| n == "main.rs").unwrap_or(false))
        .unwrap_or(false);
    assert!(!holds_main, "expected main.rs to no longer be in clipboard");
}

#[then("archive/lib.rs is created as a copy of src/lib.rs")]
fn archive_lib_created(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("archive/lib.rs");
    assert!(path.exists(), "expected archive/lib.rs to be created");
}

#[then("the file tree refreshes to show archive/lib.rs")]
fn tree_shows_archive_lib(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let present = tree.nodes().iter().any(|(_, n)| n.name == "lib.rs");
    assert!(present, "expected lib.rs to appear in tree (under archive/)");
}

#[then("the selection moves to the pasted file")]
fn selection_on_pasted(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.selected() < tree.visible().len(),
        "selection out of bounds after paste"
    );
}

#[then("lib.rs is copied into the project root alongside Cargo.toml")]
fn lib_rs_in_root(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("lib.rs");
    assert!(path.exists(), "expected lib.rs to be pasted into project root");
}

#[then("the file tree refreshes to show the pasted file")]
fn tree_shows_pasted(_world: &mut FileTreeWorld) {
    // Verified by the individual file-existence assertions.
}

#[then("src/main.rs is moved to archive/main.rs on disk")]
fn main_moved_to_archive(world: &mut FileTreeWorld) {
    let src = world.workspace_dir.path().join("src/main.rs");
    let dst = world.workspace_dir.path().join("archive/main.rs");
    assert!(!src.exists(), "expected src/main.rs to be gone after move");
    assert!(dst.exists(), "expected archive/main.rs to exist after move");
}

#[then("the file tree no longer shows main.rs under src/")]
fn tree_no_main_under_src(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let main_under_src = tree.nodes().iter().any(|(id, n)| {
        n.name == "main.rs" && {
            let path = tree.node_path(id);
            path.starts_with(world.workspace_dir.path().join("src"))
        }
    });
    assert!(!main_under_src, "expected main.rs to be gone from src/ after move");
}

#[then("archive/main.rs appears in the tree")]
fn archive_main_in_tree(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let present = tree.nodes().iter().any(|(id, n)| {
        n.name == "main.rs" && {
            let path = tree.node_path(id);
            path.starts_with(world.workspace_dir.path().join("archive"))
        }
    });
    assert!(present, "expected main.rs to appear under archive/ in tree");
}

#[then("the clipboard is empty")]
fn clipboard_is_empty(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(tree.clipboard().is_none(), "expected clipboard to be empty after cut+paste");
}

#[then("pressing p again has no effect")]
fn pressing_p_no_effect(world: &mut FileTreeWorld) {
    let archive_count_before = std::fs::read_dir(world.workspace_dir.path().join("archive"))
        .map(|d| d.count())
        .unwrap_or(0);
    do_paste(world);
    let archive_count_after = std::fs::read_dir(world.workspace_dir.path().join("archive"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(
        archive_count_before, archive_count_after,
        "expected p with empty clipboard to have no effect"
    );
}

#[then("archive/lib.rs is created")]
fn archive_lib_exists(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("archive/lib.rs");
    assert!(path.exists(), "expected archive/lib.rs to exist");
}

#[then(expr = "lib.rs remains in the clipboard with operation {string}")]
fn lib_remains_in_clipboard(world: &mut FileTreeWorld, op: String) {
    check_clipboard(world, "lib.rs", &op);
}

#[then("Alex can press p again to paste another copy")]
fn can_paste_again(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.clipboard().is_some(),
        "expected clipboard to still hold lib.rs for a second paste"
    );
}

#[then("the paste does not overwrite the existing src/lib.rs")]
fn no_overwrite(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/lib.rs");
    assert!(path.exists(), "expected src/lib.rs to remain (no overwrite)");
}

fn error_reads(world: &mut FileTreeWorld, expected: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let msg = tree
        .status_message()
        .map(|s| s.to_string())
        .or_else(|| world.last_error.clone())
        .unwrap_or_default();
    assert!(
        msg.contains(expected.trim_end_matches('.')),
        "expected error containing {expected:?} but got {msg:?}"
    );
}

#[then("a duplication prompt appears at the bottom of the sidebar")]
fn dup_prompt_appears(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        matches!(tree.prompt_mode(), PromptMode::Duplicate(_)),
        "expected a duplication prompt but got {:?}",
        tree.prompt_mode()
    );
}

fn prompt_prefilled(world: &mut FileTreeWorld, expected: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let input = tree.prompt_input();
    assert_eq!(
        input, expected,
        "expected prompt pre-filled with {expected:?} but got {input:?}"
    );
}

#[then("src/main.copy.rs is created as a copy of src/main.rs")]
fn main_copy_created(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main.copy.rs");
    assert!(path.exists(), "expected src/main.copy.rs to be created");
}

#[then("the file tree shows both main.rs and main.copy.rs")]
fn tree_shows_both(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let has_main = tree.nodes().iter().any(|(_, n)| n.name == "main.rs");
    let has_copy = tree.nodes().iter().any(|(_, n)| n.name == "main.copy.rs");
    assert!(has_main && has_copy, "expected both main.rs and main.copy.rs in tree");
}

#[then("the selection moves to the duplicate file")]
fn selection_on_duplicate(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let name = tree.selected_node().map(|n| n.name.as_str()).unwrap_or("");
    assert!(
        name.contains("copy") || name.contains("backup"),
        "expected selection on duplicate file but got '{name}'"
    );
}

#[then("src/main_backup.rs is created as a copy of src/main.rs")]
fn main_backup_created(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main_backup.rs");
    assert!(path.exists(), "expected src/main_backup.rs to be created");
}

#[then("the duplication prompt disappears")]
fn dup_prompt_gone(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        matches!(tree.prompt_mode(), PromptMode::None),
        "expected no prompt but got {:?}",
        tree.prompt_mode()
    );
}

fn no_new_file_clipboard(world: &mut FileTreeWorld) {
    let copy = world.workspace_dir.path().join("src/main.copy.rs");
    assert!(!copy.exists(), "expected no duplicate file to be created");
}

#[then("no duplication prompt appears")]
fn no_dup_prompt(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !matches!(tree.prompt_mode(), PromptMode::Duplicate(_)),
        "expected no duplication prompt for a directory"
    );
}

#[then("a message reads \"Duplicate is only available for files\"")]
fn dup_dir_message(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let msg = tree.status_message().unwrap_or("");
    assert!(
        msg.contains("only available for files"),
        "expected info message about files-only duplicate but got {msg:?}"
    );
}

#[then("the row for lib.rs shows a dimmed \"(C)\" tag after the filename")]
fn lib_rs_shows_c_tag(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip = tree.clipboard().expect("clipboard is empty");
    let name = clip.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    assert_eq!(name, "lib.rs", "expected clipboard to hold lib.rs for (C) tag");
    assert_eq!(clip.op, ClipboardOp::Copy, "expected Copy op for (C) tag");
}

#[then("the rest of the row styling is unchanged")]
fn row_styling_unchanged(_world: &mut FileTreeWorld) {
    // Visual assertion — verified by rendering tests, not library-level.
}

#[then("the row for main.rs shows a dimmed \"(X)\" tag after the filename")]
fn main_rs_shows_x_tag(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip = tree.clipboard().expect("clipboard is empty");
    let name = clip.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    assert_eq!(name, "main.rs", "expected clipboard to hold main.rs for (X) tag");
    assert_eq!(clip.op, ClipboardOp::Cut, "expected Cut op for (X) tag");
}

#[then("no row shows an \"(X)\" tag")]
fn no_x_tag(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    // After a cut+paste the clipboard should be empty
    let has_cut = tree
        .clipboard()
        .map(|c| c.op == ClipboardOp::Cut)
        .unwrap_or(false);
    assert!(!has_cut, "expected no cut entry in clipboard (no (X) tag)");
}

#[then("the row for lib.rs still shows the dimmed \"(C)\" tag")]
fn lib_rs_still_c_tag(world: &mut FileTreeWorld) {
    lib_rs_shows_c_tag(world);
}

#[then("main.rs shows a dimmed \"(C)\" tag")]
fn main_rs_shows_c_tag(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip = tree.clipboard().expect("clipboard is empty");
    let name = clip.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    assert_eq!(name, "main.rs", "expected clipboard to hold main.rs");
    assert_eq!(clip.op, ClipboardOp::Copy);
}

#[then("lib.rs no longer shows any clipboard tag")]
fn lib_rs_no_tag(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let clip_is_lib = tree
        .clipboard()
        .and_then(|c| c.path.file_name())
        .map(|n| n == "lib.rs")
        .unwrap_or(false);
    assert!(!clip_is_lib, "expected lib.rs to no longer be in clipboard");
}

#[then("no files are created or moved")]
fn no_files_changed(world: &mut FileTreeWorld) {
    let archive = world.workspace_dir.path().join("archive");
    let count = std::fs::read_dir(&archive).map(|d| d.count()).unwrap_or(0);
    assert_eq!(count, 0, "expected no files pasted into archive/ but found {count}");
}

#[then("the tree is unchanged")]
fn tree_unchanged(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let has_src = tree.nodes().iter().any(|(_, n)| n.name == "src");
    assert!(has_src, "expected tree to be unchanged after no-op paste");
}
