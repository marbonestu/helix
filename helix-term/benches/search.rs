/// Benchmarks for fuzzy search (nucleo).
///
/// Run with:
///   cargo bench -p helix-term --bench search
///
/// `fuzzy_match` is called on every keypress inside the file picker, symbol
/// picker, and buffer picker. The item list can range from a handful of buffers
/// to hundreds of thousands of files in a large repo.
///
/// Scenarios:
///   small   —  100 items  (buffer picker, command palette)
///   medium  — 1 000 items (typical project file picker)
///   large   — 10 000 items (large monorepo file picker)
///   path    — path-mode scoring for file paths
///   empty   — empty pattern (shows all items, no scoring needed)
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use helix_core::fuzzy::fuzzy_match;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_filenames(n: usize) -> Vec<String> {
    // Mix of realistic-looking paths to exercise path splitting heuristics.
    let stems = [
        "main", "lib", "utils", "config", "handler", "router", "model", "view",
        "controller", "service", "repository", "middleware", "schema", "types",
        "errors", "helpers", "constants", "index", "app", "server",
    ];
    let exts = ["rs", "ts", "go", "py", "js", "tsx", "md", "toml", "json", "yaml"];
    let dirs = [
        "src", "lib", "pkg", "internal", "cmd", "api", "ui", "core",
        "tests", "benches", "docs", "scripts",
    ];
    (0..n)
        .map(|i| {
            format!(
                "{}/{}/{}.{}",
                dirs[i % dirs.len()],
                stems[(i / dirs.len()) % stems.len()],
                stems[i % stems.len()],
                exts[i % exts.len()]
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_fuzzy_small(c: &mut Criterion) {
    let items = make_filenames(100);
    c.bench_function("search/fuzzy/100_items", |b| {
        b.iter(|| black_box(fuzzy_match(black_box("main"), &items, false)))
    });
}

fn bench_fuzzy_medium(c: &mut Criterion) {
    let items = make_filenames(1_000);
    c.bench_function("search/fuzzy/1000_items", |b| {
        b.iter(|| black_box(fuzzy_match(black_box("srcmain"), &items, false)))
    });
}

fn bench_fuzzy_large(c: &mut Criterion) {
    let items = make_filenames(10_000);
    c.bench_function("search/fuzzy/10000_items", |b| {
        b.iter(|| black_box(fuzzy_match(black_box("srcmain"), &items, false)))
    });
}

fn bench_fuzzy_path_mode(c: &mut Criterion) {
    let items = make_filenames(1_000);
    c.bench_function("search/fuzzy/path_mode_1000_items", |b| {
        b.iter(|| black_box(fuzzy_match(black_box("src/main"), &items, true)))
    });
}

fn bench_fuzzy_empty_pattern(c: &mut Criterion) {
    let items = make_filenames(10_000);
    c.bench_function("search/fuzzy/empty_pattern_10000_items", |b| {
        b.iter(|| black_box(fuzzy_match(black_box(""), &items, false)))
    });
}

// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_fuzzy_small,
    bench_fuzzy_medium,
    bench_fuzzy_large,
    bench_fuzzy_path_mode,
    bench_fuzzy_empty_pattern,
);
criterion_main!(benches);
