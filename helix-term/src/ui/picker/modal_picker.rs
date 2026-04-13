use helix_core::{movement::Direction, Position};
use helix_view::{
    editor::{Action, InputPosition},
    graphics::{CursorKind, Rect},
    input::KeyEvent,
    Editor,
};
use tui::buffer::Buffer as Surface;

use crate::{
    alt,
    compositor::{Component, Context, Event, EventResult},
    ctrl, key,
    ui::picker::{Column, FileLocation, Picker, ID},
};

/// The interaction mode of a [`ModalPicker`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickerMode {
    /// Text input is active; keystrokes are forwarded to the query prompt.
    Insert,
    /// Navigation and action mode; vim-style motions and registered actions
    /// are available. Printable keys that are not bound switch back to Insert.
    Normal,
}

/// A reusable picker UI component with vim-style Insert/Normal modes.
///
/// `ModalPicker` wraps [`Picker`] and adds two-mode interaction: Insert mode
/// for fuzzy search and Normal mode for keyboard navigation, multi-select, and
/// bulk actions. Mode logic is fully contained here so callers (buffer picker,
/// file picker, etc.) never need to implement it themselves.
pub struct ModalPicker<T: 'static + Send + Sync, D: 'static + Send + Sync> {
    pub(super) inner: Picker<T, D>,
    mode: PickerMode,
    /// Pending first keystroke of a two-key sequence (e.g. `g` before `gg`).
    pending_key: Option<KeyEvent>,
}

impl<T: 'static + Send + Sync, D: 'static + Send + Sync> ModalPicker<T, D> {
    pub fn new<C, O, F>(
        columns: C,
        primary_column: usize,
        options: O,
        editor_data: D,
        callback_fn: F,
    ) -> Self
    where
        C: IntoIterator<Item = Column<T, D>>,
        O: IntoIterator<Item = T>,
        F: Fn(&mut Context, &T, Action) + 'static,
    {
        Self {
            inner: Picker::new(columns, primary_column, options, editor_data, callback_fn),
            mode: PickerMode::Insert,
            pending_key: None,
        }
    }

    /// Expose the inner [`Picker`] for use by background handlers.
    pub fn inner_mut(&mut self) -> &mut Picker<T, D> {
        &mut self.inner
    }

    pub fn with_action(
        mut self,
        key: KeyEvent,
        f: impl Fn(&mut Context, &T, &[&T], usize) + 'static,
    ) -> Self {
        self.inner = self.inner.with_action(key, f);
        self
    }

    pub fn with_refresh_fn(mut self, f: impl Fn(&Editor) -> Vec<T> + 'static) -> Self {
        self.inner = self.inner.with_refresh_fn(f);
        self
    }

    pub fn with_preview(
        mut self,
        f: impl for<'a> Fn(&'a Editor, &'a T) -> Option<FileLocation<'a>> + 'static,
    ) -> Self {
        self.inner = self.inner.with_preview(f);
        self
    }

    pub fn with_initial_cursor(mut self, cursor: u32) -> Self {
        self.inner = self.inner.with_initial_cursor(cursor);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.inner = self.inner.with_title(title);
        self
    }

    pub fn with_history_register(mut self, history_register: Option<char>) -> Self {
        self.inner = self.inner.with_history_register(history_register);
        self
    }

    pub fn with_input_position(mut self, pos: InputPosition) -> Self {
        self.inner = self.inner.with_input_position(pos);
        self
    }

    /// Invoke a registered picker action for `key_event` if one exists.
    /// Returns `Some(EventResult)` when an action was dispatched (the caller
    /// should return that result); returns `None` when no action matched.
    fn dispatch_action(&mut self, key_event: KeyEvent, ctx: &mut Context) -> Option<EventResult> {
        let action_idx = self
            .inner
            .picker_actions
            .iter()
            .position(|(k, _)| *k == key_event)?;

        {
            let snapshot = self.inner.matcher.snapshot();
            let count = snapshot.matched_item_count() as usize;
            let cursor = self.inner.cursor as usize;
            let all: Vec<&T> = (0..count as u32)
                .filter_map(|i| snapshot.get_matched_item(i).map(|it| it.data))
                .collect();
            if let Some(&selected) = all.get(cursor) {
                let (_, action_fn) = &self.inner.picker_actions[action_idx];
                action_fn(ctx, selected, &all, cursor);
            }
            // `snapshot` and `all` are dropped here before refresh_items is called.
        }

        if self.inner.has_refresh_fn() {
            self.inner.refresh_items_if_set(ctx.editor);
            Some(EventResult::Consumed(None))
        } else {
            Some(self.inner.close_callback())
        }
    }

}

