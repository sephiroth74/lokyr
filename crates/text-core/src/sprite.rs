//! Glyphs that the terminal draws itself, at the cell's size and to the
//! pixel, rather than takes from the font: box drawing, the block elements
//! and braille, so that lines meet across cells and blocks tile without a
//! gap, and the powerline symbols, so that a prompt's segments meet their
//! neighbours' backgrounds.
//!
//! A light line takes the face's underline thickness, a heavy one twice it,
//! and a double line two light ones a light one apart. A line runs through
//! the cell's middle, rounded down, so that every cell puts it in the same
//! pixels.

use crate::backend::{Bitmap, BitmapFormat};
use crate::cell::CellMetrics;

/// Whether the terminal draws `character` itself as box drawing: the box
/// drawing characters, the block elements and braille.
pub fn is_box_drawing(character: char) -> bool {
    matches!(character, '\u{2500}'..='\u{259F}' | '\u{2800}'..='\u{28FF}')
}

/// Whether the terminal draws `character` itself as a powerline symbol: the
/// arrows, half circles and triangles of U+E0B0 to U+E0BF.
pub fn is_powerline(character: char) -> bool {
    matches!(character, '\u{E0B0}'..='\u{E0BF}')
}

/// The glyph of `character`, which [`is_box_drawing`] or [`is_powerline`]
/// covers, drawn in a cell of `cell`: a bitmap of coverage of the cell's
/// size, its top on the cell's top. `None` for another character.
pub fn draw(character: char, cell: &CellMetrics) -> Option<Bitmap> {
    let mut canvas = Canvas::new(cell);
    match u32::from(character) {
        0x2500..=0x257F => canvas.box_drawing(character),
        0x2580..=0x259F => canvas.block(character),
        0x2800..=0x28FF => canvas.braille(character),
        0xE0B0..=0xE0BF => canvas.powerline(character),
        _ => return None,
    }
    Some(canvas.into_bitmap(cell))
}

/// The weight of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weight {
    None,
    Light,
    Heavy,
    Double,
}

