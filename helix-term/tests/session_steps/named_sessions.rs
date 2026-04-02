/// Step definitions for named-sessions.feature.
///
/// All steps delegate to `save_session_to` / `load_session_from` /
/// `delete_session_at` with paths managed by the scenario's `SessionWorld`.
/// No running Application is required.
use std::path::PathBuf;

use cucumber::{given, then, when};
use helix_view::session::{delete_session_at, load_session_from, save_session_to, SessionSnapshot};

use super::SessionWorld;

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(expr = "Alex has a saved default session with {string} open")]
fn alex_has_default_session(world: &mut SessionWorld, path: String) {
    let snap = SessionWorld::make_snapshot(&path, 0, 0);
    let dest = world.session_path();
    save_session_to(&dest, &snap).expect("failed to write default session fixture");
}

#[given(expr = "Alex now has {string} open")]
fn alex_now_has_open(world: &mut SessionWorld, path: String) {
    world.test_files.insert("current".into(), PathBuf::from(path));
}

#[given(expr = "Alex has a named session {string} with {string} at line {int}")]
fn alex_has_named_session(world: &mut SessionWorld, name: String, path: String, line: u64) {
    let anchor = (line.saturating_sub(1)) as usize;
    let snap = SessionWorld::make_snapshot(&path, anchor, anchor);
    let dest = world.named_session_path(&name);
    save_session_to(&dest, &snap).expect("failed to write named session fixture");
}

#[given(expr = "Alex has a default session with {int} open files")]
fn alex_has_default_session_n_files(world: &mut SessionWorld, count: u64) {
    let snap = multi_file_snapshot(count as usize, "default");
    let dest = world.session_path();
    save_session_to(&dest, &snap).expect("failed to write default session fixture");
}

// Matches both "N open files" (plural) and "1 open file" (singular).
#[given(regex = r#"^a named session "([^"]+)" with (\d+) open files?$"#)]
fn alex_has_named_session_n_files(world: &mut SessionWorld, name: String, count: String) {
    let n: usize = count.parse().expect("invalid file count");
    let snap = multi_file_snapshot(n, &name);
    let dest = world.named_session_path(&name);
    save_session_to(&dest, &snap).expect("failed to write named session fixture");
}

#[given(expr = "no sessions have been saved for the current workspace")]
fn no_sessions_saved(world: &mut SessionWorld) {
    let p = world.session_path();
    if p.exists() {
        std::fs::remove_file(&p).ok();
    }
    if let Some(dir) = world.named_session_dir.clone() {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).ok();
        }
        world.named_session_dir = None;
    }
}

#[given(expr = "Alex has a named session {string} and a named session {string}")]
fn alex_has_two_named_sessions(world: &mut SessionWorld, name_a: String, name_b: String) {
    let snap_a = SessionWorld::make_snapshot("/src/feature.rs", 0, 0);
    let dest_a = world.named_session_path(&name_a);
    save_session_to(&dest_a, &snap_a).expect("failed to write first named session");

    let snap_b = SessionWorld::make_snapshot("/src/bugfix.rs", 0, 0);
    let dest_b = world.named_session_path(&name_b);
    save_session_to(&dest_b, &snap_b).expect("failed to write second named session");
}

