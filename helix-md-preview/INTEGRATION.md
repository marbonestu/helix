# Markdown preview — development workflow & integration

A live markdown preview for Helix, modelled on
[`markdown-preview.nvim`](https://github.com/selimacerbas/markdown-preview.nvim):
a browser tab that re-renders as you type, with mermaid, KaTeX, syntax
highlighting, and scroll-sync that follows the cursor.

## Architecture (sidecar / hybrid)

```
 Helix (helix-term)                         hx-md-preview (helix-md-preview crate)
 ─────────────────────                      ──────────────────────────────────────
 :markdown-preview  ──spawn──▶  child process
   DocumentDidChange ─┐                       stdin: newline-delimited JSON
   SelectionDidChange ┼─JSON lines──────────▶   {"kind":"doc","text":...}
                      │                          {"kind":"cursor","line":N}
                                              │
                                              ├─ HTTP  GET /        → index.html
                                              └─ SSE   GET /events  → live updates
                                                                         │
                                              Browser ◀── EventSource ───┘
                                                markdown-it + morphdom +
                                                mermaid + KaTeX + highlight.js
```

Why a sidecar: keeps the HTTP/SSE stack (axum/hyper/tower) out of `helix-term`,
is runnable & testable on its own, and could be reused by other editors.

## Protocol

Helix → sidecar, one JSON object per line on stdin:

| message  | shape                                             | when                |
|----------|---------------------------------------------------|---------------------|
| doc      | `{"kind":"doc","path":"…","text":"…"}`            | document changed    |
| cursor   | `{"kind":"cursor","line":N}` (1-based)            | selection changed   |

Sidecar → browser: each line is re-emitted as an SSE event named by `kind`.
The latest `doc` and `cursor` are cached and replayed to new/reloaded tabs.
Closing stdin (Helix exits or toggles off) makes the sidecar exit.

## Development workflow

The sidecar is decoupled, so develop it **standalone first** — no Helix rebuild
needed for the renderer/server, which is the bulk of the work:

```sh
# 1. Run the sidecar and feed it a static doc; a browser tab opens.
printf '{"kind":"doc","text":"# Hello\n\n```mermaid\ngraph TD; A-->B\n```\n\n$E=mc^2$"}\n' \
  | cargo run -p helix-md-preview

# 2. Iterate on src/index.html (renderer) — just reload the tab.

# 3. Drive live updates from a second terminal into the same process by
#    piping a stream, e.g. a tiny loop that re-emits a file on change:
while true; do
  jq -Rs '{kind:"doc",text:.}' < notes.md   # needs jq
  inotifywait -e modify notes.md             # needs inotify-tools
done | cargo run -p helix-md-preview --no-open --port 8722
# open http://127.0.0.1:8722 yourself

# 4. Only once the renderer feels right, wire Helix (below) and test end-to-end.
```

Build everything: `cargo build -p helix-md-preview` (sidecar),
`cargo build -p helix-term` (editor). Make sure `hx-md-preview` is on `PATH`
(or change `SIDECAR_BIN` in the handler to an absolute path).

## Helix-side wiring (apply AFTER the upstream merge is resolved)

The handler already exists at
`helix-term/src/handlers/markdown_preview.rs` (additive, no merge conflict).
Three small edits remain — they touch files that are **currently conflicted by
the in-progress `upstream-merge`, so do them once `git status` is clean**:

1. **Register the module & hooks** — `helix-term/src/handlers.rs`:
   ```rust
   mod markdown_preview;          // alongside the other `mod` lines
   // …inside setup(), next to the other register_hooks() calls:
   markdown_preview::register_hooks(&handlers);
   ```

2. **Add the typed command** — `helix-term/src/commands/typed.rs`:
   ```rust
   fn markdown_preview(
       cx: &mut compositor::Context,
       _args: Args,
       event: PromptEvent,
   ) -> anyhow::Result<()> {
       if event != PromptEvent::Validate {
           return Ok(());
       }
       crate::handlers::markdown_preview::toggle(cx.editor)
   }
   ```
   …and add a `TypableCommand` entry to the `TYPABLE_COMMAND_LIST` array
   (copy the shape of a neighbouring zero-arg command, e.g. `:reload`):
   ```rust
   TypableCommand {
       name: "markdown-preview",
       aliases: &["mdp"],
       doc: "Toggle a live markdown preview in the browser.",
       fun: markdown_preview,
       completer: CommandCompleter::none(),
   },
   ```
   (`toggle()` is `pub` and `register_hooks()` is `pub(super)`; make
   `pub mod markdown_preview;` in step 1 if `typed.rs` can't see `toggle`.)

3. **Make the macro visible** — the handler uses `current_ref!`. If it doesn't
   resolve, add `use helix_view::current_ref;` at the top of
   `markdown_preview.rs` (the macro is `#[macro_export]`ed from
   `helix-view/src/macros.rs`).

## Backlog / next steps

- [ ] Debounce `DocumentDidChange` with an `AsyncHook` (mirror
      `handlers/auto_save.rs`) instead of serializing the whole rope per keystroke.
- [ ] Read the sidecar's stdout `{"kind":"ready","url":…}` line so Helix can show
      the URL in the statusline and reuse a fixed port across restarts.
- [ ] Send incremental edits (use the `ChangeSet` in `DocumentDidChange`) for big
      files instead of the full text.
- [ ] Scope the preview to a specific document id; ignore changes from others.
- [ ] Offline mode: vendor the JS/CSS into the binary instead of CDN.
- [ ] Config block in `languages.toml`/editor config (port, theme, auto-open).
- [ ] `:markdown-preview` should no-op gracefully on non-markdown buffers.
```
