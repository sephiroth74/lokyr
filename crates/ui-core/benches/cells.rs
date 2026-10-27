//! The time that a frame's cells take to lay out, with the system's fonts:
//! a full screen of text, and one of spaces, whose glyphs are all in the
//! atlas already, as most frames find them.

#[cfg(target_os = "macos")]
mod cells {
    use std::hint::black_box;

    use criterion::{Criterion, Throughput};
    use lokyr_font_coretext::CoreText;
    use lokyr_text_core::FontConfig;
    use lokyr_ui_core::font::Font;
    use lokyr_ui_core::pane;
    use lokyr_vt_core::parser::Parser;
    use lokyr_vt_core::terminal::{Options, Snapshot, Terminal};

    const COLUMNS: u16 = 400;
    const ROWS: u16 = 120;

    /// A screen whose every row is `line`, repeated to its width.
    fn screen(line: &str) -> Snapshot {
        let mut terminal = Terminal::new(COLUMNS, ROWS, Options::default());
        let row: String = line.chars().cycle().take(usize::from(COLUMNS)).collect();
        let text = vec![row; usize::from(ROWS)].join("\r\n");
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    pub fn cells(c: &mut Criterion) {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0)
            .expect("the system's monospaced font");
        let cell = *font.cell();
        let mut group = c.benchmark_group("cells");
        group.throughput(Throughput::Elements(u64::from(COLUMNS) * u64::from(ROWS)));
        let screens = [
            (
                "text",
                screen(
                    "fn main() { \x1b[1mlet\x1b[m x = \x1b[32m\"hello\"\x1b[m; } // 0123456789 ",
                ),
            ),
            ("spaces", screen(" ")),
        ];
        for (name, snapshot) in &screens {
            // The first frame draws the glyphs into the atlas.
            font.begin_frame();
            pane::cells(snapshot, &cell, 0, 0, None, None, &mut |shown| {
                font.glyph(shown)
            });
            group.bench_function(*name, |b| {
                b.iter(|| {
                    font.begin_frame();
                    let batch =
                        pane::cells(black_box(snapshot), &cell, 0, 0, None, None, &mut |shown| {
                            font.glyph(shown)
                        });
                    black_box(batch)
                });
            });
        }
        group.finish();
    }
}

#[cfg(target_os = "macos")]
criterion::criterion_group!(benches, cells::cells);
#[cfg(target_os = "macos")]
criterion::criterion_main!(benches);

#[cfg(not(target_os = "macos"))]
fn main() {}
