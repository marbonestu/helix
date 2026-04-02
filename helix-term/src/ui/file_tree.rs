use helix_view::editor::Editor;
use helix_view::file_tree::{FileTree, FileTreeConfig, GitStatus, NodeKind};
use helix_view::graphics::Rect;
use tui::buffer::Buffer as Surface;

use super::file_icons;

/// Render the file tree sidebar into the given area.
pub fn render_file_tree(
    tree: &FileTree,
    area: Rect,
    surface: &mut Surface,
    editor: &Editor,
    is_focused: bool,
    config: &FileTreeConfig,
) {
    if area.width < 5 || area.height < 2 {
        return;
    }

    let theme = &editor.theme;
    let show_icons = config.icons;

    // Background — fall back to statusline for visible contrast
    let bg_style = theme
        .try_get("ui.sidebar")
        .unwrap_or_else(|| theme.get("ui.statusline"));
    surface.set_style(area, bg_style);

    // Vertical separator on right edge
    let sep_style = theme
        .try_get("ui.sidebar.separator")
        .unwrap_or_else(|| theme.get("ui.window"));
    let sep_x = area.x + area.width - 1;
    for y in area.y..area.y + area.height {
        surface.set_string(sep_x, y, "│", sep_style);
    }

    // Content area (excluding separator)
    let content_width = (area.width - 1) as usize;
    let content_area = Rect::new(area.x, area.y, area.width - 1, area.height);

    let selected_style = if is_focused {
        theme
            .try_get("ui.sidebar.selected")
            .unwrap_or_else(|| theme.get("ui.menu.selected"))
    } else {
        theme
            .try_get("ui.sidebar.selected")
            .unwrap_or_else(|| theme.get("ui.selection"))
    };

    let file_style = theme
        .try_get("ui.sidebar.file")
        .unwrap_or_else(|| theme.get("ui.text"));
    let dir_style = theme
        .try_get("ui.sidebar.directory")
        .unwrap_or_else(|| theme.get("ui.text.directory"));

    // Reserve bottom row for search prompt when active
    let search_active = tree.search_active();
    let tree_height = if search_active {
        content_area.height.saturating_sub(1)
    } else {
        content_area.height
    };

    let visible = tree.visible();
    let scroll = tree.scroll_offset();
    let selected = tree.selected();
    let height = tree_height as usize;

    for (i, &node_id) in visible.iter().skip(scroll).take(height).enumerate() {
        let Some(node) = tree.nodes().get(node_id) else {
            continue;
        };

        let y = content_area.y + i as u16;
        let is_selected = scroll + i == selected;

        // Apply selected highlight to full row
        if is_selected {
            let row = Rect::new(content_area.x, y, content_area.width, 1);
            surface.set_style(row, selected_style);
        }

        // Indentation (2 chars per depth level)
        let indent = (node.depth as usize) * 2;
        let x = content_area.x + indent as u16;
        let remaining_width = content_width.saturating_sub(indent);
        if remaining_width < 3 {
            continue;
        }

        // Expand/collapse indicator
        let indicator = match node.kind {
            NodeKind::Directory if node.expanded => "▾ ",
            NodeKind::Directory => "▸ ",
            NodeKind::File => "  ",
        };
        let base_style = match node.kind {
            NodeKind::Directory => dir_style,
            NodeKind::File => file_style,
        };
        let style = if is_selected {
            selected_style
        } else {
            base_style
        };

        surface.set_stringn(x, y, indicator, remaining_width, style);

        // Icon (2 chars: icon + space)
        let mut name_x = x + 2; // after indicator
        let mut name_width = remaining_width.saturating_sub(2);

        if show_icons && name_width >= 3 {
            let (icon, icon_scope) = match node.kind {
                NodeKind::Directory => file_icons::icon_for_directory(node.expanded),
                NodeKind::File => file_icons::icon_for_file(&node.name),
            };

            let icon_style = if is_selected {
                selected_style
            } else {
                theme
                    .try_get(icon_scope)
                    .or_else(|| theme.try_get("ui.sidebar.icon"))
                    .or_else(|| theme.try_get("ui.sidebar.file"))
                    .unwrap_or_else(|| theme.get("ui.text"))
            };

            surface.set_stringn(name_x, y, icon, name_width, icon_style);
            // Nerd font icons are typically 1-2 cells wide; use 2 for consistent spacing
            name_x += 2;
            name_width = name_width.saturating_sub(2);
        }

        // Filename
        if name_width > 0 {
            surface.set_stringn(name_x, y, &node.name, name_width, style);
        }

        // Git status indicator at end of line (if space permits)
        let git_status = tree.git_status_for(node_id);
        if git_status != GitStatus::Clean {
            let (symbol, git_style) = match git_status {
                GitStatus::Modified => (
                    "●",
                    theme
                        .try_get("ui.sidebar.git.modified")
                        .or_else(|| theme.try_get("warning"))
                        .or_else(|| theme.try_get("diff.delta"))
                        .unwrap_or(base_style),
                ),
                GitStatus::Untracked => (
                    "◌",
                    theme
                        .try_get("ui.sidebar.git.untracked")
                        .or_else(|| theme.try_get("hint"))
                        .or_else(|| theme.try_get("diff.plus"))
                        .unwrap_or(base_style),
                ),
                GitStatus::Deleted => (
                    "✕",
                    theme
                        .try_get("ui.sidebar.git.deleted")
                        .or_else(|| theme.try_get("error"))
                        .or_else(|| theme.try_get("diff.minus"))
                        .unwrap_or(base_style),
                ),
                GitStatus::Conflict => (
                    "⚠",
                    theme
                        .try_get("ui.sidebar.git.conflict")
                        .or_else(|| theme.try_get("error"))
                        .or_else(|| theme.try_get("diff.minus"))
                        .unwrap_or(base_style),
                ),
                GitStatus::Renamed => (
                    "→",
                    theme
                        .try_get("ui.sidebar.git.modified")
                        .or_else(|| theme.try_get("warning"))
                        .or_else(|| theme.try_get("diff.delta"))
                        .unwrap_or(base_style),
                ),
                GitStatus::Clean => unreachable!(),
            };

            let git_x = content_area.x + content_area.width - 2;
            if git_x > name_x {
                surface.set_string(git_x, y, symbol, git_style);
            }
        }
    }

    // Render search prompt on the bottom row when active
    if search_active {
        let prompt_y = content_area.y + tree_height;
        let prompt_style = theme
            .try_get("ui.sidebar.search")
            .unwrap_or_else(|| theme.get("ui.text"));

        // Clear the prompt row
        let row = Rect::new(content_area.x, prompt_y, content_area.width, 1);
        surface.set_style(row, bg_style);

        let query = tree.search_query();
        let prompt_text = format!("/{}", query);
        surface.set_stringn(
            content_area.x,
            prompt_y,
            &prompt_text,
            content_width,
            prompt_style,
        );
    }
}
