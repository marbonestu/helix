/// Generic sidebar panel state — independent of what content is rendered inside it.
///
/// The sidebar container owns visibility, focus, and runtime width. Content
/// (file tree, chat, etc.) is rendered by the caller based on `visible`.
#[derive(Debug, Clone)]
pub struct Sidebar {
    pub visible: bool,
    pub focused: bool,
    /// Runtime width in columns. Starts from the config default but can be
    /// adjusted at runtime with grow/shrink commands.
    pub width: u16,
    /// When true the sidebar is rendered at half the terminal width instead of
    /// `width`. Toggled by `toggle_expand`; the pre-expand width is saved so
    /// collapsing restores it exactly.
    pub expanded: bool,
    pre_expand_width: u16,
    /// Actual rendered width in the last frame. Set by the render pass so the
    /// mouse handler can use the real on-screen width regardless of expanded state.
    pub rendered_width: u16,
    /// Set when `C-w` is pressed while the sidebar is focused. The next key
    /// is interpreted as a window command (e.g. `>` to grow, `<` to shrink)
    /// directly inside the sidebar handler rather than falling through to the
    /// keymap, which would lose the focus state.
    pub window_cmd_pending: bool,
}

impl Sidebar {
    pub fn new(width: u16) -> Self {
        Self {
            visible: false,
            focused: false,
            width,
            expanded: false,
            pre_expand_width: width,
            rendered_width: 0,
            window_cmd_pending: false,
        }
    }

    /// Toggle between the normal width and an expanded state (half the
    /// terminal). `terminal_width` is used only when entering expanded mode
    /// so the caller can pass the current editor area width.
    pub fn toggle_expand(&mut self) {
        if self.expanded {
            self.width = self.pre_expand_width;
            self.expanded = false;
        } else {
            self.pre_expand_width = self.width;
            self.expanded = true;
        }
    }
}