/// The four arms of each box drawing character, from U+2500, as its
/// Unicode name gives them: up, right, down and left, each `_` for none,
/// `L` light, `H` heavy or `D` double; `*` for one drawn otherwise, a dash,
/// an arc or a diagonal.
const ARMS: [[u8; 4]; 128] = [
    *b"_L_L", // ─ 2500 light horizontal
    *b"_H_H", // ━ 2501 heavy horizontal
    *b"L_L_", // │ 2502 light vertical
    *b"H_H_", // ┃ 2503 heavy vertical
    *b"****", // ┄ 2504 light triple dash horizontal
    *b"****", // ┅ 2505 heavy triple dash horizontal
    *b"****", // ┆ 2506 light triple dash vertical
    *b"****", // ┇ 2507 heavy triple dash vertical
    *b"****", // ┈ 2508 light quadruple dash horizontal
    *b"****", // ┉ 2509 heavy quadruple dash horizontal
    *b"****", // ┊ 250A light quadruple dash vertical
    *b"****", // ┋ 250B heavy quadruple dash vertical
    *b"_LL_", // ┌ 250C light down and right
    *b"_HL_", // ┍ 250D down light and right heavy
    *b"_LH_", // ┎ 250E down heavy and right light
    *b"_HH_", // ┏ 250F heavy down and right
    *b"__LL", // ┐ 2510 light down and left
    *b"__LH", // ┑ 2511 down light and left heavy
    *b"__HL", // ┒ 2512 down heavy and left light
    *b"__HH", // ┓ 2513 heavy down and left
    *b"LL__", // └ 2514 light up and right
    *b"LH__", // ┕ 2515 up light and right heavy
    *b"HL__", // ┖ 2516 up heavy and right light
    *b"HH__", // ┗ 2517 heavy up and right
    *b"L__L", // ┘ 2518 light up and left
    *b"L__H", // ┙ 2519 up light and left heavy
    *b"H__L", // ┚ 251A up heavy and left light
    *b"H__H", // ┛ 251B heavy up and left
    *b"LLL_", // ├ 251C light vertical and right
    *b"LHL_", // ┝ 251D vertical light and right heavy
    *b"HLL_", // ┞ 251E up heavy and right down light
    *b"LLH_", // ┟ 251F down heavy and right up light
    *b"HLH_", // ┠ 2520 vertical heavy and right light
    *b"HHL_", // ┡ 2521 down light and right up heavy
    *b"LHH_", // ┢ 2522 up light and right down heavy
    *b"HHH_", // ┣ 2523 heavy vertical and right
    *b"L_LL", // ┤ 2524 light vertical and left
    *b"L_LH", // ┥ 2525 vertical light and left heavy
    *b"H_LL", // ┦ 2526 up heavy and left down light
    *b"L_HL", // ┧ 2527 down heavy and left up light
    *b"H_HL", // ┨ 2528 vertical heavy and left light
    *b"H_LH", // ┩ 2529 down light and left up heavy
    *b"L_HH", // ┪ 252A up light and left down heavy
    *b"H_HH", // ┫ 252B heavy vertical and left
    *b"_LLL", // ┬ 252C light down and horizontal
    *b"_LLH", // ┭ 252D left heavy and right down light
    *b"_HLL", // ┮ 252E right heavy and left down light
    *b"_HLH", // ┯ 252F down light and horizontal heavy
    *b"_LHL", // ┰ 2530 down heavy and horizontal light
    *b"_LHH", // ┱ 2531 right light and left down heavy
    *b"_HHL", // ┲ 2532 left light and right down heavy
    *b"_HHH", // ┳ 2533 heavy down and horizontal
    *b"LL_L", // ┴ 2534 light up and horizontal
    *b"LL_H", // ┵ 2535 left heavy and right up light
    *b"LH_L", // ┶ 2536 right heavy and left up light
    *b"LH_H", // ┷ 2537 up light and horizontal heavy
    *b"HL_L", // ┸ 2538 up heavy and horizontal light
    *b"HL_H", // ┹ 2539 right light and left up heavy
    *b"HH_L", // ┺ 253A left light and right up heavy
    *b"HH_H", // ┻ 253B heavy up and horizontal
    *b"LLLL", // ┼ 253C light vertical and horizontal
    *b"LLLH", // ┽ 253D left heavy and right vertical light
    *b"LHLL", // ┾ 253E right heavy and left vertical light
    *b"LHLH", // ┿ 253F vertical light and horizontal heavy
    *b"HLLL", // ╀ 2540 up heavy and down horizontal light
    *b"LLHL", // ╁ 2541 down heavy and up horizontal light
    *b"HLHL", // ╂ 2542 vertical heavy and horizontal light
    *b"HLLH", // ╃ 2543 left up heavy and right down light
    *b"HHLL", // ╄ 2544 right up heavy and left down light
    *b"LLHH", // ╅ 2545 left down heavy and right up light
    *b"LHHL", // ╆ 2546 right down heavy and left up light
    *b"HHLH", // ╇ 2547 down light and up horizontal heavy
    *b"LHHH", // ╈ 2548 up light and down horizontal heavy
    *b"HLHH", // ╉ 2549 right light and left vertical heavy
    *b"HHHL", // ╊ 254A left light and right vertical heavy
    *b"HHHH", // ╋ 254B heavy vertical and horizontal
    *b"****", // ╌ 254C light double dash horizontal
    *b"****", // ╍ 254D heavy double dash horizontal
    *b"****", // ╎ 254E light double dash vertical
    *b"****", // ╏ 254F heavy double dash vertical
    *b"_D_D", // ═ 2550 double horizontal
    *b"D_D_", // ║ 2551 double vertical
    *b"_DL_", // ╒ 2552 down single and right double
    *b"_LD_", // ╓ 2553 down double and right single
    *b"_DD_", // ╔ 2554 double down and right
    *b"__LD", // ╕ 2555 down single and left double
    *b"__DL", // ╖ 2556 down double and left single
    *b"__DD", // ╗ 2557 double down and left
    *b"LD__", // ╘ 2558 up single and right double
    *b"DL__", // ╙ 2559 up double and right single
    *b"DD__", // ╚ 255A double up and right
    *b"L__D", // ╛ 255B up single and left double
    *b"D__L", // ╜ 255C up double and left single
    *b"D__D", // ╝ 255D double up and left
    *b"LDL_", // ╞ 255E vertical single and right double
    *b"DLD_", // ╟ 255F vertical double and right single
    *b"DDD_", // ╠ 2560 double vertical and right
    *b"L_LD", // ╡ 2561 vertical single and left double
    *b"D_DL", // ╢ 2562 vertical double and left single
    *b"D_DD", // ╣ 2563 double vertical and left
    *b"_DLD", // ╤ 2564 down single and horizontal double
    *b"_LDL", // ╥ 2565 down double and horizontal single
    *b"_DDD", // ╦ 2566 double down and horizontal
    *b"LD_D", // ╧ 2567 up single and horizontal double
    *b"DL_L", // ╨ 2568 up double and horizontal single
    *b"DD_D", // ╩ 2569 double up and horizontal
    *b"LDLD", // ╪ 256A vertical single and horizontal double
    *b"DLDL", // ╫ 256B vertical double and horizontal single
    *b"DDDD", // ╬ 256C double vertical and horizontal
    *b"****", // ╭ 256D light arc down and right
    *b"****", // ╮ 256E light arc down and left
    *b"****", // ╯ 256F light arc up and left
    *b"****", // ╰ 2570 light arc up and right
    *b"****", // ╱ 2571 light diagonal upper right to lower left
    *b"****", // ╲ 2572 light diagonal upper left to lower right
    *b"****", // ╳ 2573 light diagonal cross
    *b"___L", // ╴ 2574 light left
    *b"L___", // ╵ 2575 light up
    *b"_L__", // ╶ 2576 light right
    *b"__L_", // ╷ 2577 light down
    *b"___H", // ╸ 2578 heavy left
    *b"H___", // ╹ 2579 heavy up
    *b"_H__", // ╺ 257A heavy right
    *b"__H_", // ╻ 257B heavy down
    *b"_H_L", // ╼ 257C light left and heavy right
    *b"L_H_", // ╽ 257D light up and heavy down
    *b"_L_H", // ╾ 257E heavy left and light right
    *b"H_L_", // ╿ 257F heavy up and light down
];

