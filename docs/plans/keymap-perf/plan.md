# Key Handling Performance Plan

## Problem

Key handling feels laggy, especially in modes like goto (`g`), window (`ctrl-w`), and other "sticky" submodes. The hot path contains unnecessary heap allocations on every single keystroke.

## Root Cause Analysis

### Hotspot 1 — Sticky node clone per keystroke (`keymap.rs:322`) 🔴 HIGH

```rust
// helix-term/src/keymap.rs:321-324
let trie_node = match self.sticky {
    Some(ref trie) => Cow::Owned(KeyTrie::Node(trie.clone())),  // ← FULL CLONE EVERY KEYSTROKE
    None => Cow::Borrowed(keymap),
};
```

When sticky mode is active (e.g., after pressing `g` for goto mode), every subsequent keystroke clones the **entire `KeyTrieNode`**, which contains an `IndexMap<KeyEvent, KeyTrie>` with 20–30 entries. This is a heap allocation + data copy on every single keypress while in that mode.

There is even a `// TODO: remove the sticky part and look up manually` comment on line 308 acknowledging the problem.

### Hotspot 2 — Infobox rebuilt on every keystroke (`editor.rs:937`) 🟡 MEDIUM

```rust
// helix-term/src/ui/editor.rs:937
cxt.editor.autoinfo = self.keymaps.sticky().map(|node| node.infobox());
```

`infobox()` iterates all node entries, groups by description, formats key strings, creates a `String`. This runs on **every keypress** in sticky mode, even when the sticky node hasn't changed and the displayed info is identical.

### Hotspot 3 — Pending node clone on partial sequences (`keymap.rs:344`) 🟡 MEDIUM

```rust
// helix-term/src/keymap.rs:344
KeymapResult::Pending(map.clone())  // ← FULL CLONE for each partial key sequence step
```

When in the middle of a multi-key sequence (e.g., typed `z` waiting for fold commands), the whole `KeyTrieNode` is cloned for the `Pending` result, used only to call `infobox()` at the call site.

### Hotspot 4 — `contains_key` double search on digit keys (`keymap.rs:295`, `editor.rs:1020`) 🟢 LOW

```rust
// editor.rs:1020 — called on every '1'-'9' keypress when count is None
(key!(i @ '1'..='9'), None) if !self.keymaps.contains_key(mode, event) => { ... }

// keymap.rs:295-302 — performs the trie traversal twice: once here, once in handle_keymap_event
pub fn contains_key(&self, mode: Mode, key: KeyEvent) -> bool {
    let keymaps = &*self.map();
    let keymap = &keymaps[&mode];
    keymap
        .search(self.pending())
        .and_then(KeyTrie::node)
        .is_some_and(|node| node.contains_key(&key))
}
```

If a digit IS a keymap binding, it does the trie lookup twice.

---

## Proposed Fixes

### Fix 1 — `KeyTrie::Node(Arc<KeyTrieNode>)` (addresses hotspots 1 & 3)

Change `KeyTrie::Node(KeyTrieNode)` to `KeyTrie::Node(Arc<KeyTrieNode>)`.

- The `Cow::Owned(KeyTrie::Node(Arc::clone(arc)))` on line 322 becomes O(1) — just a refcount bump
- `KeymapResult::Pending(Arc::clone(map))` on line 344 becomes O(1)
- `self.sticky = Some(Arc::clone(map))` on line 342 becomes O(1)
- Config-time `merge` and `merge_nodes` still clone (via `Arc::make_mut` / `Arc::unwrap_or_clone`) but those are one-time startup operations, not hot path

**Files changed:**
- `helix-view/src/info.rs` — add `Clone` derive to `Info`
- `helix-term/src/keymap.rs` — change enum variant, update `node()`, `node_mut()`, `merge()`, `search()`, `reverse_map()`, deserialization, `Keymaps` struct, `Keymaps::get()`
- `helix-term/src/keymap/macros.rs` — wrap node construction in `Arc::new()`
- `helix-term/src/ui/editor.rs` — update `Pending(node)` match arm (works via Deref, minimal change)

### Fix 2 — Cache the sticky infobox (addresses hotspot 2)

Add `sticky_infobox: Option<Info>` to `Keymaps`. Compute it exactly once when sticky is activated. Expose `sticky_infobox() -> Option<&Info>` and use it in `handle_keymap_event` instead of calling `node.infobox()` on every keypress.

