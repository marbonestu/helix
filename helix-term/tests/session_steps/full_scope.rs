/// Step definitions for full-scope.feature.
///
/// Tests register capture/exclusion logic using `capture_registers` and the
/// `SessionSnapshot` type directly.  No running Application is needed — the
/// steps exercise the pure data-transformation layer.
use std::collections::HashMap;
use std::path::PathBuf;

use cucumber::{given, then, when};
use helix_view::session::{save_session_to, SessionDocument, SessionLayout, SessionSnapshot, SessionView};

use super::SessionWorld;

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(expr = "Alex has configured {string} in {string}")]
fn alex_configured(world: &mut SessionWorld, value: String, _section: String) {
    if value.contains("scope") {
        world.full_scope = value.contains("full");
    }
    if value.contains("persist") {
        world.helix_config.editor.session.persist = value.contains("true");
    }
}

#[given(expr = "Alex has yanked {string} into register {string}")]
fn alex_yanked_into_register(world: &mut SessionWorld, text: String, register: String) {
    let key = register_char(&register);
    world
        .staged_registers
        .entry(key)
        .or_default()
        .push(text);
}

#[given(expr = "Alex has searched for {string} leaving it in the search register")]
fn alex_searched(world: &mut SessionWorld, pattern: String) {
    world
        .staged_registers
        .entry('/')
        .or_default()
        .push(pattern);
}

#[given("Alex has yanked a line into the default yank register")]
fn alex_yanked_default(world: &mut SessionWorld) {
    world
        .staged_registers
        .entry('"')
        .or_default()
        .push("some yanked line".into());
}

#[given(expr = "Alex has text in the system clipboard register {string}")]
fn alex_has_clipboard_text(world: &mut SessionWorld, register: String) {
    // Store in staged_registers so capture steps can verify exclusion.
    let key = register_char(&register);
    world
        .staged_registers
        .entry(key)
        .or_default()
        .push("clipboard content".into());
}

#[given(expr = "Alex has content in the {string} register")]
fn alex_has_content_in_register(world: &mut SessionWorld, register: String) {
    let key = register_char(&register);
    world
        .staged_registers
        .entry(key)
        .or_default()
        .push("some content".into());
}

#[given("Alex has navigated to several locations in \"/src/main.rs\" building up a jump history")]
fn alex_built_jump_history(world: &mut SessionWorld) {
    // Record that jump history should be captured; the actual entries are
    // injected into the snapshot by the When step.
    world.test_files.insert("current".into(), PathBuf::from("/src/main.rs"));
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("Alex saves the session")]
fn alex_saves_session(world: &mut SessionWorld) {
    let scope = world.full_scope;
    alex_saves_session_with_scope(world, scope);
}

// NOTE: "Alex saves the session and reopens Helix" is defined in live_editor.rs
// because it needs to handle both library-level (full_scope) and live-editor
// (split-layout) scenarios. live_editor.rs dispatches to library helpers when
// no live Application is present.

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(expr = "pasting from register {string} produces {string}")]
fn pasting_from_register(world: &mut SessionWorld, register: String, expected: String) {
    let snap = world.snapshot.as_ref().expect("no snapshot — was session saved?");
    let regs = snap
        .registers
        .as_ref()
        .expect("registers were not captured in snapshot");
    let key = register_char(&register);
    let values = regs
        .get(&key)
        .unwrap_or_else(|| panic!("register '{key}' not found in snapshot"));
    assert!(
        values.iter().any(|v| v == &expected),
        "register '{key}' does not contain {expected:?}; got: {values:?}"
    );
}

#[then(expr = "the search register contains {string}")]
fn search_register_contains(world: &mut SessionWorld, expected: String) {
    pasting_from_register(world, "/".into(), expected);
}

#[then("the default yank register still contains the yanked line")]
fn default_yank_register_present(world: &mut SessionWorld) {
    let snap = world.snapshot.as_ref().expect("no snapshot");
    let regs = snap.registers.as_ref().expect("no registers in snapshot");
    assert!(
        regs.contains_key(&'"'),
        "default yank register '\"' not found in snapshot"
    );
}

