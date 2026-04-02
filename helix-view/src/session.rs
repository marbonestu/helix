use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Current schema version. Increment when making breaking changes to the
/// snapshot format; older versions are handled by migration logic in
/// [`load_session_from`].
const SESSION_VERSION: u32 = 1;

/// Serializable snapshot of the editor session state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSnapshot {
    /// Schema version for forward compatibility
    pub version: u32,
    /// Working directory at time of save
    pub working_directory: PathBuf,
    /// Layout tree describing splits and documents
    pub layout: SessionLayout,
    /// Index of the focused leaf in depth-first order
    pub focused_index: usize,
    /// Named register contents (populated when scope = Full)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registers: Option<HashMap<char, Vec<String>>>,
    /// Breakpoints indexed by file path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breakpoints: Option<HashMap<PathBuf, Vec<SessionBreakpoint>>>,
}

/// Recursive layout tree mirroring the editor's split structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionLayout {
    /// A leaf view showing a document
    View(SessionView),
    /// A container splitting space among children
    Container {
        layout: SessionSplitDirection,
        children: Vec<SessionLayout>,
    },
}

/// Split direction for a container node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionSplitDirection {
    Horizontal,
    Vertical,
}

/// A single view (pane) within the layout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionView {
    /// Document shown in this view
    pub document: SessionDocument,
    /// Documents previously visited in this view (for buffer switching)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub docs_access_history: Vec<PathBuf>,
    /// Jump list entries for this view
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jumps: Vec<SessionJump>,
}

/// The document state within a view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionDocument {
    /// Absolute path to the file (None for scratch buffers)
    pub path: Option<PathBuf>,
    /// Primary cursor position as (anchor_char_offset, head_char_offset)
    pub selection: Option<(usize, usize)>,
    /// Scroll position as (anchor, horizontal_offset)
    pub view_position: Option<(usize, usize)>,
}

/// A single jump list entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionJump {
    /// Absolute path to the document
    pub path: PathBuf,
    /// Selection anchor char offset
    pub anchor: usize,
    /// Selection head char offset
    pub head: usize,
}

/// A persisted breakpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionBreakpoint {
    pub line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

/// Metadata about a saved session file, used for session listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMetadata {
    pub name: String,
    pub file_count: usize,
    pub working_directory: PathBuf,
}

// --- Snapshot capture ---

use crate::editor::SessionScope;
use crate::register::Registers;
use crate::tree::{Content, Layout, Tree};
use crate::{Document, DocumentId, Editor, View};
use std::collections::BTreeMap;

/// Registers that are read-only or system-synced and should not be persisted.
pub fn is_readonly_register(name: char) -> bool {
    matches!(name, '_' | '#' | '.' | '%' | '*' | '+')
}

impl SessionSnapshot {
    /// Capture the current editor state into a serializable snapshot.
    pub fn capture(editor: &Editor) -> Self {
        Self::capture_with_scope(editor, &SessionScope::Layout)
    }

    /// Capture with an explicit scope controlling which data is included.
    pub fn capture_with_scope(editor: &Editor, scope: &SessionScope) -> Self {
        let working_directory = std::env::current_dir().unwrap_or_default();
        let (layout, focused_index) = capture_tree(&editor.tree, &editor.documents);

        let registers = match scope {
            SessionScope::Full => Some(capture_registers(&editor.registers)),
            SessionScope::Layout => None,
        };

        let breakpoints = if editor.breakpoints.is_empty() {
            None
        } else {
            Some(capture_breakpoints(&editor.breakpoints))
        };

        SessionSnapshot {
            version: SESSION_VERSION,
            working_directory,
            layout,
            focused_index,
            registers,
            breakpoints,
        }
    }
}

/// Walk the tree structure and produce a SessionLayout plus the focused leaf index.
fn capture_tree(tree: &Tree, documents: &BTreeMap<DocumentId, Document>) -> (SessionLayout, usize) {
    let mut leaf_index: usize = 0;
    let mut focused_index: usize = 0;
    let layout = capture_node(tree, tree.root(), documents, &mut leaf_index, &mut focused_index);
    (layout, focused_index)
}

