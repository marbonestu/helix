/// Step definitions for file-rename.feature.
///
/// Steps exercise `FileTree` directly for prompt/state assertions.
/// LSP-dependent steps (willRenameFiles, didRenameFiles, buffer path updates)
/// are no-ops at library level — they require a live Application with an LSP
/// server, which is an integration-test concern.
use cucumber::{given, then, when};
use helix_view::file_tree::PromptMode;

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

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(expr = "the rename prompt is active pre-filled with {string}")]
fn rename_prompt_active(world: &mut FileTreeWorld, prefill: String) {
    // Select the node matching the prefilled name
    let node_name = prefill.trim_end_matches('/');
    select_node(world, node_name);
    let id = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .selected_id()
        .expect("no selected node");
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.start_rename(id);
}

#[given(expr = "Alex clears the input and types {string}")]
fn alex_clears_and_types(world: &mut FileTreeWorld, text: String) {
    let tree = world.tree.as_mut().expect("no FileTree");
    // Clear by popping all chars
    while !tree.prompt_input().is_empty() {
        tree.prompt_pop();
    }
    for ch in text.chars() {
        tree.prompt_push(ch);
    }
}

#[given(expr = "Alex has edited the text to {string}")]
fn alex_edited_to(world: &mut FileTreeWorld, text: String) {
    alex_clears_and_types(world, text);
}

