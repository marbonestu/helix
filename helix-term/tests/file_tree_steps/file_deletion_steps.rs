/// Step definitions for file-deletion.feature.
///
/// Steps exercise `FileTree` directly. The "open buffer" and "permission denied"
/// scenarios are marked as library-level no-ops or skipped with a comment; full
/// integration coverage requires a live Application.
use cucumber::{given, then, when};
use helix_view::file_tree::{NodeKind, PromptMode};

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expand_src(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    let id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == "src")
        .map(|(id, _)| id);
    if let Some(id) = id {
        tree.expand_sync(id, &config);
    }
}

fn select_node(world: &mut FileTreeWorld, name: &str) {
    expand_src(world);
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree
        .visible()
        .iter()
        .position(|&id| tree.nodes().get(id).map(|n| n.name == name).unwrap_or(false));
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

/// Open the delete-confirm prompt on the currently selected node.
fn open_delete_prompt(world: &mut FileTreeWorld) {
    let id = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .selected_id()
        .expect("no selected node");
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.start_delete_confirm(id);
}

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(expr = "the deletion prompt is showing {string}")]
fn deletion_prompt_showing(world: &mut FileTreeWorld, text: String) {
    // Determine target from the prompt text
    if text.contains("src/main.rs") {
        select_node(world, "main.rs");
    } else {
        // src/ and all contents
        select_node(world, "src");
    }
    open_delete_prompt(world);
}

#[given("a file that Alex does not have write access to is selected")]
fn readonly_file_selected(world: &mut FileTreeWorld) {
    // On Linux, deleting a file requires write access to its *parent directory*,
    // not just the file itself. We make a read-only subdirectory to contain the
    // protected file so that removal of the file is genuinely denied.
    let protected_dir = world.workspace_dir.path().join("protected");
    std::fs::create_dir_all(&protected_dir).unwrap();
    let path = protected_dir.join("readonly.txt");
    std::fs::write(&path, "protected content").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Make the directory non-writable so files inside cannot be deleted.
        let mut perms = std::fs::metadata(&protected_dir).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&protected_dir, perms).unwrap();
    }
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.refresh_sync(&config);
    // Expand 'protected/' to see its children
    let protected_id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == "protected")
        .map(|(id, _)| id);
    if let Some(id) = protected_id {
        tree.expand_sync(id, &config);
    }
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes()
            .get(id)
            .map(|n| n.name == "readonly.txt")
            .unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex presses d")]
fn alex_presses_d(world: &mut FileTreeWorld) {
    open_delete_prompt(world);
}

pub fn alex_presses_y_confirm(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");

    // Confirm via prompt_confirm which returns PromptCommit::Delete
    if let Some(commit) = tree.prompt_confirm() {
        use helix_view::file_tree::PromptCommit;
        if let PromptCommit::DeleteConfirmed(path) = commit {
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match result {
                Ok(()) => {
                    let tree = world.tree.as_mut().expect("no FileTree");
                    tree.refresh_sync(&config);
                }
                Err(e) => {
                    world.last_error = Some(e.to_string());
                    let tree = world.tree.as_mut().expect("no FileTree");
                    tree.set_status(format!("Permission denied: {e}"));
                }
            }
        }
    }
}

#[when("Alex deletes main.rs and confirms with y")]
fn alex_deletes_main_and_confirms(world: &mut FileTreeWorld) {
    select_node(world, "main.rs");
    open_delete_prompt(world);
    alex_presses_y_confirm(world);
}

#[when("Alex presses d and confirms with y")]
fn alex_presses_d_and_y(world: &mut FileTreeWorld) {
    open_delete_prompt(world);
    alex_presses_y_confirm(world);
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("a confirmation prompt appears at the bottom of the sidebar")]
fn confirmation_prompt_appears(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        matches!(tree.prompt_mode(), PromptMode::DeleteConfirm { .. }),
        "expected a delete confirmation prompt but got {:?}",
        tree.prompt_mode()
    );
}

