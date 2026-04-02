/// Benchmarks for Selection and Range operations.
///
/// Run with:
///   cargo bench -p helix-core --bench selection
///
/// Key scenarios:
///   - Construction: building selections with varying cursor counts
///   - map: updating all ranges after a document edit (runs on every keypress
///     that changes the document while cursors are present)
///   - normalize: sorting + merging overlapping ranges
///   - select_on_matches: regex-driven multi-cursor expansion
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use helix_core::{
    selection::{select_on_matches, split_on_matches},
    Range, Selection, Tendril, Transaction,
};
use helix_stdx::rope::Regex;
use ropey::Rope;
use smallvec::SmallVec;

const FIXTURE: &str = include_str!("../src/selection.rs");

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

fn bench_single_range(c: &mut Criterion) {
    c.bench_function("selection/construct/single", |b| {
        b.iter(|| black_box(Selection::single(black_box(100), black_box(200))))
    });
}

fn bench_multi_range_100(c: &mut Criterion) {
    c.bench_function("selection/construct/100_ranges", |b| {
        b.iter(|| {
            let ranges: SmallVec<[Range; 1]> =
                (0..100u32).map(|i| Range::point((i * 50) as usize)).collect();
            black_box(Selection::new(ranges, 0))
        })
    });
}

// ---------------------------------------------------------------------------
// map: shift all ranges after an insertion at position 0
//
// This runs on every document-mutating keystroke when multiple cursors are
// active. Cost scales linearly with cursor count.
// ---------------------------------------------------------------------------

fn bench_map_single(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let mid = rope.len_chars() / 2;
    let sel = Selection::single(mid - 10, mid + 10);
    let transaction =
        Transaction::change(&rope, std::iter::once((0usize, 0usize, Some(Tendril::from("x")))));
    let cs = transaction.changes();

    c.bench_function("selection/map/single_range", |b| {
        b.iter(|| black_box(sel.clone().map(black_box(cs))))
    });
}

fn bench_map_100_ranges(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let len = rope.len_chars();
    let ranges: SmallVec<[Range; 1]> = (0..100u32)
        .map(|i| Range::point((i as usize * len / 100).min(len - 1)))
        .collect();
    let sel = Selection::new(ranges, 0);
    let transaction =
        Transaction::change(&rope, std::iter::once((0usize, 0usize, Some(Tendril::from("x")))));
    let cs = transaction.changes();

    c.bench_function("selection/map/100_ranges_after_insert", |b| {
        b.iter(|| black_box(sel.clone().map(black_box(cs))))
    });
}

// ---------------------------------------------------------------------------
// normalize: sort + merge overlapping ranges (called after bulk selection ops)
//
// Selection::new normalizes internally, so constructing from unsorted ranges
// benchmarks the full sort + merge cost.
// ---------------------------------------------------------------------------

fn bench_normalize_100(c: &mut Criterion) {
    // Build 100 ranges in reverse order so normalize must sort them all.
    let ranges: SmallVec<[Range; 1]> = (0..100u32)
        .rev()
        .map(|i| Range::new((i as usize) * 1000, (i as usize) * 1000 + 50))
        .collect();

    c.bench_function("selection/normalize/100_ranges_unsorted", |b| {
        b.iter_batched(
            || ranges.clone(),
            |r| black_box(Selection::new(r, 0)),
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// transform: apply a closure to each range (e.g. extend_line, goto_line_end)
// ---------------------------------------------------------------------------

fn bench_transform_100(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let len = rope.len_chars();
    let ranges: SmallVec<[Range; 1]> = (0..100u32)
        .map(|i| Range::point((i as usize * len / 100).min(len.saturating_sub(1))))
        .collect();
    let sel = Selection::new(ranges, 0);

    c.bench_function("selection/transform/extend_100_ranges", |b| {
        b.iter(|| {
            black_box(sel.clone().transform(|r| {
                Range::new(r.anchor, (r.head + 10).min(len.saturating_sub(1)))
            }))
        })
    });
}

// ---------------------------------------------------------------------------
// select_on_matches: expand selection to all regex matches
// (Space-s, the multi-cursor expansion command)
// ---------------------------------------------------------------------------

fn bench_select_on_matches(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let sel = Selection::single(0, rope.len_chars());
    // Match all identifiers (word characters)
    let regex = Regex::new(r"\b[a-zA-Z_][a-zA-Z0-9_]{2,}\b").unwrap();

    c.bench_function("selection/select_on_matches/identifiers", |b| {
        b.iter(|| {
            black_box(select_on_matches(
                black_box(rope.slice(..)),
                black_box(&sel),
                black_box(&regex),
            ))
        })
    });
}

// ---------------------------------------------------------------------------
// split_on_matches: split selection at newlines (A-s, split on newline)
// ---------------------------------------------------------------------------

fn bench_split_on_matches(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let sel = Selection::single(0, rope.len_chars());
    let newline = Regex::new(r"\n").unwrap();

    c.bench_function("selection/split_on_matches/newlines", |b| {
        b.iter(|| {
            black_box(split_on_matches(
                black_box(rope.slice(..)),
                black_box(&sel),
                black_box(&newline),
            ))
        })
    });
}

// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_single_range,
    bench_multi_range_100,
    bench_map_single,
    bench_map_100_ranges,
    bench_normalize_100,
    bench_transform_100,
    bench_select_on_matches,
    bench_split_on_matches,
);
criterion_main!(benches);
