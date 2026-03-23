use helix_view::editor::Editor;
use helix_view::file_tree::{FileTree, GitStatus, NodeKind};
use helix_view::graphics::Rect;
use tui::buffer::Buffer as Surface;

/// Render the file tree sidebar into the given area.
pub fn render_file_tree(
    tree: &FileTree,
    area: Rect,
    surface: &mut Surface,
    editor: &Editor,
    is_focused: bool,
) {
    if area.width < 5 || area.height < 2 {
        return;
    }

    let theme = &editor.theme;

    // Background
    let bg_style = theme
        .try_get("ui.sidebar")
        .unwrap_or_else(|| theme.get("ui.background"));
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
            .unwrap_or_else(|| theme.get("ui.cursor.primary"))
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
        .unwrap_or_else(|| theme.get("ui.text"));

    let visible = tree.visible();
    let scroll = tree.scroll_offset();
    let selected = tree.selected();
    let height = content_area.height as usize;

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

        // Filename
        let name_x = x + 2; // indicator is 2 chars
        let name_width = remaining_width.saturating_sub(2);
        if name_width > 0 {
            surface.set_stringn(name_x, y, &node.name, name_width, style);
        }

        // Git status indicator at end of line (if space permits)
        let git_status = tree.git_status_for(node_id);
        if git_status != GitStatus::Clean {
            let (symbol, git_style_name) = match git_status {
                GitStatus::Modified => ("●", "ui.sidebar.git.modified"),
                GitStatus::Untracked => ("◌", "ui.sidebar.git.untracked"),
                GitStatus::Deleted => ("✕", "ui.sidebar.git.deleted"),
                GitStatus::Conflict => ("⚠", "ui.sidebar.git.conflict"),
                GitStatus::Renamed => ("→", "ui.sidebar.git.modified"),
                GitStatus::Clean => unreachable!(),
            };

            let git_style = theme.try_get(git_style_name).unwrap_or(base_style);
            let git_x = content_area.x + content_area.width - 2;
            if git_x > name_x {
                surface.set_string(git_x, y, symbol, git_style);
            }
        }
    }
}
