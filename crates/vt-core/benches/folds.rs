//! The time of a snapshot's update after a scroll of a row, of a view in a
//! scrollback of about 10 000 rows of commands, of folds: the view's rows
//! of the folds that it meets, as many as of none.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lokyr_vt_core::terminal::{Scroll, Snapshot};

#[path = "snapshot.rs"]
#[allow(dead_code)]
mod of_commands;

fn folds(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot after a scroll of a row");
    for folded in [0, 10, 99] {
        let mut terminal = of_commands::commands();
        let ids: Vec<u64> = terminal
            .commands()
            .iter()
            .map(|command| command.id)
            .collect();
        for &id in ids.iter().take(folded) {
            assert!(terminal.fold(id, false));
        }
        let mut shown = Snapshot::default();
        terminal.snapshot(&mut shown);
        let mut up = true;
        group.bench_with_input(BenchmarkId::new("folds", folded), &folded, |b, _| {
            b.iter(|| {
                terminal.scroll_viewport(if up { Scroll::Up(1) } else { Scroll::Down(1) });
                up = !up;
                black_box(terminal.snapshot(&mut shown))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, folds);
criterion_main!(benches);
