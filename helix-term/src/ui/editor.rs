use crate::{
    commands::{self, OnKeyCallback, OnKeyCallbackKind},
    compositor::{Component, Context, Event, EventResult},
    events::{OnModeSwitch, PostCommand},
    handlers::completion::CompletionItem,
    ctrl, key,
    keymap::{KeymapResult, Keymaps},
    ui::{
        document::{render_document, LinePos, TextRenderer},
        statusline,
        text_decorations::{self, Decoration, DecorationManager, InlineDiagnostics},
        Completion, ProgressSpinners,
    },
};

use helix_core::{
    diagnostic::NumberOrString,
    graphemes::{next_grapheme_boundary, prev_grapheme_boundary},
    movement::Direction,
    syntax::{self, OverlayHighlights},
    text_annotations::TextAnnotations,
    unicode::width::UnicodeWidthStr,
    visual_offset_from_block, Change, Position, Range, Selection, Transaction,
};
use helix_view::{
    annotations::diagnostics::DiagnosticFilter,
    document::{Mode, SCRATCH_BUFFER_NAME},
    editor::{CompleteAction, CursorShapeConfig},
    graphics::{Color, CursorKind, Modifier, Rect, Style},
    input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind},
    keyboard::{KeyCode, KeyModifiers},
    Document, Editor, Theme, View,
};
use std::{mem::take, num::NonZeroUsize, ops, path::PathBuf, rc::Rc, time::Instant};

use tui::{buffer::Buffer as Surface, text::Span};

pub struct EditorView {
    pub keymaps: Keymaps,
    on_next_key: Option<(OnKeyCallback, OnKeyCallbackKind)>,
    pseudo_pending: Vec<KeyEvent>,
    pub(crate) last_insert: (commands::MappableCommand, Vec<InsertEvent>),
    pub(crate) completion: Option<Completion>,
    spinners: ProgressSpinners,
    /// Tracks if the terminal window is focused by reaction to terminal focus events
    terminal_focused: bool,
    /// Tracks the last sidebar click (visible index, time) for double-click detection
    last_sidebar_click: Option<(usize, Instant)>,
    /// Accumulated numeric count for the next file-tree navigation command (e.g. `5j`).
    file_tree_count: Option<NonZeroUsize>,
    /// Terminal cursor position for the selected file-tree row, updated each frame.
    file_tree_cursor: Option<helix_core::Position>,
    /// Pending multi-key sequence in the file tree (e.g. `space`, `space g`).
    /// Stored as `KeyEvent`s so they can be fed directly to `KeyTrie::search`.
    file_tree_seq: Vec<KeyEvent>,
    /// Set when `g` is pressed in the file tree; resolved on the next key:
    /// `gg` = jump to top, `gf` = file picker, `gs` = search.
    file_tree_g_pending: bool,
    /// Vim operator-pending state (e.g. after pressing `d`, awaiting motion).
    pub vim_pending_op: Option<crate::vim::PendingOperator>,
    /// Last completed vim operator+motion for dot-repeat.
    pub vim_last_action: Option<crate::vim::VimRepeatAction>,
}

#[derive(Debug, Clone)]
pub enum InsertEvent {
    Key(KeyEvent),
    CompletionApply {
        trigger_offset: usize,
        changes: Vec<Change>,
    },
    TriggerCompletion,
    RequestCompletion,
}

impl EditorView {
    pub fn new(keymaps: Keymaps) -> Self {
        Self {
            keymaps,
            on_next_key: None,
            pseudo_pending: Vec::new(),
            last_insert: (commands::MappableCommand::normal_mode, Vec::new()),
            completion: None,
            spinners: ProgressSpinners::default(),
            terminal_focused: true,
            last_sidebar_click: None,
            file_tree_count: None,
            file_tree_cursor: None,
            file_tree_seq: Vec::new(),
            file_tree_g_pending: false,
            vim_pending_op: None,
            vim_last_action: None,
        }
    }

    pub fn spinners_mut(&mut self) -> &mut ProgressSpinners {
        &mut self.spinners
    }

    pub fn render_view(
        &self,
        editor: &Editor,
        doc: &Document,
        view: &View,
        viewport: Rect,
        surface: &mut Surface,
        is_focused: bool,
    ) {
        let inner = view.inner_area(doc);
        let area = view.area;
        let theme = &editor.theme;
        let config = editor.config();
        let loader = editor.syn_loader.load();

        let view_offset = doc.view_offset(view.id);

        let text_annotations = view.text_annotations(doc, Some(theme));
        let mut decorations = DecorationManager::default();

        if is_focused && config.cursorline {
            decorations.add_decoration(Self::cursorline(doc, view, theme));
        }

        if is_focused && config.cursorcolumn {
            Self::highlight_cursorcolumn(doc, view, surface, theme, inner, &text_annotations);
        }

        // Set DAP highlights, if needed.
        if let Some(frame) = editor.current_stack_frame() {
            let dap_line = frame.line.saturating_sub(1);
            let style = theme.get("ui.highlight.frameline");
            let line_decoration = move |renderer: &mut TextRenderer, pos: LinePos| {
                if pos.doc_line != dap_line {
                    return;
                }
                renderer.set_style(Rect::new(inner.x, pos.visual_line, inner.width, 1), style);
            };

            decorations.add_decoration(line_decoration);
        }

        let syntax_highlighter =
            Self::doc_syntax_highlighter(doc, view_offset.anchor, inner.height, &loader);
        let mut overlays = Vec::new();

        overlays.push(Self::overlay_syntax_highlights(
            doc,
            view_offset.anchor,
            inner.height,
            &text_annotations,
        ));

        if doc
            .language_config()
            .and_then(|config| config.rainbow_brackets)
            .unwrap_or(config.rainbow_brackets)
        {
            if let Some(overlay) =
                Self::doc_rainbow_highlights(doc, view_offset.anchor, inner.height, theme, &loader)
            {
                overlays.push(overlay);
            }
        }

        if let Some(overlay) = Self::doc_document_link_highlights(doc, theme) {
            overlays.push(overlay);
        }

        Self::doc_diagnostics_highlights_into(doc, theme, &mut overlays);

        if is_focused {
            if config.lsp.auto_document_highlight {
                if let Some(overlay) = Self::doc_document_highlights(doc, view, theme) {
                    overlays.push(overlay);
                }
            }
            if let Some(tabstops) = Self::tabstop_highlights(doc, theme) {
                overlays.push(tabstops);
            }
            overlays.push(Self::doc_selection_highlights(
                editor.mode(),
                doc,
                view,
                theme,
                &config.cursor_shape,
                self.terminal_focused,
            ));
            if let Some(overlay) = Self::highlight_focused_view_elements(view, doc, theme) {
                overlays.push(overlay);
            }
        }

        let gutter_overflow = view.gutter_offset(doc) == 0;
        if !gutter_overflow {
            Self::render_gutter(
                editor,
                doc,
                view,
                view.area,
                theme,
                is_focused & self.terminal_focused,
                &mut decorations,
            );
        }

        Self::render_rulers(editor, doc, view, inner, surface, theme);

        let primary_cursor = doc
            .selection(view.id)
            .primary()
            .cursor(doc.text().slice(..));
        if is_focused {
            decorations.add_decoration(text_decorations::Cursor {
                cache: &editor.cursor_cache,
                primary_cursor,
            });
        }
        let width = view.inner_width(doc);
        let config = doc.config.load();
        let enable_cursor_line = view
            .diagnostics_handler
            .show_cursorline_diagnostics(doc, view.id);
        let inline_diagnostic_config = config.inline_diagnostics.prepare(width, enable_cursor_line);
        decorations.add_decoration(InlineDiagnostics::new(
            doc,
            theme,
            primary_cursor,
            inline_diagnostic_config,
            config.end_of_line_diagnostics,
        ));
        render_document(
            surface,
            inner,
            doc,
            view_offset,
            &text_annotations,
            syntax_highlighter,
            overlays,
            theme,
            decorations,
        );

        // if we're not at the edge of the screen, draw a right border
        if viewport.right() != view.area.right() {
            let x = area.right();
            let border_style = theme.get("ui.window");
            for y in area.top()..area.bottom() {
                surface[(x, y)]
                    .set_symbol(tui::symbols::line::VERTICAL)
                    //.set_symbol(" ")
                    .set_style(border_style);
            }
        }

        if config.inline_diagnostics.disabled()
            && config.end_of_line_diagnostics == DiagnosticFilter::Disable
        {
            Self::render_diagnostics(doc, view, inner, surface, theme);
        }

        let statusline_area = view
            .area
            .clip_top(view.area.height.saturating_sub(1))
            .clip_bottom(1); // -1 from bottom to remove commandline

        let mut context =
            statusline::RenderContext::new(editor, doc, view, is_focused, &self.spinners);

        statusline::render(&mut context, statusline_area, surface);
    }

    pub fn render_rulers(
        editor: &Editor,
        doc: &Document,
        view: &View,
        viewport: Rect,
        surface: &mut Surface,
        theme: &Theme,
    ) {
        let editor_rulers = &editor.config().rulers;
        let ruler_theme = theme
            .try_get("ui.virtual.ruler")
            .unwrap_or_else(|| Style::default().bg(Color::Red));

        let rulers = doc
            .language_config()
            .and_then(|config| config.rulers.as_ref())
            .unwrap_or(editor_rulers);

        let view_offset = doc.view_offset(view.id);

        rulers
            .iter()
            // View might be horizontally scrolled, convert from absolute distance
            // from the 1st column to relative distance from left of viewport
            .filter_map(|ruler| ruler.checked_sub(1 + view_offset.horizontal_offset as u16))
            .filter(|ruler| ruler < &viewport.width)
            .map(|ruler| viewport.clip_left(ruler).with_width(1))
            .for_each(|area| surface.set_style(area, ruler_theme))
    }

    fn viewport_byte_range(
        text: helix_core::RopeSlice,
        row: usize,
        height: u16,
    ) -> std::ops::Range<usize> {
        // Calculate viewport byte ranges:
        // Saturating subs to make it inclusive zero indexing.
        let last_line = text.len_lines().saturating_sub(1);
        let last_visible_line = (row + height as usize).saturating_sub(1).min(last_line);
        let start = text.line_to_byte(row.min(last_line));
        let end = text.line_to_byte(last_visible_line + 1);

        start..end
    }

