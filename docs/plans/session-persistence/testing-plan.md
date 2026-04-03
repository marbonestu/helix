# Session Persistence Testing Plan

Testing strategy for the session persistence feature described in `plan.md`.
Follows helix's existing patterns: inline `#[cfg(test)]` unit tests in
`helix-view`, async integration tests in `helix-term/tests/` using `AppBuilder`
and `test_key_sequences`.

---

## 1. Unit Tests — `helix-view/src/session.rs`

Inline `#[cfg(test)] mod tests` at the bottom of the session module.
These test serialization types, snapshot capture, and file I/O in isolation.

### 1.1 Serialization Round-Trip

| Test | What it verifies |
|------|-----------------|
| `session_snapshot_roundtrip` | Serialize a `SessionSnapshot` to JSON and deserialize it back. All fields match. |
| `session_snapshot_unknown_fields_ignored` | Deserialize JSON with extra unknown fields (forward compat). No error. |
| `session_snapshot_version_preserved` | Version field survives round-trip unchanged. |
| `session_layout_nested_containers` | A deeply nested `Container { Container { View, View }, View }` round-trips correctly. |
| `session_document_none_path` | `SessionDocument` with `path: None` (scratch buffer) serializes/deserializes. |
| `session_document_unicode_path` | Paths with unicode characters round-trip correctly. |

### 1.2 Snapshot Capture

Requires constructing a minimal `Editor` with documents and a split tree.
Follow the pattern from `tree.rs` tests (create `View`, `Document::from(...)`,
manipulate the tree).

| Test | Setup | Assertion |
|------|-------|-----------|
| `capture_single_view` | Editor with one document open | Snapshot has `SessionLayout::View` with correct path, selection offsets, view position |
| `capture_horizontal_split` | Editor with `hsplit` (2 views, same doc) | Snapshot has `Container { Horizontal, [View, View] }` |
| `capture_vertical_split` | Editor with `vsplit` | Same but `Vertical` |
| `capture_nested_splits` | `vsplit` then `hsplit` in one pane | Nested container structure matches tree topology |
| `capture_focused_view_index` | Focus is on second view | `focused_index` is 1 |
| `capture_multiple_documents` | 3 docs open in splits | Each `SessionView.document.path` matches the corresponding doc |
| `capture_selection_offsets` | Document with cursor at known position | `selection` field matches `(anchor, head)` char offsets |
| `capture_scroll_position` | View scrolled to line 50 | `view_position` field matches `(50, 0)` |
| `capture_docs_access_history` | Switch between buffers in a view | `docs_access_history` contains paths in MRU order |
| `capture_scratch_buffer` | Scratch buffer (no path) in a view | `document.path` is `None` |
| `capture_working_directory` | `std::env::current_dir()` at capture time | `working_directory` matches |

### 1.3 Session File I/O

Use `tempfile::TempDir` to avoid polluting the filesystem.

| Test | What it verifies |
|------|-----------------|
| `save_and_load_session` | `save_session` writes file; `load_session` reads it back identically |
| `save_creates_parent_dirs` | Parent directories are created if missing |
| `load_missing_file_returns_error` | `load_session` on nonexistent path returns `Err` |
| `load_corrupt_json_returns_error` | Malformed JSON returns a parse error, not a panic |
| `delete_session_removes_file` | After `delete_session`, file no longer exists |
| `delete_nonexistent_session_ok` | Deleting when no file exists returns `Ok(())` |
| `session_path_uses_workspace_dir` | When `.helix/` exists, `session_path()` returns `.helix/session.json` |
| `session_path_falls_back_to_cache` | When `.helix/` absent, returns cache dir path with hash |
| `hash_path_deterministic` | Same path always produces same hash |
| `hash_path_different_for_different_dirs` | Different paths produce different hashes |

### 1.4 Phase 2 — Registers