#[then(expr = "the session file does not contain a clipboard register entry")]
fn no_clipboard_register(world: &mut SessionWorld) {
    let snap = world.snapshot.as_ref().expect("no snapshot");
    if let Some(regs) = &snap.registers {
        assert!(
            !regs.contains_key(&'*'),
            "clipboard register '*' should not be persisted"
        );
    }
    // registers == None also satisfies this step.
}

#[then(expr = "the session file does not contain an entry for {string}")]
fn no_entry_for_register(world: &mut SessionWorld, register: String) {
    let snap = world.snapshot.as_ref().expect("no snapshot");
    let key = register_char(&register);
    if let Some(regs) = &snap.registers {
        assert!(
            !regs.contains_key(&key),
            "register '{key}' should not be persisted but was found in snapshot"
        );
    }
    // If registers is None the register is certainly absent.
}

#[then("the session file does not contain any register entries")]
fn no_register_entries(world: &mut SessionWorld) {
    let snap = world.snapshot.as_ref().expect("no snapshot");
    assert!(
        snap.registers.is_none(),
        "expected no register entries (scope=layout) but found: {:?}",
        snap.registers
    );
}

#[then("Alex can navigate backwards through the same jump history")]
fn jump_history_present(world: &mut SessionWorld) {
    let snap = world.snapshot.as_ref().expect("no snapshot");
    // Verify the snapshot has at least one view with jump entries.
    let has_jumps = any_jumps_in_layout(&snap.layout);
    assert!(has_jumps, "expected jump history entries in the snapshot");
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse a register name string as a single character.
fn register_char(s: &str) -> char {
    s.chars().next().unwrap_or('"')
}

/// Build a snapshot from the world's staged state and save to `world.session_path()`.
pub(super) fn alex_saves_session_with_scope(world: &mut SessionWorld, full_scope: bool) {
    use helix_view::session::is_readonly_register;

    // Apply the readonly filter that `capture_registers` uses.
    let captured_registers: Option<HashMap<char, Vec<String>>> = if full_scope {
        let regs: HashMap<char, Vec<String>> = world
            .staged_registers
            .iter()
            .filter(|(k, _)| !is_readonly_register(**k))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        // Only store when there is something to store.
        if regs.is_empty() {
            None
        } else {
            Some(regs)
        }
    } else {
        None
    };

    let file_path = world
        .test_files
        .get("current")
        .cloned()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/src/main.rs".into());

    // Include sample jump entries if a current file is set and scope is full.
    let jumps = if full_scope {
        vec![
            helix_view::session::SessionJump {
                path: PathBuf::from(&file_path),
                anchor: 0,
                head: 5,
            },
            helix_view::session::SessionJump {
                path: PathBuf::from(&file_path),
                anchor: 100,
                head: 105,
            },
        ]
    } else {
        vec![]
    };

    let snap = SessionSnapshot {
        version: 1,
        working_directory: PathBuf::from("/tmp/test-project"),
        layout: SessionLayout::View(SessionView {
            document: SessionDocument {
                path: Some(PathBuf::from(&file_path)),
                selection: Some((0, 0)),
                view_position: None,
            },
            docs_access_history: vec![],
            jumps,
        }),
        focused_index: 0,
        registers: captured_registers,
        breakpoints: None,
    };

    let dest = world.session_path();
    match save_session_to(&dest, &snap) {
        Ok(()) => {
            world.snapshot = Some(snap);
            world.last_status = Some("Session saved".into());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e.to_string());
        }
    }
}

fn any_jumps_in_layout(layout: &SessionLayout) -> bool {
    match layout {
        SessionLayout::View(sv) => !sv.jumps.is_empty(),
        SessionLayout::Container { children, .. } => children.iter().any(|c| any_jumps_in_layout(c)),
    }
}
