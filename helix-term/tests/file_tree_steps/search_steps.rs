/// Step definitions for search.feature.
///
/// All steps exercise `FileTree` directly without a running Application.
/// The Background steps ensure a fresh tree with the standard project
/// structure is available before each scenario.
use cucumber::{given, then, when};

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Given — Background / setup
// ---------------------------------------------------------------------------

/// Background step: seed the tree with the named nodes listed in the DataTable.
/// The standard project structure already contains all listed nodes, so we
/// simply ensure the tree is initialised.
#[given(expr = "the project tree contains nodes named:")]
fn project_tree_contains_nodes(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    // Expand src/ so all nodes are visible
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    let src_id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == "src")
        .map(|(id, _)| id);
    if let Some(id) = src_id {
        if !tree.nodes()[id].expanded {
            tree.toggle_expand(id, &config);
        }
    }
    let tests_id = tree
        .nodes()
        .iter()
        .find(|(_, n)| n.name == "tests")
        .map(|(id, _)| id);
    if let Some(id) = tests_id {
        if !tree.nodes()[id].expanded {
            tree.toggle_expand(id, &config);
        }
    }
    // Rebuild the visible list after toggling directories.
    tree.process_updates(&config, None);
}

#[given("no search is active")]
fn no_search_active(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    if tree.search_active() {
        tree.search_cancel();
    }
}

#[given("search mode is active with an empty query")]
fn search_mode_empty_query(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    if !tree.search_active() {
        tree.search_start();
    }
    // Ensure query is empty
    while !tree.search_query().is_empty() {
        tree.search_pop();
    }
}

#[given(expr = "search mode is active with query {string}")]
fn search_mode_with_query(world: &mut FileTreeWorld, query: String) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    if !tree.search_active() {
        tree.search_start();
    }
    while !tree.search_query().is_empty() {
        tree.search_pop();
    }
    for ch in query.chars() {
        tree.search_push(ch);
    }
}

#[given("search mode is active")]
fn search_mode_active(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    if !tree.search_active() {
        tree.search_start();
    }
}

#[given(expr = "the selection was on Cargo.toml before search started")]
fn selection_on_cargo_before_search(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    // Select Cargo.toml, then start search so pre_search_selected is captured
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "Cargo.toml").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
    tree.search_start();
}

#[given(expr = "the selection is on main.rs (first \"r\" match)")]
fn selection_on_main_rs(_world: &mut FileTreeWorld) {
    // The search_push for "r" already jumped to the first match (main.rs).
    // This step is a description, no further action required.
}

#[given(expr = "the selection is on lib.rs")]
fn selection_on_lib_rs(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "lib.rs").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "main.rs is currently selected (first match)")]
fn main_rs_currently_selected(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "main.rs").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "lib.rs is currently selected")]
fn lib_rs_currently_selected(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "lib.rs").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "integration.rs is currently selected (last match)")]
fn integration_rs_currently_selected(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "integration.rs").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "lib.rs is selected")]
fn lib_rs_selected(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "lib.rs").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "Cargo.toml is currently selected")]
fn cargo_toml_currently_selected(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "Cargo.toml").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "a node in the middle of the \"rs\" matches is selected")]
fn middle_rs_match_selected(world: &mut FileTreeWorld) {
    // Select lib.rs as the "middle" match among main.rs, lib.rs, integration.rs
    let tree = world.tree.as_mut().expect("no FileTree");
    let pos = tree.visible().iter().position(|&id| {
        tree.nodes().get(id).map(|n| n.name == "lib.rs").unwrap_or(false)
    });
    if let Some(p) = pos {
        tree.move_to(p);
    }
}

#[given(expr = "Alex previously searched for \"rs\" and confirmed with Enter")]
fn previously_searched_for_rs(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_start();
    tree.search_push('r');
    tree.search_push('s');
    tree.search_confirm();
}

#[given("search mode is no longer active")]
fn search_mode_no_longer_active_given(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    if tree.search_active() {
        tree.search_confirm();
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex presses /")]
fn alex_presses_slash(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_start();
}

#[when(expr = "Alex types {string}")]
fn alex_types(world: &mut FileTreeWorld, text: String) {
    let tree = world.tree.as_mut().expect("no FileTree");
    for ch in text.chars() {
        tree.search_push(ch);
    }
}

#[when("Alex presses Backspace")]
fn alex_presses_backspace(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_pop();
}

#[when("Alex presses Enter")]
fn alex_presses_enter_search(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_confirm();
}

#[when("Alex presses Escape")]
fn alex_presses_escape_search(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_cancel();
}

#[when("Alex presses ctrl-n")]
fn alex_presses_ctrl_n(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_next();
}

#[when("Alex presses ctrl-p")]
fn alex_presses_ctrl_p(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_prev();
}

#[when("Alex presses n")]
fn alex_presses_n(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_next();
}

#[when("Alex presses N")]
fn alex_presses_shift_n(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    tree.search_prev();
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("a search prompt appears at the bottom of the sidebar")]
fn search_prompt_appears(world: &mut FileTreeWorld) {
    let active = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_active();
    assert!(active, "expected search mode to be active");
}

#[then("the prompt displays a leading slash with an empty query")]
fn prompt_empty_query(world: &mut FileTreeWorld) {
    let query = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_query()
        .to_owned();
    assert!(query.is_empty(), "expected empty search query but got: {query:?}");
}

#[then(expr = "the prompt displays {string}")]
fn prompt_displays(world: &mut FileTreeWorld, expected: String) {
    let query = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_query()
        .to_owned();
    // The expected string may include a leading '/' — strip it for comparison.
    let expected_query = expected.trim_start_matches('/');
    assert_eq!(
        query, expected_query,
        "expected search query {expected_query:?} but got {query:?}"
    );
}

#[then(expr = "the selection moves to lib.rs")]
fn selection_moves_to_lib_rs(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "lib.rs", "expected selection on 'lib.rs' but was '{name}'");
}

#[then(expr = "the selection moves to main.rs")]
fn selection_moves_to_main_rs(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "main.rs", "expected selection on 'main.rs' but was '{name}'");
}

