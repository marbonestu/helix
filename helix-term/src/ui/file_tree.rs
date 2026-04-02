use helix_view::editor::Editor;
use helix_view::file_tree::{FileTree, FileTreeConfig, GitStatus, NodeKind};
use helix_view::graphics::{Color, Rect, Style};
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

    // --- Background ---------------------------------------------------------
    // Prefer the explicit sidebar key. Without it, use the bg from ui.popup
    // (a panel-like surface) or ui.statusline, combined with the fg from
    // ui.text so the sidebar text color matches the rest of the theme.
    let bg_style = theme.try_get("ui.sidebar").unwrap_or_else(|| {
        let bg = theme
            .try_get("ui.popup")
            .or_else(|| theme.try_get("ui.statusline"))
            .and_then(|s| s.bg)
            .map(|c| Style::default().bg(c))
            .unwrap_or_default();
        let fg = theme
            .try_get("ui.text")
            .and_then(|s| s.fg)
            .map(|c| Style::default().fg(c))
            .unwrap_or_default();
        bg.patch(fg)
    });
    surface.set_style(area, bg_style);

    // --- Vertical separator on right edge -----------------------------------
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

    // --- Selection style ----------------------------------------------------
    // When the sidebar is focused use ui.menu.selected (solid highlight),
    // otherwise ui.selection (dimmer highlight for unfocused state).
    let selected_style = if is_focused {
        theme
            .try_get("ui.sidebar.selected")
            .unwrap_or_else(|| theme.get("ui.menu.selected"))
    } else {
        theme
            .try_get("ui.sidebar.selected")
            .unwrap_or_else(|| theme.get("ui.selection"))
    };

    // --- Text styles --------------------------------------------------------
    let file_style = theme
        .try_get("ui.sidebar.file")
        .unwrap_or_else(|| theme.get("ui.text"));
    // Directories use the same color as function names — most themes give
    // functions a distinctive color that works well for folder labels too.
    // Theme authors can override with ui.sidebar.directory.
    let dir_style = theme
        .try_get("ui.sidebar.directory")
        .or_else(|| theme.try_get("function"))
        .unwrap_or_else(|| theme.get("ui.text"));

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

        // Apply selected highlight to full row first so every cell on this
        // row gets the selection background.
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

        let base_style = match node.kind {
            NodeKind::Directory => dir_style,
            NodeKind::File => file_style,
        };
        let text_style = if is_selected { selected_style } else { base_style };

        // Expand/collapse indicator
        let indicator = match node.kind {
            NodeKind::Directory if node.expanded => "▾ ",
            NodeKind::Directory => "▸ ",
            NodeKind::File => "  ",
        };
        surface.set_stringn(x, y, indicator, remaining_width, text_style);

        // --- Icon -----------------------------------------------------------
        let mut name_x = x + 2;
        let mut name_width = remaining_width.saturating_sub(2);

        if show_icons && name_width >= 3 {
            let (icon, icon_scope) = match node.kind {
                NodeKind::Directory => file_icons::icon_for_directory(node.expanded),
                NodeKind::File => file_icons::icon_for_file(&node.name),
            };

            // Fallback chain for icon color:
            //  1. Theme-specific scope  (ui.sidebar.icon.rust, …)
            //  2. Generic sidebar icon  (ui.sidebar.icon)
            //  3a. Directories → inherit dir_style so color matches the theme
            //  3b. Files → canonical language color (language-brand colors
            //      that are recognisable regardless of theme)
            //  4. base_style (text color for this node type)
            let icon_style = if is_selected {
                selected_style
            } else {
                theme
                    .try_get(icon_scope)
                    .or_else(|| theme.try_get("ui.sidebar.icon"))
                    .or_else(|| match node.kind {
                        NodeKind::Directory => Some(dir_style),
                        NodeKind::File => icon_canonical_style(icon_scope),
                    })
                    .unwrap_or(base_style)
            };

            surface.set_stringn(name_x, y, icon, name_width, icon_style);
            name_x += 2;
            name_width = name_width.saturating_sub(2);
        }

        // Filename
        if name_width > 0 {
            surface.set_stringn(name_x, y, &node.name, name_width, text_style);
        }

        // --- Git status indicator at right edge -----------------------------
        // Fallback chain uses standard diagnostic/diff keys that virtually
        // every theme defines, so git colours work without any sidebar-specific
        // entries in the theme file.
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

    // --- Search prompt at bottom row ----------------------------------------
    if search_active {
        let prompt_y = content_area.y + tree_height;
        let prompt_style = theme
            .try_get("ui.sidebar.search")
            .unwrap_or_else(|| theme.get("ui.text"));

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

/// Returns the canonical language color for a sidebar icon scope, used as a
/// fallback when the active theme does not define `ui.sidebar.icon.*` entries.
///
/// These are the widely-recognised "official" colors for each language as used
/// by GitHub Linguist, VS Code, and nvim-web-devicons. Theme authors can
/// override any entry by defining the corresponding `ui.sidebar.icon.*` key.
fn icon_canonical_style(scope: &str) -> Option<Style> {
    let (r, g, b) = match scope {
        "ui.sidebar.icon.rust"       => (0xde, 0xa5, 0x84),
        "ui.sidebar.icon.python"     => (0x35, 0x72, 0xa5),
        "ui.sidebar.icon.javascript" => (0xf1, 0xe0, 0x5a),
        "ui.sidebar.icon.typescript" => (0x31, 0x78, 0xc6),
        "ui.sidebar.icon.go"         => (0x00, 0xad, 0xd8),
        "ui.sidebar.icon.c"          => (0x55, 0x55, 0x55),
        "ui.sidebar.icon.cpp"        => (0xf3, 0x4b, 0x7d),
        "ui.sidebar.icon.csharp"     => (0x17, 0x8c, 0x00),
        "ui.sidebar.icon.java"       => (0xb0, 0x72, 0x19),
        "ui.sidebar.icon.kotlin"     => (0xa9, 0x7b, 0xff),
        "ui.sidebar.icon.scala"      => (0xdc, 0x32, 0x2f),
        "ui.sidebar.icon.clojure"    => (0x5e, 0x9f, 0x3b),
        "ui.sidebar.icon.ruby"       => (0x70, 0x15, 0x16),
        "ui.sidebar.icon.lua"        => (0x00, 0x00, 0x80),
        "ui.sidebar.icon.shell"      => (0x4e, 0xaa, 0x25),
        "ui.sidebar.icon.nix"        => (0x7e, 0xba, 0xe4),
        "ui.sidebar.icon.markdown"   => (0x08, 0x3f, 0xa8),
        "ui.sidebar.icon.json"       => (0xcb, 0xcb, 0x41),
        "ui.sidebar.icon.toml"       => (0x9c, 0x4d, 0x21),
        "ui.sidebar.icon.yaml"       => (0xcb, 0x17, 0x1e),
        "ui.sidebar.icon.html"       => (0xe3, 0x4c, 0x26),
        "ui.sidebar.icon.css"        => (0x56, 0x3d, 0x7c),
        "ui.sidebar.icon.docker"     => (0x38, 0x4d, 0x54),
        "ui.sidebar.icon.git"        => (0xf0, 0x50, 0x33),
        "ui.sidebar.icon.makefile"   => (0x6d, 0x8b, 0x74),
        _ => return None,
    };
    Some(Style::default().fg(Color::Rgb(r, g, b)))
}
