//! The time that a frame's cells take to lay out, with the system's fonts:
//! a full screen of text, the same text underlined whole, and with waves of
//! a color of their own, and a screen of spaces, whose glyphs are all in
//! the atlas already, as most frames find them; and a screen of words that
//! differ, of a family with ligatures where one is installed, whose runs
//! the shaping's cache keeps.

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
        styled("", line)
    }

    /// A screen of `line` as [`screen`] makes it, after `pen`, a style
    /// that every cell takes, but where `line` changes it.
    fn styled(pen: &str, line: &str) -> Snapshot {
        let mut terminal = Terminal::new(COLUMNS, ROWS, Options::default());
        let row: String = line.chars().cycle().take(usize::from(COLUMNS)).collect();
        let text = pen.to_owned() + &vec![row; usize::from(ROWS)].join("\r\n");
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    /// A screen of words that differ, as code's are: about 48 runs a row.
    fn words() -> Snapshot {
        let mut terminal = Terminal::new(COLUMNS, ROWS, Options::default());
        // A generator of words that repeats nothing on the screen.
        let mut state: u32 = 0x9E37_79B9;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let letters = b"abcdefghijklmnopqrstuvwxyz_->=!<";
        let mut text = String::new();
        for row in 0..ROWS {
            let mut line = String::new();
            while line.len() < usize::from(COLUMNS) - 10 {
                let length = 3 + next() % 7;
                for _ in 0..length {
                    line.push(char::from(letters[(next() % 32) as usize]));
                }
                line.push(' ');
            }
            text.push_str(&line);
            if row + 1 < ROWS {
                text.push_str("\r\n");
            }
        }
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    /// The first of the families with ligatures that is installed.
    fn ligatures() -> Option<Font<CoreText>> {
        [
            "CaskaydiaCove NFM",
            "Cascadia Code",
            "JetBrains Mono",
            "Fira Code",
        ]
        .into_iter()
        .find_map(|family| {
            let config = FontConfig {
                family: Some(lokyr_text_core::Name::try_from(family.to_owned()).ok()?),
                ..FontConfig::default()
            };
            let font = Font::new(CoreText::new(), config, 2.0).ok()?;
            font.missing_family().is_none().then_some(font)
        })
    }

    pub fn cells(c: &mut Criterion) {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0)
            .expect("the system's monospaced font");
        let cell = *font.cell();
        let mut group = c.benchmark_group("cells");
        group.throughput(Throughput::Elements(u64::from(COLUMNS) * u64::from(ROWS)));
        let text = "fn main() { \x1b[1mlet\x1b[22m x = \x1b[32m\"hello\"\x1b[39m; } // 0123456789 ";
        let screens = [
            ("text", screen(text)),
            ("underlined text", styled("\x1b[4m", text)),
            ("waved text", styled("\x1b[4:3;58;5;1m", text)),
            ("spaces", screen(" ")),
        ];
        for (name, snapshot) in &screens {
            // The first frame draws the glyphs into the atlas.
            font.begin_frame();
            pane::cells(snapshot, &cell, 0, 0, None, None, &mut font);
            group.bench_function(*name, |b| {
                b.iter(|| {
                    font.begin_frame();
                    let batch =
                        pane::cells(black_box(snapshot), &cell, 0, 0, None, None, &mut font);
                    black_box(batch)
                });
            });
        }
        let words = words();
        let mut families = vec![(
            "words",
            Font::new(CoreText::new(), FontConfig::default(), 2.0)
                .expect("the system's monospaced font"),
        )];
        if let Some(font) = ligatures() {
            families.push(("words with ligatures", font));
        }
        for (name, mut font) in families {
            let cell = *font.cell();
            font.begin_frame();
            pane::cells(&words, &cell, 0, 0, None, None, &mut font);
            group.bench_function(name, |b| {
                b.iter(|| {
                    font.begin_frame();
                    let batch = pane::cells(black_box(&words), &cell, 0, 0, None, None, &mut font);
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
