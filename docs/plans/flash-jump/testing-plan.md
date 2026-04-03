# Flash Jump Testing Plan

Tests are organized in two layers: unit tests for the core helper in
`helix-core`, and integration tests for the full command flow in `helix-term`.

---

## Layer 1: Unit Tests — `helix-core/src/search.rs`

Test the `find_all_char_matches` helper in a `#[cfg(test)] mod tests` block
at the bottom of the file.

### 1.1 Basic matching

```rust
#[test]
fn find_char_matches_basic() {
    let rope = Rope::from("hello world");
    let text = rope.slice(..);
    let matches = find_all_char_matches(text, 'l', 0..text.len_chars());
    assert_eq!(matches, vec![2, 3, 9]);
}
```

### 1.2 No matches

```rust
#[test]
fn find_char_matches_none() {
    let rope = Rope::from("hello");
    let text = rope.slice(..);
    let matches = find_all_char_matches(text, 'z', 0..text.len_chars());
    assert!(matches.is_empty());
}
```

### 1.3 Restricted range

```rust
#[test]
fn find_char_matches_subrange() {
    let rope = Rope::from("abcabc");
    let text = rope.slice(..);
    // Only search chars 2..5 → "cab"
    let matches = find_all_char_matches(text, 'a', 2..5);
    assert_eq!(matches, vec![3]);
}
```

### 1.4 Unicode / multibyte characters

```rust
#[test]
fn find_char_matches_unicode() {
    let rope = Rope::from("café résumé");
    let text = rope.slice(..);
    let matches = find_all_char_matches(text, 'é', 0..text.len_chars());
    assert_eq!(matches, vec![3, 7, 11]);
}
```

### 1.5 Empty range

```rust
#[test]
fn find_char_matches_empty_range() {
    let rope = Rope::from("hello");
    let text = rope.slice(..);
    let matches = find_all_char_matches(text, 'h', 0..0);
    assert!(matches.is_empty());
}
```

### 1.6 Single-character document

```rust
#[test]
fn find_char_matches_single_char() {
    let rope = Rope::from("x");
    let text = rope.slice(..);
    let matches = find_all_char_matches(text, 'x', 0..1);
    assert_eq!(matches, vec![0]);
}
```

---

## Layer 2: Integration Tests — `helix-term/tests/test/commands/movement.rs`

These use the existing `test()` / `test_key_sequence` helpers with the
`#[tokio::test(flavor = "multi_thread")]` pattern. The tests send keystrokes
and assert final cursor position / selection.

### Challenge: Label non-determinism

`jump_to_label` assigns labels from `jump_label_alphabet` based on match
ordering. The default alphabet and label assignment are deterministic for a
given document + viewport, so we can predict which label characters to type.
However, tests must use small documents that fit entirely in the viewport to
keep label assignments predictable.

### 2.1 Flash jump — single match (auto-jump)

When only one match exists for the two-char query, the cursor should jump
directly without requiring label input.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_single_match() -> anyhow::Result<()> {
    // Cursor at 'h', type gs + "x" + "y" → only "xy" exists once → jump
    test((
        "#[h|]#ello world xy end\n",
        "gsxy",
        "hello world #[x|]#y end\n",
    ))
    .await
}
```

### 2.2 Flash jump — no matches

Cursor should not move. Status message "No matches" is set (not directly
assertable with `test()`, but cursor position confirms no movement).

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_no_match() -> anyhow::Result<()> {
    test((
        "#[h|]#ello world\n",
        "gszz",
        "#[h|]#ello world\n",
    ))
    .await
}
```

### 2.3 Flash jump — multiple matches with label selection

With multiple matches, the user types two label characters to select.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_with_labels() -> anyhow::Result<()> {
    // "he" appears twice: positions 0 and 10
    // The first match gets label "aa" (first two chars of default alphabet)
    // The second match gets label "ab"
    // Type "gs" + "he" + label chars to jump to second match
    let alphabet = // depends on default jump_label_alphabet
    // This test needs to be written once we confirm the default alphabet
    // and label ordering (proximity-based vs left-to-right)
    todo!("finalize once label ordering is confirmed")
}
```

**Note:** This test requires knowing the exact default `jump_label_alphabet`
and whether `flash_jump_impl` orders matches left-to-right (as planned) vs
proximity-to-cursor (as `goto_word` does). The plan uses left-to-right order,
which makes this test straightforward.

### 2.4 Flash jump — first char yields single match

If only one position in the viewport starts with the first typed character,
jump immediately after the first keystroke (no second char needed).

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_single_first_char_match() -> anyhow::Result<()> {
    test((
        "#[a|]#bcd efgh\n",
        "gse",
        "abcd #[e|]#fgh\n",
    ))
    .await
}
```

### 2.5 Extend flash jump — selection grows

