/// Step definitions for copy-path.feature.
///
/// Y copies the absolute path of the selected node to the "system clipboard"
/// (simulated here as `world.system_clipboard`) and sets a status message.
/// All steps exercise `FileTree` at the library level.
use cucumber::{given, then, when};

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn select_node_by_name(world: &mut FileTreeWorld, name: &str) {
    let name = name.trim_end_matches('/');
    // Ensure the tree and src/ expansion are in place.
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    // Expand both src/ and tests/ so nested files are reachable.
    for dir in ["src", "tests"] {
        let id = tree
            .nodes()
            .iter()
            .find(|(_, n)| n.name == dir)
            .map(|(id, _)| id);
        if let Some(id) = id {
            tree.expand_sync(id, &config);
        }
    }
    let pos = tree
        .visible()
        .iter()
        .position(|&id| tree.nodes().get(id).map(|n| n.name == name).unwrap_or(false));
    if let Some(p) = pos {
        tree.move_to(p);
    } else {
        panic!("node '{name}' not found in visible tree");
    }
}

fn do_copy_path(world: &mut FileTreeWorld) {
    let path = world
        .tree
        .as_mut()
        .expect("no FileTree")
        .copy_path()
        .expect("no node selected");
    world.system_clipboard = Some(path);
}

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(regex = r"^main\.rs is in the internal clipboard with operation (.+)$")]
fn main_rs_in_internal_clipboard(world: &mut FileTreeWorld, op: String) {
    select_node_by_name(world, "main.rs");
    let id = world.tree.as_ref().expect("no FileTree").selected_id().unwrap();
    let tree = world.tree.as_mut().expect("no FileTree");
    if op.trim_matches('"') == "copy" {
        tree.yank(id);
    } else {
        tree.cut(id);
    }
}

#[given(regex = r"^Alex previously pressed Y on main\.rs$")]
fn alex_previously_pressed_y_on_main(world: &mut FileTreeWorld) {
    select_node_by_name(world, "main.rs");
    do_copy_path(world);
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex presses Y")]
fn alex_presses_y(world: &mut FileTreeWorld) {
    do_copy_path(world);
}

#[when(regex = r"^Alex selects lib\.rs and presses Y$")]
fn alex_selects_lib_and_presses_y(world: &mut FileTreeWorld) {
    select_node_by_name(world, "lib.rs");
    do_copy_path(world);
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(regex = r#"^the system clipboard contains the absolute path to "([^"]+)"$"#)]
fn system_clipboard_contains_path(world: &mut FileTreeWorld, rel_path: String) {
    let clip = world
        .system_clipboard
        .as_ref()
        .expect("system clipboard is empty");
    let rel = rel_path.trim_end_matches('/');
    assert!(
        clip.contains(rel),
        "expected system clipboard to contain {rel:?} but got {clip:?}"
    );
}

#[then("the internal clipboard still holds main.rs with operation \"copy\"")]
fn internal_clipboard_unchanged(world: &mut FileTreeWorld) {
    use helix_view::file_tree::ClipboardOp;
    let clip = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .clipboard()
        .expect("internal clipboard should not be empty");
    assert_eq!(clip.op, ClipboardOp::Copy, "expected Copy in internal clipboard");
    let has_main = clip.paths.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "main.rs")
            .unwrap_or(false)
    });
    assert!(has_main, "expected main.rs in internal clipboard");
}

#[then(regex = r#"^the path to main\.rs is no longer in the system clipboard$"#)]
fn main_rs_not_in_clipboard(world: &mut FileTreeWorld) {
    let clip = world
        .system_clipboard
        .as_ref()
        .expect("system clipboard is empty");
    // After pressing Y on lib.rs, the clipboard should contain lib.rs path.
    assert!(
        !clip.ends_with("main.rs"),
        "expected main.rs path to be replaced in system clipboard but got {clip:?}"
    );
}

#[then("the status line shows a message starting with \"Copied path:\"")]
fn status_starts_with_copied_path(world: &mut FileTreeWorld) {
    let msg = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .status_message()
        .unwrap_or("");
    assert!(
        msg.starts_with("Copied path:"),
        "expected 'Copied path:' prefix but got {msg:?}"
    );
}

#[then("the message includes the full absolute path")]
fn message_includes_full_path(world: &mut FileTreeWorld) {
    let clip = world
        .system_clipboard
        .as_ref()
        .expect("system clipboard empty — Y not pressed");
    let msg = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .status_message()
        .unwrap_or("");
    assert!(
        msg.contains(clip.as_str()),
        "expected status message to contain the full path {clip:?} but got {msg:?}"
    );
}

#[then("the message includes the full absolute path to src/")]
fn message_includes_src_path(world: &mut FileTreeWorld) {
    let clip = world
        .system_clipboard
        .as_ref()
        .expect("system clipboard empty — Y not pressed");
    let msg = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .status_message()
        .unwrap_or("");
    assert!(
        msg.contains(clip.as_str()),
        "expected status message to contain src/ path {clip:?} but got {msg:?}"
    );
}
