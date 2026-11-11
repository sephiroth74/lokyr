//! An image of cells, as a plugin asks for one: its rows of text, of their
//! styles, written into a terminal of their own, which lays them out as a
//! program's output, and drawn as a pane's cells, in a frame as a macOS
//! window's, of a bar of its three buttons and a title, rounded corners and
//! a shadow, with room around it.

use std::rc::Rc;

use lokyr_plugin_sdk::host::{Color as Ink, StyleRun, TextStyle, Underline};
use lokyr_plugin_sdk::image::{CellRow, ImageBackground, ImageSpec, MOST_COLUMNS};
use lokyr_plugin_sdk::view::FontWeight;
use lokyr_render_core::{Color, DisplayList, Item, Rect, Shadow, Shape};
use lokyr_text_core::FontBackend;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Colors, Options, Snapshot, Terminal};
use lokyr_vt_core::unicode::Widths;

use crate::font::Font;
use crate::pane::{self as cells, TextLook};
use crate::tree::Typeset;

/// The height of the window's bar, in points, as macOS's titlebar's.
const BAR: f32 = 28.0;

/// The side of each of the window's buttons, in points.
const BUTTON: f32 = 12.0;

/// The room between two buttons, in points.
const BUTTON_GAP: f32 = 8.0;

/// The room at the left of the first button, in points.
const BUTTONS_LEFT: f32 = 9.0;

/// The colors of the close, the minimize and the zoom buttons, each of its
/// border, as macOS draws them.
const BUTTONS: [(Color, Color); 3] = [
    (
        Color::from_rgb8(0xff, 0x5f, 0x57),
        Color::from_rgb8(0xe2, 0x46, 0x3f),
    ),
    (
        Color::from_rgb8(0xfe, 0xbc, 0x2e),
        Color::from_rgb8(0xe1, 0xa1, 0x16),
    ),
    (
        Color::from_rgb8(0x28, 0xc8, 0x40),
        Color::from_rgb8(0x14, 0xae, 0x2c),
    ),
];

/// The title's size, in points, as macOS's titlebar's.
const TITLE_POINTS: f32 = 13.0;

/// The longest title, in points, past which it is cut of an ellipsis.
const TITLE_ROOM: f32 = 480.0;

/// The room between the title and the buttons, or the window's edge, in
/// points.
const TITLE_MARGIN: f32 = 12.0;

/// The opacity of the title's text, as macOS's label's.
const TITLE_OPACITY: f32 = 0.85;

/// The radius of the window's rounded corners, in points.
const RADIUS: f32 = 12.0;

/// The least room between the cells and the window's edges, in points, so
/// that the cells keep within rounded corners.
const LEAST_PADDING: f32 = 8.0;

/// The width of the window's border, in points, and its opacity of the
/// text's color.
const BORDER: f32 = 0.5;
const BORDER_OPACITY: f32 = 0.15;

/// The window's shadow, in points: how far down it is moved, how far it
/// blurs, and its opacity.
const SHADOW_DOWN: f32 = 4.0;
const SHADOW_BLUR: f32 = 12.0;
const SHADOW_OPACITY: f32 = 0.35;

/// How much of the theme's background the background around the window
/// keeps, darker.
const DARKER: f32 = 0.65;

/// The terminal of `rows`, in `colors`, its cells counted as `widths`
/// counts them: as many columns as its widest row's cells, of its text and
/// of its runs past the text that show something without a character, as
/// a background, `least`'s columns at least, from two, as a wide character
/// fits, to [`MOST_COLUMNS`]; and a row of each, `least`'s rows at least,
/// empty below them.
pub fn terminal(
    rows: &[CellRow],
    least: (Option<u16>, Option<u16>),
    widths: Widths,
    colors: Colors,
) -> Terminal {
    let options = Options {
        widths,
        ..Options::default()
    };
    // Each row measured on a row as wide as the widest an image draws.
    let mut measure = Terminal::new(MOST_COLUMNS, 1, options.clone());
    let mut parser = Parser::new();
    parser.advance(&mut measure, SETUP);
    let columns = rows
        .iter()
        .map(|row| write_row(&mut parser, &mut measure, 0, row, widths))
        .max()
        .unwrap_or(0)
        .max(least.0.unwrap_or(0))
        .clamp(2, MOST_COLUMNS);
    let lines = u16::try_from(rows.len())
        .unwrap_or(u16::MAX)
        .max(least.1.unwrap_or(0))
        .max(1);
    let mut terminal = Terminal::new(columns, lines, options);
    terminal.set_default_colors(colors);
    let mut parser = Parser::new();
    parser.advance(&mut terminal, SETUP);
    for (at, row) in rows.iter().enumerate() {
        let at = u16::try_from(at).unwrap_or(u16::MAX);
        write_row(&mut parser, &mut terminal, at, row, widths);
    }
    terminal
}

