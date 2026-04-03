use helix_core::{
    graphemes::next_grapheme_boundary,
    movement::Direction,
    text_annotations::Overlay,
    Range, Selection, Tendril,
};
use helix_view::{
    input::KeyEvent,
    keyboard::KeyCode,
    DocumentId, Editor, ViewId,
};

use crate::compositor::{self, Component, Context, EventResult};

use helix_view::graphics::Rect;
use tui::buffer::Buffer as Surface;

/// A treesitter node annotated with a jump label and the position where the
/// label overlay is rendered (at the start of the node).
struct LabeledNode {
    /// Full span of the matched treesitter node.
    range: Range,
    /// Single character label shown to the user.
    label: char,
    /// Char index where the label overlay is placed (start of node).
    label_pos: usize,
}

/// Interactive picker that shows jump labels on all visible treesitter objects
/// of a given type (function, class, parameter, …) within the current viewport.
///
/// On activation every visible object of `object_name` in the given direction
/// from the cursor is labelled with a letter from the configured alphabet.
/// Pressing a label character jumps the cursor to the start of that node.
/// Escape restores the original cursor position.
///
/// The picker is fully constructed with its labels assigned — there is no
/// incremental search phase. A single visible node triggers an immediate jump
/// without entering the picker at all (handled by the calling command).
pub struct TsFlashPicker {
    labeled: Vec<LabeledNode>,
    snapshot: Selection,
    view_id: ViewId,
    doc_id: DocumentId,
}

impl TsFlashPicker {
    /// Build the picker from a pre-computed list of node ranges. Labels are
    /// assigned from `alphabet` in the order the ranges are supplied (nearest
    /// first for both directions).
    pub fn new(
        nodes: Vec<Range>,
        alphabet: Vec<char>,
        view_id: ViewId,
        doc_id: DocumentId,
        snapshot: Selection,
    ) -> Self {
        let labeled = nodes
            .into_iter()
            .zip(alphabet.iter())
            .map(|(range, &label)| LabeledNode {
                label_pos: range.from(),
                range,
                label,
            })
            .collect();

        Self {
            labeled,
            snapshot,
            view_id,
            doc_id,
        }
    }

    pub fn show_labels(&self, editor: &mut Editor) {
        let mut overlays: Vec<Overlay> = self
            .labeled
            .iter()
            .map(|ln| {
                let mut t = Tendril::new();
                t.push(ln.label);
                Overlay::new(ln.label_pos, t)
            })
            .collect();
        overlays.sort_unstable_by_key(|o| o.char_idx);

        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.set_jump_labels(self.view_id, overlays);
    }

    fn find_label(&self, ch: char) -> Option<usize> {
        self.labeled.iter().position(|ln| ln.label == ch)
    }

    fn jump_to(&self, idx: usize, editor: &mut Editor) {
        let Some(ln) = self.labeled.get(idx) else {
            return;
        };

        let target_pos = ln.range.from();
        let doc = &editor.documents[&self.doc_id];
        let text = doc.text().slice(..);
        let target_end = next_grapheme_boundary(text, target_pos);

        // Record the pre-jump position in the jumplist.
        let jump = (doc.id(), doc.selection(self.view_id).clone());
        let view = editor.tree.get_mut(self.view_id);
        view.jumps.push(jump);

        let range = Range::new(target_pos, target_end)
            .with_direction(Direction::Forward);

        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.set_selection(self.view_id, range.into());
    }

    fn cleanup(&self, editor: &mut Editor) {
        let doc = editor.documents.get_mut(&self.doc_id).unwrap();
        doc.remove_jump_labels(self.view_id);
        doc.set_selection(self.view_id, self.snapshot.clone());
        editor.set_status("");
    }
}

impl Component for TsFlashPicker {
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
                close_fn
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
            } if modifiers.contains(helix_view::keyboard::KeyModifiers::CONTROL) => {
                self.cleanup(cx.editor);
                close_fn
            }
            KeyEvent {
                code: KeyCode::Char(ch),
                ..
            } => {
                if let Some(idx) = self.find_label(ch) {
                    // Clean up overlays before jumping so the jump does not
                    // inadvertently restore the snapshot selection.
                    let doc = cx.editor.documents.get_mut(&self.doc_id).unwrap();
                    doc.remove_jump_labels(self.view_id);
                    cx.editor.set_status("");
                    self.jump_to(idx, cx.editor);
                    return close_fn;
                }
                // Unknown key — ignore and stay open.
                EventResult::Consumed(None)
            }
            _ => {
                self.cleanup(cx.editor);
                close_fn
            }
        }
    }

    fn render(&mut self, _area: Rect, _surface: &mut Surface, _cx: &mut Context) {
        // Rendering is handled via jump-label overlays and the status line.
    }

    fn id(&self) -> Option<&'static str> {
        Some("ts-flash-picker")
    }
}
