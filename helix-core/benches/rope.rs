/// Benchmarks for ropey rope operations.
///
/// Run with:
///   cargo bench -p helix-core --bench rope
///
/// The fixture is helix-core/src/selection.rs (~1 400 lines, ~50 KB) — a
/// realistic mid-sized source file. Using it avoids the need to ship a
/// separate fixture and keeps the size representative of everyday editing.
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use ropey::Rope;

const FIXTURE: &str = include_str!("../src/selection.rs");

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

fn bench_from_str(c: &mut Criterion) {
    c.bench_function("rope/from_str", |b| {
        b.iter(|| black_box(Rope::from_str(black_box(FIXTURE))))
    });
}

// ---------------------------------------------------------------------------
// Single-character insert / delete (the keystroke hot path)
// ---------------------------------------------------------------------------

fn bench_insert_char_midpoint(c: &mut Criterion) {
    c.bench_function("rope/insert/char_at_midpoint", |b| {
        b.iter_batched(
            || {
                let r = Rope::from_str(FIXTURE);
                let mid = r.len_chars() / 2;
                (r, mid)
            },
            |(mut rope, mid)| {
                rope.insert_char(black_box(mid), black_box('x'));
                rope
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_delete_char_midpoint(c: &mut Criterion) {
    c.bench_function("rope/delete/char_at_midpoint", |b| {
        b.iter_batched(
            || {
                let r = Rope::from_str(FIXTURE);
                let mid = r.len_chars() / 2;
                (r, mid)
            },
            |(mut rope, mid)| {
                rope.remove(black_box(mid..mid + 1));
                rope
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Line insert / delete (dd / yp style operations)
// ---------------------------------------------------------------------------

fn bench_insert_line_midpoint(c: &mut Criterion) {
    c.bench_function("rope/insert/line_at_midpoint", |b| {
        b.iter_batched(
            || {
                let r = Rope::from_str(FIXTURE);
                let mid_char = r.line_to_char(r.len_lines() / 2);
                (r, mid_char)
            },
            |(mut rope, pos)| {
                rope.insert(black_box(pos), black_box("    let x = some_function_call();\n"));
                rope
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_delete_line_midpoint(c: &mut Criterion) {
    c.bench_function("rope/delete/line_at_midpoint", |b| {
        b.iter_batched(
            || {
                let r = Rope::from_str(FIXTURE);
                let mid = r.len_lines() / 2;
                let start = r.line_to_char(mid);
                let end = r.line_to_char(mid + 1);
                (r, start, end)
            },
            |(mut rope, start, end)| {
                rope.remove(black_box(start..end));
                rope
            },
            BatchSize::SmallInput,
        )
    });
}

// ---------------------------------------------------------------------------
// Slice (zero-copy view — called by syntax highlighting and rendering)
// ---------------------------------------------------------------------------

fn bench_slice(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let mid = rope.len_chars() / 2;
    c.bench_function("rope/slice/500_chars", |b| {
        b.iter(|| black_box(rope.slice(black_box(mid - 250..mid + 250))))
    });
}

// ---------------------------------------------------------------------------
// Index conversions (called on every rendered line)
// ---------------------------------------------------------------------------

fn bench_line_to_char(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let mid_line = rope.len_lines() / 2;
    c.bench_function("rope/line_to_char", |b| {
        b.iter(|| black_box(rope.line_to_char(black_box(mid_line))))
    });
}

fn bench_char_to_line(c: &mut Criterion) {
    let rope = Rope::from_str(FIXTURE);
    let mid_char = rope.len_chars() / 2;
    c.bench_function("rope/char_to_line", |b| {
        b.iter(|| black_box(rope.char_to_line(black_box(mid_char))))
    });
}

// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_from_str,
    bench_insert_char_midpoint,
    bench_delete_char_midpoint,
    bench_insert_line_midpoint,
    bench_delete_line_midpoint,
    bench_slice,
    bench_line_to_char,
    bench_char_to_line,
);
criterion_main!(benches);
