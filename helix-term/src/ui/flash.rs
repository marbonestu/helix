use std::collections::HashSet;

use helix_core::{
    graphemes::next_grapheme_boundary,
    movement::Direction,
    search::find_all_char_matches,
    text_annotations::Overlay,
    Range, Selection, Tendril,
};
use helix_core::movement::Movement;
use helix_view::{
    input::KeyEvent,
    keyboard::KeyCode,
    DocumentId, Editor, ViewId,
};
use helix_core::SmallVec;

use crate::compositor::{self, Component, Context, EventResult};

use helix_view::graphics::Rect;
use tui::buffer::Buffer as Surface;

/// A match with its assigned label and the position where the label overlay
/// is rendered (immediately after the matched text, like flash.nvim).
#[derive(Clone)]
struct LabeledMatch {
    range: Range,
    label: char,
    /// Char index where the label overlay is placed (after the match).
    label_pos: usize,
}

/// Interactive flash jump following flash.nvim's model.
///
/// The search and label phases are interleaved: labels appear from the first
/// character typed and are placed *after* the matched text (overlaying the
/// next character). Each subsequent keystroke first checks whether it
/// matches an existing label (→ jump) and otherwise extends the search
/// pattern to narrow down the matches.
///
/// Labels that would conflict with the "continuation character" (the char
/// right after each match) are removed from the pool so that extending the
/// search is never ambiguous with selecting a label.
///
/// When `direction` is `Some(Forward)` (the `/` key), only matches at or after
/// the cursor are labelled; `Some(Backward)` (the `?` key) restricts to matches
/// before the cursor with labels ordered closest-first. `None` searches the
/// entire visible viewport in both directions.
pub struct FlashPrompt {
    query: String,
    labeled: Vec<LabeledMatch>,
    behaviour: Movement,
    /// Restricts visible matches to one side of the cursor, or `None` for the
    /// full viewport.
    direction: Option<Direction>,
    snapshot: Selection,
    view_id: ViewId,
    doc_id: DocumentId,
    /// If set, save the query to this register on successful jump (for n/N).
    search_register: Option<char>,
}

impl FlashPrompt {
    pub fn new(
        behaviour: Movement,
        view_id: ViewId,
        doc_id: DocumentId,
        snapshot: Selection,
    ) -> Self {
        Self {
            query: String::new(),
            labeled: Vec::new(),
            behaviour,
            direction: None,
            snapshot,
            view_id,
            doc_id,
            search_register: None,
        }
    }

    /// Set a register to save the query to on successful jump (for n/N).
    pub fn with_search_register(mut self, reg: char) -> Self {
        self.search_register = Some(reg);
        self
    }

    /// Restrict match candidates to one side of the cursor.
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Visible viewport range as (start_char, end_char).
    fn viewport_range(&self, editor: &Editor) -> (usize, usize) {
        let doc = &editor.documents[&self.doc_id];
        let view = editor.tree.get(self.view_id);
        let text = doc.text().slice(..);
        let start = text.line_to_char(text.char_to_line(doc.view_offset(self.view_id).anchor));
        let end = text.line_to_char(
            (view.estimate_last_doc_line(doc) + 1).min(text.len_lines()),
        );
        (start, end)
    }

    /// Whether the current query uses case-insensitive matching.
    fn case_insensitive(&self, editor: &Editor) -> bool {
        let config = editor.config();
        if config.flash.smart_case {
            !self.query.chars().any(|c| c.is_uppercase())
        } else {
            false
        }
    }

    /// Find all char-index positions in the viewport where the full query
    /// matches (multi-char sequential match starting from first char).
    ///
    /// Results are filtered by [`Self::direction`] relative to the cursor
    /// position captured in [`Self::snapshot`]:
    /// - `Forward`: positions at or after the cursor (used for `/`)
    /// - `Backward`: positions strictly before the cursor, reversed so that
    ///   the closest match gets the first label (used for `?`)
    fn find_match_positions(&self, editor: &Editor) -> Vec<usize> {
        let ci = self.case_insensitive(editor);
        let doc = &editor.documents[&self.doc_id];
        let text = doc.text().slice(..);
        let (start, end) = self.viewport_range(editor);

        let chars: Vec<char> = self.query.chars().collect();
        if chars.is_empty() {
            return Vec::new();
        }

        // Use the snapshot cursor so the range boundary is stable while
        // matches are highlighted (the live selection changes during search).
        let cursor = self.snapshot.primary().cursor(text);
        let search_range = match self.direction {
            Some(Direction::Forward) => cursor..end,
            Some(Direction::Backward) => start..cursor,
            None => start..end,
        };

        let first_positions = find_all_char_matches(text, chars[0], search_range, ci);

        let mut positions: Vec<usize> = first_positions
            .into_iter()
            .filter(|&pos| {
                chars[1..].iter().enumerate().all(|(offset, &qch)| {
                    matches!(text.get_char(pos + 1 + offset), Some(c) if char_eq(c, qch, ci))
                })
            })
            .collect();

        // Reverse backward results so labels are assigned closest-first.
        if self.direction == Some(Direction::Backward) {
            positions.reverse();
        }

        positions
    }

