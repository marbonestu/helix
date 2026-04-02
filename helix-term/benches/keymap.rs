/// Benchmarks for the key handling hot path.
///
/// Run with:
///   cargo bench -p helix-term --bench keymap
///
/// To save a baseline and compare:
///   cargo bench -p helix-term --bench keymap -- --save-baseline master
///   cargo bench -p helix-term --bench keymap -- --baseline master
///
/// NOTE: Each `iter_batched` closure returns `(result, keymaps)` so that
/// Criterion drops the `Keymaps` trie *after* the timing window ends.
/// Without this, the ~8 µs drop cost of the full keymap trie dominates all
/// single-key measurements.
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

use helix_term::keymap::{KeyTrieNode, KeymapResult, Keymaps};
use helix_term::{ctrl, key};
use helix_view::document::Mode;
use helix_view::input::KeyEvent;

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Extracts a node from the default normal-mode keymap by single-key path.
/// Panics if the key does not lead to a `Node` variant.
fn extract_normal_node(key: KeyEvent) -> KeyTrieNode {
    let keymaps = Keymaps::default();
    let map = keymaps.map();
    map[&Mode::Normal]
        .search(&[key])
        .expect("key should exist in normal keymap")
        .node()
        .expect("key should lead to a Node")
        .clone()
}

/// Returns a Keymaps with the Z (sticky View) mode already activated.
/// Used by benchmarks that measure per-keystroke cost inside sticky mode.
fn keymaps_with_sticky_view() -> Keymaps {
    let mut keymaps = Keymaps::default();
    keymaps.get(Mode::Normal, key!('Z'));
    debug_assert!(keymaps.sticky().is_some());
    keymaps
}