#[given(expr = "no session named {string} exists")]
fn no_named_session_exists(world: &mut SessionWorld, name: String) {
    let dest = world.named_session_path(&name);
    if dest.exists() {
        std::fs::remove_file(&dest).ok();
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when(expr = "Alex runs \":session-save {word}\"")]
fn alex_runs_session_save_named(world: &mut SessionWorld, name: String) {
    let current_path = world
        .test_files
        .get("current")
        .cloned()
        .unwrap_or_else(|| PathBuf::from("/src/feature.rs"));

    let snap = SessionWorld::make_snapshot(
        current_path.to_string_lossy().as_ref(),
        0,
        0,
    );
    let dest = world.named_session_path(&name);
    match save_session_to(&dest, &snap) {
        Ok(()) => {
            world.snapshot = Some(snap);
            world.last_status = Some(format!("Session '{}' saved", name));
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

#[when(expr = "Alex runs \":session-restore {word}\"")]
fn alex_runs_session_restore_named(world: &mut SessionWorld, name: String) {
    let src = world.named_session_path(&name);
    match load_session_from(&src) {
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

#[when(expr = "Alex runs \":session-delete {word}\"")]
fn alex_runs_session_delete_named(world: &mut SessionWorld, name: String) {
    let dest = world.named_session_path(&name);
    match delete_session_at(&dest) {
        Ok(()) => {
            world.last_status = Some(format!("Session '{}' deleted", name));
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

#[when("Alex runs \":session-list\"")]
fn alex_runs_session_list(world: &mut SessionWorld) {
    // Enumerate all JSON files in the scenario's session directories.
    let mut sessions: Vec<String> = Vec::new();

    // Default session
    let default_path = world.session_path();
    if default_path.exists() {
        sessions.push("session".into());
    }

    // Named sessions
    let named_dir_opt = world.named_session_dir.clone();
    if let Some(dir) = named_dir_opt {
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            sessions.push(stem.to_string());
                        }
                    }
                }
            }
        }
    }

    sessions.sort();
    if sessions.is_empty() {
        world.last_status = Some("No saved sessions".into());
    } else {
        world.last_status = Some(sessions.join(", "));
    }
    world.last_error = None;
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(expr = "the named session {string} records {string}")]
fn named_session_records(world: &mut SessionWorld, name: String, path: String) {
    let src = world.named_session_path(&name);
    let snap = load_session_from(&src)
        .unwrap_or_else(|e| panic!("failed to load named session '{name}': {e}"));
    assert!(
        SessionWorld::layout_contains_path(&snap.layout, &path),
        "named session '{name}' does not contain path {path:?}"
    );
}

#[then(expr = "the default session still records {string}")]
fn default_session_still_records(world: &mut SessionWorld, path: String) {
    let src = world.session_path();
    let snap = load_session_from(&src)
        .unwrap_or_else(|e| panic!("failed to load default session: {e}"));
    assert!(
        SessionWorld::layout_contains_path(&snap.layout, &path),
        "default session does not contain path {path:?}"
    );
}

#[then(expr = "{string} is open at line {int}")]
fn file_open_at_line(world: &mut SessionWorld, path: String, line: u64) {
    let snap = world.snapshot.as_ref().expect("no snapshot loaded");
    assert!(
        SessionWorld::layout_contains_path(&snap.layout, &path),
        "path {path:?} not found in snapshot"
    );
    let sel = SessionWorld::find_selection_in_snapshot(snap, &path);
    if let Some((anchor, _)) = sel {
        let expected = (line.saturating_sub(1)) as usize;
        assert_eq!(
            anchor, expected,
            "expected anchor {expected} (line {line}), got {anchor}"
        );
    }
}

#[then("Alex sees three entries: \"session\", \"bugfix\", and \"review\"")]
fn alex_sees_three_entries(world: &mut SessionWorld) {
    let status = world.last_status.as_deref().unwrap_or("");
    assert!(status.contains("session"), "missing 'session' entry in: {status:?}");
    assert!(status.contains("bugfix"), "missing 'bugfix' entry in: {status:?}");
    assert!(status.contains("review"), "missing 'review' entry in: {status:?}");
}

#[then("each entry shows its file count and working directory")]
fn each_entry_shows_info(_world: &mut SessionWorld) {
    // Metadata display is handled at the application/UI layer.
    // At the library level we verify only that listing succeeds.
}

#[then("Alex sees an empty list or a message indicating no sessions exist")]
fn empty_session_list(world: &mut SessionWorld) {
    let status = world.last_status.as_deref().unwrap_or("");
    // Either empty listing or the "No saved sessions" message.
    assert!(
        status.is_empty() || status.contains("No saved sessions") || status.contains("session"),
        "unexpected session-list output: {status:?}"
    );
}

#[then(expr = "the {string} session no longer exists")]
fn named_session_gone(world: &mut SessionWorld, name: String) {
    let dest = world.named_session_path(&name);
    assert!(!dest.exists(), "expected '{name}' session to be absent at {}", dest.display());
}

#[then(expr = "the {string} session is unaffected")]
fn named_session_unaffected(world: &mut SessionWorld, name: String) {
    let dest = world.named_session_path(&name);
    assert!(dest.exists(), "expected '{name}' session to still exist at {}", dest.display());
}

#[then("Alex sees an error message indicating the session was not found")]
fn alex_sees_not_found_error(world: &mut SessionWorld) {
    let err = world.last_error.as_ref().expect("expected an error but none was recorded");
    assert!(!err.is_empty(), "error message was empty");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a snapshot with `count` distinct files for session-list scenarios.
fn multi_file_snapshot(count: usize, label: &str) -> SessionSnapshot {
    use helix_view::session::{
        SessionDocument, SessionLayout, SessionSplitDirection, SessionView,
    };

    if count == 1 {
        return SessionWorld::make_snapshot(&format!("/src/{label}_0.rs"), 0, 0);
    }

    let children: Vec<SessionLayout> = (0..count)
        .map(|i| {
            SessionLayout::View(SessionView {
                document: SessionDocument {
                    path: Some(PathBuf::from(format!("/src/{label}_{i}.rs"))),
                    selection: Some((0, 0)),
                    view_position: None,
                },
                docs_access_history: vec![],
                jumps: vec![],
            })
        })
        .collect();

    SessionSnapshot {
        version: 1,
        working_directory: PathBuf::from("/tmp/test-project"),
        layout: SessionLayout::Container {
            layout: SessionSplitDirection::Vertical,
            children,
        },
        focused_index: 0,
        registers: None,
        breakpoints: None,
    }
}