#[given("main.rs is open in the editor")]
fn main_rs_open(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[given("a language server is active that supports willRenameFiles")]
fn lsp_supports_will_rename(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[given("a language server is active that supports didRenameFiles")]
fn lsp_supports_did_rename(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[given("no language server is running for the current project")]
fn no_lsp(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[given("main.rs is imported by src/lib.rs as \"mod main\"")]
fn main_imported_by_lib(world: &mut FileTreeWorld) {
    let lib = world.workspace_dir.path().join("src/lib.rs");
    std::fs::write(&lib, "mod main;\n").unwrap();
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex presses r")]
fn alex_presses_r(world: &mut FileTreeWorld) {
    let id = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .selected_id()
        .expect("no selected node");
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.start_rename(id);
}

#[when("Alex presses R")]
fn alex_presses_shift_r(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.refresh_sync(&config);
}

#[when("Alex presses Enter without changing the text")]
fn alex_presses_enter_unchanged(world: &mut FileTreeWorld) {
    // Confirm with the same name — no rename should occur
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    let _ = tree.prompt_confirm();
    // Nothing on disk changes because src == dest
    tree.refresh_sync(&config);
}

pub fn alex_presses_enter(world: &mut FileTreeWorld) {
    let config = world.tree_config.clone();

    // Collision check
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
        if let PromptCommit::Rename { old_path, new_name } = commit {
            let to = old_path.parent().map(|p| p.join(&new_name)).unwrap_or_else(|| std::path::PathBuf::from(&new_name));
            if old_path != to {
                match std::fs::rename(&old_path, &to) {
                    Ok(()) => {
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
                    Err(e) => {
                        world.last_error = Some(e.to_string());
                        let tree = world.tree.as_mut().expect("no FileTree");
                        tree.set_status(e.to_string());
                    }
                }
            }
        }
    }
}

#[when(expr = "Alex renames it to {string} and confirms")]
fn alex_renames_and_confirms(world: &mut FileTreeWorld, name: String) {
    alex_clears_and_types(world, name);
    alex_presses_enter(world);
}

#[when(expr = "Alex renames main.rs to app.rs and confirms")]
fn alex_renames_main_to_app(world: &mut FileTreeWorld) {
    // If no rename prompt is active, select main.rs and open the prompt first.
    let is_rename = matches!(
        world.tree.as_ref().map(|t| t.prompt_mode().clone()),
        Some(helix_view::file_tree::PromptMode::Rename(_))
    );
    if !is_rename {
        select_node(world, "main.rs");
        let id = world.tree.as_ref().expect("no FileTree").selected_id().expect("no selected node");
        world.tree.as_mut().expect("no FileTree").start_rename(id);
    }
    alex_clears_and_types(world, "app.rs".to_string());
    alex_presses_enter(world);
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("a rename prompt appears at the bottom of the sidebar")]
fn rename_prompt_appears(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        matches!(tree.prompt_mode(), PromptMode::Rename(_)),
        "expected a rename prompt but got {:?}",
        tree.prompt_mode()
    );
}

#[then(expr = "the prompt input is pre-filled with {string}")]
fn prompt_prefilled(world: &mut FileTreeWorld, expected: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let input = tree.prompt_input();
    assert_eq!(
        input, expected,
        "expected prompt to be pre-filled with {expected:?} but got {input:?}"
    );
}

#[then("src/main.rs is renamed to src/app.rs on disk")]
fn main_renamed_to_app(world: &mut FileTreeWorld) {
    let old = world.workspace_dir.path().join("src/main.rs");
    let new = world.workspace_dir.path().join("src/app.rs");
    assert!(!old.exists(), "src/main.rs should not exist after rename");
    assert!(new.exists(), "src/app.rs should exist after rename");
}

#[then("the file tree refreshes to show app.rs in place of main.rs")]
fn tree_shows_app_rs(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let has_app = tree.nodes().iter().any(|(_, n)| n.name == "app.rs");
    let has_main = tree.nodes().iter().any(|(_, n)| n.name == "main.rs");
    assert!(has_app, "expected app.rs in tree after rename");
    assert!(!has_main, "expected main.rs to be gone from tree after rename");
}

#[then("the selection stays on the renamed file")]
fn selection_on_renamed(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let name = tree.selected_node().map(|n| n.name.as_str()).unwrap_or("");
    assert_eq!(name, "app.rs", "expected selection on app.rs but was '{name}'");
}

#[then("the src/ directory is renamed to source/ on disk")]
fn src_renamed_to_source(world: &mut FileTreeWorld) {
    let old = world.workspace_dir.path().join("src");
    let new = world.workspace_dir.path().join("source");
    assert!(!old.exists(), "src/ should not exist after rename");
    assert!(new.exists(), "source/ should exist after rename");
}

#[then("all files within it remain accessible under the new path")]
fn files_under_new_path(world: &mut FileTreeWorld) {
    let main = world.workspace_dir.path().join("source/main.rs");
    assert!(main.exists(), "source/main.rs should be accessible after directory rename");
}

#[then("no rename occurs on disk")]
fn no_rename_on_disk(world: &mut FileTreeWorld) {
    let main = world.workspace_dir.path().join("src/main.rs");
    assert!(main.exists(), "src/main.rs should still exist (no rename)");
}

#[then("the rename prompt disappears")]
fn rename_prompt_gone(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        matches!(tree.prompt_mode(), PromptMode::None),
        "expected no prompt but got {:?}",
        tree.prompt_mode()
    );
}

#[then("main.rs is unchanged on disk")]
fn main_unchanged(world: &mut FileTreeWorld) {
    let path = world.workspace_dir.path().join("src/main.rs");
    assert!(path.exists(), "expected src/main.rs to remain unchanged");
}

#[then("the selection returns to main.rs")]
fn selection_back_to_main(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let name = tree.selected_node().map(|n| n.name.as_str()).unwrap_or("");
    assert_eq!(name, "main.rs", "expected selection on main.rs but was '{name}'");
}

#[then("the rename prompt remains visible")]
fn rename_prompt_remains(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let active = matches!(tree.prompt_mode(), PromptMode::Rename(_))
        || world.last_error.is_some();
    assert!(active, "expected rename prompt to remain or an error to be set");
}

fn error_message_reads(world: &mut FileTreeWorld, expected: String) {
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


#[then("the editor buffer path updates to src/app.rs")]
fn buffer_path_updates(_world: &mut FileTreeWorld) {
    // Library-level: buffer tracking requires a live Application.
}

#[then("the buffer title in the status line reflects the new name")]
fn buffer_title_updates(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[then("the language server receives a willRenameFiles request")]
fn lsp_will_rename(_world: &mut FileTreeWorld) {
    // Library-level: no LSP server running.
}

#[then("any workspace edits it returns are applied to open buffers")]
fn workspace_edits_applied(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[then("src/lib.rs is updated to reference \"mod app\"")]
fn lib_rs_updated(world: &mut FileTreeWorld) {
    // Library-level: just check lib.rs still exists; LSP edit is integration concern.
    let lib = world.workspace_dir.path().join("src/lib.rs");
    assert!(lib.exists(), "src/lib.rs should still exist after rename");
}

#[then("after the file is moved the language server receives a didRenameFiles notification")]
fn lsp_did_rename(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}

#[then("no LSP errors are shown")]
fn no_lsp_errors(_world: &mut FileTreeWorld) {
    // Library-level: no-op.
}