/// What a terminal of an image takes first: no wrap at the right margin,
/// which leaves a row's cells past the last column out, and no cursor.
const SETUP: &[u8] = b"\x1b[?7l\x1b[?25l";

/// Writes `cells` on the row `row` of `terminal`, from its first column,
/// in its styles: its text, then the cells of its runs past the text that
/// show something; how many columns they take.
fn write_row(
    parser: &mut Parser,
    terminal: &mut Terminal,
    row: u16,
    cells: &CellRow,
    widths: Widths,
) -> u16 {
    let columns = terminal.cols();
    parser.advance(
        terminal,
        format!("\x1b[0m\x1b[{};1H\x1b[2K", u32::from(row) + 1).as_bytes(),
    );
    let styles = Styles::of(&cells.runs, columns);
    let mut pen = TextStyle::default();
    let mut bytes = [0; 4];
    for character in cells.text.chars() {
        // A character of no cell, as a combining mark, joins the cell before
        // in its style.
        if widths.char_width(character) > 0 {
            let style = styles.at(terminal.cursor().x);
            if style != pen {
                parser.advance(terminal, sgr(&style).as_bytes());
                pen = style;
            }
        }
        parser.advance(terminal, character.encode_utf8(&mut bytes).as_bytes());
    }
    let cursor = terminal.cursor();
    let text = if cursor.x + 1 >= columns {
        columns
    } else {
        cursor.x + u16::from(cursor.pending_wrap)
    };
    // The cells past the text that show something without a character.
    let shown = styles.shown_until().max(text);
    if shown > text {
        parser.advance(
            terminal,
            format!("\x1b[{}G", u32::from(text) + 1).as_bytes(),
        );
        for column in text..shown {
            let style = styles.at(column);
            if style != pen {
                parser.advance(terminal, sgr(&style).as_bytes());
                pen = style;
            }
            parser.advance(terminal, b" ");
        }
    }
    shown
}

/// The style of each cell of a row, of its runs, a later run over an
/// earlier one, the default one of a cell of none.
struct Styles<'a> {
    runs: &'a [StyleRun],
    /// The run of each column, by its index.
    owners: Vec<Option<usize>>,
}

impl<'a> Styles<'a> {
    /// The styles of `runs` on a row of `columns`, each cell given its last
    /// run once, so that runs over one another cost no more than the row.
    fn of(runs: &'a [StyleRun], columns: u16) -> Self {
        let width = runs
            .iter()
            .map(|run| usize::from(run.last) + 1)
            .max()
            .unwrap_or(0)
            .min(usize::from(columns));
        let mut owners = vec![None; width];
        // The next column without a run yet, at or after each, of paths
        // made short as they are walked.
        let mut next: Vec<usize> = (0..=width).collect();
        fn free(next: &mut [usize], column: usize) -> usize {
            let mut root = column;
            while next[root] != root {
                root = next[root];
            }
            let mut at = column;
            while next[at] != root {
                let up = next[at];
                next[at] = root;
                at = up;
            }
            root
        }
        for (index, run) in runs.iter().enumerate().rev() {
            let last = usize::from(run.last).min(width.saturating_sub(1));
            let mut column = free(&mut next, usize::from(run.first).min(width));
            while column <= last && column < width {
                owners[column] = Some(index);
                next[column] = column + 1;
                column = free(&mut next, column + 1);
            }
        }
        Self { runs, owners }
    }