impl<T: 'static + Send + Sync, D: 'static + Send + Sync> Component for ModalPicker<T, D> {
    fn render(&mut self, area: Rect, surface: &mut Surface, cx: &mut Context) {
        self.inner.render(area, surface, cx);
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut Context) -> EventResult {
        let key_event = match event {
            Event::Key(event) => *event,
            Event::Paste(..) => {
                return match self.mode {
                    PickerMode::Insert => self.inner.prompt_handle_event(event, ctx),
                    // Discard paste in Normal mode — user must switch to Insert first.
                    PickerMode::Normal => EventResult::Consumed(None),
                };
            }
            Event::Resize(..) => return EventResult::Consumed(None),
            _ => return EventResult::Ignored(None),
        };

        // Resolve pending two-key sequences (currently just `gg`).
        if let Some(pending) = self.pending_key.take() {
            if pending == key!('g') && key_event == key!('g') {
                self.inner.to_start();
                return EventResult::Consumed(None);
            }
            // Sequence didn't complete; the pending key is discarded and the
            // current key is handled normally below.
        }

        match self.mode {
            PickerMode::Insert => {
                match key_event {
                    key!(Esc) => {
                        self.mode = PickerMode::Normal;
                        self.inner.set_status_prefix(Some("[N] "));
                        EventResult::Consumed(None)
                    }
                    // All other events are handled by the inner picker (navigation
                    // via Tab/arrows, prompt input, Enter to accept, etc.).
                    _ => self.inner.handle_event(event, ctx),
                }
            }

            PickerMode::Normal => {
                // Visual up/down depends on whether the list is inverted.
                let (up_dir, down_dir) = match self.inner.input_position {
                    InputPosition::Bottom => (Direction::Forward, Direction::Backward),
                    InputPosition::Top => (Direction::Backward, Direction::Forward),
                };

                match key_event {
                    // --- Navigation ---
                    key!('j') | key!(Down) => {
                        self.inner.move_by(1, down_dir);
                    }
                    key!('k') | key!(Up) => {
                        self.inner.move_by(1, up_dir);
                    }
                    key!('G') => {
                        self.inner.to_end();
                    }
                    key!('g') => {
                        // Arm the `gg` sequence; resolved on the next keypress.
                        self.pending_key = Some(key_event);
                    }
                    ctrl!('d') | key!(PageDown) => {
                        let half = (self.inner.completion_height() / 2).max(1) as u32;
                        self.inner.move_by(half, down_dir);
                    }
                    ctrl!('u') | key!(PageUp) => {
                        let half = (self.inner.completion_height() / 2).max(1) as u32;
                        self.inner.move_by(half, up_dir);
                    }
                    key!(Home) => {
                        self.inner.to_start();
                    }
                    key!(End) => {
                        self.inner.to_end();
                    }

                    // --- Mode transitions ---
                    key!('i') | key!('/') => {
                        self.mode = PickerMode::Insert;
                        self.pending_key = None;
                        self.inner.set_status_prefix(None);
                    }
                    key!(Esc) => {
                        return self.inner.close_callback();
                    }

                    // --- Delegated actions (accept, split, toggle preview) ---
                    key!(Enter) | ctrl!('s') | ctrl!('v') | ctrl!('t') | alt!(Enter) => {
                        return self.inner.handle_event(event, ctx);
                    }

                    // --- Registered caller actions and unbound printable fallback ---
                    key_event => {
                        if let Some(result) = self.dispatch_action(key_event, ctx) {
                            return result;
                        }
                        // An unbound printable key enters Insert mode and appends the
                        // character to the query, preserving the character's intent.
                        if let helix_view::keyboard::KeyCode::Char(_) = key_event.code {
                            if key_event.modifiers == helix_view::keyboard::KeyModifiers::NONE {
                                self.mode = PickerMode::Insert;
                                self.pending_key = None;
                                self.inner.set_status_prefix(None);
                                return self.inner.prompt_handle_event(event, ctx);
                            }
                        }
                    }
                }
                EventResult::Consumed(None)
            }
        }
    }

    fn cursor(&self, area: Rect, editor: &Editor) -> (Option<Position>, CursorKind) {
        match self.mode {
            PickerMode::Insert => self.inner.cursor(area, editor),
            PickerMode::Normal => (None, CursorKind::Hidden),
        }
    }

    fn required_size(&mut self, viewport: (u16, u16)) -> Option<(u16, u16)> {
        self.inner.required_size(viewport)
    }

    fn id(&self) -> Option<&'static str> {
        Some(ID)
    }
}
