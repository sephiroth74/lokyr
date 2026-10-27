//! The glyphs that the terminal draws itself, against their golden images at
//! two cell sizes, and the edges where their lines meet the next cell's.
//!
//! The golden images are text, a character a pixel: `.` uncovered, `#`
//! covered, `o` half covered or more, `+` less. `LOKYR_UPDATE_SPRITES=1`
//! writes them again from the drawing.

use std::path::Path;

use lokyr_text_core::{Bitmap, CellMetrics, sprite};

/// A cell of `width` by `height`, of lines `thickness` thick.
fn cell(width: u32, height: u32, thickness: u32) -> CellMetrics {
    CellMetrics {
        width,
        height,
        baseline: height * 3 / 4,
        underline_top: height - 2 * thickness,
        underline_thickness: thickness,
        strikeout_top: height / 2,
        strikeout_thickness: thickness,
    }
}

/// The two cells: of a 13-point font at a scale of 1, and at 2.
const CELLS: [(&str, u32, u32, u32); 2] = [("9x18", 9, 18, 1), ("17x36", 17, 36, 2)];

/// The glyphs of the golden images: lines of each weight, their crossings,
/// corners and junctions, dashes, arcs and diagonals; blocks and shades;
/// braille; and the powerline symbols.
const GOLDEN: &str = "─━│┃┼╋┏┢╬╔╤╓╝╞┄┆╌╭╯╱╳▀▄█▌░▚▟▁▇⣿⠁⡇\u{E0B0}\u{E0B1}\u{E0B2}\u{E0B4}\u{E0B5}\u{E0B8}\u{E0BC}\u{E0B9}";

/// The coverage of `bitmap`, of a cell of `width` by `height`, a line a row.
fn art(bitmap: &Bitmap, width: u32, height: u32) -> String {
    let mut art = String::new();
    for y in 0..height {
        for x in 0..width {
            let value = if bitmap.width == 0 {
                0
            } else {
                bitmap.data[(y * bitmap.width + x) as usize]
            };
            art.push(match value {
                0 => '.',
                255 => '#',
                128..=254 => 'o',
                _ => '+',
            });
        }
        art.push('\n');
    }
    art
}

#[test]
fn the_glyphs_match_their_golden_images() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sprites");
    for (name, width, height, thickness) in CELLS {
        let cell = cell(width, height, thickness);
        let mut drawn = String::new();
        for character in GOLDEN.chars() {
            let bitmap = sprite::draw(character, &cell, true).expect("a glyph drawn");
            drawn.push_str(&format!("U+{:04X}\n", u32::from(character)));
            drawn.push_str(&art(&bitmap, width, height));
        }
        let path = directory.join(format!("{name}.txt"));
        if std::env::var_os("LOKYR_UPDATE_SPRITES").is_some() {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(&path, &drawn).unwrap();
            continue;
        }
        let golden = std::fs::read_to_string(&path).expect("the golden images");
        for (drawn, golden) in drawn.split("U+").zip(golden.split("U+")) {
            assert_eq!(drawn, golden, "{name}");
        }
    }
}

/// The coverage of the pixels along one edge of `bitmap`: its top row, its
/// right column, its bottom row or its left column, of 0 to 3.
fn edge(bitmap: &Bitmap, width: u32, height: u32, side: usize) -> Vec<u8> {
    let at = |x: u32, y: u32| {
        if bitmap.width == 0 {
            0
        } else {
            bitmap.data[(y * bitmap.width + x) as usize]
        }
    };
    match side {
        0 => (0..width).map(|x| at(x, 0)).collect(),
        1 => (0..height).map(|y| at(width - 1, y)).collect(),
        2 => (0..width).map(|x| at(x, height - 1)).collect(),
        _ => (0..height).map(|y| at(0, y)).collect(),
    }
}