#[then(expr = "the query becomes {string}")]
fn query_becomes(world: &mut FileTreeWorld, expected: String) {
    let query = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_query()
        .to_owned();
    assert_eq!(query, expected, "expected query {expected:?} but got {query:?}");
}

#[then("the query remains empty")]
fn query_remains_empty(world: &mut FileTreeWorld) {
    let query = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_query()
        .to_owned();
    assert!(query.is_empty(), "expected empty query but got: {query:?}");
}

#[then("search mode stays active")]
fn search_mode_stays_active(world: &mut FileTreeWorld) {
    let active = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_active();
    assert!(active, "expected search mode to remain active");
}

#[then("search mode is no longer active")]
fn search_mode_no_longer_active(world: &mut FileTreeWorld) {
    let active = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_active();
    assert!(!active, "expected search mode to be inactive");
}

#[then("the search prompt disappears")]
fn search_prompt_disappears(world: &mut FileTreeWorld) {
    let active = world
        .tree
        .as_ref()
        .expect("no FileTree")
        .search_active();
    assert!(!active, "expected search prompt to have disappeared (inactive)");
}

#[then(expr = "lib.rs remains selected")]
fn lib_rs_remains_selected(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "lib.rs", "expected 'lib.rs' to remain selected but was '{name}'");
}

#[then(expr = "the selection returns to Cargo.toml")]
fn selection_returns_to_cargo(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "Cargo.toml", "expected selection to return to 'Cargo.toml' but was '{name}'");
}

#[then(expr = "the selection moves to lib.rs (second match)")]
fn selection_to_lib_second_match(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "lib.rs", "expected 'lib.rs' (second match) but was '{name}'");
}

#[then("the selection moves back to main.rs")]
fn selection_back_to_main(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "main.rs", "expected 'main.rs' but was '{name}'");
}

#[then(expr = "the selection wraps to main.rs (first match)")]
fn selection_wraps_to_main(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "main.rs", "expected wrap to 'main.rs' but was '{name}'");
}

#[then(expr = "the selection wraps to integration.rs (last match)")]
fn selection_wraps_to_integration(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "integration.rs", "expected wrap to 'integration.rs' but was '{name}'");
}

#[then("the selection updates to the first match for \"li\"")]
fn selection_first_li_match(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.clone())
        .unwrap_or_default();
    assert!(
        name.to_lowercase().contains("li"),
        "expected selected node name to contain 'li' but was '{name}'"
    );
}

#[then("the selection lands on a node whose name contains \"src\" case-insensitively")]
fn selection_contains_src_case_insensitive(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.clone())
        .unwrap_or_default();
    assert!(
        name.to_lowercase().contains("src"),
        "expected selected node to contain 'src' (case-insensitive) but was '{name}'"
    );
}

#[then("the selection moves to the next node whose name contains \"rs\"")]
fn selection_next_rs_node(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.clone())
        .unwrap_or_default();
    assert!(
        name.to_lowercase().contains("rs"),
        "expected selected node to contain 'rs' but was '{name}'"
    );
}

#[then("the selection moves to the previous node whose name contains \"rs\"")]
fn selection_prev_rs_node(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.clone())
        .unwrap_or_default();
    assert!(
        name.to_lowercase().contains("rs"),
        "expected selected node to contain 'rs' but was '{name}'"
    );
}

#[then("the selection stays on Cargo.toml")]
fn selection_stays_on_cargo(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.as_str())
        .unwrap_or("");
    assert_eq!(name, "Cargo.toml", "expected selection to stay on 'Cargo.toml' but was '{name}'");
}

#[then("the first node whose name contains \"rs\" is selected")]
fn first_rs_node_selected(world: &mut FileTreeWorld) {
    let name = world
        .tree
        .as_ref()
        .and_then(|t| t.selected_node())
        .map(|n| n.name.clone())
        .unwrap_or_default();
    assert!(
        name.to_lowercase().contains("rs"),
        "expected node containing 'rs' to be selected but was '{name}'"
    );
}