    /// The style of the cell at `column`.
    fn at(&self, column: u16) -> TextStyle {
        self.owners
            .get(usize::from(column))
            .copied()
            .flatten()
            .map(|index| self.runs[index].style)
            .unwrap_or_default()
    }

    /// The column past the last whose style shows something without a
    /// character: a background, the colors swapped, or a line.
    fn shown_until(&self) -> u16 {
        let shows = |style: &TextStyle| {
            style.background.is_some()
                || style.inverse
                || style.underline.is_some()
                || style.strikethrough
                || style.overline
        };
        self.owners
            .iter()
            .rposition(|owner| owner.is_some_and(|index| shows(&self.runs[index].style)))
            .map_or(0, |column| u16::try_from(column + 1).unwrap_or(u16::MAX))
    }
}

/// The SGR sequence that gives the pen `style`, from the default one.
fn sgr(style: &TextStyle) -> String {
    let mut sequence = String::from("\x1b[0");
    let flags = [
        (style.bold, "1"),
        (style.faint, "2"),
        (style.italic, "3"),
        (style.blink, "5"),
        (style.inverse, "7"),
        (style.invisible, "8"),
        (style.strikethrough, "9"),
        (style.overline, "53"),
    ];
    for (on, code) in flags {
        if on {
            sequence.push(';');
            sequence.push_str(code);
        }
    }
    if let Some(underline) = style.underline {
        sequence.push_str(match underline {
            Underline::Single => ";4",
            Underline::Double => ";4:2",
            Underline::Curly => ";4:3",
            Underline::Dotted => ";4:4",
            Underline::Dashed => ";4:5",
        });
    }
    let colors = [
        (style.foreground, 38),
        (style.background, 48),
        (style.underline_color, 58),
    ];
    for (color, base) in colors {
        match color {
            Some(Ink::Palette(index)) => sequence.push_str(&format!(";{base};5;{index}")),
            Some(Ink::Rgb(red, green, blue)) => {
                sequence.push_str(&format!(";{base};2;{red};{green};{blue}"));
            }
            None => {}
        }
    }
    sequence.push('m');
    sequence
}