| Test | What it verifies |
|------|-----------------|
| `capture_registers_includes_named` | Registers `a`-`z`, `"`, `/` are captured |
| `capture_registers_excludes_special` | Registers `_`, `#`, `.`, `%`, `*`, `+` are excluded |
| `registers_roundtrip` | Captured register map survives serialize/deserialize |
| `restore_registers_merges_values` | `restore_registers` pushes values into existing Registers |
| `restore_registers_skips_readonly` | Read-only register names in saved data are silently ignored |

### 1.5 Phase 2 — Jump Lists

| Test | What it verifies |
|------|-----------------|
| `capture_jump_list` | View with jump entries produces `SessionJump` entries with correct paths/offsets |
| `capture_empty_jump_list` | View with no jumps produces empty `jumps` vec |
| `jump_list_roundtrip` | Jump list survives serialization |

---

## 2. Unit Tests — `helix-view/src/editor.rs`

### 2.1 SessionConfig

| Test | What it verifies |
|------|-----------------|
| `session_config_default` | Default is `persist: false, scope: Layout` |
| `session_config_deserialize_toml` | `persist = true` / `scope = "full"` parses correctly |
| `session_config_deny_unknown_fields` | Unknown keys in `[editor.session]` produce an error |
| `session_scope_kebab_case` | `"layout"` and `"full"` are the valid values |

---

## 3. Integration Tests — `helix-term/tests/test/session.rs`

New module `session.rs` under `helix-term/tests/test/`, registered in
`helix-term/tests/integration.rs`. Uses `AppBuilder`, `test_key_sequences`,
and `tempfile` for file fixtures.

All tests are `#[tokio::test(flavor = "multi_thread")]` and gated on
`#[cfg(feature = "integration")]`.

### 3.1 Save and Restore — Single Buffer

```
Setup:  Create temp file with known content.
        Open in AppBuilder, move cursor to line 10 col 5.
        Run :session-save.
Action: Build a new Application in the same temp dir.
        Run :session-restore.
Assert: - Same file is open
        - Cursor is at line 10 col 5
        - Status shows "Session restored"
```

### 3.2 Save and Restore — Split Layout

```
Setup:  Open file, :vsplit, open second file in new pane, :hsplit.
        Run :session-save.
Action: New Application, :session-restore.
Assert: - 3 views exist (verified via editor.tree traversal)
        - Each view shows the correct file
        - Split directions match (V containing [View, H containing [View, View]])
```

### 3.3 Restore Focus

```
Setup:  Open file, :vsplit, focus right pane.
        :session-save.
Action: New Application, :session-restore.
Assert: - Focused view is the right pane (same document as before)
```

### 3.4 Restore Scroll Position

```
Setup:  Open a file with 200+ lines.
        Scroll to line 150. :session-save.
Action: New Application, :session-restore.
Assert: - View position anchor is at/near line 150
```

### 3.5 Missing File on Restore

```
Setup:  Open two files, :session-save.
        Delete one file from disk.
Action: New Application, :session-restore.
Assert: - Remaining file is open
        - Missing file is skipped (no panic)
        - Warning or status message about skipped file
```

### 3.6 Auto-Save on Exit (`:q`)

```
Setup:  Config with session.persist = true.
        Open file via AppBuilder.
Action: Execute :q (quit).
Assert: - Session file exists on disk
        - Contains the correct file path
```

### 3.7 Auto-Restore on Startup (No CLI Files)

```
Setup:  Config with session.persist = true.
        Manually write a valid session.json for a temp file.
Action: Start Application with no files on CLI.
Assert: - The file from the session is open
        - Cursor/scroll restored
```

### 3.8 CLI Files Override Session Restore

```
Setup:  Config with session.persist = true.
        Session file references file_a.
Action: Start Application with file_b on CLI.
Assert: - file_b is open, NOT file_a
        - Session is not restored when files are given explicitly
```

### 3.9 Session Delete

```
Setup:  :session-save, verify file exists.
Action: :session-delete.
Assert: - Session file no longer exists
        - Status shows "Session deleted"
```

### 3.10 Persist Disabled by Default

