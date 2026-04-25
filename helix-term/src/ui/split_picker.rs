use std::path::PathBuf;

use helix_view::{
    editor::Action,
    graphics::Rect,
    input::Event,
    keyboard::{KeyCode, KeyModifiers},
    ViewId,
};
use tui::buffer::Buffer as Surface;

use crate::compositor::{Component, Context, EventResult};

/// Labels used to identify splits, in order.
const LABELS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
    's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

/// An entry pairing a label character with the view it targets.
#[derive(Clone)]
pub struct LabeledView {
    pub label: char,
    pub view_id: ViewId,
    pub area: Rect,
}

/// Overlay that labels every open split with a letter so the user can choose
/// which split to open a file in.
///
/// Rendered on top of the editor; pressing the label character opens `path` in
/// the corresponding split and dismisses the picker. Pressing `Esc` cancels.
pub struct SplitPicker {
    path: PathBuf,
    views: Vec<LabeledView>,
}

impl SplitPicker {
    /// Build a picker from the current editor state.
    ///
    /// Returns `None` when there are no splits to label (should not happen in
    /// practice, but guards against an empty tree).
    pub fn new(path: PathBuf, editor: &helix_view::Editor) -> Option<Self> {
        let mut sorted_views: Vec<_> = editor
            .tree
            .views()
            .map(|(view, _focused)| (view.id, view.area))
            .collect();

        // Sort by visual position: top-to-bottom, then left-to-right.
        sorted_views.sort_by_key(|&(_, area)| (area.y, area.x));

        let views: Vec<LabeledView> = sorted_views
            .into_iter()
            .zip(LABELS.iter().copied())
            .map(|((view_id, area), label)| LabeledView {
                label,
                view_id,
                area,
            })
            .collect();

        if views.is_empty() {
            return None;
        }
        Some(Self { path, views })
    }

    /// Number of labeled splits.
    pub fn view_count(&self) -> usize {
        self.views.len()
    }

    /// Labeled views.
    pub fn labeled_views(&self) -> &[LabeledView] {
        &self.views
    }
}

impl Component for SplitPicker {
    fn render(&mut self, area: Rect, surface: &mut Surface, _ctx: &mut Context) {
        use helix_view::graphics::{Color, Modifier, Style};

        let label_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let dim_style = Style::default().add_modifier(Modifier::DIM);

        for lv in &self.views {
            if lv.area.width >= 3 && lv.area.height >= 1 {
                // Dim the entire split first to draw attention to the label.
                surface.set_style(lv.area, dim_style);

                // Render `[X]` label in the top-left corner.
                let upper = lv.label.to_ascii_uppercase();
                let label = format!("[{upper}]");
                surface.set_string(lv.area.x, lv.area.y, &label, label_style);
            }
        }

        // Hint in the bottom bar area.
        let hint_style = Style::default().fg(Color::Gray);
        let hint = " Open in split: type label  <esc> cancel";
        surface.set_string(area.x, area.bottom().saturating_sub(1), hint, hint_style);
    }

    fn handle_event(&mut self, event: &Event, _cx: &mut Context) -> EventResult {
        let key = match event {
            Event::Key(k) => *k,
            _ => return EventResult::Ignored(None),
        };

        if key.code == KeyCode::Esc {
            return EventResult::Consumed(Some(Box::new(|compositor, _cx| {
                compositor.pop();
            })));
        }

        if key.modifiers != KeyModifiers::NONE && key.modifiers != KeyModifiers::SHIFT {
            return EventResult::Ignored(None);
        }

        if let KeyCode::Char(pressed) = key.code {
            let ch = pressed.to_ascii_lowercase();
            if let Some(lv) = self.views.iter().find(|lv| lv.label == ch) {
                let view_id = lv.view_id;
                let path = self.path.clone();
                return EventResult::Consumed(Some(Box::new(move |compositor, cx| {
                    compositor.pop();
                    cx.editor.focus(view_id);
                    if let Err(e) = cx.editor.open(&path, Action::Replace) {
                        cx.editor.set_error(format!("{e}"));
                    }
                    cx.editor.left_sidebar.focused = false;
                })));
            }
        }

        // Consume unrecognised keys so they don't leak into the editor.
        EventResult::Consumed(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helix_view::{editor::GutterConfig, graphics::Rect, tree::Layout, view::View, DocumentId};

    /// Helper: given a tree, produce labels sorted by visual position (same
    /// logic as `SplitPicker::new` but without needing a full `Editor`).
    fn labels_from_tree(tree: &helix_view::tree::Tree) -> Vec<(char, ViewId)> {
        let mut views: Vec<_> = tree
            .views()
            .map(|(view, _)| (view.id, view.area))
            .collect();
        views.sort_by_key(|&(_, area)| (area.y, area.x));
        views
            .into_iter()
            .zip(LABELS.iter().copied())
            .map(|((id, _), label)| (label, id))
            .collect()
    }

    #[test]
    fn labels_follow_visual_order() {
        // Build a tree with 3 vertical splits: the creation order will be
        // [first, third, second] because we focus back on `first` before
        // creating `second`. The visual layout after recalculate is:
        //
        //  | first (a) | second (b) | third (c) |
        //
        // Without sorting, labels would follow creation order and `third`
        // would get 'b' while `second` gets 'c'.
        let area = Rect::new(0, 0, 180, 80);
        let mut tree = helix_view::tree::Tree::new(area);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.insert(view);
        let first = tree.focus;

        // Split right → creates "third" (rightmost after next split).
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let right = tree.focus;

        // Go back to the first view and split again → inserts between first
        // and right, making it the visual "second".
        tree.focus = first;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let middle = tree.focus;

        tree.recalculate();

        let labels = labels_from_tree(&tree);
        assert_eq!(labels.len(), 3);

        // Verify labels are assigned by visual x-position, not creation order.
        assert_eq!(labels[0], ('a', first));
        assert_eq!(labels[1], ('b', middle));
        assert_eq!(labels[2], ('c', right));
    }

    #[test]
    fn labels_follow_visual_order_horizontal_splits() {
        // Two rows: top and bottom. Create bottom first, then top shouldn't
        // matter — labels should be 'a' for the top view, 'b' for the bottom.
        let area = Rect::new(0, 0, 180, 80);
        let mut tree = helix_view::tree::Tree::new(area);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.insert(view);
        let top = tree.focus;

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Horizontal);
        let bottom = tree.focus;

        tree.recalculate();

        let labels = labels_from_tree(&tree);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], ('a', top));
        assert_eq!(labels[1], ('b', bottom));
    }
}
