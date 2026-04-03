# Flash Jump Implementation Plan

A flash.nvim-style jump motion for Helix: type a trigger key, enter 1-2 search
characters, see labeled matches in the viewport, type a label to jump.

## Key Insight

Helix already has `goto_word` (`gw`) which demonstrates the full pattern:
generate `Overlay`s for labels, store them via `doc.set_jump_labels()`, render
them with the `ui.virtual.jump-label` theme scope, and accept two-character
label input via nested `cx.on_next_key()` calls. Flash extends this by using a
**search query** to determine the candidate set instead of word boundaries.

## Phase 1: MVP

Minimal changes to get a working 2-char flash jump by reusing existing
infrastructure.

### Step 1.1: Register commands

**File:** `helix-term/src/commands.rs`

Add to the `static_commands!` macro:

```rust
flash_jump, "Flash jump to a search match",
extend_flash_jump, "Extend selection to a flash jump match",
```

Dispatch functions:

```rust
fn flash_jump(cx: &mut Context) {
    flash_jump_impl(cx, Movement::Move)
}

fn extend_flash_jump(cx: &mut Context) {
    flash_jump_impl(cx, Movement::Extend)
}
```

### Step 1.2: Implement `flash_jump_impl`

**File:** `helix-term/src/commands.rs`

Flow:
1. Read `jump_label_alphabet` from config, compute label capacity
   (`alphabet.len()^2`).
2. `cx.on_next_key()` — capture first search character.
3. Find all occurrences of that character in the visible viewport range.
4. If zero matches → status message, return.
5. If one match → jump directly.
6. `cx.on_next_key()` — capture second search character.
7. Filter matches to positions where first char is followed by second char.
8. If zero → status message. If one → jump directly.
9. Otherwise delegate to the existing `jump_to_label()` (line ~6809).

```rust
fn flash_jump_impl(cx: &mut Context, behaviour: Movement) {
    let alphabet = &cx.editor.config().jump_label_alphabet;
    if alphabet.is_empty() {
        return;
    }
    let jump_label_limit = alphabet.len() * alphabet.len();

    cx.on_next_key(move |cx, first_event| {
        let Some(first_ch) = first_event.char() else { return; };

        let (view, doc) = current_ref!(cx.editor);
        let text = doc.text().slice(..);

        // Visible range
        let start = text.line_to_char(
            text.char_to_line(doc.view_offset(view.id).anchor),
        );
        let end = text.line_to_char(
            (view.estimate_last_doc_line(doc) + 1).min(text.len_lines()),
        );

        let first_matches: Vec<usize> =
            find_all_char_matches(text, first_ch, start..end);

        if first_matches.is_empty() {
            cx.editor.set_status("No matches");
            return;
        }
        if first_matches.len() == 1 {
            jump_to_single_match(cx, first_matches[0], behaviour);
            return;
        }

        cx.on_next_key(move |cx, second_event| {
            let Some(second_ch) = second_event.char() else { return; };

            let (view, doc) = current_ref!(cx.editor);
            let text = doc.text().slice(..);

            let filtered: Vec<Range> = first_matches
                .iter()
                .filter(|&&pos| {
                    text.get_char(pos + 1).is_some_and(|c| c == second_ch)
                })
                .map(|&pos| Range::new(pos, next_grapheme_boundary(text, pos)))
                .take(jump_label_limit)
                .collect();

            if filtered.is_empty() {
                cx.editor.set_status("No matches");
                return;
            }
            if filtered.len() == 1 {
                jump_to_single_match(cx, filtered[0].from(), behaviour);
                return;
            }

            jump_to_label(cx, filtered, behaviour);
        });
    });
}
```

### Step 1.3: Add match-collection helper

**File:** `helix-core/src/search.rs`

```rust
/// Collect all char positions in `range` where `ch` appears.
pub fn find_all_char_matches(
    text: RopeSlice,
    ch: char,
    range: std::ops::Range<usize>,
) -> Vec<usize> {
    let mut results = Vec::new();
    let mut pos = range.start;
    for c in text.chars_at(range.start) {
        if pos >= range.end {
            break;
        }
        if c == ch {
            results.push(pos);
        }
        pos += 1;
    }
    results
}
```

### Step 1.4: Add default keybindings

**File:** `helix-term/src/keymap/default.rs`

In the goto (`g`) sub-keymap for normal mode:

```rust
"s" => flash_jump,
```

In the goto sub-keymap for select mode:

```rust
"s" => extend_flash_jump,
```

### Step 1.5: Reuse `jump_to_label`

No changes needed. The existing function at `commands.rs:~6809` handles:
- Overlay creation from the alphabet
- `doc.set_jump_labels()` / `doc.remove_jump_labels()`
- Two nested `on_next_key()` calls for label character input
- `Movement::Move` vs `Movement::Extend`
- Jump list saving