/// An image's display list, and its size in pixels.
#[derive(Debug)]
pub struct Composed {
    /// What it draws.
    pub list: DisplayList,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// The most pixels that [`compose`] makes of `image`, of `cells` columns
/// and rows of `cell` pixels each, `padding` pixels across and down, at
/// `scale`: its size, of the title's room whole.
pub fn bound(
    image: &ImageSpec,
    cells: (u16, u16),
    cell: (u32, u32),
    padding: (u32, u32),
    scale: f32,
) -> (u64, u64) {
    let whole = |points: f32| u64::from((points * scale).round() as u32);
    let (padding_x, padding_y) = (
        u64::from(padding.0).max(whole(LEAST_PADDING)),
        u64::from(padding.1).max(whole(LEAST_PADDING)),
    );
    let cells_width = u64::from(cells.0) * u64::from(cell.0);
    let cells_height = u64::from(cells.1) * u64::from(cell.1);
    let bar = if image.has_bar() { whole(BAR) } else { 0 };
    let buttons_end = BUTTONS_LEFT + 3.0 * BUTTON + 2.0 * BUTTON_GAP;
    let bar_width = if image.has_bar() {
        whole(2.0 * (buttons_end + TITLE_MARGIN) + TITLE_ROOM)
    } else {
        0
    };
    let outer = whole(image.padding);
    (
        (cells_width + 2 * padding_x).max(bar_width) + 2 * outer,
        bar + cells_height + 2 * padding_y + 2 * outer,
    )
}

/// The image that `image` asks for of `snapshot`'s cells, in `look`, at
/// `scale`, of `font`, `padding` pixels across and down between the cells
/// and the window's edges, as a window's own, of [`LEAST_PADDING`] at
/// least: the window as wide as its cells, and as its bar's buttons and
/// title need. Its glyphs go into the font's atlas, at `scale`, in a frame
/// that the caller began; the font takes its scale before back.
pub fn compose<B: FontBackend>(
    image: &ImageSpec,
    snapshot: &Snapshot,
    look: TextLook,
    padding: (u32, u32),
    scale: f32,
    font: &mut Font<B>,
) -> Composed {
    let before = font.scale();
    font.set_scale(scale);
    let pixels = |points: f32| (points * scale).round();
    let whole = |points: f32| pixels(points) as u32;
    let cell = *font.cell();
    let background = cells::background(snapshot).with_alpha(1.0);
    let foreground = cells::foreground(snapshot);
    let dark = luminance(background) < luminance(foreground);
    let (cells_width, cells_height) = (
        u32::from(snapshot.cols()) * cell.width,
        u32::from(snapshot.rows()) * cell.height,
    );
    let (padding_x, padding_y) = (
        padding.0.max(whole(LEAST_PADDING)),
        padding.1.max(whole(LEAST_PADDING)),
    );
    let bar = if image.has_bar() { whole(BAR) } else { 0 };
    // The bar's buttons, and its title, in the interface's face, cut where
    // it is longer than its room, measured of its glyphs.
    let buttons_end = if image.buttons {
        BUTTONS_LEFT + 3.0 * BUTTON + 2.0 * BUTTON_GAP
    } else {
        0.0
    };
    let title_color = foreground.with_alpha(foreground.alpha * TITLE_OPACITY);
    font.set_typeset(Typeset {
        points: Some(TITLE_POINTS),
        weight: FontWeight::Semibold,
        ..Typeset::default()
    });
    let (ascent, line) = {
        let face = font.interface();
        (face.ascent(), face.line())
    };
    let title = image
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .map(|title| {
            let width = font
                .interface_carets(title)
                .last()
                .map_or(0.0, |&(_, after)| after);
            if width > TITLE_ROOM {
                font.interface_shortened(title, TITLE_ROOM)
            } else {
                Rc::from(title)
            }
        });
    let title_width = title.as_ref().map_or(0.0, |title| {
        font.interface_text(title, (0.0, 0.0), title_color, dark).1
    });
    let side = pixels(buttons_end + TITLE_MARGIN);
    let bar_width = match &title {
        Some(_) => (2.0 * side + title_width).ceil() as u32,
        None if image.buttons => whole(buttons_end + BUTTONS_LEFT),
        None => 0,
    };
    let window_width = (cells_width + 2 * padding_x).max(bar_width);
    let window_height = bar + cells_height + 2 * padding_y;
    let outer = whole(image.padding);
    let (width, height) = (window_width + 2 * outer, window_height + 2 * outer);
    let at = |pixels: u32| i32::try_from(pixels).unwrap_or(i32::MAX);
    let window = Rect {
        x: at(outer),
        y: at(outer),
        width: window_width,
        height: window_height,
    };
    let mut items = vec![Item::Shapes(vec![Shape {
        rect: window,
        radius: if image.rounded { pixels(RADIUS) } else { 0.0 },
        fill: background,
        border: (BORDER * scale).max(1.0),
        border_color: foreground.with_alpha(BORDER_OPACITY),
        shadow: image.shadow.then(|| Shadow {
            x: 0.0,
            y: pixels(SHADOW_DOWN),
            blur: pixels(SHADOW_BLUR),
            color: Color::BLACK.with_alpha(SHADOW_OPACITY),
        }),
        arc: None,
        knockout: false,
    }])];
    let origin = (at(outer + padding_x), at(outer + bar + padding_y));
    let batch = cells::cells_with(snapshot, &cell, origin, None, None, look, font);
    items.push(Item::Cells(batch));
    let middle = f32::from(u16::try_from(outer).unwrap_or(u16::MAX)) + bar as f32 / 2.0;
    if image.buttons {
        let side = pixels(BUTTON);
        let shapes = BUTTONS
            .iter()
            .enumerate()
            .map(|(index, &(fill, border))| {
                let left =
                    outer as f32 + pixels(BUTTONS_LEFT + index as f32 * (BUTTON + BUTTON_GAP));
                Shape {
                    rect: Rect {
                        x: left as i32,
                        y: (middle - side / 2.0).round() as i32,
                        width: side as u32,
                        height: side as u32,
                    },
                    radius: side / 2.0,
                    fill,
                    border: (BORDER * scale).max(1.0),
                    border_color: border,
                    shadow: None,
                    arc: None,
                    knockout: false,
                }
            })
            .collect();
        items.push(Item::Shapes(shapes));
    }
    if let Some(title) = &title {
        // In the middle of the window, clear of the buttons.
        let left =
            (outer as f32 + (window_width as f32 - title_width) / 2.0).max(outer as f32 + side);
        let baseline = middle - pixels(line) / 2.0 + pixels(ascent);
        let (glyphs, _) = font.interface_text(title, (left, baseline), title_color, dark);
        items.push(Item::Glyphs(glyphs));
    }
    font.set_typeset(Typeset::default());
    font.set_scale(before);
    let list = DisplayList {
        background: match image.background {
            ImageBackground::Theme => Color {
                red: background.red * DARKER,
                green: background.green * DARKER,
                blue: background.blue * DARKER,
                alpha: 1.0,
            },
            ImageBackground::Clear => Color::CLEAR,
            ImageBackground::Rgb(red, green, blue) => Color::from_rgb8(red, green, blue),
        },
        items,
        frame: font.atlas_frame(),
    };
    Composed {
        list,
        width,
        height,
    }
}

/// The relative luminance of `color`, of its components as they are, which
/// tells the darker of two colors.
fn luminance(color: Color) -> f32 {
    0.2126 * color.red + 0.7152 * color.green + 0.0722 * color.blue
}

#[cfg(test)]
mod tests {
    use lokyr_vt_core::page::{self, Rgb, Style};