    /// Set multi-selection on the document to highlight all match ranges.
    fn highlight_matches(&self, matches: &[Range], editor: &mut Editor) {
        if matches.is_empty() {
            return;
        }
        let ranges: SmallVec<[Range; 1]> = matches.iter().copied().collect();
        let selection = Selection::new(ranges, 0);
        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.set_selection(self.view_id, selection);
    }

    /// Assign single-character labels to matches. Labels are placed at
    /// `match_pos + query_len` (the character right after the matched text).
    /// Labels that equal a continuation character are removed from the pool
    /// (flash.nvim "smart skip").
    fn update(&mut self, editor: &mut Editor) {
        if self.query.is_empty() {
            self.labeled.clear();
            let doc = editor.documents.get_mut(&self.doc_id).unwrap();
            doc.remove_jump_labels(self.view_id);
            self.restore_snapshot(editor);
            return;
        }

        let config = editor.config();
        let alphabet = &config.jump_label_alphabet;
        if alphabet.is_empty() {
            self.labeled.clear();
            return;
        }

        let ci = self.case_insensitive(editor);
        let doc = &editor.documents[&self.doc_id];
        let text = doc.text().slice(..);
        let query_len = self.query.chars().count();

        let positions = self.find_match_positions(editor);

        // Smart skip: collect all unique continuation characters (the char
        // right after each match). Remove these from the label pool so that
        // typing the continuation char always extends the search.
        let mut skip: HashSet<char> = HashSet::new();
        for &pos in &positions {
            if let Some(ch) = text.get_char(pos + query_len) {
                let key = if ci {
                    ch.to_lowercase().next().unwrap_or(ch)
                } else {
                    ch
                };
                skip.insert(key);
            }
        }

        let mut available: Vec<char> = alphabet
            .iter()
            .copied()
            .filter(|&c| {
                let k = if ci {
                    c.to_lowercase().next().unwrap_or(c)
                } else {
                    c
                };
                !skip.contains(&k)
            })
            .collect();

        // Fallback: if every label was skipped, use the full alphabet.
        if available.is_empty() {
            available = alphabet.to_vec();
        }

        // Build labeled matches. Each match gets the next available label.
        // Stop when we run out of labels.
        let match_ranges: Vec<Range> = positions
            .iter()
            .map(|&pos| Range::new(pos, pos + query_len))
            .collect();

        self.labeled = match_ranges
            .iter()
            .zip(available.iter())
            .map(|(range, &label)| {
                let label_pos = range.to().min(text.len_chars().saturating_sub(1));
                LabeledMatch {
                    range: *range,
                    label,
                    label_pos,
                }
            })
            .collect();

        // Highlight all matches with selection
        self.highlight_matches(&match_ranges, editor);

        // Render label overlays — placed AFTER the matched text.
        self.show_labels(editor);
    }

    fn show_labels(&self, editor: &mut Editor) {
        if self.labeled.is_empty() {
            let doc = editor.documents.get_mut(&self.doc_id).unwrap();
            doc.remove_jump_labels(self.view_id);
            return;
        }

        let mut overlays: Vec<Overlay> = self
            .labeled
            .iter()
            .map(|lm| {
                let mut t = Tendril::new();
                t.push(lm.label);
                Overlay::new(lm.label_pos, t)
            })
            .collect();
        overlays.sort_unstable_by_key(|o| o.char_idx);

        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.set_jump_labels(self.view_id, overlays);
    }

    /// Check if `ch` matches a label and return the index if so.
    fn find_label(&self, ch: char) -> Option<usize> {
        self.labeled.iter().position(|lm| lm.label == ch)
    }