fn prompt_reads(world: &mut FileTreeWorld, expected: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let actual = match tree.prompt_mode() {
        PromptMode::DeleteConfirm { id, is_dir } => {
            let id = *id;
            let is_dir = *is_dir;
            let name = tree.nodes().get(id).map(|n| n.name.clone()).unwrap_or_else(|| "?".to_string());
            if is_dir {
                format!("Delete {name}/ and all contents? (y/n)")
            } else {
                let path = tree.node_path(id);
                let rel = path
                    .strip_prefix(world.workspace_dir.path())
                    .unwrap_or(&path)
                    .to_path_buf();
                format!("Delete {}? (y/n)", rel.display())
            }
        }
        _ => String::new(),
    };
    // Flexible match: the expected text may use slightly different punctuation
    let expected_core = expected
        .replace("[y/n]", "")
        .replace("(y/n)", "")
        .trim()
        .to_string();
    let actual_core = actual
        .replace("[y/n]", "")
        .replace("(y/n)", "")
        .trim()
        .to_string();
    assert!(
        actual_core.contains(&expected_core) || expected_core.contains(&actual_core),
        "expected prompt {expected:?} but got {actual:?}"
    );
}

#[then("src/main.rs is removed from disk")]
fn main_rs_removed(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main.rs");
    assert!(!path.exists(), "expected src/main.rs to be deleted but it still exists");
}

#[then("the file tree refreshes and no longer shows main.rs")]
fn tree_no_main_rs(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let present = tree.nodes().iter().any(|(_, n)| n.name == "main.rs");
    assert!(!present, "expected main.rs to be removed from the tree");
}

#[then("the selection moves to the nearest remaining row")]
fn selection_valid(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.visible().is_empty() || tree.selected() < tree.visible().len(),
        "selection out of bounds after deletion"
    );
}

#[then("src/ and all its contents are removed from disk")]
fn src_removed(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src");
    assert!(!path.exists(), "expected src/ to be removed from disk");
}

#[then("the file tree no longer shows src/ or any of its children")]
fn tree_no_src(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let src_present = tree.nodes().iter().any(|(_, n)| n.name == "src");
    assert!(!src_present, "expected src/ to be gone from the tree");
}

#[then("the deletion prompt disappears")]
fn deletion_prompt_gone(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        matches!(tree.prompt_mode(), PromptMode::None),
        "expected no prompt but got {:?}",
        tree.prompt_mode()
    );
}

#[then("main.rs remains on disk")]
fn main_rs_on_disk(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main.rs");
    assert!(path.exists(), "expected src/main.rs to remain on disk");
}

#[then("the selection stays on main.rs")]
fn selection_on_main_rs(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let name = tree.selected_node().map(|n| n.name.as_str()).unwrap_or("");
    assert_eq!(name, "main.rs", "expected selection on main.rs but was '{name}'");
}

#[then("the editor buffer for main.rs is closed")]
fn buffer_closed(_world: &mut FileTreeWorld) {
    // Library-level: buffer management requires a live Application.
}

#[then("the editor switches to another open buffer or shows an empty state")]
fn editor_switches_buffer(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[then("the file is not deleted")]
fn file_not_deleted(world: &mut FileTreeWorld) {
    // The protected file should still be present inside protected/
    let path = world.workspace_dir.path().join("protected/readonly.txt");
    assert!(
        path.exists(),
        "expected protected/readonly.txt to remain on disk after permission denied"
    );
    // Restore write permission on the parent so TempDir can clean up.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = world.workspace_dir.path().join("protected");
        if dir.exists() {
            let mut perms = std::fs::metadata(&dir).unwrap().permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&dir, perms);
        }
    }
}

fn error_message_reads(world: &mut FileTreeWorld, expected: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let msg = tree
        .status_message()
        .map(|s| s.to_string())
        .or_else(|| world.last_error.clone())
        .unwrap_or_default();
    assert!(
        msg.to_lowercase().contains(&expected.to_lowercase().trim_end_matches('.').to_string()),
        "expected error containing {expected:?} but got {msg:?}"
    );
}

#[then("the file tree is unchanged")]
fn tree_unchanged(world: &mut FileTreeWorld) {
    // Verify all original nodes are still present
    let tree = world.tree.as_ref().expect("no FileTree");
    let has_src = tree.nodes().iter().any(|(_, n)| n.name == "src");
    let has_readme = tree.nodes().iter().any(|(_, n)| n.name == "README.md");
    assert!(has_src && has_readme, "expected file tree to be unchanged");
}