---

## Phase 2: Live Preview with FlashPrompt Component

### Step 2.1: Create `FlashPrompt` component

**File:** `helix-term/src/ui/flash.rs` (new)

A lightweight `Component` that replaces the `on_next_key` chain with a richer
interactive experience:

```rust
pub struct FlashPrompt {
    query: String,
    matches: Vec<Range>,
    labels_active: bool,
    behaviour: Movement,
    snapshot: Selection,
    offset_snapshot: ViewPosition,
    view_id: ViewId,
    doc_id: DocumentId,
}

impl Component for FlashPrompt {
    fn handle_event(&mut self, event: &Event, ctx: &mut Context) -> EventResult {
        // On char input: append to query, recompute matches
        // After 2 chars: compute labels, show overlays
        // On label chars: jump and close
        // On Escape: restore snapshot, close
    }

    fn render(&mut self, area: Rect, frame: &mut Surface, ctx: &mut Context) {
        // Render "flash: XX" indicator in the status area
    }
}
```

### Step 2.2: Add match highlighting during search phase

Before labels appear, highlight all current matches with overlay highlights.
On each keystroke in the `FlashPrompt`:
1. Clear previous jump labels.
2. Recompute matches from the query.
3. If still in search phase (<2 chars or many matches), highlight matches.
4. If ready for label phase, call label assignment logic.

### Step 2.3: Add `FlashDimmer` decoration

**File:** `helix-term/src/ui/text_decorations.rs`

Dim all non-matching text while flash is active:

```rust
pub struct FlashDimmer {
    match_ranges: Vec<std::ops::Range<usize>>,
    dim_style: Style,
    current_match: usize,
}

impl Decoration for FlashDimmer {
    fn decorate_grapheme(
        &mut self,
        renderer: &mut TextRenderer,
        grapheme: &FormattedGrapheme,
    ) -> usize {
        // If grapheme is not in any match range, apply dim_style
    }
}
```

### Step 2.4: Register the module

Add `pub(crate) mod flash;` to `helix-term/src/ui/mod.rs`.

---

## Phase 3: Search Integration

### Step 3.1: Flash labels on `/` search

**File:** `helix-term/src/commands.rs`

Modify the `searcher` function to optionally show flash labels on all visible
matches when a config option is enabled. After pressing Enter to validate, show
labels on visible matches and let the user pick one instead of jumping to the
next match.

### Step 3.2: `flash_search` command

A hybrid command that opens the regex prompt, then on `PromptEvent::Validate`
collects all visible matches and delegates to `jump_to_label`.

---

## Phase 4: Configuration and Theme

### Step 4.1: Configuration

**File:** `helix-view/src/editor.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct FlashConfig {
    /// Number of search characters before showing labels (1 or 2)
    pub search_chars: u8,
    /// Dim non-matching text while flash is active
    pub dim_unmatched: bool,
    /// Follow search.smart-case setting
    pub smart_case: bool,
}
```

### Step 4.2: Theme scopes

Reuse existing `ui.virtual.jump-label` for labels. Add:

| Scope | Purpose |
|-------|---------|
| `ui.virtual.flash-match` | Highlight for matches before label assignment |
| `ui.virtual.flash-dim` | Dimming style for non-matching text |

---

## Phase 5: Edge Cases and Polish

### Handled automatically by reusing `jump_to_label`:
- **Multiple views**: labels stored per-view via `doc.jump_labels: HashMap<ViewId, ...>`
- **Extend mode**: `Movement::Extend` path already implemented
- **Jump list**: saved automatically

### Additional considerations:
- **Wrapped lines**: `estimate_last_doc_line` is an upper bound; extra labels
  beyond viewport simply don't render (same as `goto_word`)
- **Case sensitivity**: respect `config.search.smart_case` — if search chars
  are all lowercase, match case-insensitively
- **Unicode**: use `graphemes::next_grapheme_boundary` for offset calculation,
  not `pos + 1`
- **Virtual text / inlay hints**: don't affect char positions, overlay system
  handles this correctly

---

## Files Summary

| File | Action | Phase |
|------|--------|-------|
| `helix-term/src/commands.rs` | Add commands, `flash_jump_impl` | 1 |
| `helix-core/src/search.rs` | Add `find_all_char_matches` | 1 |
| `helix-term/src/keymap/default.rs` | Bind `gs` / select `gs` | 1 |
| `helix-term/src/ui/flash.rs` | New `FlashPrompt` component | 2 |
| `helix-term/src/ui/text_decorations.rs` | `FlashDimmer` decoration | 2 |
| `helix-term/src/ui/mod.rs` | Register flash module | 2 |
| `helix-view/src/editor.rs` | `FlashConfig` | 4 |
| `book/src/themes.md` | Document theme scopes | 4 |
| `book/src/keymap.md` | Document keybindings | 4 |
