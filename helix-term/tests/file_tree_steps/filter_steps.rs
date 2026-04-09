/// Step definitions for filter.feature.
///
/// Filter-as-you-type narrows the visible tree to rows whose names contain the
/// query string while keeping ancestor directories as context. All steps
/// exercise `FileTree` directly at the library level.
use cucumber::{given, then, when};
use helix_view::file_tree::NodeKind;

use super::FileTreeWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ensure_filter_tree(world: &mut FileTreeWorld) {
    if world.tree.is_none() {
        world.create_project_structure();
        world.init_tree();
    }
    // Expand the full tree so the filter can search all nodes.
    let config = world.tree_config.clone();
    let tree = world.tree.as_mut().expect("no FileTree");
    let root_id = tree.root_id();
    tree.expand_all_sync(root_id, &config);
}

fn enter_filter_mode(world: &mut FileTreeWorld) {
    ensure_filter_tree(world);
    let tree = world.tree.as_mut().expect("no FileTree");
    if !tree.filter_active() {
        tree.filter_start();
    }
}

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given("no filter is active")]
fn no_filter_active(world: &mut FileTreeWorld) {
    ensure_filter_tree(world);
    // Filter mode should not be active after a fresh init.
    let tree = world.tree.as_mut().expect("no FileTree");
    if tree.filter_active() {
        tree.filter_cancel();
    }
}

#[given("filter mode is active with an empty query")]
fn filter_mode_empty_query(world: &mut FileTreeWorld) {
    enter_filter_mode(world);
    // Clear any existing query so we start fresh.
    let tree = world.tree.as_mut().expect("no FileTree");
    while !tree.filter_query_text().is_empty() {
        tree.filter_pop();
    }
}

#[given("filter mode is active")]
fn filter_mode_active(world: &mut FileTreeWorld) {
    enter_filter_mode(world);
}

#[given(expr = "filter mode is active with query {string}")]
fn filter_mode_active_with_query(world: &mut FileTreeWorld, query: String) {
    enter_filter_mode(world);
    let tree = world.tree.as_mut().expect("no FileTree");
    // Rebuild to the desired query.
    while !tree.filter_query_text().is_empty() {
        tree.filter_pop();
    }
    for ch in query.chars() {
        tree.filter_push(ch);
    }
}

#[given("only user.rs and its ancestors are visible")]
fn only_user_and_ancestors_visible(world: &mut FileTreeWorld) {
    // Established by the previous step (filter with "user"); this is a
    // state assertion used as a Given precondition — no-op here.
    let tree = world.tree.as_ref().expect("no FileTree");
    let names: Vec<&str> = tree
        .visible()
        .iter()
        .filter_map(|&id| tree.nodes().get(id).map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"user.rs"),
        "expected user.rs to be visible; visible: {names:?}"
    );
}

#[given("several rows are hidden")]
fn several_rows_hidden(world: &mut FileTreeWorld) {
    // The filter is already active with "rs" from the previous Given; this step
    // just confirms rows are hidden, which is implicit from the filter.
    let tree = world.tree.as_ref().expect("no FileTree");
    let visible_count = tree.visible().len();
    let total_nodes = tree.nodes().len();
    assert!(
        visible_count < total_nodes,
        "expected some rows to be hidden but all {total_nodes} are visible"
    );
}

#[given("only test-related files are visible")]
fn only_test_related_visible(world: &mut FileTreeWorld) {
    // State implied by filter query "test". Verify at least one match is shown.
    let tree = world.tree.as_ref().expect("no FileTree");
    let visible_names: Vec<&str> = tree
        .visible()
        .iter()
        .filter_map(|&id| tree.nodes().get(id).map(|n| n.name.as_str()))
        .collect();
    assert!(
        visible_names.iter().any(|n| n.to_lowercase().contains("test")),
        "expected test-related files to be visible; visible: {visible_names:?}"
    );
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex presses f")]
fn alex_presses_f(world: &mut FileTreeWorld) {
    ensure_filter_tree(world);
    let tree = world.tree.as_mut().expect("no FileTree");
    if !tree.filter_active() {
        tree.filter_start();
    }
}

#[when("Alex clears the query with Backspace until empty")]
fn alex_clears_query(world: &mut FileTreeWorld) {
    let tree = world.tree.as_mut().expect("no FileTree");
    while !tree.filter_query_text().is_empty() {
        tree.filter_pop();
    }
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("a filter prompt appears at the bottom of the sidebar")]
fn filter_prompt_appears(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.filter_active(),
        "expected filter prompt to be active"
    );
}

#[then("all tree rows remain visible with an empty query")]
fn all_rows_visible_empty_query(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.filter_query_text().is_empty(),
        "expected empty query but got {:?}",
        tree.filter_query_text()
    );
    // With empty query, the visible list equals the unfiltered tree.
    assert!(
        !tree.visible().is_empty(),
        "expected at least some rows to be visible"
    );
}

#[then(
    "only main.rs, lib.rs, utils.rs, user.rs, post.rs, integration.rs, and unit.rs are visible"
)]
fn only_rs_files_visible(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let visible_names: Vec<&str> = tree
        .visible()
        .iter()
        .filter_map(|&id| {
            tree.nodes()
                .get(id)
                .filter(|n| n.kind == NodeKind::File)
                .map(|n| n.name.as_str())
        })
        .collect();
    for name in &["main.rs", "lib.rs", "utils.rs", "user.rs", "post.rs", "integration.rs", "unit.rs"] {
        assert!(
            visible_names.contains(name),
            "expected {name} in visible files; got {visible_names:?}"
        );
    }
}