/// The box drawing lines' edges are the module's tests'; these are the
/// blocks' and the powerline arrow's.
#[test]
fn blocks_tile_and_an_arrow_meets_its_segment() {
    for (name, width, height, thickness) in CELLS {
        let cell = cell(width, height, thickness);
        let drawn = |character: char| sprite::draw(character, &cell, true).unwrap();
        // A full block fills the cell; the halves tile it.
        let full = drawn('█');
        assert!(full.data.iter().all(|&pixel| pixel == 255), "{name}");
        let (upper, lower) = (drawn('▀'), drawn('▄'));
        let (left, right) = (drawn('▌'), drawn('▐'));
        for at in 0..(width * height) as usize {
            assert_eq!(upper.data[at].max(lower.data[at]), 255, "{name}");
            assert_eq!(left.data[at].max(right.data[at]), 255, "{name}");
        }
        // A powerline arrow's flat side fills its edge's middle, which the
        // segment before meets.
        let arrow = drawn('\u{E0B0}');
        let flat = edge(&arrow, width, height, 3);
        let middle = &flat[height as usize / 8..height as usize * 7 / 8];
        assert!(middle.iter().all(|&pixel| pixel == 255), "{name}: {flat:?}");
    }
}

#[test]
fn without_antialiasing_the_shades_are_patterns() {
    for (name, width, height, thickness) in CELLS {
        let cell = cell(width, height, thickness);
        let pixels = width * height;
        for (shade, quarters) in [('░', 1), ('▒', 2), ('▓', 3)] {
            let bitmap = sprite::draw(shade, &cell, false).unwrap();
            assert!(
                bitmap.data.iter().all(|&pixel| pixel == 0 || pixel == 255),
                "{name} {shade}"
            );
            let covered = bitmap.data.iter().filter(|&&pixel| pixel == 255).count() as u32;
            // A quarter of the pixels a quarter, within a row's rounding.
            let wanted = pixels * quarters / 4;
            assert!(
                covered.abs_diff(wanted) <= height,
                "{name} {shade}: {covered} of {pixels}"
            );
            // Spread over the cell: every row has pixels covered and not.
            for y in 0..height {
                let row = &bitmap.data[(y * width) as usize..((y + 1) * width) as usize];
                assert!(
                    row.contains(&255) && row.contains(&0),
                    "{name} {shade}, row {y}"
                );
            }
        }
        // ▓ covers what ░ leaves, and ▒ alternates along a row.
        let (light, medium, dark) = (
            sprite::draw('░', &cell, false).unwrap(),
            sprite::draw('▒', &cell, false).unwrap(),
            sprite::draw('▓', &cell, false).unwrap(),
        );
        for at in 0..pixels as usize {
            assert_ne!(light.data[at], dark.data[at], "{name}");
            if (at + 1) % width as usize != 0 {
                assert_ne!(medium.data[at], medium.data[at + 1], "{name}");
            }
        }
        // Antialiased, their coverage is uniform.
        let antialiased = sprite::draw('▒', &cell, true).unwrap();
        assert!(antialiased.data.iter().all(|&pixel| pixel == 128), "{name}");
        // Other glyphs without antialiasing are covered or not.
        let arc = sprite::draw('╭', &cell, false).unwrap();
        assert!(
            arc.data.iter().all(|&pixel| pixel == 0 || pixel == 255),
            "{name}"
        );
    }
}

#[test]
fn the_sets_are_the_glyphs_that_the_terminal_draws() {
    let cell = cell(9, 18, 1);
    for character in [
        '\u{2500}', '\u{257F}', '\u{2580}', '\u{259F}', '\u{2800}', '\u{28FF}',
    ] {
        assert!(sprite::is_box_drawing(character));
        assert!(!sprite::is_powerline(character));
    }
    for character in ['\u{E0B0}', '\u{E0BF}'] {
        assert!(sprite::is_powerline(character));
        assert!(!sprite::is_box_drawing(character));
    }
    for character in [
        '\u{24FF}', '\u{25A0}', '\u{27FF}', '\u{2900}', '\u{E0AF}', '\u{E0C0}', 'a',
    ] {
        assert!(!sprite::is_box_drawing(character) && !sprite::is_powerline(character));
        assert!(sprite::draw(character, &cell, true).is_none());
    }
    // The blank braille pattern draws nothing.
    assert_eq!(sprite::draw('\u{2800}', &cell, true).unwrap().width, 0);
}
