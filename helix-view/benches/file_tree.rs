use arc_swap::ArcSwap;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use helix_view::file_tree::{FileTree, FileTreeConfig, FileTreeUpdate, GitStatus, NodeId, NodeKind};
use std::sync::Arc;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal config: no git status, no follow, no watcher noise.
fn bench_config() -> FileTreeConfig {
    FileTreeConfig {
        git_status: false,
        follow_current_file: false,
        git_ignore: false,
        hidden: true,
        ..FileTreeConfig::default()
    }
}

/// Build a flat tree: root/ with `dirs` subdirectories each containing
/// `files` files, with all top-level dirs expanded.
fn make_flat_tree(dirs: usize, files: usize) -> (TempDir, FileTree, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let tmp = TempDir::new().unwrap();
    for d in 0..dirs {
        let dir = tmp.path().join(format!("dir_{d:04}"));
        std::fs::create_dir(&dir).unwrap();
        for f in 0..files {
            std::fs::write(dir.join(format!("file_{f:04}.rs")), "").unwrap();
        }
    }

    let config = bench_config();
    let mut tree = FileTree::new(tmp.path().to_path_buf(), &config).unwrap();

    // Expand all top-level directories.
    let dirs_to_expand: Vec<NodeId> = tree
        .nodes()
        .iter()
        .filter(|(_, n)| n.parent == Some(tree.root_id()) && n.kind == NodeKind::Directory)
        .map(|(id, _)| id)
        .collect();
    for id in dirs_to_expand {
        tree.expand_sync(id, &config);
    }

    (tmp, tree, rt)
}

/// Build a deep single-chain tree: root/ nested_0/ nested_1/ … leaf.rs
fn make_deep_tree(depth: usize) -> (TempDir, FileTree, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let tmp = TempDir::new().unwrap();
    let mut cur = tmp.path().to_path_buf();
    for d in 0..depth {
        cur = cur.join(format!("nested_{d}"));
        std::fs::create_dir(&cur).unwrap();
    }
    std::fs::write(cur.join("leaf.rs"), "").unwrap();

    let config = bench_config();
    let mut tree = FileTree::new(tmp.path().to_path_buf(), &config).unwrap();

    // Expand the entire chain so leaf is reachable.
    expand_all_sync(&mut tree, &config);

    (tmp, tree, rt)
}

/// Expand all directories depth-first, loading children as we go.
fn expand_all_sync(tree: &mut FileTree, config: &FileTreeConfig) {
    let mut stack = vec![tree.root_id()];
    while let Some(id) = stack.pop() {
        tree.expand_sync(id, config);
        // Collect newly loaded children for further expansion.
        let children: Vec<NodeId> = tree
            .nodes()
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for child in children {
            if tree.nodes().get(child).map(|n| n.kind == NodeKind::Directory).unwrap_or(false) {
                stack.push(child);
            }
        }
    }
}

/// Find the leaf file id in a tree.
fn find_leaf(tree: &FileTree) -> NodeId {
    tree.nodes()
        .iter()
        .find(|(_, n)| n.kind == NodeKind::File)
        .map(|(id, _)| id)
        .expect("no leaf file found")
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// `node_path(id)` — the critical first step when opening a file.
/// Shows the O(depth) parent-pointer walk + PathBuf allocation cost.
fn bench_node_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_path");

    for depth in [1usize, 3, 5, 8, 12] {
        let (_tmp, tree, _rt) = make_deep_tree(depth);
        let leaf_id = find_leaf(&tree);

        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| std::hint::black_box(tree.node_path(leaf_id)))
        });
    }

    group.finish();
}

/// `rebuild_visible()` — called on every expand/collapse/follow/reveal.
/// Parameterized by total visible node count.
fn bench_rebuild_visible(c: &mut Criterion) {
    let mut group = c.benchmark_group("rebuild_visible");

    for (dirs, files) in [(10usize, 10usize), (50, 20), (100, 50), (200, 50)] {
        let visible_count = 1 + dirs + dirs * files; // root + dirs + files
        let (_tmp, mut tree, _rt) = make_flat_tree(dirs, files);

        group.bench_with_input(
            BenchmarkId::new("visible", visible_count),
            &visible_count,
            |b, _| b.iter(|| tree.force_rebuild_visible()),
        );
    }

    group.finish();
}