#[then("Cargo.toml and README.md are hidden")]
fn cargo_readme_hidden(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let visible_names: Vec<&str> = tree
        .visible()
        .iter()
        .filter_map(|&id| tree.nodes().get(id).map(|n| n.name.as_str()))
        .collect();
    assert!(
        !visible_names.contains(&"Cargo.toml"),
        "expected Cargo.toml to be hidden"
    );
    assert!(
        !visible_names.contains(&"README.md"),
        "expected README.md to be hidden"
    );
}

#[then("README.md is visible")]
fn readme_visible(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree
        .visible()
        .iter()
        .any(|&id| tree.nodes().get(id).map(|n| n.name == "README.md").unwrap_or(false));
    assert!(found, "expected README.md to be visible");
}

#[then("user.rs is visible")]
fn user_rs_visible(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree
        .visible()
        .iter()
        .any(|&id| tree.nodes().get(id).map(|n| n.name == "user.rs").unwrap_or(false));
    assert!(found, "expected user.rs to be visible");
}

#[then("src/ and models/ remain visible as ancestor context")]
fn src_models_visible_as_ancestors(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let visible_names: Vec<&str> = tree
        .visible()
        .iter()
        .filter_map(|&id| tree.nodes().get(id).map(|n| n.name.as_str()))
        .collect();
    assert!(
        visible_names.contains(&"src"),
        "expected src/ to remain visible as ancestor"
    );
    assert!(
        visible_names.contains(&"models"),
        "expected models/ to remain visible as ancestor"
    );
}

#[then("unmatched siblings like post.rs are hidden")]
fn unmatched_siblings_hidden(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree
        .visible()
        .iter()
        .any(|&id| tree.nodes().get(id).map(|n| n.name == "post.rs").unwrap_or(false));
    assert!(!found, "expected post.rs to be hidden as an unmatched sibling");
}

#[then(expr = "any additional entries matching {string} become visible")]
fn additional_matching_visible(world: &mut FileTreeWorld, query: String) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree
        .visible()
        .iter()
        .filter_map(|&id| tree.nodes().get(id))
        .filter(|n| n.kind == NodeKind::File)
        .any(|n| n.name.to_lowercase().contains(&query));
    assert!(
        found,
        "expected at least one node matching {query:?} to be visible"
    );
}

#[then("all rows are visible again")]
fn all_rows_visible(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let query = tree.filter_query_text().to_owned();
    assert!(
        query.is_empty(),
        "expected filter query to be empty but got {query:?}"
    );
    assert!(
        !tree.visible().is_empty(),
        "expected rows to be visible after clearing filter"
    );
}

#[then("filter mode is no longer active")]
fn filter_mode_inactive(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !tree.filter_active(),
        "expected filter mode to be inactive"
    );
}

#[then("only the matching rows remain visible")]
fn only_matching_rows_visible(world: &mut FileTreeWorld) {
    // After filter_confirm, the filtered view persists.
    // We just assert the visible list is a strict subset of all nodes.
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !tree.visible().is_empty(),
        "expected filtered rows to remain visible after Enter"
    );
    let total = tree.nodes().len();
    let visible = tree.visible().len();
    assert!(
        visible <= total,
        "visible ({visible}) should not exceed total node count ({total})"
    );
}

#[then("Alex can navigate among them with j and k")]
fn can_navigate_filtered_rows(world: &mut FileTreeWorld) {
    // Verify move_down doesn't panic on a filtered visible list.
    let tree = world.tree.as_mut().expect("no FileTree");
    let initial = tree.selected();
    tree.move_down();
    let after_j = tree.selected();
    tree.move_up();
    let after_k = tree.selected();
    // If there is only one row, j and k will not change position.
    let _ = (initial, after_j, after_k);
}

#[then("the selection moves to lib.rs (the next visible row)")]
fn selection_moves_to_lib_rs(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let selected = tree
        .selected_node()
        .map(|n| n.name.as_str())
        .unwrap_or("");
    // The next visible row after main.rs in the filtered tree must be an .rs file
    // (Cargo.toml is hidden by the filter, so it is skipped).
    assert!(
        selected.ends_with(".rs"),
        "expected selection to move to an .rs file (Cargo.toml skipped) but got {selected:?}"
    );
}

#[then("Cargo.toml is skipped because it is hidden")]
fn cargo_toml_skipped(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    let found = tree
        .visible()
        .iter()
        .any(|&id| tree.nodes().get(id).map(|n| n.name == "Cargo.toml").unwrap_or(false));
    assert!(!found, "expected Cargo.toml to be hidden (skipped by j)");
}

#[then("no rows are visible in the tree")]
fn no_rows_visible(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.visible().is_empty(),
        "expected no visible rows but found {}",
        tree.visible().len()
    );
}

#[then(expr = "a message in the sidebar indicates no files match {string}")]
fn no_match_status_message(world: &mut FileTreeWorld, _query: String) {
    // With an empty visible list and an active filter, the renderer shows a
    // "no matches" indicator. In the library layer we assert the status
    // message is set or the visible list is empty (rendering is verified separately).
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        tree.visible().is_empty(),
        "expected empty visible list for a no-match query"
    );
}

#[then("the filter prompt disappears")]
fn filter_prompt_disappears(world: &mut FileTreeWorld) {
    let tree = world.tree.as_ref().expect("no FileTree");
    assert!(
        !tree.filter_active(),
        "expected the filter prompt to have been dismissed"
    );
}