Using `extend_flash_jump` (select mode `gs`), the selection should extend
from the current anchor to the jump target.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn extend_flash_jump() -> anyhow::Result<()> {
    test((
        "#[h|]#ello world xy end\n",
        "vgsxy",
        "#[hello world x|]#y end\n",
    ))
    .await
}
```

### 2.6 Flash jump — escape cancels

Pressing escape during either the first or second character input should
cancel without moving the cursor.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_escape_first_char() -> anyhow::Result<()> {
    test((
        "#[h|]#ello\n",
        "gs<esc>",
        "#[h|]#ello\n",
    ))
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_escape_second_char() -> anyhow::Result<()> {
    test((
        "#[h|]#ello hello\n",
        "gsh<esc>",
        "#[h|]#ello hello\n",
    ))
    .await
}
```

### 2.7 Flash jump — with count prefix

If the plan supports count (e.g., `3gs` → resize step = 3 × 0.2), verify
it works. If count is not supported, verify it doesn't cause unexpected
behavior.

### 2.8 Jump list integration

After a flash jump, `<C-o>` should return to the original position.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_saves_to_jumplist() -> anyhow::Result<()> {
    test((
        "#[h|]#ello world xy end\n",
        "gsxy<C-o>",
        "#[h|]#ello world xy end\n",
    ))
    .await
}
```

---

## Layer 3: Edge Case Tests

### 3.1 Empty document

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_empty_document() -> anyhow::Result<()> {
    test((
        "#[|\n]#",
        "gsa",
        "#[|\n]#",
    ))
    .await
}
```

### 3.2 Search char at end of line

Verify matching a character at the last position of a line doesn't panic
when checking `pos + 1` for the second character.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_match_at_eol() -> anyhow::Result<()> {
    // 'x' is at end of line, no second char follows → filtered out
    test((
        "#[a|]#bcx\ndefx\n",
        "gsxy",
        "#[a|]#bcx\ndefx\n",
    ))
    .await
}
```

### 3.3 Case sensitivity

If `smart_case` is deferred to a later phase, document that these tests
should be added when it lands:

- All-lowercase query matches uppercase occurrences (smart_case on)
- Mixed-case query matches exact case only
- Config toggle disables smart_case

### 3.4 Multibyte / grapheme cluster boundaries

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_multibyte() -> anyhow::Result<()> {
    // Search for "ñ" followed by "o"
    test((
        "#[h|]#ello niño end\n",
        "gsño",
        "hello ni#[ñ|]#o end\n",
    ))
    .await
}
```

### 3.5 Label capacity overflow

When the viewport has more matches than `alphabet.len()^2`, only the first
N matches should get labels. Remaining matches are silently ignored.

This is best tested with a `test_key_sequence` call using a custom config
with a very small alphabet (e.g., `['a', 'b']` → capacity 4) and a document
with more than 4 matches.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn flash_jump_label_overflow() -> anyhow::Result<()> {
    let config = Config {
        editor: helix_view::editor::Config {
            jump_label_alphabet: vec!['a', 'b'],  // capacity = 4
            ..test_editor_config()
        },
        ..test_config()
    };
    let app = AppBuilder::new()
        .with_config(config)
        .with_input_text("#[e|]#e ee ee ee ee ee\n")
        .build()?;

    // 'e' appears many times but only 4 labels available
    // Typing label "ba" should jump to the 3rd match (index 2)
    test_key_sequence(
        &mut app,
        Some("gsee ba"),  // search "ee", pick label "ba"
        Some(&|app| {
            let (view, doc) = current_ref!(app.editor);
            let sel = doc.selection(view.id).primary();
            // Verify cursor moved to one of the labeled matches
            assert_ne!(sel.cursor(doc.text().slice(..)), 0);
        }),
        false,
    )
    .await
}
```

---

## Test Matrix Summary

| Test | Layer | What it verifies |
|------|-------|-----------------|
| Basic char matching | Unit | `find_all_char_matches` correctness |
| No matches | Unit | Empty result handling |
| Subrange search | Unit | Range boundary respect |
| Unicode matching | Unit | Multibyte char support |
| Empty range | Unit | Edge case: zero-width range |
| Single match auto-jump | Integration | Skip label phase |
| No match no-op | Integration | Cursor stays put |
| Label selection | Integration | Full label flow |
| Single first-char match | Integration | Early exit after 1 char |
| Extend mode | Integration | Selection extension |
| Escape cancellation | Integration | Clean abort at each stage |
| Jump list save | Integration | `<C-o>` returns |
| Empty document | Edge case | No panic on empty |
| Match at EOL | Edge case | Boundary safety |
| Multibyte jump | Edge case | Grapheme correctness |
| Label overflow | Edge case | Capacity limiting |

---

## Tests NOT needed (and why)

- **Overlay rendering**: Visual overlays are an existing mechanism tested
  implicitly through `goto_word`. Flash reuses `jump_to_label` unchanged.
- **`jump_to_label` internals**: Already in production via `gw`. Testing
  flash's usage of it (correct `Vec<Range>` passed in) is sufficient.
- **Terminal resize during flash**: The `on_next_key` callback model means
  resize events are handled by the editor loop, not by flash code.
- **Multiple cursors with flash**: Flash operates on the primary selection
  only (same as `goto_word`). No special handling needed.