```
Setup:  Default config (session.persist = false).
Action: :q.
Assert: - No session file is written
```

### 3.11 Cursor Position Clamped on Restore

```
Setup:  Open file with 100 chars, place cursor at offset 80.
        :session-save.
        Truncate file to 50 chars on disk.
Action: :session-restore.
Assert: - File opens without panic
        - Cursor is clamped to valid position (≤ 50)
```

### 3.12 Empty Session Restore (No Session File)

```
Action: :session-restore with no session file on disk.
Assert: - Error status "No session found: ..."
        - Editor state unchanged
```

---

## 4. Integration Tests — Phase 2

### 4.1 Registers Round-Trip

```
Setup:  Config with scope = "full".
        Set register "a to "hello", register "b to "world".
        :session-save.
Action: New Application, :session-restore.
Assert: - Register "a contains "hello"
        - Register "b contains "world"
```

### 4.2 Search History Round-Trip

```
Setup:  scope = "full". Perform /search_term.
        :session-save.
Action: New Application, :session-restore.
Assert: - Register / contains "search_term"
```

### 4.3 Jump List Round-Trip

```
Setup:  Open file, jump to several locations (gg, G, 50G).
        :session-save.
Action: New Application, :session-restore.
Assert: - Jump list entries match saved locations
```

---

## 5. Edge Case Tests

| Test | Category | What it verifies |
|------|----------|-----------------|
| `restore_with_wrong_cwd` | Integration | Session saved in /a, restored from /b — paths resolve correctly (absolute) |
| `schema_version_mismatch` | Unit | Loading version 99 session file is handled gracefully (error or migration) |
| `empty_editor_capture` | Unit | Capturing session from editor with only scratch buffer produces valid snapshot |
| `very_large_jump_list` | Unit | Jump list at capacity (30) round-trips without truncation issues |
| `concurrent_save_no_corruption` | Unit | Two rapid `save_session` calls don't produce corrupt JSON (atomic write) |
| `symlinked_file_path` | Integration | File opened via symlink — path in session resolves on restore |
| `read_only_session_dir` | Unit | `save_session` returns meaningful error when directory is not writable |
| `binary_file_in_session` | Integration | Session with a binary file open doesn't panic on capture/restore |

---

## 6. Test Infrastructure Needed

### 6.1 Helpers to Add

Add to `helix-term/tests/test/helpers.rs` or a new `session_helpers.rs`:

```rust
/// Create an AppBuilder with session persistence enabled
fn app_builder_with_session(persist: bool, scope: SessionScope) -> AppBuilder

/// Write a session file to a temp directory for restore testing
fn write_test_session(dir: &Path, snapshot: &SessionSnapshot)

/// Read and parse the session file from a temp directory
fn read_test_session(dir: &Path) -> SessionSnapshot

/// Assert that two SessionSnapshots are structurally equivalent
/// (ignoring working_directory differences)
fn assert_sessions_equivalent(a: &SessionSnapshot, b: &SessionSnapshot)
```

### 6.2 Test Fixtures

- `fixture_200_lines.txt` — 200-line file for scroll position tests
- No other fixtures needed — use `tempfile::NamedTempFile` with inline content

---

## 7. Test Execution

```bash
# Unit tests (session module)
cargo test -p helix-view --lib session

# Unit tests (config)
cargo test -p helix-view --lib editor::tests::session

# Integration tests
cargo test -p helix-term --features integration session
```

---

## 8. Phase Coverage Matrix

| Phase | Unit Tests | Integration Tests | Total |
|-------|-----------|-------------------|-------|
| 1 (MVP) | ~22 | ~12 | ~34 |
| 2 (Registers) | ~8 | ~3 | ~11 |
| 3 (History) | — | ~2 | ~2 |
| 4 (Named) | ~2 | ~4 | ~6 |
| 5 (Edge cases) | ~4 | ~4 | ~8 |
| **Total** | **~36** | **~25** | **~61** |