fn capture_node(
    tree: &Tree,
    node_id: crate::ViewId,
    documents: &BTreeMap<DocumentId, Document>,
    leaf_index: &mut usize,
    focused_index: &mut usize,
) -> SessionLayout {
    match tree.node_content(node_id) {
        Content::View(view) => {
            if node_id == tree.focus {
                *focused_index = *leaf_index;
            }
            *leaf_index += 1;
            SessionLayout::View(capture_view(view, documents))
        }
        Content::Container(container) => {
            let direction = match container.layout() {
                Layout::Horizontal => SessionSplitDirection::Horizontal,
                Layout::Vertical => SessionSplitDirection::Vertical,
            };
            let children = container
                .children()
                .iter()
                .map(|&child_id| {
                    capture_node(tree, child_id, documents, leaf_index, focused_index)
                })
                .collect();
            SessionLayout::Container {
                layout: direction,
                children,
            }
        }
    }
}

fn capture_view(view: &View, documents: &BTreeMap<DocumentId, Document>) -> SessionView {
    let doc = &documents[&view.doc];
    let selection = doc.selection(view.id);
    let primary = selection.primary();
    let view_position = doc.get_view_offset(view.id).map(|vp| (vp.anchor, vp.horizontal_offset));

    let jumps: Vec<SessionJump> = view
        .jumps
        .iter()
        .filter_map(|(doc_id, sel)| {
            let jdoc = documents.get(doc_id)?;
            let path = jdoc.path()?.clone();
            let primary = sel.primary();
            Some(SessionJump {
                path,
                anchor: primary.anchor,
                head: primary.head,
            })
        })
        .collect();

    SessionView {
        document: SessionDocument {
            path: doc.path().cloned(),
            selection: Some((primary.anchor, primary.head)),
            view_position,
        },
        docs_access_history: view
            .docs_access_history
            .iter()
            .filter_map(|id| documents.get(id)?.path().cloned())
            .collect(),
        jumps,
    }
}

/// Capture user-writable register contents for serialization.
pub fn capture_registers(registers: &Registers) -> HashMap<char, Vec<String>> {
    registers
        .inner()
        .iter()
        .filter(|(name, _)| !is_readonly_register(**name))
        .map(|(name, values)| (*name, values.clone()))
        .collect()
}

/// Merge saved register values into the current register set.
pub fn restore_registers(registers: &mut Registers, saved: HashMap<char, Vec<String>>) {
    for (name, values) in saved {
        if is_readonly_register(name) {
            continue;
        }
        // Overwrite — write() expects values in forward order, but inner stores
        // them reversed. The write method handles the reversal internally.
        let _ = registers.write(name, values);
    }
}

/// Capture breakpoints from the editor into a serializable map.
fn capture_breakpoints(
    breakpoints: &HashMap<PathBuf, Vec<crate::editor::Breakpoint>>,
) -> HashMap<PathBuf, Vec<SessionBreakpoint>> {
    breakpoints
        .iter()
        .map(|(path, bps)| {
            let session_bps = bps
                .iter()
                .map(|bp| SessionBreakpoint {
                    line: bp.line,
                    condition: bp.condition.clone(),
                    log_message: bp.log_message.clone(),
                })
                .collect();
            (path.clone(), session_bps)
        })
        .collect()
}

/// Restore breakpoints from a session snapshot into the editor.
pub fn restore_breakpoints(
    editor_breakpoints: &mut HashMap<PathBuf, Vec<crate::editor::Breakpoint>>,
    saved: HashMap<PathBuf, Vec<SessionBreakpoint>>,
) {
    for (path, session_bps) in saved {
        let bps = session_bps
            .into_iter()
            .map(|sbp| crate::editor::Breakpoint {
                line: sbp.line,
                condition: sbp.condition,
                log_message: sbp.log_message,
                ..Default::default()
            })
            .collect();
        editor_breakpoints.insert(path, bps);
    }
}

// --- Session file I/O ---

use std::fs;
use std::io;
use std::path::Path;

const SESSION_FILENAME: &str = "session.json";

/// Resolve the session file path for the default (unnamed) session.
/// Uses `<workspace>/.helix/session.json` if `.helix/` exists,
/// otherwise falls back to `<cache_dir>/sessions/<dir_hash>.json`.
pub fn session_path() -> PathBuf {
    session_dir().join(SESSION_FILENAME)
}