    use super::*;

    fn row(text: &str, runs: Vec<StyleRun>) -> CellRow {
        CellRow {
            text: text.to_owned(),
            runs,
        }
    }

    fn run(first: u16, last: u16, style: TextStyle) -> StyleRun {
        StyleRun { first, last, style }
    }

    fn made(rows: &[CellRow]) -> Terminal {
        terminal(rows, (None, None), Widths::default(), Colors::default())
    }

    /// The terminal's columns, and the text of each of its rows.
    fn texts(rows: &[CellRow]) -> (u16, Vec<String>) {
        let terminal = made(rows);
        let texts = (0..i64::from(terminal.rows()))
            .map(|row| {
                terminal
                    .line_text(row, 0)
                    .map(|(text, _)| text)
                    .unwrap_or_default()
            })
            .collect();
        (terminal.cols(), texts)
    }

    /// The style of the cell at `column` of `terminal`'s row `row`.
    fn style(terminal: &Terminal, column: u16, row: i64) -> Style {
        terminal
            .row_styles(row)
            .unwrap()
            .into_iter()
            .find(|(first, last, _)| (*first..=*last).contains(&column))
            .unwrap()
            .2
    }

    #[test]
    fn an_image_s_terminal_is_as_wide_as_its_widest_row_of_a_row_each() {
        let (columns, rows) = texts(&[row("ls -la", Vec::new()), row("日本語", Vec::new())]);
        assert_eq!(columns, 6);
        assert_eq!(rows, ["ls -la", "日本語"]);
        // A combining mark joins its cell.
        let (columns, rows) = texts(&[row("e\u{301}tude", Vec::new())]);
        assert_eq!(columns, 5);
        assert_eq!(rows, ["e\u{301}tude"]);
        // No row, or empty ones, two columns at least, as a wide
        // character fits.
        assert_eq!(texts(&[]).0, 2);
        assert_eq!(texts(&[row("", Vec::new())]).0, 2);
        assert_eq!(texts(&[row("x", Vec::new())]).0, 2);
        // Past the widest an image draws, cut.
        let (columns, _) = texts(&[row(&"x".repeat(1200), Vec::new())]);
        assert_eq!(columns, MOST_COLUMNS);
    }

