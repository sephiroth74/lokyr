//! vtebench's scenarios (<https://github.com/alacritty/vtebench>), whose
//! payloads its scripts print, rebuilt as they print them at 120 by 40, with
//! the data that they read from a checkout of vtebench, and the line feeds
//! that a PTY turns into CR LF. `lokyr-throughput` times them, and the
//! `instructions` benchmark counts the instructions that the VT core takes
//! for them.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

/// The terminal's columns.
pub const COLS: usize = 120;

/// The terminal's rows.
pub const ROWS: usize = 40;

/// The size of a read from a PTY.
pub const READ: usize = 64 * 1024;

/// How many bytes a sample's payload has at least, as vtebench's default.
pub const MIN_BYTES: usize = 1024 * 1024;

/// A scenario of vtebench: the output of its setup, as a PTY hands it
/// over, and of its benchmark, as the script prints it.
pub struct Scenario {
    /// vtebench's name for it.
    pub name: &'static str,
    /// What the setup prints, its line feeds as a PTY hands them over.
    pub setup: Vec<u8>,
    /// What the benchmark prints once, as the script prints it.
    pub payload: Vec<u8>,
}

/// The payload of a sample, as a PTY hands it over: `payload`, as the
/// script prints it, repeated to [`MIN_BYTES`] at least, as vtebench
/// repeats it before the PTY adds the carriage returns.
pub fn samples(payload: &[u8]) -> Vec<u8> {
    let count = (MIN_BYTES - 1) / payload.len() + 1;
    onlcr(&payload.repeat(count))
}

/// `text` as a PTY hands it over: each line feed after a carriage return.
pub fn onlcr(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + text.len() / 16);
    for &byte in text {
        if byte == b'\n' {
            out.push(b'\r');
        }
        out.push(byte);
    }
    out
}

/// The letters that several scenarios draw with, one screen each.
const LETTERS: std::ops::RangeInclusive<u8> = b'A'..=b'Z';

/// vtebench's scenarios, as its scripts print them at 120 by 40, with the
/// data files of the checkout in `dir`.
pub fn scenarios(dir: &Path) -> Result<Vec<Scenario>> {
    let read = |path: &str| {
        let path = dir.join("benchmarks").join(path);
        std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
    };
    let alternate = b"\x1b[?1049h".to_vec();
    // The setup of the scrolling scenarios, which fills the scrollback: a
    // shell that expands `{0..100000}`, as macOS's does.
    let filled = b"y\n".repeat(100_001);
    let region =
        |top: usize, bottom: usize| format!("\x1b[?1049h\x1b[{top};{bottom}r").into_bytes();
    let scenarios = vec![
        Scenario {
            name: "cursor_motion",
            setup: Vec::new(),
            payload: cursor_motion(),
        },
        Scenario {
            name: "dense_cells",
            setup: alternate.clone(),
            payload: dense_cells(),
        },
        Scenario {
            name: "light_cells",
            setup: alternate.clone(),
            payload: light_cells(),
        },
        Scenario {
            name: "medium_cells",
            setup: Vec::new(),
            payload: read("medium_cells/vim_session")?,
        },
        Scenario {
            name: "scrolling",
            setup: filled.clone(),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_bottom_region",
            setup: region(1, ROWS - 1),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_bottom_small_region",
            setup: region(1, ROWS / 2),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_fullscreen",
            setup: filled,
            payload: scrolling_fullscreen(),
        },
        Scenario {
            name: "scrolling_top_region",
            setup: region(2, ROWS),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_top_small_region",
            setup: region(ROWS / 2, ROWS),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "sync_medium_cells",
            setup: Vec::new(),
            payload: read("sync_medium_cells/vim_session")?,
        },
        Scenario {
            name: "unicode",
            setup: alternate,
            payload: read("unicode/symbols")?,
        },
    ];
    Ok(scenarios
        .into_iter()
        .map(|scenario| Scenario {
            setup: onlcr(&scenario.setup),
            ..scenario
        })
        .collect())
}

/// A letter at every cell, the cursor moved to each, along the edges of
/// smaller and smaller rectangles, a letter a screen.
fn cursor_motion() -> Vec<u8> {
    let mut out = String::new();
    for letter in LETTERS.map(char::from) {
        let (mut column_start, mut column_end) = (1, COLS);
        let (mut line_start, mut line_end) = (1, ROWS);
        loop {
            let (mut column, mut line) = (column_start, line_start);
            while column < column_end {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                column += 1;
            }
            while line < line_end {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                line += 1;
            }
            while column > column_start {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                column -= 1;
            }
            while line > line_start {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                line -= 1;
            }
            column_start += 1;
            line_start += 1;
            column_end -= 1;
            line_end -= 1;
            if column_start > column_end || line_start > line_end {
                break;
            }
        }
    }
    out.into_bytes()
}

/// Every cell in 256 colors, bold, italic and underlined, a letter a
/// screen.
fn dense_cells() -> Vec<u8> {
    let mut out = String::new();
    for (offset, letter) in LETTERS.map(char::from).enumerate() {
        out.push_str("\x1b[H");
        for line in 1..=ROWS {
            for column in 1..=COLS {
                let index = line + column + offset;
                let foreground = index % 156 + 100;
                let background = 255 - index % 156 + 100;
                let _ = write!(
                    out,
                    "\x1b[38;5;{foreground};48;5;{background};1;3;4m{letter}"
                );
            }
        }
    }
    out.into_bytes()
}

/// The screen full of a letter, a letter a screen.
fn light_cells() -> Vec<u8> {
    let mut out = Vec::new();
    for letter in LETTERS {
        out.extend_from_slice(b"\x1b[H");
        out.extend(std::iter::repeat_n(letter, COLS * ROWS));
    }
    out
}

/// Full rows of a letter, each a line, a letter a row.
fn scrolling_fullscreen() -> Vec<u8> {
    let mut out = Vec::new();
    for letter in LETTERS {
        out.extend(std::iter::repeat_n(letter, COLS));
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pty_puts_a_carriage_return_before_each_line_feed() {
        assert_eq!(onlcr(b"a\nb\r\n"), b"a\r\nb\r\r\n");
    }

    #[test]
    fn a_sample_repeats_the_payload_to_a_mebibyte_before_the_carriage_returns() {
        // Two bytes a line, repeated to 1 MiB, which the carriage returns
        // then make 1.5 MiB.
        let sample = samples(b"y\n");
        assert_eq!(sample.len(), 3 * MIN_BYTES / 2);
        assert!(sample.starts_with(b"y\r\ny\r\n"));
    }

    #[test]
    fn the_scenarios_print_as_their_scripts_print_at_120_by_40() {
        let escapes = |text: &[u8]| text.iter().filter(|&&byte| byte == 0x1B).count();
        // A letter a screen: the cursor home, then every cell.
        assert_eq!(light_cells().len(), 26 * (3 + COLS * ROWS));
        assert_eq!(escapes(&dense_cells()), 26 * (1 + COLS * ROWS));
        // A move for every cell along the edges of twenty rectangles, a
        // letter at a time, as the script's loops count them.
        let motion = cursor_motion();
        assert_eq!(escapes(&motion), 26 * 4800);
        assert!(motion.starts_with(b"\x1b[1;1HA\x1b[1;2HA"));
        // Full rows, a line each.
        assert_eq!(scrolling_fullscreen().len(), 26 * (COLS + 1));
    }
}