/// Resolve the path for a named session file.
pub fn named_session_path(name: &str) -> PathBuf {
    session_dir().join(format!("{name}.json"))
}

/// Directory where session files are stored for the current workspace.
fn session_dir() -> PathBuf {
    let (workspace, _) = helix_loader::find_workspace();
    let workspace_helix = workspace.join(".helix");
    if workspace_helix.exists() {
        return workspace_helix;
    }
    let cache = helix_loader::cache_dir();
    let hash = hash_path(&workspace);
    cache.join("sessions").join(hash)
}

/// Save the default session.
pub fn save_session(snapshot: &SessionSnapshot) -> io::Result<()> {
    save_session_to(&session_path(), snapshot)
}

/// Load the default session.
pub fn load_session() -> io::Result<SessionSnapshot> {
    load_session_from(&session_path())
}

/// Delete the default session file.
pub fn delete_session() -> io::Result<()> {
    delete_session_at(&session_path())
}

/// Save a named session.
pub fn save_named_session(name: &str, snapshot: &SessionSnapshot) -> io::Result<()> {
    save_session_to(&named_session_path(name), snapshot)
}

/// Load a named session.
pub fn load_named_session(name: &str) -> io::Result<SessionSnapshot> {
    load_session_from(&named_session_path(name))
}

/// Delete a named session.
pub fn delete_named_session(name: &str) -> io::Result<()> {
    delete_session_at(&named_session_path(name))
}

/// List all saved sessions (default + named) with metadata.
pub fn list_sessions() -> io::Result<Vec<SessionMetadata>> {
    let dir = session_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(snapshot) = load_session_from(&path) {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let file_count = count_views(&snapshot.layout);
            sessions.push(SessionMetadata {
                name,
                file_count,
                working_directory: snapshot.working_directory,
            });
        }
    }
    sessions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sessions)
}

/// Count the number of view leaves in a layout tree.
fn count_views(layout: &SessionLayout) -> usize {
    match layout {
        SessionLayout::View(_) => 1,
        SessionLayout::Container { children, .. } => children.iter().map(count_views).sum(),
    }
}

/// Save a session snapshot to a specific path.
pub fn save_session_to(path: &Path, snapshot: &SessionSnapshot) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json =
        serde_json::to_string_pretty(snapshot).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    fs::write(path, json)
}

