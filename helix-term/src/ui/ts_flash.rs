use helix_core::{
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
/// On activation the calling command immediately jumps to the first (nearest)
/// match, then pushes this picker so the user can refine by pressing a label.
/// Pressing a label character selects the full span of the corresponding node,
/// mirroring the behaviour of `goto_next/prev_*` commands. Escape restores the
/// cursor to where it was before `]f` (or equivalent) was pressed.
///
/// Jumplist management is handled once by the calling command at activation
/// time; `jump_to` does not push an additional entry.
pub struct TsFlashPicker {
    labeled: Vec<LabeledNode>,
    /// Navigation direction; determines which end of the node range the cursor
    /// is placed on (Forward → cursor at end, Backward → cursor at start),
    /// matching `goto_ts_object_impl` behaviour.
    direction: Direction,
    snapshot: Selection,
    view_id: ViewId,
    doc_id: DocumentId,
}

impl TsFlashPicker {
    /// Build the picker from a pre-computed list of node ranges.
    ///
    /// Labels are assigned from `alphabet` in the order the ranges are supplied
    /// (nearest first for both directions).
    pub fn new(
        nodes: Vec<Range>,
        alphabet: Vec<char>,
        direction: Direction,
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
            direction,
            snapshot,
            view_id,
            doc_id,
        }
    }

    /// Place label overlays on the document so the user can see which key to
    /// press for each visible node. Call this once after construction and
    /// before pushing the picker onto the compositor stack.
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

    /// Jump to the labeled node at `idx`, selecting its full span with the
    /// direction applied by `goto_ts_object_impl`. Does **not** push a new
    /// jumplist entry — the calling command already did that at activation.
    fn jump_to(&self, idx: usize, editor: &mut Editor) {
        let Some(ln) = self.labeled.get(idx) else {
            return;
        };

        let range = ln.range.with_direction(self.direction);
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
