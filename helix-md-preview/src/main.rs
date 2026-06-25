//! `hx-md-preview` — standalone live markdown preview sidecar.
//!
//! Protocol: Helix (or any driver) writes newline-delimited JSON to this
//! process's stdin. Each line is one message:
//!
//!   {"kind":"doc","path":"/abs/file.md","text":"# hello"}
//!   {"kind":"cursor","line":12}
//!
//! The sidecar serves a single HTML page and an SSE endpoint (`/events`).
//! Every stdin message is forwarded verbatim to all connected browsers as an
//! SSE event named after its `kind`. The most recent `doc` and `cursor` are
//! cached so a freshly opened/reloaded browser tab gets current state.
//!
//! Run standalone for development:
//!   printf '{"kind":"doc","text":"# hi\\n\\n- a\\n- b"}\n' | cargo run -p helix-md-preview
//!   # then in another terminal, feed more lines into the same stdin.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures_util::stream::{self, Stream, StreamExt};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

/// A single SSE message: `event: <kind>\ndata: <json>`.
#[derive(Clone, Debug)]
struct Wire {
    kind: String,
    data: String,
}

#[derive(Default)]
struct Last {
    doc: Option<Wire>,
    cursor: Option<Wire>,
}

struct AppState {
    tx: broadcast::Sender<Wire>,
    last: Mutex<Last>,
}

struct Args {
    port: u16,
    open: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        port: 0, // 0 => OS-assigned free port
        open: true,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => {
                if let Some(p) = it.next() {
                    args.port = p.parse().unwrap_or(0);
                }
            }
            "--no-open" => args.open = false,
            _ => {}
        }
    }
    args
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();

    let (tx, _rx) = broadcast::channel::<Wire>(256);
    let state = Arc::new(AppState {
        tx,
        last: Mutex::new(Last::default()),
    });

    // Read newline-delimited JSON from stdin and fan it out over SSE.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let mut lines = BufReader::new(stdin).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let kind = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(String::from));
                let Some(kind) = kind else { continue };

                let wire = Wire {
                    kind: kind.clone(),
                    data: line.to_string(),
                };

                {
                    let mut last = state.last.lock().unwrap();
                    match kind.as_str() {
                        "doc" => last.doc = Some(wire.clone()),
                        "cursor" => last.cursor = Some(wire.clone()),
                        _ => {}
                    }
                }
                // Ignore the "no receivers" error — browsers may not be connected yet.
                let _ = state.tx.send(wire);
            }
            // stdin closed (Helix exited) — shut the sidecar down.
            std::process::exit(0);
        });
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/events", get(sse))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port)).await?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}/");
    eprintln!("hx-md-preview: serving {url}");
    // Machine-readable line on stdout so Helix can learn the chosen port.
    println!("{{\"kind\":\"ready\",\"url\":\"{url}\"}}");

    if args.open {
        open_url(&url);
    }

    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("index.html"))
}

async fn sse(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Replay the latest doc + cursor to a newly connected tab, then go live.
    let initial: Vec<Wire> = {
        let last = state.last.lock().unwrap();
        last.doc.iter().chain(last.cursor.iter()).cloned().collect()
    };
    let live = BroadcastStream::new(state.tx.subscribe())
        .filter_map(|res| async move { res.ok() });

    let to_event = |w: Wire| Ok(Event::default().event(w.kind).data(w.data));
    let stream = stream::iter(initial).chain(live).map(to_event);

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Best-effort cross-platform "open this URL in the default browser".
fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = ("open", vec![url]);
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = ("xdg-open", vec![url]);

    let _ = std::process::Command::new(cmd.0).args(cmd.1).spawn();
}
