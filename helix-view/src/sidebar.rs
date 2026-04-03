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
    /// Set when `C-w` is pressed while the sidebar is focused. The next key
    /// is interpreted as a window command (e.g. `>` to grow, `<` to shrink)
    /// directly inside the sidebar handler rather than falling through to the
    /// keymap, which would lose the focus state.
    pub window_cmd_pending: bool,
}

impl Sidebar {
    pub fn new(width: u16) -> Self {
        Self { visible: false, focused: false, width, window_cmd_pending: false }
    }
}