**Files changed:**
- `helix-term/src/keymap.rs` — add `sticky_infobox` field, populate on sticky activation, clear on reset
- `helix-term/src/ui/editor.rs` — use `sticky_infobox().cloned()` instead of `sticky().map(|n| n.infobox())`

### Fix 3 — Eliminate `contains_key` double search (addresses hotspot 4)

Integrate the `contains_key` guard into the main keymap dispatch path. Instead of doing a separate `contains_key` check followed by a full `handle_keymap_event` lookup, cache the result of the initial lookup so it's only done once per digit key.

**Files changed:**
- `helix-term/src/keymap.rs` — add a `probe()` method that checks existence without full dispatch
- `helix-term/src/ui/editor.rs:command_mode` — restructure to avoid the double lookup

---

## Benchmark Plan

No benchmark infrastructure exists. We need to add it **before** any code changes so we can measure the current baseline.

### Benchmark crate

Add `helix-bench` as a new workspace crate using **criterion** (more established, HTML reports) or **divan** (lower boilerplate). Recommended: **divan** for its simplicity.

Location: `helix-bench/benches/keymap.rs`

### What to benchmark

| Benchmark | Scenario | What it measures |
|-----------|----------|-----------------|
| `keymap_normal_single_key` | Press `j` in normal mode, no sticky | Baseline: common case |
| `keymap_sticky_active_matched` | Press `h` in goto sticky mode | Hotspot 1: sticky clone |
| `keymap_sticky_active_pending` | Press `z` entering fold submenu | Hotspot 1 + 3: sticky + Pending clone |
| `keymap_sticky_activate` | Press `g` to enter goto mode | Sticky activation cost |
| `keymap_insert_mode` | Press `a` in insert mode | Insert mode hot path |
| `keymap_infobox_build` | `KeyTrieNode::infobox()` for goto node | Hotspot 2: raw infobox cost |
| `keymap_infobox_cached` | Accessing cached `sticky_infobox` | Fix 2 improvement |
| `keymaps_get_sequence` | Full `g`→`g` sequence | End-to-end multi-key |

### Workflow

```
# Baseline on master
git checkout master
cargo bench -p helix-bench -- --save-baseline master

# Implement fixes in feature branch
git checkout -b perf/keymap-arc-sticky

# Measure improvement
cargo bench -p helix-bench -- --baseline master
```

Criterion produces HTML reports in `target/criterion/` with regression detection. Any benchmark that regresses by more than 5% should block the PR.

### CI integration (optional)

Add a `bench` job to CI that runs on PRs touching `helix-term/src/keymap.rs` or `helix-term/src/ui/editor.rs`, comparing against the `master` baseline stored as a CI artifact.

---

## Implementation Order

1. **Create `helix-bench` crate** with criterion and the benchmark suite above
2. **Run baseline** on `master`, commit baseline data
3. **Create branch** `perf/keymap-arc-sticky`
4. **Implement Fix 1** (`Arc<KeyTrieNode>`) — biggest impact
5. **Run benchmarks** to confirm improvement
6. **Implement Fix 2** (infobox cache) — second biggest
7. **Run benchmarks** again
8. **Implement Fix 3** (double search) — smaller, do last
9. **Final benchmark run**, PR

---

## Expected Results

| Benchmark | Expected improvement |
|-----------|---------------------|
| `keymap_sticky_active_matched` | 5–20× faster (eliminate heap alloc) |
| `keymap_sticky_active_pending` | 5–20× faster |
| `keymap_sticky_activate` | ~2× faster (one Arc::new instead of double clone) |
| `keymap_infobox_build` | N/A (measures current cost as reference) |
| `keymap_infobox_cached` | ~50–100× faster vs rebuild |
| `keymap_normal_single_key` | No change (non-sticky path unchanged) |

---

## Files Involved

| File | Role |
|------|------|
| `helix-term/src/keymap.rs` | Core data structures + lookup (most changes) |
| `helix-term/src/keymap/macros.rs` | Node construction macro |
| `helix-term/src/ui/editor.rs` | Key dispatch, autoinfo update |
| `helix-view/src/info.rs` | Add `Clone` derive |
| `helix-bench/` | New crate (benchmarks) |
| `Cargo.toml` | Add `helix-bench` to workspace |
