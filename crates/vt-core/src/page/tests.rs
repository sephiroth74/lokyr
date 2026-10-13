//! Pages: their size, their rows, and their tables as cells use them.

use super::*;

/// The widths that the tests try: from one column to a very wide terminal.
const WIDTHS: [u16; 7] = [1, 2, 80, 132, 200, 500, 1000];

/// A bold style in palette color `fg`.
fn bold(fg: u8) -> Style {
    Style {
        fg: Color::Palette(fg),
        bold: true,
        ..Style::DEFAULT
    }
}

/// A style that differs for every `n`.
fn nth_style(n: u32) -> Style {
    let [_, r, g, b] = n.to_be_bytes();
    Style {
        fg: Color::Rgb(Rgb::new(r, g, b)),
        ..Style::DEFAULT
    }
}

/// Writes `c` in `style` at column `x` of row `y`, as the terminal will,
/// with its pen: the pen acquires the style, the cell takes its own
/// reference, and the pen lets go.
fn print(page: &mut Page, y: u16, x: u16, c: char, style: &Style) -> Result<(), TableFull> {
    let id = page.styles_mut().acquire(style)?;
    page.set_cell(
        y,
        x,
        Cell::EMPTY.with_content(Content::Char(c)).with_style(id),
    );
    page.styles_mut().release(id);
    Ok(())
}

#[test]
fn a_standard_page_fills_its_block() {
    for cols in WIDTHS {
        let capacity = Capacity::standard(cols);
        let page = Page::new(capacity);
        assert_eq!(page.memory(), capacity.bytes(), "{cols} columns");
        assert!(page.memory() <= PAGE_BYTES, "{cols} columns");
        let one_more = Capacity {
            rows: capacity.rows + 1,
            ..capacity
        };
        assert!(
            one_more.bytes() > PAGE_BYTES,
            "{cols} columns: a row more would fit"
        );
    }
}

#[test]
fn a_row_too_wide_for_the_block_gets_a_page_of_its_own() {
    let capacity = Capacity::standard(u16::MAX);
    assert_eq!(capacity.rows, 1);
    assert!(capacity.bytes() > PAGE_BYTES);
}

#[test]
fn rows_come_into_use_one_after_the_other() {
    let mut page = Page::new(Capacity::standard(80));
    assert!(page.is_empty());
    for y in 0..page.capacity().rows {
        assert_eq!(page.push_row(), Some(y));
        assert!(page.cells(y).iter().all(|cell| cell.is_empty()));
    }
    assert!(page.is_full());
    assert_eq!(page.push_row(), None);
}

#[test]
fn cells_hold_references_to_their_styles() {
    let mut page = Page::new(Capacity::standard(80));
    page.push_row();
    print(&mut page, 0, 0, 'a', &bold(1)).unwrap();
    print(&mut page, 0, 1, 'b', &bold(1)).unwrap();
    print(&mut page, 0, 2, 'c', &bold(2)).unwrap();
    let red = page.cells(0)[0].style();
    assert_eq!(page.cells(0)[1].style(), red);
    assert_eq!(page.styles().references(red), 2);
    assert_eq!(page.styles().len(), 2);

    // Overwriting a cell drops its reference; the last one frees the style.
    print(&mut page, 0, 2, 'd', &bold(1)).unwrap();
    assert_eq!(page.styles().references(red), 3);
    assert_eq!(page.styles().len(), 1);
    print(&mut page, 0, 0, 'e', &Style::DEFAULT).unwrap();
    assert_eq!(page.styles().references(red), 2);
}

#[test]
fn writing_marks_the_row() {
    let mut page = Page::new(Capacity::standard(80));
    page.push_row();
    page.push_row();
    print(&mut page, 0, 0, 'a', &Style::DEFAULT).unwrap();
    print(&mut page, 1, 0, 'b', &bold(1)).unwrap();
    assert!(page.row(0).has(RowFlag::Dirty));
    assert!(!page.row(0).has(RowFlag::Styles));
    assert!(page.row(1).has(RowFlag::Dirty));
    assert!(page.row(1).has(RowFlag::Styles));
}

#[test]
fn clearing_a_row_frees_its_styles_and_resets_it() {
    let mut page = Page::new(Capacity::standard(80));
    page.push_row();
    for x in 0..80 {
        print(&mut page, 0, x, 'x', &bold(x as u8 % 4)).unwrap();
    }
    page.row_mut(0).set(RowFlag::SoftWrapped, true);
    assert_eq!(page.styles().len(), 4);
    page.clear_row(0);
    assert!(page.styles().is_empty());
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
    assert!(!page.row(0).has(RowFlag::SoftWrapped));
    assert!(!page.row(0).has(RowFlag::Styles));
    assert!(page.row(0).has(RowFlag::Dirty));
}

/// A copy of `page` with twice the room for styles, after checking that it
/// keeps the cells and stays within its block.
fn grow(page: &Page) -> Page {
    let capacity = Capacity {
        styles: page.capacity().styles.saturating_mul(2),
        ..page.capacity()
    };
    let grown = page.grown(capacity);
    for y in 0..page.len() {
        assert_eq!(grown.cells(y), page.cells(y));
    }
    assert_eq!(grown.memory(), capacity.bytes());
    grown
}

#[test]
fn a_page_whose_style_table_fills_up_grows_it() {
    for cols in [1, 80, 500] {
        let mut page = Page::new(Capacity::standard(cols));
        let cells = u32::from(page.capacity().rows) * u32::from(cols);
        assert!(
            cells < u32::from(u16::MAX),
            "every cell can have a style of its own"
        );

        // Every cell gets a style of its own, which fills the table again
        // and again.
        let mut n = 0;
        while let Some(y) = page.push_row() {
            for x in 0..cols {
                if print(&mut page, y, x, 'x', &nth_style(n)) == Err(TableFull) {
                    page = grow(&page);
                    print(&mut page, y, x, 'x', &nth_style(n)).unwrap();
                }
                n += 1;
            }
        }
        assert!(page.capacity().styles > PAGE_STYLES, "{cols} columns");

        let mut n = 0;
        for y in 0..page.len() {
            for &cell in page.cells(y) {
                assert_eq!(page.styles().get(cell.style()), &nth_style(n));
                n += 1;
            }
        }
    }
}

#[test]
fn a_grown_page_keeps_its_rows_and_their_metadata() {
    let mut page = Page::new(Capacity::standard(80));
    for y in 0..3 {
        page.push_row();
        print(&mut page, y, 0, 'a', &bold(y as u8)).unwrap();
    }
    page.row_mut(1).set(RowFlag::SoftWrapped, true);
    let grown = page.grown(Capacity {
        styles: 1000,
        ..page.capacity()
    });
    assert_eq!(grown.len(), 3);
    for y in 0..3 {
        assert_eq!(grown.row(y), page.row(y));
        assert_eq!(grown.cells(y), page.cells(y));
    }
    assert_eq!(grown.styles().len(), page.styles().len());
    assert_eq!(grown.capacity().styles, 1000);
}

#[test]
#[should_panic(expected = "row 0 is not in use")]
fn a_row_not_in_use_cannot_be_read() {
    Page::new(Capacity::standard(80)).cells(0);
}
