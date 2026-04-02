/// Benchmarks for terminal cell diffing.
///
/// Run with:
///   cargo bench -p helix-tui --bench buffer
///
/// `Buffer::diff` is called every rendered frame to find cells that changed
/// between the previous and current draw pass. Only changed cells are written
/// to the terminal backend, so the diff cost dominates at low change rates.
///
/// Scenarios:
///   identical   — nothing changed (best case, common during idle)
///   all_changed — every cell changed (worst case, e.g. first draw / resize)
///   typical     — ~10 % of cells changed (realistic editing frame)
///   wide_chars  — buffer contains multi-column Unicode characters
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use helix_tui::buffer::Buffer;
use helix_view::graphics::Rect;

const WIDTH: u16 = 220;
const HEIGHT: u16 = 50;

fn make_area() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: WIDTH,
        height: HEIGHT,
    }
}

// ---------------------------------------------------------------------------
// Identical buffers — diff should return nothing
// ---------------------------------------------------------------------------

fn bench_diff_identical(c: &mut Criterion) {
    let area = make_area();
    let buf = {
        let lines: Vec<String> = (0..HEIGHT)
            .map(|i| format!("{:>4} {:.<width$}", i, "", width = (WIDTH - 5) as usize))
            .collect();
        Buffer::with_lines(lines)
    };

    c.bench_function("buffer/diff/identical", |b| {
        b.iter(|| black_box(buf.diff(black_box(&buf))).len())
    });

    let _ = area;
}

// ---------------------------------------------------------------------------
// All cells changed — every cell is in the output (worst case)
// ---------------------------------------------------------------------------

fn bench_diff_all_changed(c: &mut Criterion) {
    let lines_a: Vec<String> = (0..HEIGHT)
        .map(|_| "a".repeat(WIDTH as usize))
        .collect();
    let lines_b: Vec<String> = (0..HEIGHT)
        .map(|_| "b".repeat(WIDTH as usize))
        .collect();
    let buf_a = Buffer::with_lines(lines_a);
    let buf_b = Buffer::with_lines(lines_b);

    c.bench_function("buffer/diff/all_changed", |b| {
        b.iter(|| black_box(buf_a.diff(black_box(&buf_b))).len())
    });
}

// ---------------------------------------------------------------------------
// ~10 % changed — realistic editing frame (cursor moved, line re-rendered)
// ---------------------------------------------------------------------------

fn bench_diff_typical(c: &mut Criterion) {
    let lines: Vec<String> = (0..HEIGHT)
        .map(|i| format!("{:>4} {:.<width$}", i, "", width = (WIDTH - 5) as usize))
        .collect();
    let buf_a = Buffer::with_lines(lines.clone());
    let mut buf_b = Buffer::with_lines(lines);

    // Modify every 10th cell to simulate a typical partial-redraw frame.
    for i in (0..buf_b.content.len()).step_by(10) {
        buf_b.content[i].symbol = "X".to_string();
    }

    c.bench_function("buffer/diff/typical_10pct_changed", |b| {
        b.iter(|| black_box(buf_a.diff(black_box(&buf_b))).len())
    });
}

// ---------------------------------------------------------------------------
// Wide characters — the diff must skip the phantom cells that follow them
// ---------------------------------------------------------------------------

fn bench_diff_wide_chars(c: &mut Criterion) {
    // CJK characters are 2 columns wide; mix them with ASCII.
    let wide_line: String = "コード ".repeat((WIDTH as usize) / 4);
    let lines_a: Vec<String> = (0..HEIGHT).map(|_| wide_line.clone()).collect();
    let lines_b: Vec<String> = (0..HEIGHT)
        .map(|i| {
            if i % 5 == 0 {
                "a".repeat(WIDTH as usize)
            } else {
                wide_line.clone()
            }
        })
        .collect();
    let buf_a = Buffer::with_lines(lines_a);
    let buf_b = Buffer::with_lines(lines_b);

    c.bench_function("buffer/diff/wide_chars", |b| {
        b.iter(|| black_box(buf_a.diff(black_box(&buf_b))).len())
    });
}

// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_diff_identical,
    bench_diff_all_changed,
    bench_diff_typical,
    bench_diff_wide_chars,
);
criterion_main!(benches);