    fn jump_to(&self, idx: usize, editor: &mut Editor) {
        let Some(lm) = self.labeled.get(idx) else {
            return;
        };

        let target_pos = lm.range.from();
        let doc = &editor.documents[&self.doc_id];
        let text = doc.text().slice(..);
        let target_end = next_grapheme_boundary(text, target_pos);

        let primary = self.snapshot.primary();
        let range = if self.behaviour == Movement::Extend {
            let anchor = if target_pos >= primary.from() {
                primary.from()
            } else {
                primary.to()
            };
            Range::new(anchor, target_end)
        } else {
            Range::new(target_pos, target_end).with_direction(Direction::Forward)
        };

        // Save to jumplist
        let jump = (doc.id(), doc.selection(self.view_id).clone());
        let view = editor.tree.get_mut(self.view_id);
        view.jumps.push(jump);

        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.set_selection(self.view_id, range.into());

        // Save query to search register so n/N work.
        // Escape as regex literal since search_next uses regex matching.
        if let Some(reg) = self.search_register {
            if !self.query.is_empty() {
                let escaped = helix_core::regex::escape(&self.query);
                let _ = editor.registers.push(reg, escaped);
                editor.registers.last_search_register = reg;
            }
        }
    }

    /// Returns the status-line prefix for the current mode:
    /// `"/"` (forward search), `"?"` (backward search), `"%"` (whole-doc
    /// search), or `"flash"` (pure flash jump without register).
    fn status_prefix(&self) -> &'static str {
        match self.search_register {
            Some(_) => match self.direction {
                Some(Direction::Forward) => "/",
                Some(Direction::Backward) => "?",
                None => "%",
            },
            None => "flash",
        }
    }

    /// Formats the full status line string for the given query text.
    fn format_status(&self, query: &str) -> String {
        let prefix = self.status_prefix();
        if query.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix} {query}")
        }
    }

    fn cleanup(&self, editor: &mut Editor) {
        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.remove_jump_labels(self.view_id);
        doc.set_selection(self.view_id, self.snapshot.clone());
        editor.set_status("");
    }

    fn restore_snapshot(&self, editor: &mut Editor) {
        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.set_selection(self.view_id, self.snapshot.clone());
    }
}

fn char_eq(a: char, b: char, case_insensitive: bool) -> bool {
    if case_insensitive {
        a.to_lowercase().eq(b.to_lowercase())
    } else {
        a == b
    }
}

impl Component for FlashPrompt {
    fn handle_event(&mut self, event: &compositor::Event, cx: &mut Context) -> EventResult {
        let close_fn =
            EventResult::Consumed(Some(Box::new(|compositor: &mut compositor::Compositor, _| {
                compositor.pop();
            })));

        let key_event = match event {
            compositor::Event::Key(ev) => *ev,
            compositor::Event::Resize(..) => return EventResult::Consumed(None),
            _ => return EventResult::Ignored(None),
        };

        match key_event {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.cleanup(cx.editor);
                self.restore_snapshot(cx.editor);
                close_fn
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
            } if modifiers.contains(helix_view::keyboard::KeyModifiers::CONTROL) => {
                self.cleanup(cx.editor);
                self.restore_snapshot(cx.editor);
                close_fn
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                self.query.pop();
                self.update(cx.editor);
                cx.editor.set_status(self.format_status(&self.query));
                EventResult::Consumed(None)
            }
            KeyEvent {
                code: KeyCode::Char(ch),
                ..
            } => {
                // flash.nvim order: check if ch matches a label from the
                // CURRENT state first.  If it does, jump immediately.
                // Because of smart-skip, label chars never collide with the
                // continuation character, so this is unambiguous.
                if let Some(idx) = self.find_label(ch) {
                    self.cleanup(cx.editor);
                    self.jump_to(idx, cx.editor);
                    return close_fn;
                }

                // Not a label — extend the search pattern and recompute.
                self.query.push(ch);
                self.update(cx.editor);
                cx.editor.set_status(self.format_status(&self.query));

                // Auto-jump when exactly one match remains.
                if self.labeled.len() == 1 {
                    self.cleanup(cx.editor);
                    self.jump_to(0, cx.editor);
                    return close_fn;
                }

                // No matches at all — give up.
                if self.labeled.is_empty() {
                    let positions = self.find_match_positions(cx.editor);
                    if positions.is_empty() {
                        cx.editor.set_status("No matches");
                        self.cleanup(cx.editor);
                        return close_fn;
                    }
                }

                EventResult::Consumed(None)
            }
            _ => {
                self.cleanup(cx.editor);
                close_fn
            }
        }
    }

    fn render(&mut self, _area: Rect, _surface: &mut Surface, _cx: &mut Context) {
        // Rendering is handled via jump label overlays and the status line.
    }

    fn id(&self) -> Option<&'static str> {
        Some("flash-prompt")
    }
}