// ---------------------------------------------------------------------------
// Scenario 1: single key matched in normal mode (no sticky, no pending)
//
// Most common case (e.g. pressing 'j' to move down).
// Expected: Cow::Borrowed path — no clones, pure hash lookup.
// ---------------------------------------------------------------------------
fn bench_single_key_normal(c: &mut Criterion) {
    c.bench_function("keymap/normal/single_key_matched", |b| {
        b.iter_batched(
            Keymaps::default,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(key!('j')));
                debug_assert!(matches!(result, KeymapResult::Matched(_)));
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 2: first key of a multi-key sequence → Pending
//
// Pressing 'g' leads to the Goto sub-map (~24 entries). Produces
// KeymapResult::Pending(goto_node.clone()) — the most common expensive clone.
// ---------------------------------------------------------------------------
fn bench_pending_first_key(c: &mut Criterion) {
    c.bench_function("keymap/normal/pending_first_key_goto", |b| {
        b.iter_batched(
            Keymaps::default,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(key!('g')));
                debug_assert!(matches!(result, KeymapResult::Pending(_)));
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 3: second key completing a sequence → Matched
//
// After pressing 'g', press 'g' again (goto_file_start). The state machine
// traverses two trie levels and clears pending state.
// ---------------------------------------------------------------------------
fn bench_sequence_completion(c: &mut Criterion) {
    c.bench_function("keymap/normal/sequence_completion_gg", |b| {
        b.iter_batched(
            || {
                let mut keymaps = Keymaps::default();
                let r = keymaps.get(Mode::Normal, key!('g'));
                debug_assert!(matches!(r, KeymapResult::Pending(_)));
                keymaps
            },
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(key!('g')));
                debug_assert!(matches!(result, KeymapResult::Matched(_)));
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 4: sticky mode activation ('Z' enters sticky View mode)
//
// Pressing 'Z' clones the view node into self.sticky and returns Pending.
// This is the one-time entry cost before the per-keystroke hot path begins.
// ---------------------------------------------------------------------------
fn bench_sticky_activation(c: &mut Criterion) {
    c.bench_function("keymap/normal/sticky_activation_Z", |b| {
        b.iter_batched(
            Keymaps::default,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(key!('Z')));
                debug_assert!(matches!(result, KeymapResult::Pending(_)));
                debug_assert!(keymaps.sticky().is_some());
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 5: keystroke while sticky mode is active
//
// After 'Z' activates sticky View mode, each subsequent key goes through
// Cow::Owned(KeyTrie::Node(sticky_node.clone())) — the main hotspot.
// Sticky mode is only cleared by ESC; a matched leaf leaves it intact.
// ---------------------------------------------------------------------------
fn bench_sticky_active_matched(c: &mut Criterion) {
    c.bench_function("keymap/normal/sticky_active_matched", |b| {
        b.iter_batched(
            keymaps_with_sticky_view,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(key!('z')));
                debug_assert!(matches!(result, KeymapResult::Matched(_)));
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 6: ESC while sticky mode is active (clear sticky state)
// ---------------------------------------------------------------------------
fn bench_sticky_escape(c: &mut Criterion) {
    c.bench_function("keymap/normal/sticky_escape", |b| {
        b.iter_batched(
            keymaps_with_sticky_view,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(key!(Esc)));
                debug_assert!(keymaps.sticky().is_none());
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 7: single key in insert mode (typing a character)
//
// Insert mode has a small keymap; most characters fall through to NotFound
// and are then inserted as text. The most common editor operation.
// ---------------------------------------------------------------------------
fn bench_insert_mode_char(c: &mut Criterion) {
    c.bench_function("keymap/insert/char_not_found", |b| {
        b.iter_batched(
            Keymaps::default,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Insert), black_box(key!('a')));
                debug_assert!(matches!(result, KeymapResult::NotFound));
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 8: ctrl-w sequence (Window mode) — common two-key binding
// ---------------------------------------------------------------------------
fn bench_window_mode_pending(c: &mut Criterion) {
    c.bench_function("keymap/normal/pending_ctrl_w_window", |b| {
        b.iter_batched(
            Keymaps::default,
            |mut keymaps| {
                let result = keymaps.get(black_box(Mode::Normal), black_box(ctrl!('w')));
                debug_assert!(matches!(result, KeymapResult::Pending(_)));
                (result, keymaps)
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Scenario 9: infobox build cost for the Goto node (~24 entries)
//
// infobox() is called in handle_keymap_event on every keypress when sticky
// is active and on every Pending result. Measures the raw rebuild cost.
// The node is extracted once outside the loop to isolate infobox() alone.
// ---------------------------------------------------------------------------
fn bench_infobox_build_goto(c: &mut Criterion) {
    let goto_node = extract_normal_node(key!('g'));
    c.bench_function("keymap/infobox/build_goto_node", |b| {
        b.iter(|| black_box(goto_node.infobox()))
    });
}

// ---------------------------------------------------------------------------
// Scenario 10: infobox build cost for the sticky View node (~14 entries)
// ---------------------------------------------------------------------------
fn bench_infobox_build_view_sticky(c: &mut Criterion) {
    let view_node = extract_normal_node(key!('Z'));
    c.bench_function("keymap/infobox/build_view_sticky_node", |b| {
        b.iter(|| black_box(view_node.infobox()))
    });
}

// ---------------------------------------------------------------------------
// Scenario 11: sustained sticky mode — 100 consecutive keystrokes
//
// Simulates rapid scrolling while Z (sticky View) is active. Sticky mode
// persists across matched leaves — only ESC clears it — so each iteration
// correctly exercises the Cow::Owned clone on every single key.
// Keys are pre-expanded to eliminate modulo overhead from the measurement.
// ---------------------------------------------------------------------------
fn bench_sticky_sustained_sequence(c: &mut Criterion) {
    let scroll_keys = [key!('j'), key!('k'), key!('j'), key!('k'), key!('z')];
    let keys: Vec<KeyEvent> = (0..100).map(|i| scroll_keys[i % scroll_keys.len()]).collect();

    c.bench_function("keymap/normal/sticky_sustained_100_keys", |b| {
        b.iter_batched(
            keymaps_with_sticky_view,
            |mut keymaps| {
                for &k in &keys {
                    black_box(keymaps.get(Mode::Normal, black_box(k)));
                }
                keymaps
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_single_key_normal,
    bench_pending_first_key,
    bench_sequence_completion,
    bench_sticky_activation,
    bench_sticky_active_matched,
    bench_sticky_escape,
    bench_insert_mode_char,
    bench_window_mode_pending,
    bench_infobox_build_goto,
    bench_infobox_build_view_sticky,
    bench_sticky_sustained_sequence,
);
criterion_main!(benches);
