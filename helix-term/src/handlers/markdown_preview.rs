//! Live markdown preview driven by the `hx-md-preview` sidecar process.
//!
//! `:markdown-preview` toggles a child `hx-md-preview` process (see the
//! `helix-md-preview` crate). While active, document edits and cursor moves are
//! streamed to the sidecar as newline-delimited JSON over its stdin; the sidecar
//! serves a browser page that re-renders live and scroll-syncs to the cursor.
//!
//! NOTE: This module is additive and is not yet wired in. To enable it, apply
//! the three edits described in `helix-md-preview/INTEGRATION.md`. It has not
//! been compiled yet (the workspace currently has an in-progress upstream merge).

use std::sync::Mutex;

use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use helix_event::register_hook;
use helix_view::{
    events::{DocumentDidChange, SelectionDidChange},
    handlers::Handlers,
    Document, Editor, ViewId,
};

/// Name of the sidecar binary. Built by `cargo build -p helix-md-preview`.
const SIDECAR_BIN: &str = "hx-md-preview";

struct Preview {
    /// Lines (without trailing `\n`) destined for the sidecar's stdin.
    tx: mpsc::UnboundedSender<String>,
    /// Kept alive so dropping it (on toggle-off / exit) terminates the sidecar.
    _child: Child,
}

static PREVIEW: Mutex<Option<Preview>> = Mutex::new(None);

fn is_active() -> bool {
    PREVIEW.lock().unwrap().is_some()
}

fn send(line: String) {
    if let Some(p) = PREVIEW.lock().unwrap().as_ref() {
        let _ = p.tx.send(line);
    }
}

/// Toggle the preview for the current document. Bound to `:markdown-preview`.
pub fn toggle(editor: &mut Editor) -> Result<()> {
    {
        let mut guard = PREVIEW.lock().unwrap();
        if guard.take().is_some() {
            // Dropping `Preview` closes the stdin channel and kills the child.
            editor.set_status("Markdown preview stopped");
            return Ok(());
        }
    }

    let mut child = Command::new(SIDECAR_BIN)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start `{SIDECAR_BIN}`: {e}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("sidecar stdin unavailable"))?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if stdin.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdin.write_all(b"\n").await;
        }
    });

    *PREVIEW.lock().unwrap() = Some(Preview { tx, _child: child });

    // Push the initial state.
    let (view, doc) = current_ref!(editor);
    send(doc_message(doc));
    send(cursor_message(doc, view.id));

    editor.set_status("Markdown preview started");
    Ok(())
}

fn doc_message(doc: &Document) -> String {
    let path = doc.path().map(|p| p.to_string_lossy().into_owned());
    serde_json::json!({
        "kind": "doc",
        "path": path,
        "text": doc.text().to_string(),
    })
    .to_string()
}

fn cursor_message(doc: &Document, view: ViewId) -> String {
    let text = doc.text().slice(..);
    let pos = doc.selection(view).primary().cursor(text);
    let line = text.char_to_line(pos) + 1; // 1-based to match the renderer
    serde_json::json!({ "kind": "cursor", "line": line }).to_string()
}

pub(super) fn register_hooks(_handlers: &Handlers) {
    register_hook!(move |event: &mut DocumentDidChange<'_>| {
        if is_active() && !event.ghost_transaction {
            // TODO(perf): debounce via an AsyncHook (mirror handlers/auto_save.rs)
            // instead of serializing the whole rope on every keystroke.
            send(doc_message(event.doc));
        }
        Ok(())
    });

    register_hook!(move |event: &mut SelectionDidChange<'_>| {
        if is_active() {
            send(cursor_message(event.doc, event.view));
        }
        Ok(())
    });
}