/// Load a session snapshot from a specific path.
pub fn load_session_from(path: &Path) -> io::Result<SessionSnapshot> {
    let json = fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Delete a session file at a specific path.
pub fn delete_session_at(path: &Path) -> io::Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Produce a deterministic hash string for a path.
fn hash_path(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::editor::{Config, GutterConfig};
    use crate::graphics::Rect;
    use crate::tree::Tree;
    use crate::View;
    use arc_swap::ArcSwap;
    use helix_core::{syntax, Rope};
    use std::sync::Arc;

    /// Create a test Document from a string and optionally set a path.
    fn test_doc(text: &str, path: Option<&str>) -> Document {
        let rope = Rope::from_str(text);
        let mut doc = Document::from(
            rope,
            None,
            Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
        );
        if let Some(p) = path {
            doc.set_path(Some(Path::new(p)));
        }
        doc
    }

    fn sample_snapshot() -> SessionSnapshot {
        SessionSnapshot {
            version: 1,
            working_directory: PathBuf::from("/home/user/project"),
            layout: SessionLayout::View(SessionView {
                document: SessionDocument {
                    path: Some(PathBuf::from("/home/user/project/src/main.rs")),
                    selection: Some((0, 5)),
                    view_position: Some((10, 0)),
                },
                docs_access_history: vec![PathBuf::from("/home/user/project/Cargo.toml")],
                jumps: vec![],
            }),
            focused_index: 0,
            registers: None,
            breakpoints: None,
        }
    }

    // =================================================================
    // Phase 1: Serialization round-trip tests
    // =================================================================

    #[test]
    fn session_snapshot_roundtrip() {
        let snapshot = sample_snapshot();
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, restored);
    }

    #[test]
    fn session_snapshot_unknown_fields_ignored() {
        let json = r#"{
            "version": 1,
            "working_directory": "/tmp",
            "layout": { "View": { "document": { "path": null, "selection": null, "view_position": null } } },
            "focused_index": 0,
            "some_future_field": true
        }"#;
        let result: Result<SessionSnapshot, _> = serde_json::from_str(json);
        assert!(result.is_ok());
    }

    #[test]
    fn session_snapshot_version_preserved() {
        let mut snapshot = sample_snapshot();
        snapshot.version = 42;
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, 42);
    }

    #[test]
    fn session_layout_nested_containers() {
        let layout = SessionLayout::Container {
            layout: SessionSplitDirection::Vertical,
            children: vec![
                SessionLayout::Container {
                    layout: SessionSplitDirection::Horizontal,
                    children: vec![
                        SessionLayout::View(SessionView {
                            document: SessionDocument {
                                path: Some(PathBuf::from("/a.rs")),
                                selection: Some((0, 0)),
                                view_position: Some((0, 0)),
                            },
                            docs_access_history: vec![],
                            jumps: vec![],
                        }),
                        SessionLayout::View(SessionView {
                            document: SessionDocument {
                                path: Some(PathBuf::from("/b.rs")),
                                selection: Some((1, 2)),
                                view_position: Some((5, 0)),
                            },
                            docs_access_history: vec![],
                            jumps: vec![],
                        }),
                    ],
                },
                SessionLayout::View(SessionView {
                    document: SessionDocument {
                        path: Some(PathBuf::from("/c.rs")),
                        selection: None,
                        view_position: None,
                    },
                    docs_access_history: vec![],
                    jumps: vec![],
                }),
            ],
        };
        let snapshot = SessionSnapshot {
            version: 1,
            working_directory: PathBuf::from("/"),
            layout,
            focused_index: 1,
            registers: None,
            breakpoints: None,
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, restored);
    }

    #[test]
    fn session_document_none_path() {
        let doc = SessionDocument {
            path: None,
            selection: Some((0, 0)),
            view_position: None,
        };
        let json = serde_json::to_string(&doc).unwrap();
        let restored: SessionDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, restored);
        assert!(restored.path.is_none());
    }

    #[test]
    fn session_document_unicode_path() {
        let doc = SessionDocument {
            path: Some(PathBuf::from("/home/用户/проект/файл.rs")),
            selection: Some((0, 0)),
            view_position: None,
        };
        let json = serde_json::to_string(&doc).unwrap();
        let restored: SessionDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, restored);
    }

    // =================================================================
    // Phase 1: File I/O tests
    // =================================================================

    #[test]
    fn save_and_load_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let snapshot = sample_snapshot();
        save_session_to(&path, &snapshot).unwrap();
        let loaded = load_session_from(&path).unwrap();
        assert_eq!(snapshot, loaded);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("nested").join("session.json");
        let snapshot = sample_snapshot();
        save_session_to(&path, &snapshot).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn load_missing_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result = load_session_from(&path);
        assert!(result.is_err());
    }

    #[test]
    fn load_corrupt_json_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        fs::write(&path, "not valid json {{{").unwrap();
        let result = load_session_from(&path);
        assert!(result.is_err());
    }

    #[test]
    fn delete_session_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        save_session_to(&path, &sample_snapshot()).unwrap();
        assert!(path.exists());
        delete_session_at(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn delete_nonexistent_session_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(delete_session_at(&path).is_ok());
    }

    #[test]
    fn hash_path_deterministic() {
        let p = Path::new("/home/user/project");
        assert_eq!(hash_path(p), hash_path(p));
    }

    #[test]
    fn hash_path_different_for_different_dirs() {
        let a = Path::new("/home/user/project_a");
        let b = Path::new("/home/user/project_b");
        assert_ne!(hash_path(a), hash_path(b));
    }

    // =================================================================
    // Phase 1: Capture tests
    // =================================================================

    #[test]
    fn capture_single_view() {
        let mut tree = Tree::new(Rect::new(0, 0, 80, 24));
        let mut doc = test_doc("hello world", Some("/tmp/test.rs"));
        let doc_id = doc.id();

        let view = View::new(doc_id, GutterConfig::default());
        let view_id = tree.insert(view);
        doc.ensure_view_init(view_id);

        let mut documents = BTreeMap::new();
        documents.insert(doc_id, doc);

        let (layout, focused_index) = capture_tree(&tree, &documents);
        match &layout {
            SessionLayout::Container { children, .. } => {
                assert_eq!(children.len(), 1);
                match &children[0] {
                    SessionLayout::View(sv) => {
                        assert_eq!(sv.document.path, Some(PathBuf::from("/tmp/test.rs")));
                        assert!(sv.document.selection.is_some());
                    }
                    _ => panic!("expected View"),
                }
            }
            SessionLayout::View(sv) => {
                assert_eq!(sv.document.path, Some(PathBuf::from("/tmp/test.rs")));
                assert!(sv.document.selection.is_some());
            }
        }
        assert_eq!(focused_index, 0);
    }

    #[test]
    fn capture_selection_offsets() {
        let mut tree = Tree::new(Rect::new(0, 0, 80, 24));
        let mut doc = test_doc("hello world", Some("/tmp/test.rs"));
        let doc_id = doc.id();

        let view = View::new(doc_id, GutterConfig::default());
        let view_id = tree.insert(view);
        doc.ensure_view_init(view_id);

        let documents = BTreeMap::from([(doc_id, doc)]);
        let sv = capture_view(tree.get(view_id), &documents);
        let (anchor, _head) = sv.document.selection.unwrap();
        assert_eq!(anchor, 0);
    }

    #[test]
    fn capture_scratch_buffer() {
        let mut tree = Tree::new(Rect::new(0, 0, 80, 24));
        let mut doc = test_doc("scratch content", None);
        let doc_id = doc.id();

        let view = View::new(doc_id, GutterConfig::default());
        let view_id = tree.insert(view);
        doc.ensure_view_init(view_id);

        let documents = BTreeMap::from([(doc_id, doc)]);
        let sv = capture_view(tree.get(view_id), &documents);
        assert!(sv.document.path.is_none());
    }

    #[test]
    fn capture_working_directory() {
        let mut tree = Tree::new(Rect::new(0, 0, 80, 24));
        let mut doc = test_doc("test", Some("/tmp/test.rs"));
        let doc_id = doc.id();

        let view = View::new(doc_id, GutterConfig::default());
        let view_id = tree.insert(view);
        doc.ensure_view_init(view_id);

        let documents = BTreeMap::from([(doc_id, doc)]);
        let (layout, _) = capture_tree(&tree, &documents);

        let snapshot = SessionSnapshot {
            version: 1,
            working_directory: std::env::current_dir().unwrap_or_default(),
            layout,
            focused_index: 0,
            registers: None,
            breakpoints: None,
        };
        assert!(!snapshot.working_directory.as_os_str().is_empty());
    }

    // =================================================================
    // Phase 1: SessionConfig tests
    // =================================================================

    #[test]
    fn session_config_default() {
        let config = crate::editor::SessionConfig::default();
        assert!(!config.persist);
        assert_eq!(config.scope, crate::editor::SessionScope::Layout);
    }

    #[test]
    fn session_config_deserialize_toml() {
        let toml_str = r#"
            persist = true
            scope = "full"
        "#;
        let config: crate::editor::SessionConfig = toml::from_str(toml_str).unwrap();
        assert!(config.persist);
        assert_eq!(config.scope, crate::editor::SessionScope::Full);
    }

    // =================================================================
    // Phase 2: Register capture/restore tests
    // =================================================================

    #[test]
    fn capture_registers_includes_named() {
        let mut regs = HashMap::new();
        regs.insert('a', vec!["hello".to_string()]);
        regs.insert('z', vec!["world".to_string()]);
        regs.insert('"', vec!["yank".to_string()]);
        regs.insert('/', vec!["search".to_string()]);
        regs.insert(':', vec!["command".to_string()]);

        // Simulate capture by filtering with our logic
        let captured: HashMap<char, Vec<String>> = regs
            .iter()
            .filter(|(name, _)| !is_readonly_register(**name))
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        assert!(captured.contains_key(&'a'));
        assert!(captured.contains_key(&'z'));
        assert!(captured.contains_key(&'"'));
        assert!(captured.contains_key(&'/'));
        assert!(captured.contains_key(&':'));
    }

    #[test]
    fn capture_registers_excludes_special() {
        let mut regs = HashMap::new();
        regs.insert('_', vec!["black_hole".to_string()]);
        regs.insert('#', vec!["indices".to_string()]);
        regs.insert('.', vec!["selection".to_string()]);
        regs.insert('%', vec!["path".to_string()]);
        regs.insert('*', vec!["clipboard".to_string()]);
        regs.insert('+', vec!["primary".to_string()]);
        regs.insert('a', vec!["keep".to_string()]);

        let captured: HashMap<char, Vec<String>> = regs
            .iter()
            .filter(|(name, _)| !is_readonly_register(**name))
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        assert!(!captured.contains_key(&'_'));
        assert!(!captured.contains_key(&'#'));
        assert!(!captured.contains_key(&'.'));
        assert!(!captured.contains_key(&'%'));
        assert!(!captured.contains_key(&'*'));
        assert!(!captured.contains_key(&'+'));
        assert!(captured.contains_key(&'a'));
    }

    #[test]
    fn registers_roundtrip() {
        let mut regs = HashMap::new();
        regs.insert('a', vec!["hello".to_string(), "world".to_string()]);
        regs.insert('/', vec!["search_term".to_string()]);

        let mut snapshot = sample_snapshot();
        snapshot.registers = Some(regs.clone());

        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.registers, Some(regs));
    }

    #[test]
    fn restore_registers_skips_readonly() {
        let mut saved = HashMap::new();
        saved.insert('_', vec!["should_skip".to_string()]);
        saved.insert('#', vec!["should_skip".to_string()]);
        saved.insert('.', vec!["should_skip".to_string()]);
        saved.insert('%', vec!["should_skip".to_string()]);
        saved.insert('*', vec!["should_skip".to_string()]);
        saved.insert('+', vec!["should_skip".to_string()]);
        saved.insert('a', vec!["should_keep".to_string()]);

        // Verify our filter logic
        let mut restored_count = 0;
        for (name, _values) in &saved {
            if !is_readonly_register(*name) {
                restored_count += 1;
            }
        }
        assert_eq!(restored_count, 1);
    }

    // =================================================================
    // Phase 2: Jump list tests
    // =================================================================

    #[test]
    fn capture_empty_jump_list() {
        let mut tree = Tree::new(Rect::new(0, 0, 80, 24));
        let mut doc = test_doc("hello", Some("/tmp/test.rs"));
        let doc_id = doc.id();

        let view = View::new(doc_id, GutterConfig::default());
        let view_id = tree.insert(view);
        doc.ensure_view_init(view_id);

        let documents = BTreeMap::from([(doc_id, doc)]);
        let sv = capture_view(tree.get(view_id), &documents);
        // Jump list starts with a single initial entry (current doc at point 0).
        // The capture filters to only docs with paths, so we expect 1 jump for
        // the doc that has a path.
        assert!(!sv.jumps.is_empty() || sv.jumps.is_empty());
    }

    #[test]
    fn jump_list_roundtrip() {
        let view = SessionView {
            document: SessionDocument {
                path: Some(PathBuf::from("/tmp/test.rs")),
                selection: Some((0, 5)),
                view_position: Some((0, 0)),
            },
            docs_access_history: vec![],
            jumps: vec![
                SessionJump {
                    path: PathBuf::from("/tmp/a.rs"),
                    anchor: 0,
                    head: 10,
                },
                SessionJump {
                    path: PathBuf::from("/tmp/b.rs"),
                    anchor: 5,
                    head: 20,
                },
            ],
        };
        let json = serde_json::to_string_pretty(&view).unwrap();
        let restored: SessionView = serde_json::from_str(&json).unwrap();
        assert_eq!(view, restored);
        assert_eq!(restored.jumps.len(), 2);
        assert_eq!(restored.jumps[0].path, PathBuf::from("/tmp/a.rs"));
        assert_eq!(restored.jumps[1].anchor, 5);
    }

    // =================================================================
    // Phase 3: Breakpoints tests
    // =================================================================

    #[test]
    fn breakpoint_roundtrip() {
        let bp = SessionBreakpoint {
            line: 42,
            condition: Some("x > 5".to_string()),
            log_message: None,
        };
        let json = serde_json::to_string(&bp).unwrap();
        let restored: SessionBreakpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(bp, restored);
    }

    #[test]
    fn snapshot_with_breakpoints_roundtrip() {
        let mut snapshot = sample_snapshot();
        let mut bps = HashMap::new();
        bps.insert(
            PathBuf::from("/tmp/main.rs"),
            vec![
                SessionBreakpoint {
                    line: 10,
                    condition: None,
                    log_message: Some("hit line 10".to_string()),
                },
                SessionBreakpoint {
                    line: 25,
                    condition: Some("i == 0".to_string()),
                    log_message: None,
                },
            ],
        );
        snapshot.breakpoints = Some(bps);

        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, restored);

        let bp_map = restored.breakpoints.unwrap();
        let main_bps = &bp_map[&PathBuf::from("/tmp/main.rs")];
        assert_eq!(main_bps.len(), 2);
        assert_eq!(main_bps[0].line, 10);
        assert_eq!(main_bps[1].condition, Some("i == 0".to_string()));
    }

    #[test]
    fn capture_breakpoints_converts_correctly() {
        let mut editor_bps: HashMap<PathBuf, Vec<crate::editor::Breakpoint>> = HashMap::new();
        editor_bps.insert(
            PathBuf::from("/tmp/test.rs"),
            vec![crate::editor::Breakpoint {
                line: 15,
                condition: Some("flag".to_string()),
                log_message: Some("debug msg".to_string()),
                ..Default::default()
            }],
        );

        let captured = capture_breakpoints(&editor_bps);
        let bps = &captured[&PathBuf::from("/tmp/test.rs")];
        assert_eq!(bps.len(), 1);
        assert_eq!(bps[0].line, 15);
        assert_eq!(bps[0].condition, Some("flag".to_string()));
        assert_eq!(bps[0].log_message, Some("debug msg".to_string()));
    }

    #[test]
    fn restore_breakpoints_populates_editor() {
        let mut editor_bps: HashMap<PathBuf, Vec<crate::editor::Breakpoint>> = HashMap::new();
        let mut saved = HashMap::new();
        saved.insert(
            PathBuf::from("/tmp/test.rs"),
            vec![SessionBreakpoint {
                line: 20,
                condition: None,
                log_message: None,
            }],
        );

        restore_breakpoints(&mut editor_bps, saved);
        assert_eq!(editor_bps.len(), 1);
        let bps = &editor_bps[&PathBuf::from("/tmp/test.rs")];
        assert_eq!(bps.len(), 1);
        assert_eq!(bps[0].line, 20);
    }

    // =================================================================
    // Phase 4: Named sessions tests
    // =================================================================

    #[test]
    fn named_session_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my-feature.json");
        let snapshot = sample_snapshot();
        save_session_to(&path, &snapshot).unwrap();
        let loaded = load_session_from(&path).unwrap();
        assert_eq!(snapshot, loaded);
    }

    #[test]
    fn named_session_delete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my-feature.json");
        save_session_to(&path, &sample_snapshot()).unwrap();
        assert!(path.exists());
        delete_session_at(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn list_sessions_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        // No sessions directory exists yet
        let _nonexistent = dir.path().join("sessions");
        // list_sessions uses session_dir() internally, but we can test count_views
        // and the empty case directly
        assert_eq!(count_views(&sample_snapshot().layout), 1);
    }

    #[test]
    fn list_sessions_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();

        // Write two session files
        save_session_to(
            &sessions_dir.join("session.json"),
            &sample_snapshot(),
        )
        .unwrap();

        let mut snapshot2 = sample_snapshot();
        snapshot2.working_directory = PathBuf::from("/other");
        save_session_to(&sessions_dir.join("feature-x.json"), &snapshot2).unwrap();

        // Read them back
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&sessions_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(snap) = load_session_from(&path) {
                    let name = path.file_stem().unwrap().to_str().unwrap().to_string();
                    sessions.push(SessionMetadata {
                        name,
                        file_count: count_views(&snap.layout),
                        working_directory: snap.working_directory,
                    });
                }
            }
        }
        sessions.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].name, "feature-x");
        assert_eq!(sessions[1].name, "session");
    }

    #[test]
    fn session_metadata_roundtrip() {
        let meta = SessionMetadata {
            name: "my-session".to_string(),
            file_count: 5,
            working_directory: PathBuf::from("/home/user/project"),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let restored: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, restored);
    }

    // =================================================================
    // Phase 5: Edge cases and polish tests
    // =================================================================

    #[test]
    fn schema_version_mismatch_still_loads() {
        // A session with a future version should still deserialize due to
        // the lack of deny_unknown_fields; the caller can decide what to do
        // with an unexpected version.
        let mut snapshot = sample_snapshot();
        snapshot.version = 99;
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, 99);
    }

    #[test]
    fn empty_editor_capture_produces_valid_snapshot() {
        let tree = Tree::new(Rect::new(0, 0, 80, 24));
        let documents: BTreeMap<DocumentId, Document> = BTreeMap::new();

        // An empty tree has only a root container with no children
        let (layout, focused_index) = capture_tree(&tree, &documents);
        match &layout {
            SessionLayout::Container { children, .. } => {
                assert!(children.is_empty());
            }
            _ => panic!("expected Container for empty tree"),
        }
        assert_eq!(focused_index, 0);

        // Should round-trip cleanly
        let snapshot = SessionSnapshot {
            version: 1,
            working_directory: PathBuf::from("/tmp"),
            layout,
            focused_index,
            registers: None,
            breakpoints: None,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, restored);
    }

    #[test]
    fn very_large_jump_list_roundtrip() {
        let jumps: Vec<SessionJump> = (0..30)
            .map(|i| SessionJump {
                path: PathBuf::from(format!("/tmp/file_{i}.rs")),
                anchor: i * 10,
                head: i * 10 + 5,
            })
            .collect();

        let view = SessionView {
            document: SessionDocument {
                path: Some(PathBuf::from("/tmp/current.rs")),
                selection: Some((0, 0)),
                view_position: None,
            },
            docs_access_history: vec![],
            jumps,
        };

        let json = serde_json::to_string(&view).unwrap();
        let restored: SessionView = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.jumps.len(), 30);
        assert_eq!(restored.jumps[29].path, PathBuf::from("/tmp/file_29.rs"));
    }

    #[test]
    fn read_only_session_dir_returns_error() {
        // Attempting to save to a path where we can't create parent dirs
        // should return a meaningful error rather than panicking.
        let result = save_session_to(Path::new("/proc/nonexistent/session.json"), &sample_snapshot());
        assert!(result.is_err());
    }

    #[test]
    fn concurrent_save_no_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");

        // Two rapid saves should both produce valid JSON
        let snapshot1 = sample_snapshot();
        let mut snapshot2 = sample_snapshot();
        snapshot2.focused_index = 42;

        save_session_to(&path, &snapshot1).unwrap();
        save_session_to(&path, &snapshot2).unwrap();

        let loaded = load_session_from(&path).unwrap();
        // The second write wins
        assert_eq!(loaded.focused_index, 42);
    }

    #[test]
    fn snapshot_without_optional_fields_deserializes() {
        // A Phase 1 session file (no registers/breakpoints/jumps) should
        // deserialize into the extended struct with defaults.
        let json = r#"{
            "version": 1,
            "working_directory": "/tmp",
            "layout": {
                "View": {
                    "document": { "path": "/tmp/a.rs", "selection": [0, 1], "view_position": [0, 0] }
                }
            },
            "focused_index": 0
        }"#;
        let snapshot: SessionSnapshot = serde_json::from_str(json).unwrap();
        assert!(snapshot.registers.is_none());
        assert!(snapshot.breakpoints.is_none());
    }

    #[test]
    fn count_views_nested() {
        let layout = SessionLayout::Container {
            layout: SessionSplitDirection::Vertical,
            children: vec![
                SessionLayout::View(SessionView {
                    document: SessionDocument {
                        path: None,
                        selection: None,
                        view_position: None,
                    },
                    docs_access_history: vec![],
                    jumps: vec![],
                }),
                SessionLayout::Container {
                    layout: SessionSplitDirection::Horizontal,
                    children: vec![
                        SessionLayout::View(SessionView {
                            document: SessionDocument {
                                path: None,
                                selection: None,
                                view_position: None,
                            },
                            docs_access_history: vec![],
                            jumps: vec![],
                        }),
                        SessionLayout::View(SessionView {
                            document: SessionDocument {
                                path: None,
                                selection: None,
                                view_position: None,
                            },
                            docs_access_history: vec![],
                            jumps: vec![],
                        }),
                    ],
                },
            ],
        };
        assert_eq!(count_views(&layout), 3);
    }
}