/// `reveal_path()` — used by follow-current-file (~100ms debounce) and C-e.
/// Measures reveal of a deeply nested file whose ancestors are already expanded.
fn bench_reveal_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("reveal_path");
    let config = bench_config();

    for depth in [2usize, 4, 6, 10] {
        let (_tmp, mut tree, _rt) = make_deep_tree(depth);
        let leaf_id = find_leaf(&tree);
        let leaf_path = tree.node_path(leaf_id);

        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, _| {
            b.iter(|| tree.reveal_path(&leaf_path, &config))
        });
    }

    group.finish();
}

/// First-time directory load via `expand_sync` — the cost of the user opening
/// a never-before-expanded folder. Measures disk I/O + node insertion +
/// visible rebuild in one shot.
fn bench_expand_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("expand_sync");
    let config = bench_config();

    for file_count in [50usize, 200, 500, 1000] {
        // Create a root with a single unloaded directory containing `file_count` files.
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Re-create a fresh tree for each benchmark so the directory is
        // always unloaded at the start of every iteration.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("target_dir");
        std::fs::create_dir(&dir).unwrap();
        for f in 0..file_count {
            std::fs::write(dir.join(format!("file_{f:04}.rs")), "").unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("files", file_count),
            &file_count,
            |b, _| {
                let _guard = rt.enter();
                b.iter_batched(
                    || {
                        // Setup: fresh tree each iteration so target_dir is unloaded.
                        let mut tree =
                            FileTree::new(tmp.path().to_path_buf(), &config).unwrap();
                        let dir_id = tree
                            .nodes()
                            .iter()
                            .find(|(_, n)| n.kind == NodeKind::Directory && n.parent == Some(tree.root_id()))
                            .map(|(id, _)| id)
                            .expect("target dir not found");
                        (tree, dir_id)
                    },
                    |(mut tree, dir_id)| {
                        // Measurement: expand the unloaded directory.
                        tree.expand_sync(dir_id, &config);
                        std::hint::black_box(tree)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Directory rescan via `ChildrenLoaded` — triggered by the filesystem watcher
/// on every external change. Measures old-state snapshot, node replacement,
/// and the resulting visible rebuild.
fn bench_children_loaded_rescan(c: &mut Criterion) {
    let mut group = c.benchmark_group("children_loaded_rescan");
    let config = bench_config();

    for file_count in [50usize, 200, 500] {
        let (_tmp, mut tree, _rt) = make_flat_tree(1, file_count);

        let dir_id = tree
            .nodes()
            .iter()
            .find(|(_, n)| n.kind == NodeKind::Directory && n.parent == Some(tree.root_id()))
            .map(|(id, _)| id)
            .expect("dir not found");

        // Simulate what the background scan would send back.
        let entries: Vec<(String, NodeKind)> = (0..file_count)
            .map(|f| (format!("file_{f:04}.rs"), NodeKind::File))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("files", file_count),
            &file_count,
            |b, _| {
                b.iter(|| {
                    tree.update_tx()
                        .try_send(FileTreeUpdate::ChildrenLoaded {
                            parent: dir_id,
                            entries: entries.clone(),
                        })
                        .unwrap();
                    tree.process_updates(&config, None);
                })
            },
        );
    }

    group.finish();
}

/// Inject synthetic git status into an already-built tree via the update channel.
/// Marks every `nth` file as Modified so roughly `1/nth` of files have dirty status.
fn inject_git_status(tree: &mut FileTree, config: &FileTreeConfig, nth: usize) {
    let statuses: Vec<(std::path::PathBuf, GitStatus)> = tree
        .nodes()
        .iter()
        .filter(|(_, n)| n.kind == NodeKind::File)
        .enumerate()
        .filter(|(i, _)| i % nth == 0)
        .map(|(_, (id, _))| (tree.node_path(id), GitStatus::Modified))
        .collect();

    tree.update_tx()
        .try_send(FileTreeUpdate::GitStatusBegin)
        .unwrap();
    tree.update_tx()
        .try_send(FileTreeUpdate::GitStatus(statuses))
        .unwrap();
    tree.process_updates(config, None);
}

/// `Document::open` — the actual file-load triggered after `node_path`.
/// Measures disk read + rope construction + language detection end-to-end,
/// parameterized by file size (lines of Rust source).
fn bench_document_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_open");

    // Build a loader once — it's expensive to construct but identical across iterations.
    let lang_config = helix_loader::config::default_lang_config();
    let syn_loader = Arc::new(ArcSwap::from_pointee(
        helix_core::syntax::Loader::new(lang_config.try_into().unwrap()).unwrap(),
    ));
    let config: Arc<ArcSwap<helix_view::editor::Config>> =
        Arc::new(ArcSwap::new(Arc::new(helix_view::editor::Config::default())));

    for line_count in [100usize, 1_000, 5_000, 20_000] {
        // Write a synthetic Rust source file of the target size.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bench.rs");
        let content: String = (0..line_count)
            .map(|i| format!("fn func_{i}() {{ let x = {i}; }}\n"))
            .collect();
        std::fs::write(&path, &content).unwrap();

        group.bench_with_input(
            BenchmarkId::new("lines", line_count),
            &line_count,
            |b, _| {
                b.iter(|| {
                    std::hint::black_box(
                        helix_view::document::Document::open(
                            &path,
                            None,
                            true,
                            config.clone(),
                            syn_loader.clone(),
                        )
                        .unwrap(),
                    )
                })
            },
        );
    }

    group.finish();
}

/// Navigation: `move_up` / `move_down` in isolation.
///
/// These are O(1) index arithmetic — this benchmark confirms the low constant
/// cost and catches regressions if the impl ever changes.
fn bench_navigate(c: &mut Criterion) {
    let mut group = c.benchmark_group("navigate");

    for (dirs, files) in [(10usize, 10usize), (50, 20), (200, 50)] {
        let visible_count = 1 + dirs + dirs * files;
        let (_tmp, mut tree, _rt) = make_flat_tree(dirs, files);

        group.bench_with_input(
            BenchmarkId::new("move_down", visible_count),
            &visible_count,
            |b, _| {
                b.iter(|| {
                    // Walk all the way down, then back up.
                    for _ in 0..visible_count {
                        tree.move_down();
                    }
                    for _ in 0..visible_count {
                        tree.move_up();
                    }
                })
            },
        );
    }

    group.finish();
}

/// Per-frame render cost: `git_status_for` called for every node in the
/// visible viewport. This is the dominant cost of each rendered frame when
/// git status is enabled and the user holds down j/k.
///
/// Parameterized by total visible nodes; viewport is capped at 40 rows to
/// reflect a realistic terminal height.
fn bench_frame_git_status(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_git_status");
    let config = bench_config();
    const VIEWPORT: usize = 40;

    for (dirs, files) in [(10usize, 10usize), (50, 20), (100, 50)] {
        let visible_count = 1 + dirs + dirs * files;
        let (_tmp, mut tree, _rt) = make_flat_tree(dirs, files);
        // Mark every 5th file as modified to exercise the status lookup path.
        inject_git_status(&mut tree, &config, 5);

        let scroll = tree.scroll_offset();

        group.bench_with_input(
            BenchmarkId::new("visible", visible_count),
            &visible_count,
            |b, _| {
                b.iter(|| {
                    let visible = tree.visible();
                    for &node_id in visible.iter().skip(scroll).take(VIEWPORT) {
                        std::hint::black_box(tree.git_status_for(node_id));
                    }
                })
            },
        );
    }

    group.finish();
}

/// Per-frame clipboard path check cost: `node_path` called for every visible
/// node to compare against the clipboard path. This matches the render loop
/// hot path when the user has yanked or cut a file.
fn bench_frame_clipboard_node_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_clipboard_node_path");
    const VIEWPORT: usize = 40;

    for (dirs, files) in [(10usize, 10usize), (50, 20), (100, 50)] {
        let visible_count = 1 + dirs + dirs * files;
        let (_tmp, tree, _rt) = make_flat_tree(dirs, files);

        let scroll = tree.scroll_offset();
        // Grab a path that won't match anything, simulating the clipboard
        // check iterating all visible rows without an early exit.
        let nonexistent = std::path::PathBuf::from("/no/match");

        group.bench_with_input(
            BenchmarkId::new("visible", visible_count),
            &visible_count,
            |b, _| {
                b.iter(|| {
                    let visible = tree.visible();
                    for &node_id in visible.iter().skip(scroll).take(VIEWPORT) {
                        std::hint::black_box(tree.node_path(node_id) == nonexistent);
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_node_path,
    bench_rebuild_visible,
    bench_reveal_path,
    bench_expand_sync,
    bench_children_loaded_rescan,
    bench_document_open,
    bench_navigate,
    bench_frame_git_status,
    bench_frame_clipboard_node_path,
);
criterion_main!(benches);
