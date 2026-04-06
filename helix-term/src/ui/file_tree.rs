use helix_view::editor::Editor;
use helix_view::file_tree::{ClipboardOp, FileTree, FileTreeConfig, GitStatus, NodeKind, PromptMode};
use helix_view::graphics::{Color, Modifier, Rect, Style};
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
    // Italic is stripped because italic directory names look odd in a tree.
    let dir_style = theme
        .try_get("ui.sidebar.directory")
        .or_else(|| {
            theme
                .try_get("function")
                .map(|s| s.remove_modifier(Modifier::ITALIC))
        })
        .unwrap_or_else(|| theme.get("ui.text"));

    // Reserve the bottom row when any prompt is active or a status message is pending.
    let prompt_row_needed = !matches!(tree.prompt_mode(), PromptMode::None)
        || tree.status_message().is_some();
    let tree_height = if prompt_row_needed {
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

        // Apply selected highlight to full row first so every cell gets the
        // selection background before individual elements are painted over it.
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

        // --- Git status color -----------------------------------------------
        // The git status is expressed as the fg color of the indicator and
        // filename. No right-side symbol is rendered.
        let git_status = tree.git_status_for(node_id);
        let git_color: Option<Style> = match git_status {
            GitStatus::Modified | GitStatus::Renamed => theme
                .try_get("ui.sidebar.git.modified")
                .or_else(|| theme.try_get("warning"))
                .or_else(|| theme.try_get("diff.delta")),
            GitStatus::Added => theme
                .try_get("ui.sidebar.git.added")
                .or_else(|| theme.try_get("diff.plus")),
            GitStatus::Staged => theme
                .try_get("ui.sidebar.git.staged")
                .or_else(|| theme.try_get("ui.sidebar.git.modified"))
                .or_else(|| theme.try_get("diff.delta")),
            GitStatus::Untracked => theme
                .try_get("ui.sidebar.git.untracked")
                // `ui.text.inactive` is the standard "muted text" scope used by
                // most themes for dimmed/inactive content — the same visual
                // effect as comments without pulling from the comment scope.
                .or_else(|| theme.try_get("ui.text.inactive"))
                .or_else(|| {
                    theme
                        .try_get("ui.text")
                        .map(|s| s.add_modifier(Modifier::DIM))
                }),
            GitStatus::Deleted => theme
                .try_get("ui.sidebar.git.deleted")
                .or_else(|| theme.try_get("error"))
                .or_else(|| theme.try_get("diff.minus")),
            GitStatus::Conflict => theme
                .try_get("ui.sidebar.git.conflict")
                .or_else(|| theme.try_get("error"))
                .or_else(|| theme.try_get("diff.minus")),
            GitStatus::Clean => None,
        };

        // text_style is applied to the expand indicator and the filename.
        // On selected rows the selection background is preserved; only the
        // fg is replaced by the git status color when present.
        let text_style = {
            let s = if is_selected { selected_style } else { base_style };
            match git_color {
                Some(git) => s.patch(git),
                None => s,
            }
        };

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

            // Icons keep their own color regardless of git status so that
            // file type is always visually distinct from git state.
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

        // Filename — colored by git status when dirty
        if name_width > 0 {
            let written = surface.set_stringn(name_x, y, &node.name, name_width, text_style);
            let after_x = written.0;
            let used = (after_x - name_x) as usize;
            let remaining_after_name = name_width.saturating_sub(used);

            // Clipboard tag: " (C)" for yanked, " (X)" for cut
            if let Some(clip) = tree.clipboard() {
                if clip.node_id == node_id && remaining_after_name >= 4 {
                    let tag = match clip.op {
                        ClipboardOp::Copy => " (C)",
                        ClipboardOp::Cut => " (X)",
                    };
                    let tag_style = if is_selected {
                        selected_style.add_modifier(Modifier::DIM)
                    } else {
                        text_style.add_modifier(Modifier::DIM)
                    };
                    surface.set_stringn(after_x, y, tag, remaining_after_name, tag_style);
                }
            }
        }
    }

    // --- Bottom row: prompt or status message --------------------------------
    if prompt_row_needed {
        let prompt_y = content_area.y + tree_height;
        let row = Rect::new(content_area.x, prompt_y, content_area.width, 1);
        surface.set_style(row, bg_style);

        let prompt_style = theme
            .try_get("ui.sidebar.search")
            .unwrap_or_else(|| theme.get("ui.text"));

        // For text-input modes, split into label + before-cursor + cursor char + after-cursor.
        enum PromptContent<'a> {
            /// label, full input string, cursor byte offset
            Input { label: &'a str, input: &'a str, cursor: usize },
            /// static confirmation text — no cursor
            Static(String),
        }

        let content = match tree.prompt_mode() {
            PromptMode::Search => PromptContent::Input {
                label: "/",
                input: tree.search_query(),
                cursor: tree.search_query().len(), // search cursor always at end
            },
            PromptMode::NewFile { .. } => PromptContent::Input {
                label: "New file: ",
                input: tree.prompt_input(),
                cursor: tree.prompt_cursor(),
            },
            PromptMode::NewDir { .. } => PromptContent::Input {
                label: "New dir: ",
                input: tree.prompt_input(),
                cursor: tree.prompt_cursor(),
            },
            PromptMode::Rename(_) => PromptContent::Input {
                label: "Rename to: ",
                input: tree.prompt_input(),
                cursor: tree.prompt_cursor(),
            },
            PromptMode::Duplicate(_) => PromptContent::Input {
                label: "Duplicate as: ",
                input: tree.prompt_input(),
                cursor: tree.prompt_cursor(),
            },
            PromptMode::DeleteConfirm { id, is_dir } => {
                let name = tree.nodes().get(*id).map(|n| n.name.as_str()).unwrap_or("?");
                let text = if *is_dir {
                    format!("Delete '{name}/' and all contents? [y/n]")
                } else {
                    format!("Delete '{name}'? [y/n]")
                };
                PromptContent::Static(text)
            }
            PromptMode::None => {
                // Show status message in a dimmed style
                let status = tree.status_message().unwrap_or("");
                let status_style = prompt_style.add_modifier(Modifier::DIM);
                surface.set_stringn(content_area.x, prompt_y, status, content_width, status_style);
                return;
            }
        };

        match content {
            PromptContent::Input { label, input, cursor } => {
                let mut x = content_area.x;
                let remaining = |x: u16| (content_area.x + content_area.width).saturating_sub(x) as usize;

                // Dimmed label
                let label_style = prompt_style.add_modifier(Modifier::DIM);
                let label_width = label.chars().count() as u16;
                surface.set_stringn(x, prompt_y, label, remaining(x), label_style);
                x += label_width;

                // Text before the cursor
                let before = &input[..cursor.min(input.len())];
                let before_width = before.chars().count() as u16;
                surface.set_stringn(x, prompt_y, before, remaining(x), prompt_style);
                x += before_width;

                // The cursor cell: the character under the cursor (or a space at end)
                let cursor_style = prompt_style.add_modifier(Modifier::REVERSED);
                let after = &input[cursor.min(input.len())..];
                let cursor_ch = after.chars().next().unwrap_or(' ');
                let cursor_ch_width = cursor_ch.len_utf8(); // byte width for slicing
                if remaining(x) > 0 {
                    let next = surface.set_stringn(x, prompt_y, cursor_ch.to_string(), remaining(x), cursor_style);
                    x = next.0;
                }

                // Text after the cursor
                let after_cursor = &after[cursor_ch_width.min(after.len())..];
                surface.set_stringn(x, prompt_y, after_cursor, remaining(x), prompt_style);
            }
            PromptContent::Static(text) => {
                surface.set_stringn(content_area.x, prompt_y, &text, content_width, prompt_style);
            }
        }
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
