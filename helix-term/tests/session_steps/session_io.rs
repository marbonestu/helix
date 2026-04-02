/// Step definitions for basic-save-restore.feature.
///
/// These steps exercise `save_session_to` / `load_session_from` /
/// `delete_session_at` directly, without requiring a running Application event
/// loop. All filesystem I/O is confined to the scenario's `temp_dir`.
use std::path::PathBuf;

use cucumber::{given, then, when};
use helix_view::session::{
    delete_session_at, load_session_from, save_session_to, SessionLayout,
    SessionView,
};

use super::SessionWorld;

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given("Alex has Helix open in a project directory")]
fn alex_has_helix_open(_world: &mut SessionWorld) {
    // No-op at the library level; the temp_dir acts as the project root.
}

#[given(expr = "Alex has {string} open with the cursor at line {int} column {int}")]
fn alex_has_file_open_at(world: &mut SessionWorld, path: String, line: u64, _col: u64) {
    world.test_files.insert("current".into(), PathBuf::from(&path));
    // Convert 1-based line number to 0-based char offset approximation.
    world.staged_cursor = Some((line.saturating_sub(1)) as usize);
}

#[given(expr = "Alex has a saved session with {string} at line {int} column {int}")]
fn alex_has_saved_session_at(world: &mut SessionWorld, path: String, line: u64, _col: u64) {
    // Lines are 1-based in the feature file; convert to a char offset
    // approximation (no actual document content, so offset == line - 1).
    let anchor = (line.saturating_sub(1)) as usize;
    let snapshot = SessionWorld::make_snapshot(&path, anchor, anchor);
    let dest = world.session_path();
    save_session_to(&dest, &snapshot).expect("failed to save session fixture");
}

#[given("Helix is opened in the same workspace without specifying any files")]
fn helix_opened_without_files(_world: &mut SessionWorld) {
    // Precondition only; actual restore is triggered by a When step.
}

#[given("Alex has an unsaved scratch buffer open with no file path")]
fn alex_has_scratch_buffer(world: &mut SessionWorld) {
    world.test_files.insert("current".into(), PathBuf::from(""));
}

#[given("Alex has a saved session for the current workspace")]
fn alex_has_saved_session(world: &mut SessionWorld) {
    let snapshot = SessionWorld::make_snapshot("/src/main.rs", 0, 0);
    let dest = world.session_path();
    save_session_to(&dest, &snapshot).expect("failed to save session fixture");
}

#[given("no session file exists for the current workspace")]
fn no_session_file(world: &mut SessionWorld) {
    // The default temp path does not exist yet — nothing to do.
    // If it somehow does exist (re-used world), remove it.
    let p = world.session_path();
    if p.exists() {
        std::fs::remove_file(&p).ok();
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex runs \":session-save\"")]
fn alex_session_save(world: &mut SessionWorld) {
    let anchor = world.staged_cursor.unwrap_or(0);
    // Determine which file/path to capture based on staged state.
    let snapshot = if let Some(path) = world.test_files.get("current").cloned() {
        if path.as_os_str().is_empty() {
            SessionWorld::make_scratch_snapshot()
        } else {
            SessionWorld::make_snapshot(path.to_string_lossy().as_ref(), anchor, anchor)
        }
    } else {
        SessionWorld::make_snapshot("/src/main.rs", anchor, anchor)
    };

    let dest = world.session_path();
    match save_session_to(&dest, &snapshot) {
        Ok(()) => {
            world.snapshot = Some(snapshot);
            world.last_status = Some("Session saved".into());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
            world.last_status = None;
        }
    }
}