/// The arms' directions, in the order of [`ARMS`].
const UP: usize = 0;
const RIGHT: usize = 1;
const DOWN: usize = 2;
const LEFT: usize = 3;

/// The samples of a pixel's side, for the shapes that are not rectangles:
/// a pixel's coverage is the share of its samples inside.
const SAMPLES: u32 = 4;

/// The pixels of a glyph being drawn, of coverage.
struct Canvas {
    width: u32,
    height: u32,
    /// A light line's thickness.
    light: u32,
    data: Vec<u8>,
}

impl Canvas {
    fn new(cell: &CellMetrics) -> Self {
        let (width, height) = (cell.width.max(1), cell.height.max(1));
        Self {
            width,
            height,
            light: cell.underline_thickness.max(1),
            data: vec![0; width as usize * height as usize],
        }
    }

    /// The thickness of a line of `weight`, a double one's of each of its
    /// lines.
    fn thickness(&self, weight: Weight) -> u32 {
        match weight {
            Weight::Heavy => self.light * 2,
            _ => self.light,
        }
    }

    /// Covers the pixels from `x0` to `x1` and from `y0` to `y1`, ends
    /// excluded, with `value` where it covers them more.
    fn rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, value: u8) {
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        for y in y0.max(0)..y1.min(height) {
            for x in x0.max(0)..x1.min(width) {
                let pixel = &mut self.data[(y * width + x) as usize];
                *pixel = (*pixel).max(value);
            }
        }
    }

    /// Covers each pixel by the share of its samples that `inside` takes,
    /// at points in pixels from the cell's top left corner.
    fn cover(&mut self, inside: impl Fn(f64, f64) -> bool) {
        let width = self.width as usize;
        let samples = f64::from(SAMPLES);
        for y in 0..self.height {
            for x in 0..self.width {
                let mut count = 0;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let px = f64::from(x) + (f64::from(sx) + 0.5) / samples;
                        let py = f64::from(y) + (f64::from(sy) + 0.5) / samples;
                        count += u32::from(inside(px, py));
                    }
                }
                let value = (count * 255 / (SAMPLES * SAMPLES)) as u8;
                let pixel = &mut self.data[y as usize * width + x as usize];
                *pixel = (*pixel).max(value);
            }
        }
    }

    /// The bitmap of the canvas, its top on the cell's top: empty when it
    /// covers nothing.
    fn into_bitmap(self, cell: &CellMetrics) -> Bitmap {
        if self.data.iter().all(|&pixel| pixel == 0) {
            return Bitmap::empty(BitmapFormat::Gray);
        }
        Bitmap {
            format: BitmapFormat::Gray,
            width: self.width,
            height: self.height,
            left: 0,
            top: i32::try_from(cell.baseline).unwrap_or(i32::MAX),
            data: self.data,
        }
    }

    /// Draws a box drawing character, U+2500 to U+257F.
    fn box_drawing(&mut self, character: char) {
        let index = (u32::from(character) - 0x2500) as usize;
        let arms = ARMS[index].map(|arm| match arm {
            b'L' => Weight::Light,
            b'H' => Weight::Heavy,
            b'D' => Weight::Double,
            _ => Weight::None,
        });
        if ARMS[index] == *b"****" {
            match character {
                '\u{2504}'..='\u{250B}' | '\u{254C}'..='\u{254F}' => self.dashes(character),
                '\u{256D}'..='\u{2570}' => self.arc(character),
                _ => self.diagonals(character),
            }
            return;
        }
        for direction in [UP, RIGHT, DOWN, LEFT] {
            match arms[direction] {
                Weight::None => {}
                Weight::Double => self.double_arm(direction, arms),
                weight => self.arm(direction, weight, arms),
            }
        }
    }

    /// The cell's length along the axis of an arm of `direction`, and across
    /// it.
    fn lengths(&self, direction: usize) -> (u32, u32) {
        if direction == UP || direction == DOWN {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// Covers an arm's rectangle, `along` its axis and `across` it.
    fn fill(&mut self, direction: usize, along: (i64, i64), across: (i64, i64)) {
        if direction == UP || direction == DOWN {
            self.rect(across.0, along.0, across.1, along.1, 255);
        } else {
            self.rect(along.0, across.0, along.1, across.1, 255);
        }
    }

    /// The pixels from the edge of an arm of `direction` to `stop`, from
    /// the cell's top or left edge, along a cell of `length`: up and left
    /// run from the start to `stop.1`, down and right from `stop.0` to the
    /// end.
    fn reach(direction: usize, stop: (i64, i64), length: u32) -> (i64, i64) {
        if direction == UP || direction == LEFT {
            (0, stop.1)
        } else {
            (stop.0, i64::from(length))
        }
    }

    /// The arms across the axis of an arm of `direction`, as its two sides:
    /// left and right of a vertical arm, up and down of a horizontal one.
    fn sides(direction: usize) -> [usize; 2] {
        if direction == UP || direction == DOWN {
            [LEFT, RIGHT]
        } else {
            [UP, DOWN]
        }
    }

    /// The band, along `length`, of a line of `weight`: a double line's two.
    fn bands(&self, length: u32, weight: Weight) -> [(i64, i64); 2] {
        if weight == Weight::Double {
            let light = i64::from(self.light);
            let start = (i64::from(length) - 3 * light).max(0) / 2;
            [
                (start, start + light),
                (start + 2 * light, start + 3 * light),
            ]
        } else {
            let thickness = i64::from(self.thickness(weight));
            let start = (i64::from(length) - thickness).max(0) / 2;
            [(start, start + thickness); 2]
        }
    }

    /// Draws a light or heavy arm. It runs through the middle to the
    /// opposite arm; to a double line across it that runs through, it stops
    /// at the nearer line; to a double arm across it on one side, it goes on
    /// to the farther line, which it caps; and else to the far side of its
    /// own band, which covers the lines across it.
    fn arm(&mut self, direction: usize, weight: Weight, arms: [Weight; 4]) {
        let (along, across) = self.lengths(direction);
        let opposite = arms[(direction + 2) % 4];
        let doubles = Self::sides(direction)
            .into_iter()
            .filter(|&side| arms[side] == Weight::Double)
            .count();
        let own = self.bands(along, weight)[0];
        let lines = self.bands(along, Weight::Double);
        let (near, far) = if direction == UP || direction == LEFT {
            (lines[0], lines[1])
        } else {
            (lines[1], lines[0])
        };
        let stop = match (opposite, doubles) {
            (Weight::None, 2) => near,
            (Weight::None, 1) => far,
            _ => own,
        };
        let reach = Self::reach(direction, stop, along);
        self.fill(direction, reach, self.bands(across, weight)[0]);
    }

    /// Draws a double arm, line by line. A line stops at an arm across it on
    /// its own side, at that arm's nearer line when it is double; runs
    /// through the middle to the opposite arm; or else goes on to an arm
    /// across it on the other side, to its farther line, as a corner's
    /// outer line does.
    fn double_arm(&mut self, direction: usize, arms: [Weight; 4]) {
        let (along, across) = self.lengths(direction);
        let opposite = arms[(direction + 2) % 4];
        let sides = Self::sides(direction);
        let lines = self.bands(across, Weight::Double);
        let from_start = direction == UP || direction == LEFT;
        for (line, band) in lines.into_iter().enumerate() {
            let (own, other) = (arms[sides[line]], arms[sides[1 - line]]);
            let stop = if own != Weight::None {
                let bands = self.bands(along, own);
                if from_start { bands[0] } else { bands[1] }
            } else if opposite != Weight::None {
                let middle = i64::from(along) / 2;
                (middle, i64::from(along) - middle)
            } else if other != Weight::None {
                let bands = self.bands(along, other);
                if from_start { bands[1] } else { bands[0] }
            } else {
                let middle = i64::from(along) / 2;
                (middle, middle)
            };
            let reach = Self::reach(direction, stop, along);
            self.fill(direction, reach, band);
        }
    }

    /// Draws a dashed line: two, three or four dashes, light or heavy.
    fn dashes(&mut self, character: char) {
        let (count, weight, vertical) = match character {
            '\u{2504}' => (3, Weight::Light, false),
            '\u{2505}' => (3, Weight::Heavy, false),
            '\u{2506}' => (3, Weight::Light, true),
            '\u{2507}' => (3, Weight::Heavy, true),
            '\u{2508}' => (4, Weight::Light, false),
            '\u{2509}' => (4, Weight::Heavy, false),
            '\u{250A}' => (4, Weight::Light, true),
            '\u{250B}' => (4, Weight::Heavy, true),
            '\u{254C}' => (2, Weight::Light, false),
            '\u{254D}' => (2, Weight::Heavy, false),
            '\u{254E}' => (2, Weight::Light, true),
            _ => (2, Weight::Heavy, true),
        };
        let direction = if vertical { DOWN } else { RIGHT };
        let (along, across) = self.lengths(direction);
        let band = self.bands(across, weight)[0];
        let part = f64::from(along) / f64::from(count);
        let gap = (part * 0.3).round().max(1.0);
        for dash in 0..count {
            let start = (f64::from(dash) * part + gap / 2.0).round() as i64;
            let end = (f64::from(dash + 1) * part - gap / 2.0).round() as i64;
            self.fill(direction, (start, end.max(start + 1)), band);
        }
    }

    /// Draws a light arc, from the middle of one edge to the middle of
    /// another, as a quarter circle between the lines' bands, which stay in
    /// the pixels of the straight lines' bands.
    fn arc(&mut self, character: char) {
        // Which way the arc's lines go: right or left, down or up.
        let (right, down) = match character {
            '\u{256D}' => (true, true),
            '\u{256E}' => (false, true),
            '\u{256F}' => (false, false),
            _ => (true, false),
        };
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        let half = f64::from(self.light) / 2.0;
        // The lines' middles, as the light lines' bands put them.
        let x_band = self.bands(self.width, Weight::Light)[0];
        let y_band = self.bands(self.height, Weight::Light)[0];
        let (mx, my) = (x_band.0 as f64 + half, y_band.0 as f64 + half);
        let (sx, sy) = (
            if right { 1.0 } else { -1.0 },
            if down { 1.0 } else { -1.0 },
        );
        let room_x = if right { width - mx } else { mx };
        let room_y = if down { height - my } else { my };
        // The lines run straight for a line's thickness at the edges, so
        // that the curve does not touch the edge's next pixels.
        let radius = (room_x.min(room_y) - f64::from(self.light)).max(half);
        let (cx, cy) = (mx + sx * radius, my + sy * radius);
        self.cover(|x, y| {
            if (x - cx) * sx >= 0.0 {
                // The line on to the side edge.
                (y - my).abs() <= half
            } else if (y - cy) * sy >= 0.0 {
                // The line on to the top or bottom edge.
                (x - mx).abs() <= half
            } else {
                let distance = (x - cx).hypot(y - cy);
                (distance - radius).abs() <= half
            }
        });
    }

    /// Draws the light diagonals, from corner to corner.
    fn diagonals(&mut self, character: char) {
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        let half = f64::from(self.light) / 2.0;
        let rising = matches!(character, '\u{2571}' | '\u{2573}');
        let falling = matches!(character, '\u{2572}' | '\u{2573}');
        let length = width.hypot(height);
        self.cover(|x, y| {
            // The distances from the lines through the corners.
            let falls = (height * x - width * y).abs() / length;
            let rises = (height * x + width * y - width * height).abs() / length;
            (falling && falls <= half) || (rising && rises <= half)
        });
    }

    /// Draws a block element, U+2580 to U+259F.
    fn block(&mut self, character: char) {
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let eighths = |length: i64, count: i64| (length * count * 2 + 8) / 16;
        let (mid_x, mid_y) = (eighths(width, 4), eighths(height, 4));
        let quadrants = |canvas: &mut Self, upper_left, upper_right, lower_left, lower_right| {
            for (on, x0, y0, x1, y1) in [
                (upper_left, 0, 0, mid_x, mid_y),
                (upper_right, mid_x, 0, width, mid_y),
                (lower_left, 0, mid_y, mid_x, height),
                (lower_right, mid_x, mid_y, width, height),
            ] {
                if on {
                    canvas.rect(x0, y0, x1, y1, 255);
                }
            }
        };
        match u32::from(character) {
            0x2580 => self.rect(0, 0, width, mid_y, 255),
            code @ 0x2581..=0x2588 => {
                let rows = eighths(height, i64::from(code - 0x2580));
                self.rect(0, height - rows, width, height, 255);
            }
            code @ 0x2589..=0x258F => {
                let columns = eighths(width, i64::from(0x2590 - code));
                self.rect(0, 0, columns, height, 255);
            }
            0x2590 => self.rect(mid_x, 0, width, height, 255),
            0x2591 => self.rect(0, 0, width, height, 64),
            0x2592 => self.rect(0, 0, width, height, 128),
            0x2593 => self.rect(0, 0, width, height, 191),
            0x2594 => self.rect(0, 0, width, eighths(height, 1), 255),
            0x2595 => self.rect(width - eighths(width, 1), 0, width, height, 255),
            0x2596 => quadrants(self, false, false, true, false),
            0x2597 => quadrants(self, false, false, false, true),
            0x2598 => quadrants(self, true, false, false, false),
            0x2599 => quadrants(self, true, false, true, true),
            0x259A => quadrants(self, true, false, false, true),
            0x259B => quadrants(self, true, true, true, false),
            0x259C => quadrants(self, true, true, false, true),
            0x259D => quadrants(self, false, true, false, false),
            0x259E => quadrants(self, false, true, true, false),
            _ => quadrants(self, false, true, true, true),
        }
    }

    /// Draws a braille pattern, U+2800 to U+28FF: its eight dots, two
    /// columns of four, as its bits give them.
    fn braille(&mut self, character: char) {
        let bits = u32::from(character) - 0x2800;
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        let side = ((width / 2.0).min(height / 4.0) * 0.5).round().max(1.0);
        // The dots by their bits: 1, 2, 3 and 7 down the left column, 4, 5,
        // 6 and 8 down the right one.
        const DOTS: [(u32, u32); 8] = [
            (0, 0),
            (0, 1),
            (0, 2),
            (1, 0),
            (1, 1),
            (1, 2),
            (0, 3),
            (1, 3),
        ];
        for (bit, (column, row)) in DOTS.into_iter().enumerate() {
            if bits & (1 << bit) == 0 {
                continue;
            }
            let x = (width * f64::from(2 * column + 1) / 4.0 - side / 2.0).round() as i64;
            let y = (height * f64::from(2 * row + 1) / 8.0 - side / 2.0).round() as i64;
            let side = side as i64;
            self.rect(x, y, x + side, y + side, 255);
        }
    }

    /// Draws a powerline symbol, U+E0B0 to U+E0BF.
    fn powerline(&mut self, character: char) {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let half = f64::from(self.light) / 2.0;
        match character {
            '\u{E0B0}' => self.cover(|x, y| x <= w * (1.0 - (2.0 * y / h - 1.0).abs())),
            '\u{E0B2}' => self.cover(|x, y| w - x <= w * (1.0 - (2.0 * y / h - 1.0).abs())),
            '\u{E0B1}' | '\u{E0B3}' => {
                let left = character == '\u{E0B1}';
                self.cover(|x, y| {
                    let x = if left { x } else { w - x };
                    let tip = (w, h / 2.0);
                    segment_distance((x, y), (0.0, 0.0), tip) <= half
                        || segment_distance((x, y), tip, (0.0, h)) <= half
                });
            }
            '\u{E0B4}' | '\u{E0B6}' => {
                let right = character == '\u{E0B4}';
                self.cover(|x, y| {
                    let x = if right { x } else { w - x };
                    (x / w).powi(2) + ((y - h / 2.0) / (h / 2.0)).powi(2) <= 1.0
                });
            }
            '\u{E0B5}' | '\u{E0B7}' => {
                let right = character == '\u{E0B5}';
                let scale = w.min(h / 2.0);
                self.cover(|x, y| {
                    let x = if right { x } else { w - x };
                    let rho = ((x / w).powi(2) + ((y - h / 2.0) / (h / 2.0)).powi(2)).sqrt();
                    ((rho - 1.0) * scale).abs() <= half
                });
            }
            // The triangles of a corner, and the lines of their diagonals.
            '\u{E0B8}' => self.cover(|x, y| x / w <= y / h),
            '\u{E0BA}' => self.cover(|x, y| x / w >= 1.0 - y / h),
            '\u{E0BC}' => self.cover(|x, y| x / w <= 1.0 - y / h),
            '\u{E0BE}' => self.cover(|x, y| x / w >= y / h),
            '\u{E0B9}' | '\u{E0BF}' => {
                self.cover(|x, y| segment_distance((x, y), (0.0, 0.0), (w, h)) <= half);
            }
            _ => self.cover(|x, y| segment_distance((x, y), (0.0, h), (w, 0.0)) <= half),
        }
    }
}

/// The distance of `point` from the segment from `a` to `b`.
fn segment_distance(point: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((point.0 - a.0) * dx + (point.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    };
    (point.0 - a.0 - t * dx).hypot(point.1 - a.1 - t * dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The coverage of one edge of the glyph of `character` in `cell`: its
    /// top row, right column, bottom row or left column, by the direction.
    fn edge(character: char, cell: &CellMetrics, direction: usize) -> Vec<u8> {
        let bitmap = draw(character, cell).unwrap();
        let (width, height) = (cell.width, cell.height);
        let at = |x: u32, y: u32| {
            if bitmap.width == 0 {
                0
            } else {
                bitmap.data[(y * bitmap.width + x) as usize]
            }
        };
        match direction {
            UP => (0..width).map(|x| at(x, 0)).collect(),
            RIGHT => (0..height).map(|y| at(width - 1, y)).collect(),
            DOWN => (0..width).map(|x| at(x, height - 1)).collect(),
            _ => (0..height).map(|y| at(0, y)).collect(),
        }
    }

    #[test]
    fn every_arm_meets_the_next_cell_s_line_of_its_weight() {
        for (width, height, thickness) in [(9, 18, 1), (17, 36, 2), (8, 17, 1), (16, 33, 3)] {
            let cell = CellMetrics {
                width,
                height,
                baseline: height * 3 / 4,
                underline_top: height - thickness,
                underline_thickness: thickness,
                strikeout_top: height / 2,
                strikeout_thickness: thickness,
            };
            // The edges of the straight lines of each weight, which every arm
            // of that weight meets.
            let straight = |horizontal: char, vertical: char| {
                [
                    edge(vertical, &cell, UP),
                    edge(horizontal, &cell, RIGHT),
                    edge(vertical, &cell, DOWN),
                    edge(horizontal, &cell, LEFT),
                ]
            };
            let light = straight('─', '│');
            let heavy = straight('━', '┃');
            let double = straight('═', '║');
            for (index, arms) in ARMS.iter().enumerate() {
                let character = char::from_u32(0x2500 + index as u32).unwrap();
                let arms = match arms {
                    // An arc meets the light lines of its two edges.
                    b"****" if ('\u{256D}'..='\u{2570}').contains(&character) => match character {
                        '\u{256D}' => *b"_LL_",
                        '\u{256E}' => *b"__LL",
                        '\u{256F}' => *b"L__L",
                        _ => *b"LL__",
                    },
                    b"****" => continue,
                    arms => *arms,
                };
                for direction in [UP, RIGHT, DOWN, LEFT] {
                    let expected = match arms[direction] {
                        b'L' => light[direction].clone(),
                        b'H' => heavy[direction].clone(),
                        b'D' => double[direction].clone(),
                        _ => vec![0; edge(character, &cell, direction).len()],
                    };
                    assert_eq!(
                        edge(character, &cell, direction),
                        expected,
                        "{width}x{height}: {character} U+{:04X}, direction {direction}",
                        u32::from(character)
                    );
                }
            }
        }
    }
}