    #[test]
    fn an_image_s_terminal_takes_its_least_columns_and_rows() {
        let rows = [row("ls", Vec::new())];
        let least = terminal(
            &rows,
            (Some(80), Some(10)),
            Widths::default(),
            Colors::default(),
        );
        assert_eq!((least.cols(), least.rows()), (80, 10));
        assert_eq!(least.line_text(0, 0).unwrap().0.trim_end(), "ls");
        // A wider text, or more rows, than the least, theirs.
        let wide = [row("a long line", Vec::new()), row("b", Vec::new())];
        let terminal = terminal(
            &wide,
            (Some(4), Some(1)),
            Widths::default(),
            Colors::default(),
        );
        assert_eq!((terminal.cols(), terminal.rows()), (11, 2));
    }

    #[test]
    fn a_row_s_runs_style_its_cells_a_later_over_an_earlier() {
        let red = TextStyle {
            foreground: Some(Ink::Palette(1)),
            bold: true,
            ..TextStyle::default()
        };
        let curly = TextStyle {
            underline: Some(Underline::Curly),
            underline_color: Some(Ink::Rgb(0, 160, 255)),
            italic: true,
            ..TextStyle::default()
        };
        let rows = [row(
            "abcdef",
            vec![
                run(0, 5, red),
                run(2, 3, curly),
                run(4, 4, TextStyle::default()),
            ],
        )];
        let terminal = made(&rows);
        let style = |column: u16| style(&terminal, column, 0);
        assert!(style(0).bold && style(1).bold);
        assert_eq!(style(0).fg, page::Color::Palette(1));
        assert!(style(2).italic && !style(2).bold);
        assert_eq!(style(3).underline, page::Underline::Curly);
        assert_eq!(
            style(3).underline_color,
            page::Color::Rgb(Rgb {
                r: 0,
                g: 160,
                b: 255
            })
        );
        assert!(!style(4).bold && !style(4).italic);
        assert!(style(5).bold);
    }

    #[test]
    fn a_run_past_the_text_widens_the_row_where_it_shows_something() {
        let bar = TextStyle {
            background: Some(Ink::Rgb(40, 44, 52)),
            ..TextStyle::default()
        };
        let plain = TextStyle::default();
        // A background to the 20th column, as a program's status bar's.
        let rows = [
            row("NORMAL", vec![run(0, 19, bar)]),
            row("x", vec![run(0, 39, plain)]),
        ];
        assert_eq!(texts(&rows).0, 20);
        let terminal = made(&rows);
        assert_eq!(
            style(&terminal, 19, 0).bg,
            page::Color::Rgb(Rgb {
                r: 40,
                g: 44,
                b: 52
            })
        );
        // A later run of the default style hides an earlier background.
        let hidden = [row("x", vec![run(0, 19, bar), run(5, 19, plain)])];
        assert_eq!(texts(&hidden).0, 5);
    }

    #[test]
    fn many_runs_over_one_another_cost_no_more_than_the_row() {
        let runs: Vec<StyleRun> = (0..MOST_COLUMNS)
            .map(|at| {
                run(
                    0,
                    MOST_COLUMNS - 1,
                    TextStyle {
                        bold: at % 2 == 0,
                        ..TextStyle::default()
                    },
                )
            })
            .collect();
        let styles = Styles::of(&runs, MOST_COLUMNS);
        // The last run over every cell.
        assert!(!styles.at(0).bold && !styles.at(MOST_COLUMNS - 1).bold);
        assert_eq!(styles.shown_until(), 0);
    }

    #[test]
    fn the_sgr_of_a_style_gives_its_attributes_and_colors() {
        assert_eq!(sgr(&TextStyle::default()), "\x1b[0m");
        let style = TextStyle {
            foreground: Some(Ink::Palette(196)),
            background: Some(Ink::Rgb(1, 2, 3)),
            underline: Some(Underline::Dashed),
            underline_color: Some(Ink::Palette(4)),
            bold: true,
            faint: true,
            italic: true,
            blink: true,
            inverse: true,
            invisible: true,
            strikethrough: true,
            overline: true,
        };
        assert_eq!(
            sgr(&style),
            "\x1b[0;1;2;3;5;7;8;9;53;4:5;38;5;196;48;2;1;2;3;58;5;4m"
        );
    }
}