    /// Get the syntax highlighter for a document in a view represented by the first line
    /// and column (`offset`) and the last line. This is done instead of using a view
    /// directly to enable rendering syntax highlighted docs anywhere (eg. picker preview)
    pub fn doc_syntax_highlighter<'editor>(
        doc: &'editor Document,
        anchor: usize,
        height: u16,
        loader: &'editor syntax::Loader,
    ) -> Option<syntax::Highlighter<'editor>> {
        let syntax = doc.syntax()?;
        let text = doc.text().slice(..);
        let row = text.char_to_line(anchor.min(text.len_chars()));
        let range = Self::viewport_byte_range(text, row, height);
        let range = range.start as u32..range.end as u32;

        let highlighter = syntax.highlighter(text, loader, range);
        Some(highlighter)
    }

    pub fn overlay_syntax_highlights(
        doc: &Document,
        anchor: usize,
        height: u16,
        text_annotations: &TextAnnotations,
    ) -> OverlayHighlights {
        let text = doc.text().slice(..);
        let row = text.char_to_line(anchor.min(text.len_chars()));

        let mut range = Self::viewport_byte_range(text, row, height);
        range = text.byte_to_char(range.start)..text.byte_to_char(range.end);

        text_annotations.collect_overlay_highlights(range)
    }

    pub fn doc_rainbow_highlights(
        doc: &Document,
        anchor: usize,
        height: u16,
        theme: &Theme,
        loader: &syntax::Loader,
    ) -> Option<OverlayHighlights> {
        let syntax = doc.syntax()?;
        let text = doc.text().slice(..);
        let row = text.char_to_line(anchor.min(text.len_chars()));
        let visible_range = Self::viewport_byte_range(text, row, height);
        let start = syntax::child_for_byte_range(
            &syntax.tree().root_node(),
            visible_range.start as u32..visible_range.end as u32,
        )
        .map_or(visible_range.start as u32, |node| node.start_byte());
        let range = start..visible_range.end as u32;

        Some(syntax.rainbow_highlights(text, theme.rainbow_length(), loader, range))
    }

    /// Get highlight spans for document diagnostics
    pub fn doc_diagnostics_highlights_into(
        doc: &Document,
        theme: &Theme,
        overlay_highlights: &mut Vec<OverlayHighlights>,
    ) {
        use helix_core::diagnostic::{DiagnosticTag, Range, Severity};
        let get_scope_of = |scope| {
            theme
                .find_highlight_exact(scope)
                // get one of the themes below as fallback values
                .or_else(|| theme.find_highlight_exact("diagnostic"))
                .or_else(|| theme.find_highlight_exact("ui.cursor"))
                .or_else(|| theme.find_highlight_exact("ui.selection"))
                .expect(
                    "at least one of the following scopes must be defined in the theme: `diagnostic`, `ui.cursor`, or `ui.selection`",
                )
        };

        // Diagnostic tags
        let unnecessary = theme.find_highlight_exact("diagnostic.unnecessary");
        let deprecated = theme.find_highlight_exact("diagnostic.deprecated");

        let mut default_vec = Vec::new();
        let mut info_vec = Vec::new();
        let mut hint_vec = Vec::new();
        let mut warning_vec = Vec::new();
        let mut error_vec = Vec::new();
        let mut unnecessary_vec = Vec::new();
        let mut deprecated_vec = Vec::new();

        let push_diagnostic = |vec: &mut Vec<ops::Range<usize>>, range: Range| {
            // If any diagnostic overlaps ranges with the prior diagnostic,
            // merge the two together. Otherwise push a new span.
            match vec.last_mut() {
                Some(existing_range) if range.start <= existing_range.end => {
                    // This branch merges overlapping diagnostics, assuming that the current
                    // diagnostic starts on range.start or later. If this assertion fails,
                    // we will discard some part of `diagnostic`. This implies that
                    // `doc.diagnostics()` is not sorted by `diagnostic.range`.
                    debug_assert!(existing_range.start <= range.start);
                    existing_range.end = range.end.max(existing_range.end)
                }
                _ => vec.push(range.start..range.end),
            }
        };

        for diagnostic in doc.diagnostics() {
            // Separate diagnostics into different Vecs by severity.
            let vec = match diagnostic.severity {
                Some(Severity::Info) => &mut info_vec,
                Some(Severity::Hint) => &mut hint_vec,
                Some(Severity::Warning) => &mut warning_vec,
                Some(Severity::Error) => &mut error_vec,
                _ => &mut default_vec,
            };

            // If the diagnostic has tags and a non-warning/error severity, skip rendering
            // the diagnostic as info/hint/default and only render it as unnecessary/deprecated
            // instead. For warning/error diagnostics, render both the severity highlight and
            // the tag highlight.
            if diagnostic.tags.is_empty()
                || matches!(
                    diagnostic.severity,
                    Some(Severity::Warning | Severity::Error)
                )
            {
                push_diagnostic(vec, diagnostic.range);
            }

            for tag in &diagnostic.tags {
                match tag {
                    DiagnosticTag::Unnecessary => {
                        if unnecessary.is_some() {
                            push_diagnostic(&mut unnecessary_vec, diagnostic.range)
                        }
                    }
                    DiagnosticTag::Deprecated => {
                        if deprecated.is_some() {
                            push_diagnostic(&mut deprecated_vec, diagnostic.range)
                        }
                    }
                }
            }
        }

        overlay_highlights.push(OverlayHighlights::Homogeneous {
            highlight: get_scope_of("diagnostic"),
            ranges: default_vec,
        });
        if let Some(highlight) = unnecessary {
            overlay_highlights.push(OverlayHighlights::Homogeneous {
                highlight,
                ranges: unnecessary_vec,
            });
        }
        if let Some(highlight) = deprecated {
            overlay_highlights.push(OverlayHighlights::Homogeneous {
                highlight,
                ranges: deprecated_vec,
            });
        }
        overlay_highlights.extend([
            OverlayHighlights::Homogeneous {
                highlight: get_scope_of("diagnostic.info"),
                ranges: info_vec,
            },
            OverlayHighlights::Homogeneous {
                highlight: get_scope_of("diagnostic.hint"),
                ranges: hint_vec,
            },
            OverlayHighlights::Homogeneous {
                highlight: get_scope_of("diagnostic.warning"),
                ranges: warning_vec,
            },
            OverlayHighlights::Homogeneous {
                highlight: get_scope_of("diagnostic.error"),
                ranges: error_vec,
            },
        ]);
    }

    pub fn doc_document_highlights(
        doc: &Document,
        view: &View,
        theme: &Theme,
    ) -> Option<OverlayHighlights> {
        let ranges = doc.document_highlights(view.id)?;
        if ranges.is_empty() {
            return None;
        }

        let highlight = theme
            .find_highlight_exact("ui.highlight")
            .or_else(|| theme.find_highlight_exact("ui.selection"))
            .or_else(|| theme.find_highlight_exact("ui.cursor"))?;

        Some(OverlayHighlights::Homogeneous {
            highlight,
            ranges: ranges.to_vec(),
        })
    }

    pub fn doc_document_link_highlights(
        doc: &Document,
        theme: &Theme,
    ) -> Option<OverlayHighlights> {
        let highlight = theme
            .find_highlight_exact("markup.link.url")
            .or_else(|| theme.find_highlight_exact("markup.link"))?;

        if doc.document_links.is_empty() {
            return None;
        }

        let mut ranges: Vec<ops::Range<usize>> = Vec::new();
        for link in &doc.document_links {
            if link.start >= link.end {
                continue;
            }

            match ranges.last_mut() {
                Some(existing_range) if link.start <= existing_range.end => {
                    existing_range.end = existing_range.end.max(link.end);
                }
                _ => ranges.push(link.start..link.end),
            }
        }

        if ranges.is_empty() {
            return None;
        }

        Some(OverlayHighlights::Homogeneous { highlight, ranges })
    }

    /// Get highlight spans for selections in a document view.
    pub fn doc_selection_highlights(
        mode: Mode,
        doc: &Document,
        view: &View,
        theme: &Theme,
        cursor_shape_config: &CursorShapeConfig,
        is_terminal_focused: bool,
    ) -> OverlayHighlights {
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id);
        let primary_idx = selection.primary_index();

        let cursorkind = cursor_shape_config.from_mode(mode);
        let cursor_is_block = cursorkind == CursorKind::Block;

        let selection_scope = theme
            .find_highlight_exact("ui.selection")
            .expect("could not find `ui.selection` scope in the theme!");
        let primary_selection_scope = theme
            .find_highlight_exact("ui.selection.primary")
            .unwrap_or(selection_scope);

        let base_cursor_scope = theme
            .find_highlight_exact("ui.cursor")
            .unwrap_or(selection_scope);
        let base_primary_cursor_scope = theme
            .find_highlight("ui.cursor.primary")
            .unwrap_or(base_cursor_scope);

        let cursor_scope = match mode {
            Mode::Insert => theme.find_highlight_exact("ui.cursor.insert"),
            Mode::Select | Mode::Visual => theme.find_highlight_exact("ui.cursor.select"),
            Mode::Normal => theme.find_highlight_exact("ui.cursor.normal"),
        }
        .unwrap_or(base_cursor_scope);

        let primary_cursor_scope = match mode {
            Mode::Insert => theme.find_highlight_exact("ui.cursor.primary.insert"),
            Mode::Select | Mode::Visual => theme.find_highlight_exact("ui.cursor.primary.select"),
            Mode::Normal => theme.find_highlight_exact("ui.cursor.primary.normal"),
        }
        .unwrap_or(base_primary_cursor_scope);

        let mut spans = Vec::new();
        for (i, range) in selection.iter().enumerate() {
            let selection_is_primary = i == primary_idx;
            let (cursor_scope, selection_scope) = if selection_is_primary {
                (primary_cursor_scope, primary_selection_scope)
            } else {
                (cursor_scope, selection_scope)
            };

            // Special-case: cursor at end of the rope.
            if range.head == range.anchor && range.head == text.len_chars() {
                if !selection_is_primary || (cursor_is_block && is_terminal_focused) {
                    // Bar and underline cursors are drawn by the terminal
                    // BUG: If the editor area loses focus while having a bar or
                    // underline cursor (eg. when a regex prompt has focus) then
                    // the primary cursor will be invisible. This doesn't happen
                    // with block cursors since we manually draw *all* cursors.
                    spans.push((cursor_scope, range.head..range.head + 1));
                }
                continue;
            }

            let range = range.min_width_1(text);
            if range.head > range.anchor {
                // Standard case.
                let cursor_start = prev_grapheme_boundary(text, range.head);
                // non block cursors look like they exclude the cursor
                let selection_end =
                    if selection_is_primary && !cursor_is_block && mode != Mode::Insert {
                        range.head
                    } else {
                        cursor_start
                    };
                spans.push((selection_scope, range.anchor..selection_end));
                // add block cursors
                // skip primary cursor if terminal is unfocused - terminal cursor is used in that case
                if !selection_is_primary || (cursor_is_block && is_terminal_focused) {
                    spans.push((cursor_scope, cursor_start..range.head));
                }
            } else {
                // Reverse case.
                let cursor_end = next_grapheme_boundary(text, range.head);
                // add block cursors
                // skip primary cursor if terminal is unfocused - terminal cursor is used in that case
                if !selection_is_primary || (cursor_is_block && is_terminal_focused) {
                    spans.push((cursor_scope, range.head..cursor_end));
                }
                // non block cursors look like they exclude the cursor
                let selection_start = if selection_is_primary
                    && !cursor_is_block
                    && !(mode == Mode::Insert && cursor_end == range.anchor)
                {
                    range.head
                } else {
                    cursor_end
                };
                spans.push((selection_scope, selection_start..range.anchor));
            }
        }

        OverlayHighlights::Heterogenous { highlights: spans }
    }

    /// Render brace match, etc (meant for the focused view only)
    pub fn highlight_focused_view_elements(
        view: &View,
        doc: &Document,
        theme: &Theme,
    ) -> Option<OverlayHighlights> {
        // Highlight matching braces
        let syntax = doc.syntax()?;
        let highlight = theme.find_highlight_exact("ui.cursor.match")?;
        let text = doc.text().slice(..);
        let pos = doc.selection(view.id).primary().cursor(text);
        let pos = helix_core::match_brackets::find_matching_bracket(syntax, text, pos)?;
        Some(OverlayHighlights::single(highlight, pos..pos + 1))
    }

    pub fn tabstop_highlights(doc: &Document, theme: &Theme) -> Option<OverlayHighlights> {
        let snippet = doc.active_snippet.as_ref()?;
        let highlight = theme.find_highlight_exact("tabstop")?;
        let mut ranges = Vec::new();
        for tabstop in snippet.tabstops() {
            ranges.extend(tabstop.ranges.iter().map(|range| range.start..range.end));
        }
        Some(OverlayHighlights::Homogeneous { highlight, ranges })
    }

    /// Render bufferline at the top
    pub fn render_bufferline(editor: &Editor, viewport: Rect, surface: &mut Surface) {
        let scratch = PathBuf::from(SCRATCH_BUFFER_NAME); // default filename to use for scratch buffer
        surface.clear_with(
            viewport,
            editor
                .theme
                .try_get("ui.bufferline.background")
                .unwrap_or_else(|| editor.theme.get("ui.statusline")),
        );

        let bufferline_active = editor
            .theme
            .try_get("ui.bufferline.active")
            .unwrap_or_else(|| editor.theme.get("ui.statusline.active"));

        let bufferline_inactive = editor
            .theme
            .try_get("ui.bufferline")
            .unwrap_or_else(|| editor.theme.get("ui.statusline.inactive"));

        let mut x = viewport.x;
        let current_doc = view!(editor).doc;

        for doc in editor.documents() {
            let fname = doc
                .path()
                .unwrap_or(&scratch)
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();

            let style = if current_doc == doc.id() {
                bufferline_active
            } else {
                bufferline_inactive
            };

            let text = format!(" {}{} ", fname, if doc.is_modified() { "[+]" } else { "" });
            let used_width = viewport.x.saturating_sub(x);
            let rem_width = surface.area.width.saturating_sub(used_width);

            x = surface
                .set_stringn(x, viewport.y, text, rem_width as usize, style)
                .0;

            if x >= surface.area.right() {
                break;
            }
        }
    }

    pub fn render_gutter<'d>(
        editor: &'d Editor,
        doc: &'d Document,
        view: &View,
        viewport: Rect,
        theme: &Theme,
        is_focused: bool,
        decoration_manager: &mut DecorationManager<'d>,
    ) {
        let text = doc.text().slice(..);
        let cursors: Rc<[_]> = doc
            .selection(view.id)
            .iter()
            .map(|range| range.cursor_line(text))
            .collect();

        let mut offset = 0;

        let gutter_style = theme.get("ui.gutter");
        let gutter_selected_style = theme.get("ui.gutter.selected");
        let gutter_style_virtual = theme.get("ui.gutter.virtual");
        let gutter_selected_style_virtual = theme.get("ui.gutter.selected.virtual");

        for gutter_type in view.gutters() {
            let mut gutter = gutter_type.style(editor, doc, view, theme, is_focused);
            let width = gutter_type.width(view, doc);
            // avoid lots of small allocations by reusing a text buffer for each line
            let mut text = String::with_capacity(width);
            let cursors = cursors.clone();
            let gutter_decoration = move |renderer: &mut TextRenderer, pos: LinePos| {
                // TODO handle softwrap in gutters
                let selected = cursors.contains(&pos.doc_line);
                let x = viewport.x + offset;
                let y = pos.visual_line;

                let gutter_style = match (selected, pos.first_visual_line) {
                    (false, true) => gutter_style,
                    (true, true) => gutter_selected_style,
                    (false, false) => gutter_style_virtual,
                    (true, false) => gutter_selected_style_virtual,
                };

                if let Some(style) =
                    gutter(pos.doc_line, selected, pos.first_visual_line, &mut text)
                {
                    renderer.set_stringn(x, y, &text, width, gutter_style.patch(style));
                } else {
                    renderer.set_style(
                        Rect {
                            x,
                            y,
                            width: width as u16,
                            height: 1,
                        },
                        gutter_style,
                    );
                }
                text.clear();
            };
            decoration_manager.add_decoration(gutter_decoration);

            offset += width as u16;
        }
    }

    pub fn render_diagnostics(
        doc: &Document,
        view: &View,
        viewport: Rect,
        surface: &mut Surface,
        theme: &Theme,
    ) {
        use helix_core::diagnostic::Severity;
        use tui::{
            layout::Alignment,
            text::Text,
            widgets::{Paragraph, Widget, Wrap},
        };

        let cursor = doc
            .selection(view.id)
            .primary()
            .cursor(doc.text().slice(..));

        let diagnostics = doc.diagnostics().iter().filter(|diagnostic| {
            diagnostic.range.start <= cursor && diagnostic.range.end >= cursor
        });

        let warning = theme.get("warning");
        let error = theme.get("error");
        let info = theme.get("info");
        let hint = theme.get("hint");

        let mut lines = Vec::new();
        let background_style = theme.get("ui.background");
        for diagnostic in diagnostics {
            let style = Style::reset()
                .patch(background_style)
                .patch(match diagnostic.severity {
                    Some(Severity::Error) => error,
                    Some(Severity::Warning) | None => warning,
                    Some(Severity::Info) => info,
                    Some(Severity::Hint) => hint,
                });
            let text = Text::styled(&diagnostic.message, style);
            lines.extend(text.lines);
            let code = diagnostic.code.as_ref().map(|x| match x {
                NumberOrString::Number(n) => format!("({n})"),
                NumberOrString::String(s) => format!("({s})"),
            });
            if let Some(code) = code {
                let span = Span::styled(code, style);
                lines.push(span.into());
            }
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(&text)
            .alignment(Alignment::Right)
            .wrap(Wrap { trim: true });
        let width = 100.min(viewport.width);
        let height = 15.min(viewport.height);
        paragraph.render(
            Rect::new(viewport.right() - width, viewport.y + 1, width, height),
            surface,
        );
    }

    /// Apply the highlighting on the lines where a cursor is active
    pub fn cursorline(doc: &Document, view: &View, theme: &Theme) -> impl Decoration {
        let text = doc.text().slice(..);
        // TODO only highlight the visual line that contains the cursor instead of the full visual line
        let primary_line = doc.selection(view.id).primary().cursor_line(text);

        // The secondary_lines do contain the primary_line, it doesn't matter
        // as the else-if clause in the loop later won't test for the
        // secondary_lines if primary_line == line.
        // It's used inside a loop so the collect isn't needless:
        // https://github.com/rust-lang/rust-clippy/issues/6164
        #[allow(clippy::needless_collect)]
        let secondary_lines: Vec<_> = doc
            .selection(view.id)
            .iter()
            .map(|range| range.cursor_line(text))
            .collect();

        let primary_style = theme.get("ui.cursorline.primary");
        let secondary_style = theme.get("ui.cursorline.secondary");
        let viewport = view.area;

        move |renderer: &mut TextRenderer, pos: LinePos| {
            let area = Rect::new(viewport.x, pos.visual_line, viewport.width, 1);
            if primary_line == pos.doc_line {
                renderer.set_style(area, primary_style);
            } else if secondary_lines.binary_search(&pos.doc_line).is_ok() {
                renderer.set_style(area, secondary_style);
            }
        }
    }

    /// Apply the highlighting on the columns where a cursor is active
    pub fn highlight_cursorcolumn(
        doc: &Document,
        view: &View,
        surface: &mut Surface,
        theme: &Theme,
        viewport: Rect,
        text_annotations: &TextAnnotations,
    ) {
        let text = doc.text().slice(..);

        // Manual fallback behaviour:
        // ui.cursorcolumn.{p/s} -> ui.cursorcolumn -> ui.cursorline.{p/s}
        let primary_style = theme
            .try_get_exact("ui.cursorcolumn.primary")
            .or_else(|| theme.try_get_exact("ui.cursorcolumn"))
            .unwrap_or_else(|| theme.get("ui.cursorline.primary"));
        let secondary_style = theme
            .try_get_exact("ui.cursorcolumn.secondary")
            .or_else(|| theme.try_get_exact("ui.cursorcolumn"))
            .unwrap_or_else(|| theme.get("ui.cursorline.secondary"));

        let inner_area = view.inner_area(doc);

        let selection = doc.selection(view.id);
        let view_offset = doc.view_offset(view.id);
        let primary = selection.primary();
        let text_format = doc.text_format(viewport.width, None);
        for range in selection.iter() {
            let is_primary = primary == *range;
            let cursor = range.cursor(text);

            let Position { col, .. } =
                visual_offset_from_block(text, cursor, cursor, &text_format, text_annotations).0;

            // if the cursor is horizontally in the view
            if col >= view_offset.horizontal_offset
                && inner_area.width > (col - view_offset.horizontal_offset) as u16
            {
                let area = Rect::new(
                    inner_area.x + (col - view_offset.horizontal_offset) as u16,
                    view.area.y,
                    1,
                    view.area.height,
                );
                if is_primary {
                    surface.set_style(area, primary_style)
                } else {
                    surface.set_style(area, secondary_style)
                }
            }
        }
    }

    /// Handle events by looking them up in `self.keymaps`. Returns None
    /// if event was handled (a command was executed or a subkeymap was
    /// activated). Only KeymapResult::{NotFound, Cancelled} is returned
    /// otherwise.
    fn handle_keymap_event(
        &mut self,
        mode: Mode,
        cxt: &mut commands::Context,
        event: KeyEvent,
    ) -> Option<KeymapResult> {
        let mut last_mode = mode;
        self.pseudo_pending.extend(self.keymaps.pending());
        let key_result = self.keymaps.get(mode, event);
        cxt.editor.autoinfo = self.keymaps.sticky_infobox().cloned();

        let mut execute_command = |command: &commands::MappableCommand| {
            command.execute(cxt);
            helix_event::dispatch(PostCommand { command, cx: cxt });

            let current_mode = cxt.editor.mode();
            if current_mode != last_mode {
                helix_event::dispatch(OnModeSwitch {
                    old_mode: last_mode,
                    new_mode: current_mode,
                    cx: cxt,
                });

                // HAXX: if we just entered insert mode from normal, clear key buf
                // and record the command that got us into this mode.
                if current_mode == Mode::Insert {
                    // how we entered insert mode is important, and we should track that so
                    // we can repeat the side effect.
                    self.last_insert.0 = command.clone();
                    self.last_insert.1.clear();
                }
            }

            last_mode = current_mode;
        };

        match &key_result {
            KeymapResult::Matched(command) => {
                execute_command(command);
            }
            KeymapResult::Pending(node) => cxt.editor.autoinfo = Some(node.infobox()),
            KeymapResult::MatchedSequence(commands) => {
                for command in commands {
                    execute_command(command);
                }
            }
            KeymapResult::NotFound | KeymapResult::Cancelled(_) => return Some(key_result),
        }
        None
    }

    pub fn insert_mode(&mut self, cx: &mut commands::Context, event: KeyEvent) {
        if let Some(keyresult) = self.handle_keymap_event(Mode::Insert, cx, event) {
            match keyresult {
                KeymapResult::NotFound => {
                    if !self.on_next_key(OnKeyCallbackKind::Fallback, cx, event) {
                        if let Some(ch) = event.char() {
                            commands::insert::insert_char(cx, ch)
                        }
                    }
                }
                KeymapResult::Cancelled(pending) => {
                    for ev in pending {
                        match ev.char() {
                            Some(ch) => commands::insert::insert_char(cx, ch),
                            None => {
                                if let KeymapResult::Matched(command) =
                                    self.keymaps.get(Mode::Insert, ev)
                                {
                                    command.execute(cx);
                                }
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn command_mode(&mut self, mode: Mode, cxt: &mut commands::Context, event: KeyEvent) {
        // Vim operator-pending: intercept keys after an operator like `d`, `y`, `c`
        if self.vim_pending_op.is_some() {
            self.vim_operator_pending_dispatch(cxt, event);
            return;
        }

        match (event, cxt.editor.count) {
            // If the count is already started and the input is a number, always continue the count.
            (key!(i @ '0'..='9'), Some(count)) => {
                let i = i.to_digit(10).unwrap() as usize;
                let count = count.get() * 10 + i;
                if count > 100_000_000 {
                    return;
                }
                cxt.editor.count = NonZeroUsize::new(count);
            }
            // A non-zero digit will start the count if that number isn't used by a keymap.
            (key!(i @ '1'..='9'), None) if !self.keymaps.contains_key(mode, event) => {
                let i = i.to_digit(10).unwrap() as usize;
                cxt.editor.count = NonZeroUsize::new(i);
            }
            // special handling for repeat operator
            (key!('.'), _) if self.keymaps.pending().is_empty() => {
                for _ in 0..cxt.editor.count.map_or(1, NonZeroUsize::into) {
                    // first execute whatever put us into insert mode
                    self.last_insert.0.execute(cxt);
                    let mut last_savepoint = None;
                    let mut last_request_savepoint = None;
                    // then replay the inputs
                    for key in self.last_insert.1.clone() {
                        match key {
                            InsertEvent::Key(key) => self.insert_mode(cxt, key),
                            InsertEvent::CompletionApply {
                                trigger_offset,
                                changes,
                            } => {
                                let (view, doc) = current!(cxt.editor);

                                if let Some(last_savepoint) = last_savepoint.as_deref() {
                                    doc.restore(view, last_savepoint, true);
                                }

                                let text = doc.text().slice(..);
                                let cursor = doc.selection(view.id).primary().cursor(text);

                                let shift_position = |pos: usize| -> usize {
                                    (pos + cursor).saturating_sub(trigger_offset)
                                };

                                let tx = Transaction::change(
                                    doc.text(),
                                    changes.iter().cloned().map(|(start, end, t)| {
                                        (shift_position(start), shift_position(end), t)
                                    }),
                                );
                                doc.apply(&tx, view.id);
                            }
                            InsertEvent::TriggerCompletion => {
                                last_savepoint = take(&mut last_request_savepoint);
                            }
                            InsertEvent::RequestCompletion => {
                                let (view, doc) = current!(cxt.editor);
                                last_request_savepoint = Some(doc.savepoint(view));
                            }
                        }
                    }
                }
                cxt.editor.count = None;
            }
            _ => {
                // set the count
                cxt.count = cxt.editor.count;
                // TODO: edge case: 0j -> reset to 1
                // if this fails, count was Some(0)
                // debug_assert!(cxt.count != 0);

                // set the register
                cxt.register = cxt.editor.selected_register.take();

                let res = self.handle_keymap_event(mode, cxt, event);
                if matches!(&res, Some(KeymapResult::NotFound)) {
                    self.on_next_key(OnKeyCallbackKind::Fallback, cxt, event);
                }
                if self.keymaps.pending().is_empty() {
                    cxt.editor.count = None
                } else {
                    cxt.editor.selected_register = cxt.register.take();
                }
            }
        }
    }

    /// Dispatch a key while a vim operator is pending (e.g. `d` awaiting a motion).
    fn vim_operator_pending_dispatch(
        &mut self,
        cxt: &mut commands::Context,
        event: KeyEvent,
    ) {
        use helix_view::keyboard::KeyCode;

        let pending = match self.vim_pending_op.take() {
            Some(p) => p,
            None => return,
        };

        match event {
            // Escape cancels operator-pending
            key!(Esc) => {}

            // Digit accumulates motion count
            key!(i @ '0'..='9') if cxt.editor.count.is_some() || i != '0' => {
                let digit = i.to_digit(10).unwrap() as usize;
                let count = cxt
                    .editor
                    .count
                    .map_or(digit, |c| c.get() * 10 + digit);
                if count <= 100_000_000 {
                    cxt.editor.count = std::num::NonZeroUsize::new(count);
                }
                // Put pending op back and wait for more keys
                self.vim_pending_op = Some(pending);
            }

            // Doubled operator (dd, yy, cc, >>, <<) → act on whole line(s)
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: helix_view::keyboard::KeyModifiers::NONE,
            } if c == pending.op.key_char() => {
                let pre = pending.pre_count.map_or(1usize, |n| n.get());
                let motion = cxt.editor.count.map_or(1usize, |n| n.get());
                let total = pre * motion;
                cxt.editor.count = None;

                // Select `total` lines from cursor line
                let (view, doc) = current!(cxt.editor);
                let text = doc.text().slice(..);
                let selection =
                    doc.selection(view.id).clone().transform(|range| {
                        let cursor_line = range.cursor_line(text);
                        let start = text.line_to_char(cursor_line);
                        let end_line =
                            (cursor_line + total).min(text.len_lines());
                        let end = text.line_to_char(end_line);
                        Range::new(start, end)
                    });
                doc.set_selection(view.id, selection);

                if let Some(reg) = pending.register {
                    cxt.register = Some(reg);
                }
                self.vim_apply_operator(cxt, &pending.op);

                // Record for dot-repeat
                let key_events = vec![event];
                self.vim_last_action = Some(crate::vim::VimRepeatAction {
                    register: pending.register,
                    total_count: total,
                    op: pending.op,
                    motion_keys: key_events,
                });
            }

            // Text-object: `i` or `a` prefix (e.g. `diw`, `ca"`)
            key!('i') | key!('a') => {
                let inner = event == key!('i');
                let objtype = if inner {
                    helix_core::textobject::TextObject::Inside
                } else {
                    helix_core::textobject::TextObject::Around
                };

                let pre = pending.pre_count.map_or(1usize, |n| n.get());
                let motion = cxt.editor.count.map_or(1usize, |n| n.get());
                let total = pre * motion;
                cxt.editor.count = None;
                cxt.count = std::num::NonZeroUsize::new(total);

                // Call select_textobject which will set on_next_key to wait for obj char
                commands::select_textobject(cxt, objtype);

                // Wrap the on_next_key callback to also apply the operator afterwards
                if let Some((orig_cb, kind)) = cxt.on_next_key_callback.take() {
                    let op = pending.op;
                    let register = pending.register;
                    let ia_key = event;
                    cxt.on_next_key_callback = Some((
                        Box::new(move |cx: &mut commands::Context, obj_event: KeyEvent| {
                            // First run the textobject selection
                            orig_cb(cx, obj_event);

                            // Then apply the operator via callback
                            let motion_keys = vec![ia_key, obj_event];
                            cx.callback.push(Box::new(move |compositor, cx2| {
                                if let Some(editor_view) = compositor
                                    .find::<crate::ui::EditorView>()
                                {
                                    let mut fake_cx = commands::Context {
                                        register,
                                        count: std::num::NonZeroUsize::new(total),
                                        editor: cx2.editor,
                                        callback: Vec::new(),
                                        on_next_key_callback: None,
                                        jobs: cx2.jobs,
                                    };
                                    editor_view.vim_apply_operator(&mut fake_cx, &op);
                                    editor_view.vim_last_action =
                                        Some(crate::vim::VimRepeatAction {
                                            register,
                                            total_count: total,
                                            op,
                                            motion_keys,
                                        });
                                }
                            }));
                        }),
                        kind,
                    ));
                }
            }

            // Compound motions starting with `g` (e.g. `gg` = goto file start)
            key!('g') => {
                let pending_for_g = pending;
                cxt.on_next_key_callback = Some((
                    Box::new(move |cx: &mut commands::Context, g_event: KeyEvent| {
                        let extend_cmd = match g_event {
                            key!('g') => Some(commands::MappableCommand::extend_to_file_start),
                            key!('e') => Some(commands::MappableCommand::extend_to_last_line),
                            _ => None,
                        };
                        if let Some(cmd) = extend_cmd {
                            let pre = pending_for_g.pre_count.map_or(1usize, |n| n.get());
                            let motion = cx.editor.count.map_or(1usize, |n| n.get());
                            let total = pre * motion;
                            cx.editor.count = None;
                            cx.count = std::num::NonZeroUsize::new(total);

                            cmd.execute(cx);

                            if let Some(reg) = pending_for_g.register {
                                cx.register = Some(reg);
                            }

                            let op = pending_for_g.op;
                            let register = pending_for_g.register;
                            let motion_keys = vec![key!('g'), g_event];
                            cx.callback.push(Box::new(move |compositor, cx2| {
                                if let Some(editor_view) = compositor
                                    .find::<crate::ui::EditorView>()
                                {
                                    let mut fake_cx = commands::Context {
                                        register,
                                        count: std::num::NonZeroUsize::new(total),
                                        editor: cx2.editor,
                                        callback: Vec::new(),
                                        on_next_key_callback: None,
                                        jobs: cx2.jobs,
                                    };
                                    editor_view.vim_apply_operator(&mut fake_cx, &op);
                                    editor_view.vim_last_action =
                                        Some(crate::vim::VimRepeatAction {
                                            register,
                                            total_count: total,
                                            op,
                                            motion_keys,
                                        });
                                }
                            }));
                        }
                    }),
                    OnKeyCallbackKind::PseudoPending,
                ));
            }

            // Motion key → look up extend_* equivalent, execute it, apply operator
            _ => {
                let pre = pending.pre_count.map_or(1usize, |n| n.get());
                let motion = cxt.editor.count.map_or(1usize, |n| n.get());
                let total = pre * motion;
                cxt.editor.count = None;
                cxt.count = std::num::NonZeroUsize::new(total);

                if let Some(extend_cmd) = Self::vim_motion_to_extend(event) {
                    extend_cmd.execute(cxt);

                    // If the motion set an on_next_key callback (like f/t/F/T),
                    // wrap it to apply the operator after the motion completes.
                    if cxt.on_next_key_callback.is_some() {
                        let op = pending.op;
                        let register = pending.register;
                        let motion_event = event;
                        if let Some((orig_cb, kind)) = cxt.on_next_key_callback.take() {
                            cxt.on_next_key_callback = Some((
                                Box::new(move |cx: &mut commands::Context, char_event: KeyEvent| {
                                    orig_cb(cx, char_event);

                                    let motion_keys = vec![motion_event, char_event];
                                    cx.callback.push(Box::new(move |compositor, cx2| {
                                        if let Some(editor_view) = compositor
                                            .find::<crate::ui::EditorView>()
                                        {
                                            let mut fake_cx = commands::Context {
                                                register,
                                                count: std::num::NonZeroUsize::new(total),
                                                editor: cx2.editor,
                                                callback: Vec::new(),
                                                on_next_key_callback: None,
                                                jobs: cx2.jobs,
                                            };
                                            editor_view.vim_apply_operator(&mut fake_cx, &op);
                                            editor_view.vim_last_action =
                                                Some(crate::vim::VimRepeatAction {
                                                    register,
                                                    total_count: total,
                                                    op,
                                                    motion_keys,
                                                });
                                        }
                                    }));
                                }),
                                kind,
                            ));
                        }
                    } else {
                        // For linewise motions (j/k), expand selection to full lines
                        if Self::vim_is_linewise_motion(event) {
                            Self::vim_expand_to_line_bounds(cxt.editor);
                        }

                        if let Some(reg) = pending.register {
                            cxt.register = Some(reg);
                        }
                        self.vim_apply_operator(cxt, &pending.op);

                        // Record for dot-repeat
                        self.vim_last_action = Some(crate::vim::VimRepeatAction {
                            register: pending.register,
                            total_count: total,
                            op: pending.op,
                            motion_keys: vec![event],
                        });
                    }
                }
                // Unrecognized key → cancel (pending already taken)
            }
        }
    }

    /// Apply a vim operator to the current selection.
    pub fn vim_apply_operator(
        &mut self,
        cxt: &mut commands::Context,
        op: &crate::vim::Operator,
    ) {
        use crate::vim::Operator;

        match op {
            Operator::Delete => {
                commands::MappableCommand::delete_selection.execute(cxt);
            }
            Operator::Yank => {
                commands::MappableCommand::yank.execute(cxt);
                // Collapse selection after yank
                let (view, doc) = current!(cxt.editor);
                let text = doc.text().slice(..);
                let selection =
                    doc.selection(view.id).clone().transform(|range| {
                        Range::point(range.cursor(text))
                    });
                doc.set_selection(view.id, selection);
            }
            Operator::Change => {
                commands::MappableCommand::change_selection.execute(cxt);
            }
            Operator::Indent => {
                commands::MappableCommand::indent.execute(cxt);
                // Collapse selection after indent
                let (view, doc) = current!(cxt.editor);
                let text = doc.text().slice(..);
                let selection =
                    doc.selection(view.id).clone().transform(|range| {
                        Range::point(range.cursor(text))
                    });
                doc.set_selection(view.id, selection);
            }
            Operator::Unindent => {
                commands::MappableCommand::unindent.execute(cxt);
                let (view, doc) = current!(cxt.editor);
                let text = doc.text().slice(..);
                let selection =
                    doc.selection(view.id).clone().transform(|range| {
                        Range::point(range.cursor(text))
                    });
                doc.set_selection(view.id, selection);
            }
        }
    }

    /// Whether a vim motion in operator-pending mode should be linewise.
    fn vim_is_linewise_motion(event: KeyEvent) -> bool {
        matches!(event, key!('j') | key!(Down) | key!('k') | key!(Up))
    }

    /// Expand the current selection to full line boundaries (linewise).
    fn vim_expand_to_line_bounds(editor: &mut Editor) {
        let (view, doc) = current!(editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            let from_line = text.char_to_line(range.from());
            let to_line = text.char_to_line(range.to());
            let start = text.line_to_char(from_line);
            let end = text.line_to_char((to_line + 1).min(text.len_lines()));
            Range::new(start, end)
        });
        doc.set_selection(view.id, selection);
    }

    /// Map a vim motion keypress to the corresponding `extend_*` command.
    pub fn vim_motion_to_extend(event: KeyEvent) -> Option<commands::MappableCommand> {
        use commands::MappableCommand as C;

        Some(match event {
            key!('h') | key!(Left) => C::extend_char_left,
            key!('j') | key!(Down) => C::extend_visual_line_down,
            key!('k') | key!(Up) => C::extend_visual_line_up,
            key!('l') | key!(Right) => C::extend_char_right,
            key!('w') => C::extend_next_word_start,
            key!('b') => C::extend_prev_word_start,
            key!('e') => C::extend_next_word_end,
            key!('W') => C::extend_next_long_word_start,
            key!('B') => C::extend_prev_long_word_start,
            key!('E') => C::extend_next_long_word_end,
            key!('f') => C::extend_next_char,
            key!('t') => C::extend_till_char,
            key!('F') => C::extend_prev_char,
            key!('T') => C::extend_till_prev_char,
            key!('0') => C::extend_to_line_start,
            key!('$') | key!(End) => C::extend_to_line_end,
            key!(Home) => C::extend_to_line_start,
            key!('G') => C::extend_to_last_line,
            key!('^') => C::extend_to_first_nonwhitespace,
            key!('n') => C::extend_search_next,
            key!('N') => C::extend_search_prev,
            _ => return None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_completion(
        &mut self,
        editor: &mut Editor,
        items: Vec<CompletionItem>,
        trigger_offset: usize,
        size: Rect,
    ) -> Option<Rect> {
        let mut completion = Completion::new(editor, items, trigger_offset);

        if completion.is_empty() {
            // skip if we got no completion results
            return None;
        }

        let area = completion.area(size, editor);
        editor.last_completion = Some(CompleteAction::Triggered);
        self.last_insert.1.push(InsertEvent::TriggerCompletion);

        // TODO : propagate required size on resize to completion too
        self.completion = Some(completion);
        Some(area)
    }

    pub fn clear_completion(&mut self, editor: &mut Editor) -> Option<OnKeyCallback> {
        self.completion = None;
        let mut on_next_key: Option<OnKeyCallback> = None;
        editor.handlers.completions.request_controller.restart();
        editor.handlers.completions.active_completions.clear();
        if let Some(last_completion) = editor.last_completion.take() {
            match last_completion {
                CompleteAction::Triggered => (),
                CompleteAction::Applied {
                    trigger_offset,
                    changes,
                    placeholder,
                } => {
                    self.last_insert.1.push(InsertEvent::CompletionApply {
                        trigger_offset,
                        changes,
                    });
                    on_next_key = placeholder.then_some(Box::new(|cx, key| {
                        if let Some(c) = key.char() {
                            let (view, doc) = current!(cx.editor);
                            if let Some(snippet) = &doc.active_snippet {
                                doc.apply(&snippet.delete_placeholder(doc.text()), view.id);
                            }
                            commands::insert::insert_char(cx, c);
                        }
                    }))
                }
                CompleteAction::Selected { savepoint } => {
                    let (view, doc) = current!(editor);
                    doc.restore(view, &savepoint, false);
                }
            }
        }
        on_next_key
    }

    pub fn handle_idle_timeout(&mut self, cx: &mut commands::Context) -> EventResult {
        commands::compute_inlay_hints_for_all_views(cx.editor, cx.jobs);

        EventResult::Ignored(None)
    }
}

impl EditorView {
    /// must be called whenever the editor processed input that
    /// is not a `KeyEvent`. In these cases any pending keys/on next
    /// key callbacks must be canceled.
    fn handle_non_key_input(&mut self, cxt: &mut commands::Context) {
        cxt.editor.status_msg = None;
        cxt.editor.reset_idle_timer();
        // HACKS: create a fake key event that will never trigger any actual map
        // and therefore simply acts as "dismiss"
        let null_key_event = KeyEvent {
            code: KeyCode::Null,
            modifiers: KeyModifiers::empty(),
        };
        // dismiss any pending keys
        if let Some((on_next_key, _)) = self.on_next_key.take() {
            on_next_key(cxt, null_key_event);
        }
        self.handle_keymap_event(cxt.editor.mode, cxt, null_key_event);
        self.pseudo_pending.clear();
    }

    fn handle_mouse_event(
        &mut self,
        event: &MouseEvent,
        cxt: &mut commands::Context,
    ) -> EventResult {
        if event.kind != MouseEventKind::Moved {
            self.handle_non_key_input(cxt)
        }

        let config = cxt.editor.config();
        let MouseEvent {
            kind,
            row,
            column,
            modifiers,
            ..
        } = *event;

        // Check if click is in the file tree sidebar area
        if cxt.editor.left_sidebar.rendered_width > 0 && cxt.editor.left_sidebar.visible {
            let sidebar_width = cxt.editor.left_sidebar.rendered_width;
            if column < sidebar_width {
                match kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        cxt.editor.left_sidebar.focused = true;

                        // Calculate which node was clicked
                        let tree_row = row.saturating_sub(
                            if matches!(config.bufferline,
                                helix_view::editor::BufferLine::Always)
                                || (matches!(config.bufferline,
                                    helix_view::editor::BufferLine::Multiple)
                                    && cxt.editor.documents.len() > 1)
                            {
                                1
                            } else {
                                0
                            },
                        );
                        let clicked_idx = cxt.editor.file_tree.as_ref()
                            .map(|t| t.scroll_offset() + tree_row as usize)
                            .unwrap_or(0);
                        let in_bounds = cxt.editor.file_tree.as_ref()
                            .map(|t| clicked_idx < t.visible().len())
                            .unwrap_or(false);

                        if in_bounds {
                            let now = Instant::now();
                            let is_double = self.last_sidebar_click
                                .map(|(last_idx, last_time)| {
                                    last_idx == clicked_idx
                                        && now.duration_since(last_time).as_millis() < 400
                                })
                                .unwrap_or(false);

                            if is_double {
                                self.last_sidebar_click = None;
                                // Reuse the same open/toggle logic as the Enter key handler
                                let open_path = if let Some(ref mut tree) = cxt.editor.file_tree {
                                    tree.move_to(clicked_idx);
                                    if let Some(id) = tree.selected_id() {
                                        match tree.nodes().get(id).map(|n| n.kind) {
                                            Some(helix_view::file_tree::NodeKind::Directory) => {
                                                tree.toggle_expand(id, &config.file_tree);
                                                None
                                            }
                                            Some(helix_view::file_tree::NodeKind::File) => {
                                                Some(tree.node_path(id))
                                            }
                                            None => None,
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };
                                if let Some(path) = open_path {
                                    let view_count = cxt.editor.tree.views().count();
                                    if view_count > 1 {
                                        if let Some(picker) =
                                            crate::ui::split_picker::SplitPicker::new(path, cxt.editor)
                                        {
                                            cxt.callback.push(Box::new(move |compositor, _cx| {
                                                compositor.push(Box::new(picker));
                                            }));
                                        }
                                    } else if let Err(e) =
                                        cxt.editor.open(&path, helix_view::editor::Action::Replace)
                                    {
                                        cxt.editor.set_error(format!("{}", e));
                                    } else {
                                        cxt.editor.left_sidebar.focused = false;
                                    }
                                }
                            } else {
                                self.last_sidebar_click = Some((clicked_idx, now));
                                if let Some(ref mut tree) = cxt.editor.file_tree {
                                    tree.move_to(clicked_idx);
                                }
                            }
                        }
                        return EventResult::Consumed(None);
                    }
                    MouseEventKind::ScrollUp => {
                        if let Some(ref mut tree) = cxt.editor.file_tree {
                            for _ in 0..3 {
                                tree.move_up();
                            }
                        }
                        return EventResult::Consumed(None);
                    }
                    MouseEventKind::ScrollDown => {
                        if let Some(ref mut tree) = cxt.editor.file_tree {
                            for _ in 0..3 {
                                tree.move_down();
                            }
                        }
                        return EventResult::Consumed(None);
                    }
                    _ => {}
                }
            } else {
                // Click in editor area: unfocus sidebar
                if cxt.editor.left_sidebar.focused
                    && matches!(kind, MouseEventKind::Down(MouseButton::Left))
                {
                    cxt.editor.left_sidebar.focused = false;
                }
            }
        }

        let pos_and_view = |editor: &Editor, row, column, ignore_virtual_text| {
            editor.tree.views().find_map(|(view, _focus)| {
                view.pos_at_screen_coords(
                    &editor.documents[&view.doc],
                    row,
                    column,
                    ignore_virtual_text,
                )
                .map(|pos| (pos, view.id))
            })
        };

        let gutter_coords_and_view = |editor: &Editor, row, column| {
            editor.tree.views().find_map(|(view, _focus)| {
                view.gutter_coords_at_screen_coords(row, column)
                    .map(|coords| (coords, view.id))
            })
        };

        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let editor = &mut cxt.editor;

                if let Some((pos, view_id)) = pos_and_view(editor, row, column, true) {
                    editor.focus(view_id);

                    let prev_view_id = view!(editor).id;
                    let doc = doc_mut!(editor, &view!(editor, view_id).doc);

                    if modifiers == KeyModifiers::ALT {
                        let selection = doc.selection(view_id).clone();
                        doc.set_selection(view_id, selection.push(Range::point(pos)));
                    } else if editor.mode == Mode::Select || editor.mode == Mode::Visual {
                        // Discards non-primary selections for consistent UX with normal mode
                        let primary = doc.selection(view_id).primary().put_cursor(
                            doc.text().slice(..),
                            pos,
                            true,
                        );
                        editor.mouse_down_range = Some(primary);
                        doc.set_selection(view_id, Selection::single(primary.anchor, primary.head));
                    } else {
                        doc.set_selection(view_id, Selection::point(pos));
                    }

                    if view_id != prev_view_id {
                        self.clear_completion(editor);
                    }

                    editor.ensure_cursor_in_view(view_id);

                    return EventResult::Consumed(None);
                }

                if let Some((coords, view_id)) = gutter_coords_and_view(editor, row, column) {
                    editor.focus(view_id);

                    let (view, doc) = current!(cxt.editor);

                    let path = match doc.path() {
                        Some(path) => path.clone(),
                        None => return EventResult::Ignored(None),
                    };

                    if let Some(char_idx) =
                        view.pos_at_visual_coords(doc, coords.row as u16, coords.col as u16, true)
                    {
                        let line = doc.text().char_to_line(char_idx);
                        commands::dap_toggle_breakpoint_impl(cxt, path, line);
                        return EventResult::Consumed(None);
                    }
                }

                EventResult::Ignored(None)
            }

            MouseEventKind::Drag(MouseButton::Left) => {
                let (view, doc) = current!(cxt.editor);

                let pos = match view.pos_at_screen_coords(doc, row, column, true) {
                    Some(pos) => pos,
                    None => return EventResult::Ignored(None),
                };

                let mut selection = doc.selection(view.id).clone();
                let primary = selection.primary_mut();
                *primary = primary.put_cursor(doc.text().slice(..), pos, true);
                doc.set_selection(view.id, selection);
                let view_id = view.id;
                cxt.editor.ensure_cursor_in_view(view_id);
                EventResult::Consumed(None)
            }

            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let current_view = cxt.editor.tree.focus;

                let direction = match event.kind {
                    MouseEventKind::ScrollUp => Direction::Backward,
                    MouseEventKind::ScrollDown => Direction::Forward,
                    _ => unreachable!(),
                };

                match pos_and_view(cxt.editor, row, column, false) {
                    Some((_, view_id)) => cxt.editor.tree.focus = view_id,
                    None => return EventResult::Ignored(None),
                }

                let offset = config.scroll_lines.unsigned_abs();
                commands::scroll(cxt, offset, direction, false);

                cxt.editor.tree.focus = current_view;
                cxt.editor.ensure_cursor_in_view(current_view);

                EventResult::Consumed(None)
            }

            MouseEventKind::Up(MouseButton::Left) => {
                if !config.middle_click_paste {
                    return EventResult::Ignored(None);
                }

                let (view, doc) = current!(cxt.editor);

                let should_yank = match cxt.editor.mouse_down_range.take() {
                    Some(down_range) => doc.selection(view.id).primary() != down_range,
                    None => {
                        // This should not happen under normal cases. We fall back to the original
                        // behavior of yanking on non-single-char selections.
                        doc.selection(view.id)
                            .primary()
                            .slice(doc.text().slice(..))
                            .len_chars()
                            > 1
                    }
                };

                if should_yank {
                    commands::yank_main_selection_to_register(
                        cxt.editor,
                        config.mouse_yank_register,
                    );
                    EventResult::Consumed(None)
                } else {
                    EventResult::Ignored(None)
                }
            }

            MouseEventKind::Up(MouseButton::Right) => {
                if let Some((pos, view_id)) = gutter_coords_and_view(cxt.editor, row, column) {
                    cxt.editor.focus(view_id);

                    if let Some((pos, _)) = pos_and_view(cxt.editor, row, column, true) {
                        doc_mut!(cxt.editor).set_selection(view_id, Selection::point(pos));
                    } else {
                        let (view, doc) = current!(cxt.editor);

                        if let Some(pos) = view.pos_at_visual_coords(doc, pos.row as u16, 0, true) {
                            doc.set_selection(view_id, Selection::point(pos));
                            match modifiers {
                                KeyModifiers::ALT => {
                                    commands::MappableCommand::dap_edit_log.execute(cxt)
                                }
                                _ => commands::MappableCommand::dap_edit_condition.execute(cxt),
                            };
                        }
                    }

                    cxt.editor.ensure_cursor_in_view(view_id);
                    return EventResult::Consumed(None);
                }
                EventResult::Ignored(None)
            }

            MouseEventKind::Up(MouseButton::Middle) => {
                let editor = &mut cxt.editor;
                if !config.middle_click_paste {
                    return EventResult::Ignored(None);
                }

                if modifiers == KeyModifiers::ALT {
                    commands::replace_selections_with_register(
                        cxt.editor,
                        config.mouse_yank_register,
                        cxt.count(),
                    );

                    return EventResult::Consumed(None);
                }

                if let Some((pos, view_id)) = pos_and_view(editor, row, column, true) {
                    let doc = doc_mut!(editor, &view!(editor, view_id).doc);
                    doc.set_selection(view_id, Selection::point(pos));
                    cxt.editor.focus(view_id);

                    commands::paste(
                        cxt.editor,
                        config.mouse_yank_register,
                        commands::Paste::Before,
                        cxt.count(),
                    );

                    return EventResult::Consumed(None);
                }

                EventResult::Ignored(None)
            }

            _ => EventResult::Ignored(None),
        }
    }
    fn on_next_key(
        &mut self,
        kind: OnKeyCallbackKind,
        ctx: &mut commands::Context,
        event: KeyEvent,
    ) -> bool {
        if let Some((on_next_key, kind_)) = self.on_next_key.take() {
            if kind == kind_ {
                on_next_key(ctx, event);
                true
            } else {
                self.on_next_key = Some((on_next_key, kind_));
                false
            }
        } else {
            false
        }
    }

    fn handle_file_tree_key(
        &mut self,
        key: KeyEvent,
        cx: &mut commands::Context,
    ) -> EventResult {
        use helix_view::file_tree::{NodeKind, PromptMode};

        let config = cx.editor.config().file_tree.clone();

        // Check whether any prompt is active. When active, all keys are
        // consumed by the prompt and do not reach the navigation bindings.
        let prompt_active = cx
            .editor
            .file_tree
            .as_ref()
            .map_or(false, |t| !matches!(t.prompt_mode(), PromptMode::None));

        if prompt_active {
            match key.code {
                KeyCode::Esc => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        tree.prompt_cancel();
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Enter => {
                    // Pre-validate: if the prompt would create a path that
                    // already exists, show an error and keep the prompt open.
                    let conflict = cx
                        .editor
                        .file_tree
                        .as_ref()
                        .and_then(|t| t.prompt_would_create_path())
                        .filter(|p| p.exists());
                    if let Some(conflict_path) = conflict {
                        if let Some(ref mut tree) = cx.editor.file_tree {
                            tree.set_status(format!(
                                "Already exists: {}",
                                conflict_path.display()
                            ));
                        }
                        return EventResult::Consumed(None);
                    }

                    let commit = cx
                        .editor
                        .file_tree
                        .as_mut()
                        .and_then(|t| t.prompt_confirm());
                    if let Some(commit) = commit {
                        Self::dispatch_prompt_commit(commit, cx);
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Backspace => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        tree.prompt_pop();
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Left => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        tree.prompt_cursor_left();
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Right => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        tree.prompt_cursor_right();
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        if matches!(tree.prompt_mode(), PromptMode::Search) {
                            tree.search_next();
                        }
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        if matches!(tree.prompt_mode(), PromptMode::Search) {
                            tree.search_prev();
                        }
                    }
                    return EventResult::Consumed(None);
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // In DeleteConfirm mode only accept 'y' to confirm; anything
                    // else cancels.
                    let is_delete_confirm = cx
                        .editor
                        .file_tree
                        .as_ref()
                        .map_or(false, |t| matches!(
                            t.prompt_mode(),
                            PromptMode::DeleteConfirm { .. } | PromptMode::DeleteConfirmMulti { .. }
                        ));

                    if is_delete_confirm {
                        let commit = if ch == 'y' {
                            cx.editor
                                .file_tree
                                .as_mut()
                                .and_then(|t| t.prompt_confirm())
                        } else {
                            if let Some(ref mut tree) = cx.editor.file_tree {
                                tree.prompt_cancel();
                            }
                            None
                        };
                        if let Some(commit) = commit {
                            Self::dispatch_prompt_commit(commit, cx);
                        }
                    } else {
                        if let Some(ref mut tree) = cx.editor.file_tree {
                            tree.prompt_push(ch);
                        }
                    }
                    return EventResult::Consumed(None);
                }
                _ => return EventResult::Consumed(None),
            }
        }

        // If C-w was pressed on the previous keypress, intercept only the
        // sidebar-resize keys (> / <). All other keys are forwarded to normal
        // mode by replaying the full C-w + follow-up key sequence, so that
        // window commands like `C-w q` (wclose) or `C-w s` (hsplit) work
        // transparently from the file tree.
        if cx.editor.left_sidebar.window_cmd_pending {
            cx.editor.left_sidebar.window_cmd_pending = false;
            return match key.code {
                KeyCode::Char('>') => {
                    cx.editor.left_sidebar.grow(cx.count() as u16);
                    EventResult::Consumed(None)
                }
                KeyCode::Char('<') => {
                    cx.editor.left_sidebar.shrink(cx.count() as u16);
                    EventResult::Consumed(None)
                }
                _ => {
                    cx.editor.left_sidebar.focused = false;
                    cx.callback.push(Box::new(move |compositor, cx| {
                        compositor.handle_event(&Event::Key(ctrl!('w')), cx);
                        compositor.handle_event(&Event::Key(key), cx);
                    }));
                    EventResult::Consumed(None)
                }
            };
        }

        // --- Keymap-driven scroll: runs before the match for modifier-key chords ---
        // When a modifier key (e.g. C-e, C-d) is remapped in the user's normal-mode
        // keymap to a scroll command or macro sequence, mirror that in the file tree.
        // This lets custom bindings like `C-e = "@9zj"` or `C-d = "@5j"` work here.
        if !key.modifiers.is_empty() {
            self.file_tree_g_pending = false;
            use helix_view::document::Mode;
            use crate::keymap::KeyTrie;
            use crate::commands::MappableCommand;

            let trie_entry = {
                let map = self.keymaps.map();
                map.get(&Mode::Normal)
                    .and_then(|trie| trie.search(&[key]))
                    .cloned()
            };

            if let Some(trie) = trie_entry {
                match trie {
                    KeyTrie::MappableCommand(MappableCommand::Static { name, .. }) => {
                        let height = cx.editor.tree.area().height as usize;
                        let count = self.file_tree_count.take().map_or(1, |n| n.get());
                        let half = (height / 2).max(1);
                        let full = height.max(1);
                        let handled = match name {
                            "page_cursor_half_up" => {
                                if let Some(ref mut tree) = cx.editor.file_tree {
                                    tree.page_up((half * count).max(1));
                                }
                                true
                            }
                            "page_cursor_half_down" => {
                                if let Some(ref mut tree) = cx.editor.file_tree {
                                    tree.page_down((half * count).max(1));
                                }
                                true
                            }
                            "page_up" => {
                                if let Some(ref mut tree) = cx.editor.file_tree {
                                    tree.page_up((full * count).max(1));
                                }
                                true
                            }
                            "page_down" => {
                                if let Some(ref mut tree) = cx.editor.file_tree {
                                    tree.page_down((full * count).max(1));
                                }
                                true
                            }
                            "scroll_up" => {
                                if let Some(ref mut tree) = cx.editor.file_tree {
                                    for _ in 0..count { tree.scroll_view_up(); }
                                }
                                true
                            }
                            "scroll_down" => {
                                if let Some(ref mut tree) = cx.editor.file_tree {
                                    for _ in 0..count { tree.scroll_view_down(); }
                                }
                                true
                            }
                            _ => false,
                        };
                        if handled {
                            return EventResult::Consumed(None);
                        }
                    }
                    KeyTrie::MappableCommand(MappableCommand::Macro { keys, .. }) => {
                        // Replay macro keys through the compositor so they re-enter
                        // this handler. Count-then-action sequences (e.g. `@5j` →
                        // keys [5, j]) then work naturally: digit accumulates the
                        // count, j/k consume it.
                        self.file_tree_count = None;
                        cx.callback.push(Box::new(move |compositor, cx| {
                            for k in keys {
                                compositor.handle_event(&Event::Key(k), cx);
                            }
                        }));
                        return EventResult::Consumed(None);
                    }
                    _ => {}
                }
            }
        }

        // Multi-key sequence handler.
        // `space` enters "command sequence" mode; subsequent keys are looked up
        // in the normal-mode keymap trie so any command the user has bound under
        // `space` (e.g. `space g o f`) works in the file tree without hardcoding.
        let is_space = key.modifiers.is_empty() && key.code == KeyCode::Char(' ');
        let in_seq   = !self.file_tree_seq.is_empty();

        if is_space || in_seq {
            self.file_tree_g_pending = false;
            if key.code == KeyCode::Esc {
                self.file_tree_seq.clear();
                return EventResult::Consumed(None);
            }

            // Only plain (non-modifier) chars extend a sequence.
            if key.modifiers.is_empty() {
                self.file_tree_seq.push(key);

                use crate::keymap::KeyTrie;
                use crate::commands::MappableCommand;

                let trie_entry = {
                    let map = self.keymaps.map();
                    map.get(&helix_view::document::Mode::Normal)
                        .and_then(|trie| trie.search(&self.file_tree_seq))
                        .cloned()
                };

                match trie_entry {
                    // Partial match — wait for more keys.
                    Some(KeyTrie::Node(_)) => return EventResult::Consumed(None),

                    // Full match: static command — execute in file tree context.
                    Some(KeyTrie::MappableCommand(MappableCommand::Static { name, .. })) => {
                        self.file_tree_seq.clear();
                        self.file_tree_count = None;
                        self.handle_file_tree_command(name, cx);
                        return EventResult::Consumed(None);
                    }

                    // Full match: macro — replay the key sequence.
                    Some(KeyTrie::MappableCommand(MappableCommand::Macro { keys, .. })) => {
                        self.file_tree_seq.clear();
                        self.file_tree_count = None;
                        cx.callback.push(Box::new(move |compositor, cx| {
                            for k in keys {
                                compositor.handle_event(&Event::Key(k), cx);
                            }
                        }));
                        return EventResult::Consumed(None);
                    }

                    // No match or other trie variant — abort sequence, eat the key.
                    _ => {
                        self.file_tree_seq.clear();
                        return EventResult::Consumed(None);
                    }
                }
            } else {
                // Modifier key while in a sequence → abort; fall through to normal handling.
                self.file_tree_seq.clear();
            }
        }

        // `g` prefix handler: `gg` = jump to top, `gf` = file picker,
        // `gs` = search in directory.
        if self.file_tree_g_pending {
            self.file_tree_g_pending = false;
            self.file_tree_count = None;
            match key.code {
                KeyCode::Char('g') => {
                    if let Some(ref mut tree) = cx.editor.file_tree {
                        tree.jump_to_top();
                    }
                }
                KeyCode::Char('f') => {
                    use helix_view::file_tree::PickerRoot;
                    let root = cx.editor.file_tree.as_ref().and_then(|tree| {
                        match config.picker_root {
                            PickerRoot::Directory => tree.selected_dir_path(),
                            PickerRoot::Workspace => Some(tree.root().to_path_buf()),
                        }
                    });
                    if let Some(root) = root {
                        cx.editor.left_sidebar.focused = false;
                        let picker = crate::ui::file_picker(&cx.editor, root);
                        cx.callback.push(Box::new(move |compositor, _cx| {
                            compositor.push(Box::new(crate::ui::overlay::overlaid(picker)));
                        }));
                    }
                }
                KeyCode::Char('s') => {
                    use helix_view::file_tree::PickerRoot;
                    let root = cx.editor.file_tree.as_ref().and_then(|tree| {
                        match config.picker_root {
                            PickerRoot::Directory => tree.selected_dir_path(),
                            PickerRoot::Workspace => Some(tree.root().to_path_buf()),
                        }
                    });
                    if let Some(root) = root {
                        cx.editor.left_sidebar.focused = false;
                        commands::global_search_in_dir(cx, root);
                    }
                }
                _ => {}
            }
            return EventResult::Consumed(None);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let count = self.file_tree_count.take().map_or(1, |n| n.get());
                if let Some(ref mut tree) = cx.editor.file_tree {
                    for _ in 0..count { tree.move_down(); }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let count = self.file_tree_count.take().map_or(1, |n| n.get());
                if let Some(ref mut tree) = cx.editor.file_tree {
                    for _ in 0..count { tree.move_up(); }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('h') | KeyCode::Left if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if let Some(id) = tree.selected_id() {
                        let node = tree.nodes().get(id);
                        if let Some(node) = node {
                            if node.kind == NodeKind::Directory && node.expanded {
                                tree.toggle_expand(id, &config);
                            } else if let Some(parent_id) = node.parent {
                                // Move selection to parent directory
                                if let Some(pos) =
                                    tree.visible().iter().position(|&vid| vid == parent_id)
                                {
                                    tree.move_to(pos);
                                }
                            }
                        }
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                cx.editor.left_sidebar.focused = false;
                let leftmost = cx
                    .editor
                    .tree
                    .views()
                    .min_by_key(|(view, _)| view.area.left())
                    .map(|(view, _)| view.id);
                if let Some(id) = leftmost {
                    cx.editor.focus(id);
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let open_path = cx.editor.file_tree.as_ref().and_then(|tree| {
                    let id = tree.selected_id()?;
                    matches!(tree.nodes().get(id).map(|n| n.kind), Some(NodeKind::File))
                        .then(|| tree.node_path(id))
                });
                if let Some(path) = open_path {
                    if let Err(e) =
                        cx.editor.open(&path, helix_view::editor::Action::VerticalSplit)
                    {
                        cx.editor.set_error(format!("{}", e));
                    } else {
                        cx.editor.left_sidebar.focused = false;
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let open_path = cx.editor.file_tree.as_ref().and_then(|tree| {
                    let id = tree.selected_id()?;
                    matches!(tree.nodes().get(id).map(|n| n.kind), Some(NodeKind::File))
                        .then(|| tree.node_path(id))
                });
                if let Some(path) = open_path {
                    if let Err(e) =
                        cx.editor.open(&path, helix_view::editor::Action::HorizontalSplit)
                    {
                        cx.editor.set_error(format!("{}", e));
                    } else {
                        cx.editor.left_sidebar.focused = false;
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let open_path = if let Some(ref mut tree) = cx.editor.file_tree {
                    if let Some(id) = tree.selected_id() {
                        match tree.nodes().get(id).map(|n| n.kind) {
                            Some(NodeKind::Directory) => {
                                tree.toggle_expand(id, &config);
                                None
                            }
                            Some(NodeKind::File) => Some(tree.node_path(id)),
                            None => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(path) = open_path {
                    let view_count = cx.editor.tree.views().count();
                    if view_count > 1 {
                        if let Some(picker) =
                            crate::ui::split_picker::SplitPicker::new(path, cx.editor)
                        {
                            cx.callback.push(Box::new(move |compositor, _cx| {
                                compositor.push(Box::new(picker));
                            }));
                        }
                    } else if let Err(e) =
                        cx.editor.open(&path, helix_view::editor::Action::Replace)
                    {
                        cx.editor.set_error(format!("{}", e));
                    } else {
                        cx.editor.left_sidebar.focused = false;
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('q') => {
                cx.editor.left_sidebar.visible = false;
                cx.editor.left_sidebar.focused = false;
                EventResult::Consumed(None)
            }
            KeyCode::Esc => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.clear_selection();
                }
                cx.editor.left_sidebar.focused = false;
                EventResult::Consumed(None)
            }
            // Phase 5: 'R' refreshes the tree (previously 'r')
            KeyCode::Char('R') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.refresh(&config);
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('E') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if let Some(id) = tree.selected_id() {
                        tree.expand_all(id, &config);
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('C') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.collapse_all();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('g') => {
                self.file_tree_g_pending = true;
                EventResult::Consumed(None)
            }
            KeyCode::Char('G') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.jump_to_bottom();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('/') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.search_start();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('n') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.search_next();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('N') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.search_prev();
                }
                EventResult::Consumed(None)
            }
            // --- File management bindings ---
            KeyCode::Char('a') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.start_new_file();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('A') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.start_new_dir();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if let Some(id) = tree.selected_id() {
                        tree.start_rename(id);
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('v') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if let Some(id) = tree.selected_id() {
                        tree.toggle_selection(id);
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if tree.has_selection() {
                        let paths = tree.selected_paths();
                        tree.start_delete_confirm_multi(paths);
                    } else if let Some(id) = tree.selected_id() {
                        tree.start_delete_confirm(id);
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('y') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if tree.has_selection() {
                        if let Some(count) = tree.yank_selection() {
                            tree.set_status(format!("Yanked {count} items"));
                        }
                    } else if let Some(id) = tree.selected_id() {
                        let path = tree.node_path(id);
                        let display = path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        tree.yank(id);
                        tree.set_status(format!("Yanked: {display}"));
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('Y') => {
                if let Some(id) = cx.editor.file_tree.as_ref().and_then(|t| t.selected_id()) {
                    let path_str = cx.editor.file_tree.as_ref().unwrap()
                        .node_path(id).to_string_lossy().into_owned();
                    let status = format!("Copied path: {path_str}");
                    match cx.editor.registers.write('+', vec![path_str]) {
                        Ok(_) => {
                            if let Some(ref mut tree) = cx.editor.file_tree {
                                tree.set_status(status);
                            }
                        }
                        Err(err) => cx.editor.set_error(err.to_string()),
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('x') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if tree.has_selection() {
                        if let Some(count) = tree.cut_selection() {
                            tree.set_status(format!("Cut {count} items"));
                        }
                    } else if let Some(id) = tree.selected_id() {
                        let path = tree.node_path(id);
                        let display = path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        tree.cut(id);
                        tree.set_status(format!("Cut: {display}"));
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('p') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                cx.callback.push(Box::new(|_compositor, cx| {
                    use helix_view::file_tree_ops::{spawn_copy_file, spawn_move_path};
                    use helix_view::file_tree::ClipboardOp;

                    // Gather the data we need from the tree before borrowing editor.
                    let (clip, dest_dir, tx) = {
                        let Some(ref tree) = cx.editor.file_tree else { return };
                        let Some(clip) = tree.clipboard().cloned() else { return };
                        let Some(dest_dir) = tree.selected_dir_path() else { return };
                        let tx = tree.update_tx();
                        (clip, dest_dir, tx)
                    };

                    match clip.op {
                        ClipboardOp::Copy => {
                            for path in clip.paths {
                                spawn_copy_file(tx.clone(), path, dest_dir.clone());
                            }
                        }
                        ClipboardOp::Cut => {
                            if let Some(ref mut tree) = cx.editor.file_tree {
                                tree.clear_clipboard();
                            }
                            for path in clip.paths {
                                let new_path = dest_dir.join(path.file_name().unwrap());
                                if let Some(doc) = cx.editor.document_by_path(&path) {
                                    let id = doc.id();
                                    cx.editor.set_doc_path(id, &new_path);
                                }
                                spawn_move_path(tx.clone(), path, dest_dir.clone());
                            }
                        }
                    }
                }));
                EventResult::Consumed(None)
            }
            KeyCode::Char('D') => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    if let Some(id) = tree.selected_id() {
                        if tree.nodes().get(id).map(|n| n.kind) == Some(NodeKind::Directory) {
                            tree.set_status("Duplicate is only available for files");
                        } else {
                            tree.start_duplicate(id);
                        }
                    }
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('t') => {
                let dir = cx.editor.file_tree.as_ref()
                    .and_then(|t| t.selected_dir_path());
                if let Some(dir) = dir {
                    let cmd = resolve_terminal_open_cmd(&cx.editor.config());
                    match cmd {
                        Some((prog, args)) => {
                            let result = std::process::Command::new(&prog)
                                .args(&args)
                                .arg(&dir)
                                .spawn();
                            if let Err(e) = result {
                                cx.editor.set_error(format!("terminal: {e}"));
                            }
                        }
                        None => {
                            cx.editor.set_error(
                                "No terminal configured. Set [editor.file-tree] open-terminal or [editor.terminal].".to_string()
                            );
                        }
                    }
                }
                EventResult::Consumed(None)
            }
            // --- Scroll bindings ---
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    let half = (cx.editor.tree.area().height as usize) / 2;
                    tree.page_up(half.max(1));
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    let half = (cx.editor.tree.area().height as usize) / 2;
                    tree.page_down(half.max(1));
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    let page = cx.editor.tree.area().height as usize;
                    tree.page_up(page.max(1));
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    let page = cx.editor.tree.area().height as usize;
                    tree.page_down(page.max(1));
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.scroll_view_up();
                }
                EventResult::Consumed(None)
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                cx.editor.left_sidebar.focused = false;
                EventResult::Consumed(None)
            }
            KeyCode::Char('e') => {
                cx.editor.left_sidebar.toggle_expand();
                EventResult::Consumed(None)
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Consume C-w and arm the pending flag so the *next* key is
                // handled as a window command inside the sidebar handler.
                // This keeps focused=true until we know whether the next key
                // is a resize (> / <) or a navigation/other command.
                cx.editor.left_sidebar.window_cmd_pending = true;
                EventResult::Consumed(None)
            }
            // Pass space through so chord sequences like `space e`, `space f`, etc.
            // work from the file tree.
            KeyCode::Char(' ') if key.modifiers.is_empty() => {
                cx.editor.left_sidebar.focused = false;
                EventResult::Ignored(None)
            }
            // C-right / C-left — grow or shrink the sidebar width, mirroring the
            // same bindings that resize splits when an editor pane is focused.
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                cx.editor.left_sidebar.grow(cx.count() as u16);
                EventResult::Consumed(None)
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                cx.editor.left_sidebar.shrink(cx.count() as u16);
                EventResult::Consumed(None)
            }
            // 's' — grep/search inside the selected directory
            KeyCode::Char('s') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let dir = cx.editor.file_tree.as_ref()
                    .and_then(|t| t.selected_dir_path());
                if let Some(dir) = dir {
                    cx.editor.left_sidebar.focused = false;
                    commands::global_search_in_dir(cx, dir);
                }
                EventResult::Consumed(None)
            }
            // Numeric prefix: accumulate a repeat count for the next j/k action.
            KeyCode::Char(c) if c.is_ascii_digit() && key.modifiers.is_empty() => {
                let digit = (c as usize) - ('0' as usize);
                self.file_tree_count = Some(
                    NonZeroUsize::new(
                        self.file_tree_count
                            .map_or(0, |n| n.get().saturating_mul(10))
                            .saturating_add(digit)
                            .max(1),
                    )
                    .unwrap(),
                );
                EventResult::Consumed(None)
            }
            KeyCode::Char('z') if key.modifiers.is_empty() => {
                // `z` is a no-op separator used in macro sequences like `@9zj`.
                // Preserving the accumulated count lets the next j/k consume it.
                EventResult::Consumed(None)
            }
            _ => {
                self.file_tree_count = None;
                self.file_tree_g_pending = false;
                EventResult::Consumed(None)
            }
        }
    }

    /// Execute a named Helix command in the file tree context.
    ///
    /// Commands that don't make sense without a cursor position (e.g. line-
    /// based git open) are adapted: the file tree has no line concept so the
    /// line argument is omitted.
    fn handle_file_tree_command(&mut self, name: &'static str, cx: &mut commands::Context) {
        match name {
            "git_open_file_in_browser" | "git_open_line_in_browser" => {
                let path = cx.editor.file_tree.as_ref().and_then(|tree| {
                    let id = tree.selected_id()?;
                    Some(tree.node_path(id).to_path_buf())
                });
                if let Some(path) = path {
                    match commands::git_web_url_for_path(&path, None) {
                        Ok(url) => {
                            #[cfg(target_os = "macos")]
                            let opener = "open";
                            #[cfg(not(target_os = "macos"))]
                            let opener = "xdg-open";
                            match std::process::Command::new(opener).arg(&url).spawn() {
                                Ok(_)  => cx.editor.set_status(format!("Opening: {url}")),
                                Err(e) => cx.editor.set_error(format!("Cannot open browser: {e}")),
                            }
                        }
                        Err(e) => cx.editor.set_error(e.to_string()),
                    }
                }
            }
            // Add more file-tree-adapted commands here as needed.
            _ => {}
        }
    }

    /// Dispatch a confirmed prompt action to the appropriate async filesystem
    /// operation or editor action.
    fn dispatch_prompt_commit(
        commit: helix_view::file_tree::PromptCommit,
        cx: &mut commands::Context,
    ) {
        use helix_view::file_tree::PromptCommit;
        use helix_view::file_tree_ops::{spawn_create_dir, spawn_create_file, spawn_delete};

        match commit {
            PromptCommit::Search | PromptCommit::DeleteCancelled => {
                // Nothing to do
            }
            PromptCommit::NewFile { parent_dir, name } => {
                if let Some(ref tree) = cx.editor.file_tree {
                    let tx = tree.update_tx();
                    spawn_create_file(tx, parent_dir, name);
                }
            }
            PromptCommit::NewDir { parent_dir, name } => {
                if let Some(ref tree) = cx.editor.file_tree {
                    let tx = tree.update_tx();
                    spawn_create_dir(tx, parent_dir, name);
                }
            }
            PromptCommit::Rename { old_path, new_name } => {
                let new_path = old_path
                    .parent()
                    .map(|p| p.join(&new_name))
                    .unwrap_or_else(|| PathBuf::from(&new_name));
                let config = cx.editor.config().file_tree.clone();
                cx.callback.push(Box::new(move |_compositor, cx| {
                    match cx.editor.move_path(&old_path, &new_path) {
                        Ok(()) => {
                            // Refresh the tree so the renamed node appears and reveal it.
                            if let Some(ref mut tree) = cx.editor.file_tree {
                                tree.refresh(&config);
                                tree.reveal_path(&new_path, &config);
                            }
                        }
                        Err(e) => {
                            cx.editor.set_error(format!("Rename failed: {e}"));
                        }
                    }
                }));
            }
            PromptCommit::Duplicate { src_path, new_name } => {
                if let Some(ref tree) = cx.editor.file_tree {
                    let tx = tree.update_tx();
                    let dest_dir = src_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_default();
                    let dest = dest_dir.join(&new_name);
                    // Duplicate copies to the same directory with a user-supplied name.
                    tokio::task::spawn_blocking(move || {
                        use helix_view::file_tree::FileTreeUpdate;
                        let result = std::fs::copy(&src_path, &dest).map(|_| ());
                        let update = match result {
                            Ok(()) => FileTreeUpdate::FsOpComplete {
                                refresh_parent: dest_dir,
                                select_path: Some(dest),
                            },
                            Err(e) => FileTreeUpdate::FsOpError {
                                message: format!("Duplicate failed: {}", e),
                            },
                        };
                        let _ = tx.blocking_send(update);
                        helix_event::request_redraw();
                    });
                }
            }
            PromptCommit::DeleteConfirmed(path) => {
                let is_dir = path.is_dir();
                let tx = cx.editor.file_tree.as_ref().map(|t| t.update_tx());
                Self::close_buffers_for_path(cx.editor, &path);
                if let Some(tx) = tx {
                    spawn_delete(tx, path, is_dir);
                }
            }
            PromptCommit::DeleteConfirmedMulti(paths) => {
                let tx = cx.editor.file_tree.as_ref().map(|t| t.update_tx());
                if let Some(tx) = tx {
                    for path in paths {
                        let is_dir = path.is_dir();
                        Self::close_buffers_for_path(cx.editor, &path);
                        spawn_delete(tx.clone(), path, is_dir);
                    }
                }
            }
        }
    }

    fn close_buffers_for_path(editor: &mut Editor, path: &std::path::Path) {
        let ids: Vec<_> = editor
            .documents()
            .filter(|d| d.path().map_or(false, |p| p.starts_with(path)))
            .map(|d| d.id())
            .collect();
        for id in ids {
            let _ = editor.close_document(id, true);
        }
    }
}

/// Resolve the command + leading args used to open a terminal at a directory.
/// The caller appends the directory path as the final argument.
///
/// Resolution order:
/// 1. `[editor.file-tree] open-terminal` — explicit list, e.g. `["tmux", "split-window", "-c"]`
/// 2. `[editor.terminal]` auto-detect — appends the appropriate CWD flag for
///    known terminals (tmux → `-c`, wezterm → `--cwd`).
fn resolve_terminal_open_cmd(
    config: &helix_view::editor::Config,
) -> Option<(String, Vec<String>)> {
    // Explicit override in file-tree config wins.
    if let Some(ref cmd) = config.file_tree.open_terminal {
        if let Some((prog, rest)) = cmd.split_first() {
            return Some((prog.clone(), rest.to_vec()));
        }
    }

    // Fall back to editor.terminal with a CWD flag appended.
    let terminal = config.terminal.as_ref()?;
    let mut args = terminal.args.clone();
    match terminal.command.as_str() {
        "tmux" => args.push("-c".to_string()),
        "wezterm" => args.push("--cwd".to_string()),
        _ => {} // unknown terminal — just append the path directly after args
    }
    Some((terminal.command.clone(), args))
}

impl Component for EditorView {
    fn handle_event(
        &mut self,
        event: &Event,
        context: &mut crate::compositor::Context,
    ) -> EventResult {
        let mut cx = commands::Context {
            editor: context.editor,
            count: None,
            register: None,
            callback: Vec::new(),
            on_next_key_callback: None,
            jobs: context.jobs,
        };

        match event {
            Event::Paste(contents) => {
                self.handle_non_key_input(&mut cx);
                cx.count = cx.editor.count;
                commands::paste_bracketed_value(&mut cx, contents.clone());
                cx.editor.count = None;

                let config = cx.editor.config();
                let mode = cx.editor.mode();
                let (view, doc) = current!(cx.editor);
                view.ensure_cursor_in_view(doc, config.scrolloff);

                // Store a history state if not in insert mode. Otherwise wait till we exit insert
                // to include any edits to the paste in the history state.
                if mode != Mode::Insert {
                    doc.append_changes_to_history(view);
                }

                EventResult::Consumed(None)
            }
            Event::Resize(_width, _height) => {
                // Ignore this event, we handle resizing just before rendering to screen.
                // Handling it here but not re-rendering will cause flashing
                EventResult::Consumed(None)
            }
            Event::Key(mut key) => {
                cx.editor.reset_idle_timer();
                canonicalize_key(&mut key);

                // clear status
                cx.editor.status_msg = None;

                // Intercept keys when file tree is focused
                if cx.editor.left_sidebar.focused {
                    let result = self.handle_file_tree_key(key, &mut cx);
                    if let EventResult::Consumed(_) = &result {
                        // Clear count so it doesn't leak to the next keystroke.
                        cx.editor.count = None;
                        // Collect any callbacks pushed by file tree handlers
                        let callbacks = take(&mut cx.callback);
                        let callback = if callbacks.is_empty() {
                            None
                        } else {
                            let callback: crate::compositor::Callback =
                                Box::new(move |compositor, cx| {
                                    for callback in callbacks {
                                        callback(compositor, cx)
                                    }
                                });
                            Some(callback)
                        };
                        return EventResult::Consumed(callback);
                    }
                }

                let mode = cx.editor.mode();

                if !self.on_next_key(OnKeyCallbackKind::PseudoPending, &mut cx, key) {
                    match mode {
                        Mode::Insert => {
                            // let completion swallow the event if necessary
                            let mut consumed = false;
                            if let Some(completion) = &mut self.completion {
                                let res = {
                                    // use a fake context here
                                    let mut cx = Context {
                                        editor: cx.editor,
                                        jobs: cx.jobs,
                                        scroll: None,
                                    };

                                    if let EventResult::Consumed(callback) =
                                        completion.handle_event(event, &mut cx)
                                    {
                                        consumed = true;
                                        Some(callback)
                                    } else if let EventResult::Consumed(callback) =
                                        completion.handle_event(&Event::Key(key!(Enter)), &mut cx)
                                    {
                                        Some(callback)
                                    } else {
                                        None
                                    }
                                };

                                if let Some(callback) = res {
                                    if callback.is_some() {
                                        // assume close_fn
                                        if let Some(cb) = self.clear_completion(cx.editor) {
                                            if consumed {
                                                cx.on_next_key_callback =
                                                    Some((cb, OnKeyCallbackKind::Fallback))
                                            } else {
                                                self.on_next_key =
                                                    Some((cb, OnKeyCallbackKind::Fallback));
                                            }
                                        }
                                    }
                                }
                            }

                            // if completion didn't take the event, we pass it onto commands
                            if !consumed {
                                self.insert_mode(&mut cx, key);

                                // record last_insert key
                                self.last_insert.1.push(InsertEvent::Key(key));
                            }
                        }
                        mode => self.command_mode(mode, &mut cx, key),
                    }
                }

                self.on_next_key = cx.on_next_key_callback.take();
                match self.on_next_key {
                    Some((_, OnKeyCallbackKind::PseudoPending)) => self.pseudo_pending.push(key),
                    _ => self.pseudo_pending.clear(),
                }

                // appease borrowck
                let callbacks = take(&mut cx.callback);

                // if the command consumed the last view, skip the render.
                // on the next loop cycle the Application will then terminate.
                if cx.editor.should_close() {
                    return EventResult::Ignored(None);
                }

                let config = cx.editor.config();
                let mode = cx.editor.mode();
                let (view, doc) = current!(cx.editor);

                view.ensure_cursor_in_view(doc, config.scrolloff);

                // Store a history state if not in insert mode. This also takes care of
                // committing changes when leaving insert mode.
                if mode != Mode::Insert {
                    doc.append_changes_to_history(view);
                }
                let callback = if callbacks.is_empty() {
                    None
                } else {
                    let callback: crate::compositor::Callback = Box::new(move |compositor, cx| {
                        for callback in callbacks {
                            callback(compositor, cx)
                        }
                    });
                    Some(callback)
                };

                EventResult::Consumed(callback)
            }

            Event::Mouse(event) => self.handle_mouse_event(event, &mut cx),
            Event::IdleTimeout => self.handle_idle_timeout(&mut cx),
            Event::FocusGained => {
                self.terminal_focused = true;
                EventResult::Consumed(None)
            }
            Event::FocusLost => {
                if context.editor.config().auto_save.focus_lost {
                    let options = commands::WriteAllOptions {
                        force: false,
                        write_scratch: false,
                        auto_format: false,
                    };
                    if let Err(e) = commands::typed::write_all_impl(context, options) {
                        context.editor.set_error(format!("{}", e));
                    }
                }
                self.terminal_focused = false;
                EventResult::Consumed(None)
            }
        }
    }

    fn render(&mut self, area: Rect, surface: &mut Surface, cx: &mut Context) {
        // clear with background color
        surface.set_style(area, cx.editor.theme.get("ui.background"));
        let config = cx.editor.config();

        // check if bufferline should be rendered
        use helix_view::editor::BufferLine;
        let use_bufferline = match config.bufferline {
            BufferLine::Always => true,
            BufferLine::Multiple if cx.editor.documents.len() > 1 => true,
            _ => false,
        };

        // -1 for commandline and -1 for bufferline
        let mut editor_area = area.clip_bottom(1);
        if use_bufferline {
            editor_area = editor_area.clip_top(1);
        }

        // File tree sidebar — carve out space before resize
        let sidebar_width = if cx.editor.left_sidebar.visible {
            if cx.editor.left_sidebar.expanded {
                // Expanded: fill up to half the available editor area.
                (editor_area.width / 2).max(cx.editor.left_sidebar.width)
            } else {
                cx.editor.left_sidebar.width.min(editor_area.width.saturating_sub(10) / 3)
            }
        } else {
            0
        };
        cx.editor.left_sidebar.rendered_width = sidebar_width;
        let sidebar_area = if sidebar_width > 0 {
            let sa = Rect::new(
                editor_area.x,
                editor_area.y,
                sidebar_width,
                editor_area.height,
            );
            // +1 for separator column (drawn inside sidebar_area)
            editor_area = editor_area.clip_left(sidebar_width);
            sa
        } else {
            Rect::default()
        };

        // Process pending file tree updates before rendering
        let diff_providers = cx.editor.diff_providers.clone();

        // Follow current file: queue a debounced reveal (only when tree
        // is not focused, so user navigation isn't interrupted)
        if config.file_tree.follow_current_file
            && cx.editor.left_sidebar.visible
            && !cx.editor.left_sidebar.focused
            && self.terminal_focused
        {
            let current_path = {
                let (_view, doc) = current!(cx.editor);
                doc.path().cloned()
            };
            if let Some(path) = current_path {
                if let Some(ref mut tree) = cx.editor.file_tree {
                    tree.request_follow(path);
                }
            }
        }

        if let Some(ref mut tree) = cx.editor.file_tree {
            tree.process_updates(&config.file_tree, Some(&diff_providers));
        }

        // Open any file that was just created by a new-file operation.
        let pending_open = cx.editor.file_tree.as_mut().and_then(|t| t.take_pending_open());
        if let Some(path) = pending_open {
            if let Err(e) = cx.editor.open(&path, helix_view::editor::Action::Replace) {
                cx.editor.set_error(format!("{e}"));
            } else {
                cx.editor.left_sidebar.focused = false;
            }
        }

        // if the terminal size suddenly changed, we need to trigger a resize
        cx.editor.resize(editor_area);

        if use_bufferline {
            Self::render_bufferline(cx.editor, area.with_height(1), surface);
        }

        let sidebar_focused = cx.editor.left_sidebar.focused;
        for (view, is_focused) in cx.editor.tree.views() {
            let doc = cx.editor.document(view.doc).unwrap();
            self.render_view(cx.editor, doc, view, area, surface, is_focused && !sidebar_focused);
        }

        // Render file tree sidebar
        if sidebar_width > 0 {
            // Clamp scroll to viewport before rendering
            if let Some(ref mut tree) = cx.editor.file_tree {
                tree.clamp_scroll(sidebar_area.height as usize);
            }
            if let Some(ref tree) = cx.editor.file_tree {
                super::file_tree::render_file_tree(
                    tree,
                    sidebar_area,
                    surface,
                    cx.editor,
                    cx.editor.left_sidebar.focused,
                    &config.file_tree,
                );
                // Track selected row position so cursor() can place the terminal cursor there.
                let visible_row = tree.selected().saturating_sub(tree.scroll_offset());
                self.file_tree_cursor = Some(helix_core::Position {
                    row: sidebar_area.y as usize + visible_row,
                    col: sidebar_area.x as usize,
                });
            }
        }

        if config.auto_info {
            if let Some(mut info) = cx.editor.autoinfo.take() {
                info.render(area, surface, cx);
                cx.editor.autoinfo = Some(info)
            }
        }

        let key_width = 15u16; // for showing pending keys
        let mut status_msg_width = 0;

        // render status msg
        if let Some((status_msg, severity)) = &cx.editor.status_msg {
            status_msg_width = status_msg.width();
            use helix_view::editor::Severity;
            let style = if *severity == Severity::Error {
                cx.editor.theme.get("error")
            } else {
                cx.editor.theme.get("ui.text")
            };

            surface.set_string(
                area.x,
                area.y + area.height.saturating_sub(1),
                status_msg,
                style,
            );
        }

        if area.width.saturating_sub(status_msg_width as u16) > key_width {
            let mut disp = String::new();
            if let Some(count) = cx.editor.count {
                disp.push_str(&count.to_string())
            }
            for key in self.keymaps.pending() {
                disp.push_str(&key.key_sequence_format());
            }
            for key in &self.pseudo_pending {
                disp.push_str(&key.key_sequence_format());
            }
            let style = cx.editor.theme.get("ui.text");
            let macro_width = if cx.editor.macro_recording.is_some() {
                3
            } else {
                0
            };
            surface.set_string(
                area.x + area.width.saturating_sub(key_width + macro_width),
                area.y + area.height.saturating_sub(1),
                disp.get(disp.len().saturating_sub(key_width as usize)..)
                    .unwrap_or(&disp),
                style,
            );
            if let Some((reg, _)) = cx.editor.macro_recording {
                let disp = format!("[{}]", reg);
                let style = style
                    .fg(helix_view::graphics::Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                surface.set_string(
                    area.x + area.width.saturating_sub(3),
                    area.y + area.height.saturating_sub(1),
                    &disp,
                    style,
                );
            }
        }

        if let Some(completion) = self.completion.as_mut() {
            completion.render(area, surface, cx);
        }
    }

    fn cursor(&self, _area: Rect, editor: &Editor) -> (Option<Position>, CursorKind) {
        if editor.left_sidebar.focused {
            return (self.file_tree_cursor, CursorKind::Block);
        }
        match editor.cursor() {
            // all block cursors are drawn manually
            (pos, CursorKind::Block) => {
                if self.terminal_focused {
                    (pos, CursorKind::Hidden)
                } else {
                    // use terminal cursor when terminal loses focus
                    (pos, CursorKind::Underline)
                }
            }
            cursor => cursor,
        }
    }
}

fn canonicalize_key(key: &mut KeyEvent) {
    if let KeyEvent {
        code: KeyCode::Char(_),
        modifiers: _,
    } = key
    {
        key.modifiers.remove(KeyModifiers::SHIFT)
    }
}
