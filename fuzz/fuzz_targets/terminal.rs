//! Feeds arbitrary bytes to the VT core's terminal, with every option that
//! lets programs do more turned on, so that no input makes it panic: the
//! strings, the reports and the window's operations too, which the
//! differential target leaves out or cannot compare.
//!
//! The input's first byte picks the size, and the terminal is resized
//! halfway, so that the reflow meets the input too.

#![no_main]

use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, Terminal};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&size, rest)) = data.split_first() else {
        return;
    };
    let (cols, rows) = (u16::from(size % 32) + 1, u16::from(size / 32) + 1);
    let options = Options {
        title_reports: true,
        clipboard_reads: true,
        rectangle_checksums: true,
        window_ops: true,
        ..Options::default()
    };
    let mut terminal = Terminal::new(cols, rows, options);
    terminal.set_cell_size(8, 16);
    terminal.set_display_size(800, 480);
    let mut parser = Parser::new();
    let (first, second) = rest.split_at(rest.len() / 2);
    parser.advance(&mut terminal, first);
    terminal.take_replies();
    terminal.take_events();
    terminal.resize(cols + 5, rows + 3);
    parser.advance(&mut terminal, second);
    terminal.take_replies();
    terminal.take_events();
});
