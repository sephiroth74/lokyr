//! The VT420's operations on rectangles of the screen: DECCRA copies one,
//! DECERA and DECFRA erase and fill one, DECSERA erases the cells of one
//! that DECSCA does not protect, and DECRQCRA, among the reports, sums one.
//!
//! A rectangle counts from the screen's top left, or from the margins' in
//! origin mode, and stays within the screen, or the margins. The cursor
//! does not move.

use std::ops::Range;

use super::Terminal;
use crate::page::{Cell, Content};
use crate::parser::Params;

/// A rectangle of the screen: its rows and its columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Area {
    pub(super) rows: Range<u16>,
    pub(super) columns: Range<u16>,
}

impl Terminal {
    /// The rectangle that the parameters from `first` give: its top, left,
    /// bottom and right, all included and counted from 1, the edges by
    /// default, within the bounds. `None` if the bottom is above the top or
    /// the right left of the left.
    pub(super) fn rectangle(&self, params: &Params, first: usize) -> Option<Area> {
        let bounds = self.rectangle_bounds();
        let from = |value: u16, bounds: &Range<u16>| {
            bounds
                .start
                .saturating_add(value.max(1) - 1)
                .min(bounds.end - 1)
        };
        let to = |value: u16, bounds: &Range<u16>| match value {
            0 => bounds.end,
            value => bounds.start.saturating_add(value).min(bounds.end),
        };
        let rows =
            from(params.value(first), &bounds.rows)..to(params.value(first + 2), &bounds.rows);
        let columns = from(params.value(first + 1), &bounds.columns)
            ..to(params.value(first + 3), &bounds.columns);
        (!rows.is_empty() && !columns.is_empty()).then_some(Area { rows, columns })
    }

    /// Where rectangles stay: the margins in origin mode, the screen
    /// otherwise.
    fn rectangle_bounds(&self) -> Area {
        if self.origin {
            let margins = self.margins;
            Area {
                rows: margins.top..margins.bottom + 1,
                columns: margins.left..margins.right + 1,
            }
        } else {
            Area {
                rows: 0..self.rows,
                columns: 0..self.cols,
            }
        }
    }

    /// DECCRA, `CSI Pts ; Pls ; Pbs ; Prs ; Pps ; Ptd ; Pld ; Ppd $ v`:
    /// copies the rectangle from `Pts`, `Pls` to `Pbs`, `Prs` so that its
    /// top left goes to row `Ptd` and column `Pld`, 1 by default, with the
    /// cells' attributes, as through a buffer, so that the two can overlap.
    /// What would go past the bounds is left out. The pages, `Pps` and
    /// `Ppd`, are the screen's only one.
    pub(super) fn copy_rectangle(&mut self, params: &Params) {
        let Some(source) = self.rectangle(params, 0) else {
            return;
        };
        let bounds = self.rectangle_bounds();
        let at = |value: u16, bounds: &Range<u16>| bounds.start.saturating_add(value.max(1) - 1);
        let (y, x) = (
            at(params.value(5), &bounds.rows),
            at(params.value(6), &bounds.columns),
        );
        if y >= bounds.rows.end || x >= bounds.columns.end {
            return;
        }
        let height = (source.rows.end - source.rows.start).min(bounds.rows.end - y);
        let width = (source.columns.end - source.columns.start).min(bounds.columns.end - x);
        self.moved();
        let rows = source.rows.start..source.rows.start + height;
        let columns = source.columns.start..source.columns.start + width;
        self.screen_mut().rows.copy_rectangle(rows, columns, x, y);
    }

    /// DECERA, `CSI Pt ; Pl ; Pb ; Pr $ z`, and DECSERA, `CSI Pt ; Pl ;
    /// Pb ; Pr $ {` if `selective`: erases the rectangle, as ED erases, but
    /// for the cells that DECSCA protected if `selective`.
    pub(super) fn erase_rectangle(&mut self, params: &Params, selective: bool) {
        let Some(area) = self.rectangle(params, 0) else {
            return;
        };
        self.moved();
        for y in area.rows {
            self.erase_in_row(y, area.columns.clone(), selective);
        }
    }

    /// DECFRA, `CSI Pch ; Pt ; Pl ; Pb ; Pr $ x`: fills the rectangle with
    /// the character whose code is `Pch`, in the pen's style. The code is of
    /// a graphic character of ASCII or of Latin-1, 32 to 126 or 160 to 255;
    /// others fill nothing.
    pub(super) fn fill_rectangle(&mut self, params: &Params) {
        let code = params.value(0);
        let Ok(byte) = u8::try_from(code) else {
            return;
        };
        if !matches!(byte, 0x20..=0x7E | 0xA0..=0xFF) {
            return;
        }
        let Some(area) = self.rectangle(params, 1) else {
            return;
        };
        self.moved();
        let width = area.columns.end - area.columns.start;
        let x = area.columns.start;
        for y in area.rows {
            self.with_pen(y, |page, row, style| {
                page.clear_halves(row, x, width);
                let template = Cell::EMPTY.with_style(style);
                if byte.is_ascii() {
                    page.write_ascii(row, x, &vec![byte; usize::from(width)], template);
                } else {
                    let cell = template.with_content(Content::Char(char::from(byte)));
                    for x in area.columns.clone() {
                        page.set_cell(row, x, cell);
                    }
                }
            });
        }
    }
}