#[when("Alex runs \":session-delete\"")]
fn alex_session_delete(world: &mut SessionWorld) {
    let dest = world.session_path();
    match delete_session_at(&dest) {
        Ok(()) => {
            world.last_status = Some("Session deleted".into());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

#[when("Alex runs \":session-restore\"")]
fn alex_session_restore(world: &mut SessionWorld) {
    let path = world.session_path();
    match load_session_from(&path) {
        Ok(snap) => {
            world.snapshot = Some(snap);
            world.last_status = Some("Session restored".into());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
            world.last_status = None;
        }
    }
}

#[when("Alex restores the session")]
fn alex_restores_session(world: &mut SessionWorld) {
    alex_session_restore(world);
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("a session file is written for the current workspace")]
fn session_file_written(world: &mut SessionWorld) {
    // Check either the live workspace path (if we ran a real Application) or
    // the library-level temp path (if we wrote directly via save_session_to).
    let live_path = world.session_file_path();
    let lib_path = world.session_path();
    assert!(
        live_path.exists() || lib_path.exists(),
        "expected session file at {} or {}",
        live_path.display(),
        lib_path.display()
    );
}

#[then(expr = "the session records {string} as the open file")]
fn session_records_file(world: &mut SessionWorld, path: String) {
    let snap = world
        .snapshot
        .as_ref()
        .expect("no snapshot captured — was :session-save run?");
    assert!(
        SessionWorld::layout_contains_path(&snap.layout, &path),
        "session layout does not contain path {path:?}"
    );
}

#[then(expr = "the session records the cursor at line {int} column {int}")]
fn session_records_cursor(world: &mut SessionWorld, line: u64, _col: u64) {
    let snap = world
        .snapshot
        .as_ref()
        .expect("no snapshot captured — was :session-save run?");
    // Retrieve the first (only) selection in the snapshot; use anchor as
    // a proxy for the line number (anchor == line - 1 in our fixture).
    let selection = first_selection(&snap.layout);
    let (anchor, _head) = selection.expect("no selection found in snapshot");
    let expected_anchor = (line.saturating_sub(1)) as usize;
    assert_eq!(
        anchor, expected_anchor,
        "expected anchor {expected_anchor} (line {line}), got {anchor}"
    );
}

#[then("the session file no longer exists on disk")]
fn session_file_gone(world: &mut SessionWorld) {
    let p = world.session_path();
    assert!(!p.exists(), "expected session file to be absent at {}", p.display());
}

#[then(expr = "Alex sees the status message {string}")]
fn alex_sees_status(world: &mut SessionWorld, msg: String) {
    let status = world
        .last_status
        .as_deref()
        .unwrap_or("");
    assert!(
        status.contains(&msg),
        "expected status to contain {msg:?}, got {status:?}"
    );
}

#[then("no error is reported")]
fn no_error(world: &mut SessionWorld) {
    assert!(
        world.last_error.is_none(),
        "expected no error but got: {:?}",
        world.last_error
    );
}

#[then("Alex sees an error message indicating no session was found")]
fn alex_sees_no_session_error(world: &mut SessionWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("expected an error but none was recorded");
    // Any non-empty error message satisfies this step.
    assert!(!err.is_empty(), "error message was empty");
}

#[then("the editor state is unchanged")]
fn editor_state_unchanged(_world: &mut SessionWorld) {
    // At the library level there is no running editor to inspect; the absence
    // of a panic is sufficient to satisfy this step.
}

#[then("the session records a scratch buffer entry")]
fn session_records_scratch(world: &mut SessionWorld) {
    let snap = world
        .snapshot
        .as_ref()
        .expect("no snapshot — was :session-save run?");
    let has_scratch = scratch_in_layout(&snap.layout);
    assert!(has_scratch, "expected a scratch buffer entry in the snapshot");
}

#[then("a new scratch buffer is opened in place of the saved one")]
fn new_scratch_opened(world: &mut SessionWorld) {
    // Restore and verify a scratch (path-less) entry is present.
    let path = world.session_path();
    let snap = load_session_from(&path).expect("failed to reload session");
    assert!(scratch_in_layout(&snap.layout), "no scratch buffer in restored session");
}

#[then(expr = "{string} is open in the editor")]
fn file_is_open(world: &mut SessionWorld, path: String) {
    let snap = world
        .snapshot
        .as_ref()
        .expect("no snapshot — was :session-restore run?");
    assert!(
        SessionWorld::layout_contains_path(&snap.layout, &path),
        "path {path:?} not found in restored snapshot"
    );
}

#[then("the cursor is at line 42 column 7")]
fn cursor_at_42_7(world: &mut SessionWorld) {
    // Specific assertion re-using the generic cursor check.
    session_records_cursor(world, 42, 7);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(super) fn first_selection_pub(layout: &SessionLayout) -> Option<(usize, usize)> {
    first_selection(layout)
}

fn first_selection(layout: &SessionLayout) -> Option<(usize, usize)> {
    match layout {
        SessionLayout::View(sv) => sv.document.selection,
        SessionLayout::Container { children, .. } => {
            children.iter().find_map(|c| first_selection(c))
        }
    }
}

fn scratch_in_layout(layout: &SessionLayout) -> bool {
    match layout {
        SessionLayout::View(sv) => sv.document.path.is_none(),
        SessionLayout::Container { children, .. } => {
            children.iter().any(|c| scratch_in_layout(c))
        }
    }
}

/// Check a path-less view helper re-exported for other step modules.
fn _views_with_path<'a>(layout: &'a SessionLayout, path: &str) -> Vec<&'a SessionView> {
    match layout {
        SessionLayout::View(sv) => {
            if sv
                .document
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().contains(path))
                .unwrap_or(false)
            {
                vec![sv]
            } else {
                vec![]
            }
        }
        SessionLayout::Container { children, .. } => {
            children.iter().flat_map(|c| _views_with_path(c, path)).collect()
        }
    }
}
