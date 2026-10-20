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
    let capacity = page.capacity().grown_for(TableFull::Styles).unwrap();
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
                if print(&mut page, y, x, 'x', &nth_style(n)) == Err(TableFull::Styles) {
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

/// A page of 80 columns with `rows` rows in use.
fn page_with_rows(rows: u16) -> Page {
    let mut page = Page::new(Capacity::standard(80));
    for _ in 0..rows {
        page.push_row();
    }
    page
}

/// Writes `text` at the start of row `y`, a grapheme per cell, the way the
/// terminal will: the first codepoint as a character, the others pushed.
fn print_graphemes(page: &mut Page, y: u16, text: &[&str]) -> Result<(), TableFull> {
    for (x, grapheme) in (0..).zip(text) {
        let mut chars = grapheme.chars();
        let first = chars.next().expect("a grapheme has a codepoint");
        print(page, y, x, first, &Style::DEFAULT)?;
        for c in chars {
            page.push_codepoint(y, x, c)?;
        }
    }
    Ok(())
}

#[test]
fn codepoints_after_the_first_make_a_grapheme() {
    let mut page = page_with_rows(1);
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    print_graphemes(&mut page, 0, &["e\u{301}", "a", family, "\u{2764}\u{FE0F}"]).unwrap();
    assert_eq!(page.cells(0)[0].content(), Content::Grapheme('e'));
    assert_eq!(page.grapheme(0, 0), ['\u{301}']);
    assert_eq!(page.cells(0)[1].content(), Content::Char('a'));
    assert_eq!(page.grapheme(0, 1), []);
    let rest: Vec<char> = family.chars().skip(1).collect();
    assert_eq!(page.grapheme(0, 2), rest, "a cluster longer than a chunk");
    assert_eq!(page.grapheme(0, 3), ['\u{FE0F}']);
    assert!(page.row(0).has(RowFlag::Graphemes));
}

#[test]
fn overwriting_a_grapheme_frees_its_entry() {
    let mut page = page_with_rows(1);
    print_graphemes(&mut page, 0, &["e\u{301}", "o\u{308}"]).unwrap();
    assert_eq!(page.graphemes.len(), 2);
    print(&mut page, 0, 0, 'x', &Style::DEFAULT).unwrap();
    assert_eq!(page.cells(0)[0].content(), Content::Char('x'));
    assert_eq!(page.grapheme(0, 0), []);
    assert_eq!(page.graphemes.len(), 1);
    // The cell can become a grapheme again.
    page.push_codepoint(0, 0, '\u{302}').unwrap();
    assert_eq!(page.grapheme(0, 0), ['\u{302}']);
}

#[test]
fn clearing_a_row_frees_its_graphemes_only() {
    let mut page = page_with_rows(2);
    print_graphemes(&mut page, 0, &["e\u{301}", "o\u{308}"]).unwrap();
    print_graphemes(&mut page, 1, &["u\u{30A}"]).unwrap();
    page.clear_row(0);
    assert_eq!(page.graphemes.len(), 1);
    assert_eq!(page.grapheme(0, 0), []);
    assert!(!page.row(0).has(RowFlag::Graphemes));
    assert_eq!(page.grapheme(1, 0), ['\u{30A}']);
}

#[test]
fn a_page_whose_grapheme_table_fills_up_grows_it() {
    let mut page = page_with_rows(page_with_rows(0).capacity().rows);
    let cells = page.len() * page.cols();
    let mark = |n: u32| char::from_u32(0x300 + n % 0x70).expect("a combining mark");
    // Every cell gets a grapheme of its own, which fills the table again and
    // again.
    let mut n = 0;
    for y in 0..page.len() {
        for x in 0..page.cols() {
            print(&mut page, y, x, 'a', &Style::DEFAULT).unwrap();
            if let Err(full) = page.push_codepoint(y, x, mark(n)) {
                assert_eq!(full, TableFull::Graphemes);
                page = page.grown(page.capacity().grown_for(full).unwrap());
                page.push_codepoint(y, x, mark(n)).unwrap();
            }
            n += 1;
        }
    }
    assert_eq!(page.graphemes.len(), usize::from(cells));
    let mut n = 0;
    for y in 0..page.len() {
        for x in 0..page.cols() {
            assert_eq!(page.grapheme(y, x), [mark(n)]);
            n += 1;
        }
    }
    assert_eq!(page.memory(), page.capacity().bytes());
}

fn link<'a>(id: &'a str, uri: &'a str) -> Hyperlink<'a> {
    Hyperlink {
        id: id.as_bytes(),
        uri: uri.as_bytes(),
    }
}

/// Writes `text` from the start of row `y` as part of `target`, the way the
/// terminal will: its pen holds the link while the cells take theirs, and
/// lets go whether they all could or not.
fn print_link(
    page: &mut Page,
    y: u16,
    text: &str,
    target: Hyperlink<'_>,
) -> Result<HyperlinkId, TableFull> {
    let id = page.hyperlinks_mut().acquire(target)?;
    let linked = link_cells(page, y, text, id);
    page.hyperlinks_mut().release(id);
    linked.map(|()| id)
}

fn link_cells(page: &mut Page, y: u16, text: &str, id: HyperlinkId) -> Result<(), TableFull> {
    for (x, c) in (0..).zip(text.chars()) {
        print(page, y, x, c, &Style::DEFAULT)?;
        page.set_hyperlink(y, x, Some(id))?;
    }
    Ok(())
}

#[test]
fn cells_become_part_of_a_link() {
    let mut page = page_with_rows(2);
    let id = print_link(&mut page, 0, "docs", link("1", "https://example.com/docs")).unwrap();
    print(&mut page, 1, 0, 'x', &Style::DEFAULT).unwrap();
    for x in 0..4 {
        assert_eq!(page.hyperlink(0, x), Some(id));
        assert!(page.cells(0)[usize::from(x)].has_hyperlink());
    }
    assert_eq!(page.hyperlink(0, 4), None);
    assert_eq!(
        page.hyperlinks().get(id),
        link("1", "https://example.com/docs")
    );
    assert_eq!(page.hyperlinks().references(id), 4);
    assert!(page.row(0).has(RowFlag::Hyperlinks));
    assert!(!page.row(1).has(RowFlag::Hyperlinks));
}

#[test]
fn overwriting_a_linked_cell_frees_its_entry() {
    let mut page = page_with_rows(1);
    let id = print_link(&mut page, 0, "ab", link("", "https://example.com")).unwrap();
    print(&mut page, 0, 0, 'x', &Style::DEFAULT).unwrap();
    assert_eq!(page.hyperlink(0, 0), None);
    assert!(!page.cells(0)[0].has_hyperlink());
    assert_eq!(page.hyperlinks().references(id), 1);
    // A cell can also leave its link without being overwritten; the last
    // cell frees the link.
    page.set_hyperlink(0, 1, None).unwrap();
    assert_eq!(page.hyperlink(0, 1), None);
    let again = print_link(&mut page, 0, "c", link("", "https://other.example")).unwrap();
    assert_eq!(again, id, "the freed id serves again");
}

#[test]
fn clearing_a_row_frees_its_links_only() {
    let mut page = page_with_rows(2);
    let first = print_link(&mut page, 0, "first", link("", "a")).unwrap();
    let both = print_link(&mut page, 1, "both", link("", "b")).unwrap();
    page.set_hyperlink(0, 9, Some(both)).unwrap();
    page.clear_row(0);
    assert_eq!(page.hyperlink(0, 0), None);
    assert!(!page.row(0).has(RowFlag::Hyperlinks));
    assert_eq!(page.hyperlinks().references(both), 4);
    assert_eq!(page.hyperlink(1, 0), Some(both));
    // The first link has no cell left, so it is gone.
    assert_eq!(print_link(&mut page, 0, "x", link("", "c")).unwrap(), first);
}

#[test]
fn a_page_whose_hyperlink_table_fills_up_grows_it() {
    let mut page = page_with_rows(page_with_rows(0).capacity().rows);
    // A link of its own for every row, over every cell of the row: both
    // the targets and the linked cells run out again and again.
    for y in 0..page.len() {
        let uri = format!("https://example.com/{y}");
        loop {
            match print_link(&mut page, y, &"x".repeat(80), link("", &uri)) {
                Ok(_) => break,
                Err(full) => page = page.grown(page.capacity().grown_for(full).unwrap()),
            }
        }
    }
    for y in 0..page.len() {
        let uri = format!("https://example.com/{y}");
        let id = page.hyperlink(y, 79).expect("the row is linked");
        assert_eq!(page.hyperlinks().get(id), link("", &uri));
        assert_eq!(page.hyperlinks().references(id), 80);
    }
    assert!(page.capacity().hyperlinks > PAGE_HYPERLINKS);
    assert!(page.capacity().linked_cells > PAGE_LINKED_CELLS);
    assert_eq!(page.memory(), page.capacity().bytes());
}

#[test]
fn a_reset_page_is_as_new() {
    let mut page = page_with_rows(3);
    print(&mut page, 0, 0, 'a', &bold(1)).unwrap();
    print_graphemes(&mut page, 1, &["e\u{301}"]).unwrap();
    print_link(&mut page, 2, "ab", link("", "https://example.com")).unwrap();
    page.row_mut(0).set(RowFlag::SoftWrapped, true);
    page.reset();
    assert!(page.is_empty());
    assert!(page.styles().is_empty());
    assert_eq!(page.graphemes.len(), 0);
    // Its rows come back empty, and its tables work as new ones.
    for _ in 0..3 {
        page.push_row();
    }
    for y in 0..3 {
        assert!(page.cells(y).iter().all(|&cell| cell == Cell::EMPTY));
        assert_eq!(page.row(y), Row::default());
    }
    print(&mut page, 0, 0, 'b', &bold(2)).unwrap();
    assert_eq!(page.styles().get(page.cells(0)[0].style()), &bold(2));
    print_graphemes(&mut page, 1, &["o\u{308}"]).unwrap();
    assert_eq!(page.grapheme(1, 0), ['\u{308}']);
    let id = print_link(&mut page, 2, "c", link("", "https://other.example")).unwrap();
    assert_eq!(page.hyperlinks().get(id), link("", "https://other.example"));
}

#[test]
fn inserted_blanks_move_cells_with_their_graphemes_and_links() {
    let mut page = Page::new(Capacity::standard(8));
    page.push_row();
    print_graphemes(&mut page, 0, &["a", "e\u{301}", "c"]).unwrap();
    let id = page
        .hyperlinks_mut()
        .acquire(link("", "https://example.com"))
        .unwrap();
    page.set_hyperlink(0, 2, Some(id)).unwrap();
    page.hyperlinks_mut().release(id);
    for x in 3..8 {
        print(&mut page, 0, x, char::from(b'0' + x as u8), &Style::DEFAULT).unwrap();
    }
    page.insert_blanks(0, 1, 2, 8);
    let text: String = page
        .cells(0)
        .iter()
        .map(|cell| match cell.content() {
            Content::Char('\0') => '.',
            Content::Char(c) | Content::Grapheme(c) => c,
            _ => '.',
        })
        .collect();
    assert_eq!(text, "a..ec345", "the last two cells fell off");
    assert_eq!(page.grapheme(0, 3), ['\u{301}']);
    assert_eq!(page.grapheme(0, 1), []);
    assert_eq!(page.hyperlink(0, 4), Some(id));
    assert_eq!(page.hyperlink(0, 2), None);
    assert_eq!(page.hyperlinks().references(id), 1);
    // Cells that fall off drop their entries.
    page.insert_blanks(0, 0, 8, 8);
    assert_eq!(page.graphemes.len(), 0);
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
}

#[test]
fn a_cell_can_change_width_and_keep_its_grapheme() {
    let mut page = page_with_rows(1);
    print_graphemes(&mut page, 0, &["\u{2764}\u{FE0F}"]).unwrap();
    page.set_width(0, 0, Width::Wide);
    assert_eq!(page.cells(0)[0].width(), Width::Wide);
    assert_eq!(page.grapheme(0, 0), ['\u{FE0F}']);
}

/// Writes into row `y` of `page` "a" in bold red, "e" with a combining
/// acute accent, and "lk" in bold red, part of a link, and marks the row
/// soft-wrapped. Returns the link.
fn sample_row(page: &mut Page, y: u16) -> HyperlinkId {
    print(page, y, 0, 'a', &bold(1)).unwrap();
    print(page, y, 1, 'e', &Style::DEFAULT).unwrap();
    page.push_codepoint(y, 1, '\u{301}').unwrap();
    let id = page
        .hyperlinks_mut()
        .acquire(link("x", "https://example.com"))
        .unwrap();
    for (x, c) in [(2, 'l'), (3, 'k')] {
        print(page, y, x, c, &bold(1)).unwrap();
        page.set_hyperlink(y, x, Some(id)).unwrap();
    }
    page.hyperlinks_mut().release(id);
    page.row_mut(y).set(RowFlag::SoftWrapped, true);
    id
}

/// Checks that row `y` of `page` holds what [`sample_row`] writes.
fn assert_sample_row(page: &Page, y: u16) {
    let cells = page.cells(y);
    assert_eq!(page.styles().get(cells[0].style()), &bold(1));
    assert_eq!(cells[0].content(), Content::Char('a'));
    assert_eq!(cells[1].content(), Content::Grapheme('e'));
    assert_eq!(page.grapheme(y, 1), ['\u{301}']);
    for x in [2, 3] {
        let link = page.hyperlink(y, x).expect("a linked cell");
        let target = page.hyperlinks().get(link);
        assert_eq!(
            (target.id, target.uri),
            (&b"x"[..], &b"https://example.com"[..])
        );
    }
    assert!(page.row(y).has(RowFlag::SoftWrapped));
}

#[test]
fn a_row_moves_within_its_page_with_its_graphemes_and_links() {
    let mut page = page_with_rows(3);
    let id = sample_row(&mut page, 0);
    print(&mut page, 2, 0, 'z', &bold(2)).unwrap();
    page.move_row(0, 2);
    assert_sample_row(&page, 2);
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
    assert!(!page.row(0).has(RowFlag::SoftWrapped));
    assert!(page.row(0).has(RowFlag::Dirty));
    // What row 2 held is gone; the moved cells hold their references.
    assert_eq!(page.styles().len(), 1);
    assert_eq!(page.styles().references(page.cells(2)[0].style()), 3);
    assert_eq!(page.graphemes.len(), 1);
    assert_eq!(page.hyperlinks().references(id), 2);
    page.move_row(2, 2);
    assert_sample_row(&page, 2);
}

#[test]
fn a_row_copies_into_another_page_with_its_tables() {
    let mut source = page_with_rows(1);
    sample_row(&mut source, 0);
    let mut target = page_with_rows(2);
    print(&mut target, 1, 0, 'z', &bold(2)).unwrap();
    target.copy_row(1, &source, 0, WhenFull::Fail).unwrap();
    assert_sample_row(&target, 1);
    assert_sample_row(&source, 0);
    // The target holds its own references, and dropped bold(2).
    assert_eq!(target.styles().len(), 1);
    assert_eq!(target.styles().references(target.cells(1)[0].style()), 3);
    assert_eq!(target.graphemes.len(), 1);
    let link = target.hyperlink(1, 2).unwrap();
    assert_eq!(target.hyperlinks().references(link), 2);
}

#[test]
fn a_copy_into_a_full_table_leaves_the_row_empty() {
    let mut source = page_with_rows(1);
    for x in 0..3 {
        print(&mut source, 0, x, 'a', &nth_style(u32::from(x) + 1)).unwrap();
    }
    let capacity = Capacity {
        styles: 2,
        ..Capacity::standard(80)
    };
    let mut target = Page::new(capacity);
    target.push_row();
    assert_eq!(
        target.copy_row(0, &source, 0, WhenFull::Fail),
        Err(TableFull::Styles)
    );
    assert!(target.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
    assert_eq!(
        target.styles().len(),
        0,
        "the two styles copied are dropped"
    );
    let mut grown = target.grown(capacity.grown_for(TableFull::Styles).unwrap());
    grown.copy_row(0, &source, 0, WhenFull::Fail).unwrap();
    assert_eq!(grown.styles().len(), 3);
}

#[test]
fn cells_moved_within_a_page_bring_the_flags_of_their_row() {
    let mut page = page_with_rows(2);
    print(&mut page, 0, 2, 'b', &bold(1)).unwrap();
    print(&mut page, 0, 3, 'e', &Style::DEFAULT).unwrap();
    page.push_codepoint(0, 3, '\u{301}').unwrap();
    let id = page.hyperlinks_mut().acquire(link("", "http://a")).unwrap();
    print(&mut page, 0, 4, 'l', &Style::DEFAULT).unwrap();
    page.set_hyperlink(0, 4, Some(id)).unwrap();
    page.hyperlinks_mut().release(id);
    page.move_cells(0, 1, 2..5);
    let row = page.row(1);
    for flag in [RowFlag::Styles, RowFlag::Graphemes, RowFlag::Hyperlinks] {
        assert!(row.has(flag), "{flag:?}");
    }
}

#[test]
fn a_copy_into_any_full_table_leaves_the_cells_empty() {
    let mut source = page_with_rows(1);
    print(&mut source, 0, 0, 'e', &Style::DEFAULT).unwrap();
    source.push_codepoint(0, 0, '\u{301}').unwrap();
    print(&mut source, 0, 1, 'o', &Style::DEFAULT).unwrap();
    source.push_codepoint(0, 1, '\u{301}').unwrap();
    for (x, uri) in [(2, "http://a"), (3, "http://b"), (4, "http://b")] {
        let id = source.hyperlinks_mut().acquire(link("", uri)).unwrap();
        print(&mut source, 0, x, 'l', &Style::DEFAULT).unwrap();
        source.set_hyperlink(0, x, Some(id)).unwrap();
        source.hyperlinks_mut().release(id);
    }
    let standard = Capacity::standard(80);
    for (capacity, full) in [
        (
            Capacity {
                graphemes: 1,
                ..standard
            },
            TableFull::Graphemes,
        ),
        (
            Capacity {
                hyperlinks: 1,
                ..standard
            },
            TableFull::Hyperlinks,
        ),
        (
            Capacity {
                linked_cells: 2,
                ..standard
            },
            TableFull::LinkedCells,
        ),
    ] {
        let mut target = Page::new(capacity);
        target.push_row();
        assert_eq!(
            target.copy_cells(0, &source, 0, 0..5, WhenFull::Fail),
            Err(full)
        );
        assert!(
            target.cells(0).iter().all(|&cell| cell == Cell::EMPTY),
            "{full:?}"
        );
        assert_eq!(target.graphemes.len(), 0, "{full:?}");
        assert!(target.hyperlinks().is_empty(), "{full:?}");
        let mut grown = target.grown(capacity.grown_for(full).unwrap());
        grown
            .copy_cells(0, &source, 0, 0..5, WhenFull::Fail)
            .unwrap();
        assert_eq!(&row_chars(&grown, 0)[..5], "eolll", "{full:?}");
    }
}

#[test]
fn rows_that_trade_blocks_keep_their_cells_through_growth_and_reset() {
    let mut page = page_with_rows(4);
    for y in 0..4 {
        print(
            &mut page,
            y,
            0,
            char::from(b'a' + y as u8),
            &nth_style(u32::from(y) + 1),
        )
        .unwrap();
        page.push_codepoint(y, 0, '\u{301}').unwrap();
    }
    // Rotate the rows up twice: each moves into the row the move before
    // left empty.
    for _ in 0..2 {
        page.clear_row(0);
        for y in 1..4 {
            page.move_row_into_empty(y, y - 1);
        }
    }
    // A row moved into itself stays.
    page.move_row(1, 1);
    assert_eq!(&row_chars(&page, 0)[..1], "c");
    assert_eq!(&row_chars(&page, 1)[..1], "d");
    assert_eq!(page.grapheme(1, 0), ['\u{301}']);
    assert_eq!(page.styles().len(), 2);
    let grown = page.grown(page.capacity().grown_for(TableFull::Styles).unwrap());
    assert_eq!(&row_chars(&grown, 1)[..1], "d");
    assert_eq!(grown.grapheme(1, 0), ['\u{301}']);
    assert_eq!(grown.styles().get(grown.cells(1)[0].style()), &nth_style(4));
    // The rows in use are now in the last blocks.
    page.pop_row();
    page.pop_row();
    assert_eq!(&row_chars(&page, 0)[..1], "c");
    // A reset empties every block, wherever the rows left them.
    page.reset();
    for _ in 0..4 {
        page.push_row();
    }
    for y in 0..4 {
        assert!(
            page.cells(y).iter().all(|&cell| cell == Cell::EMPTY),
            "row {y}"
        );
    }
    assert_eq!(page.graphemes.len(), 0);
}

#[test]
fn a_table_at_its_largest_does_not_grow() {
    let largest = Capacity {
        styles: u16::MAX,
        graphemes: u16::MAX,
        hyperlinks: u16::MAX,
        linked_cells: u16::MAX,
        ..Capacity::standard(80)
    };
    for full in [
        TableFull::Styles,
        TableFull::Graphemes,
        TableFull::Hyperlinks,
        TableFull::LinkedCells,
    ] {
        assert_eq!(largest.grown_for(full), None, "{full:?}");
    }
    let near = Capacity {
        graphemes: 40_000,
        ..Capacity::standard(80)
    };
    let grown = near.grown_for(TableFull::Graphemes).unwrap();
    assert_eq!(grown.graphemes, u16::MAX, "the last step is short of twice");
    assert_eq!(grown.grown_for(TableFull::Graphemes), None);
}

#[test]
fn a_copy_that_may_drop_leaves_out_only_what_does_not_fit() {
    let mut source = page_with_rows(1);
    for x in 0..3 {
        print(&mut source, 0, x, 'a', &nth_style(u32::from(x) + 1)).unwrap();
    }
    for (x, cluster) in [(3, "e\u{301}\u{302}"), (4, "o\u{301}")] {
        let mut chars = cluster.chars();
        print(&mut source, 0, x, chars.next().unwrap(), &Style::DEFAULT).unwrap();
        for c in chars {
            source.push_codepoint(0, x, c).unwrap();
        }
    }
    for (x, uri) in [(5, "http://a"), (6, "http://b")] {
        let id = source.hyperlinks_mut().acquire(link("", uri)).unwrap();
        print(&mut source, 0, x, 'l', &Style::DEFAULT).unwrap();
        source.set_hyperlink(0, x, Some(id)).unwrap();
        source.hyperlinks_mut().release(id);
    }
    // Room for two styles, one grapheme and one link.
    let capacity = Capacity {
        styles: 2,
        graphemes: 1,
        hyperlinks: 1,
        ..Capacity::standard(80)
    };
    let mut target = Page::new(capacity);
    target.push_row();
    assert_eq!(
        target.copy_row(0, &source, 0, WhenFull::Fail),
        Err(TableFull::Styles)
    );
    target.copy_row(0, &source, 0, WhenFull::Drop).unwrap();
    assert_eq!(
        &row_chars(&target, 0)[..7],
        "aaaeoll",
        "every cell is copied"
    );
    let styles: Vec<_> = (0..3)
        .map(|x| *target.styles().get(target.cells(0)[x].style()))
        .collect();
    assert_eq!(styles, [nth_style(1), nth_style(2), Style::DEFAULT]);
    assert_eq!(target.grapheme(0, 3), ['\u{301}', '\u{302}']);
    assert_eq!(target.cells(0)[4].content(), Content::Char('o'));
    let first = target.hyperlink(0, 5).unwrap();
    assert_eq!(target.hyperlinks().get(first), link("", "http://a"));
    assert_eq!(target.hyperlink(0, 6), None);
    assert_eq!(target.styles().len(), 2);
    assert_eq!(target.hyperlinks().references(first), 1);
    assert_eq!(target.graphemes.len(), 1);
}

/// The characters of row `y`, `.` for a cell without one.
fn row_chars(page: &Page, y: u16) -> String {
    page.cells(y)
        .iter()
        .map(|cell| match cell.content() {
            Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => '.',
            Content::Char(c) | Content::Grapheme(c) => c,
        })
        .collect()
}

#[test]
fn erasing_puts_blanks_and_keeps_protected_cells_when_selective() {
    let mut page = Page::new(Capacity::standard(6));
    page.push_row();
    for (x, c) in (0..).zip("abcdef".chars()) {
        print(&mut page, 0, x, c, &bold(1)).unwrap();
    }
    let protected = page.cells(0)[2].with_protected(true);
    page.set_cell(0, 2, protected);
    let blank = Cell::EMPTY.with_content(Content::BgPalette(4));
    page.erase(0, 1..4, blank, true);
    assert_eq!(row_chars(&page, 0), "a.c.ef");
    assert_eq!(page.cells(0)[1], blank);
    // a, c, e and f keep the style.
    assert_eq!(page.styles().references(page.cells(0)[0].style()), 4);
    page.erase(0, 0..6, Cell::EMPTY, false);
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
    assert_eq!(page.styles().len(), 0);
    page.erase(0, 3..3, blank, false);
    assert!(
        page.cells(0).iter().all(|&cell| cell == Cell::EMPTY),
        "an empty range"
    );
}

#[test]
fn erasing_half_of_a_wide_character_erases_it_whole() {
    let mut page = Page::new(Capacity::standard(6));
    page.push_row();
    let wide = |page: &mut Page, x: u16, protected: bool| {
        let head = Cell::EMPTY
            .with_content(Content::Char('中'))
            .with_width(Width::Wide)
            .with_protected(protected);
        page.set_cell(0, x, head);
        let tail = Cell::EMPTY
            .with_width(Width::SpacerTail)
            .with_protected(protected);
        page.set_cell(0, x + 1, tail);
    };
    wide(&mut page, 0, false);
    wide(&mut page, 2, false);
    page.erase(0, 1..3, Cell::EMPTY, false);
    assert!(page.cells(0)[..4].iter().all(|&cell| cell == Cell::EMPTY));
    // A protected one stays whole through a selective erase.
    wide(&mut page, 2, true);
    page.erase(0, 0..3, Cell::EMPTY, true);
    assert_eq!(page.cells(0)[2].width(), Width::Wide);
    assert_eq!(page.cells(0)[3].width(), Width::SpacerTail);
}

#[test]
fn dropping_the_first_rows_moves_the_others_up() {
    let mut page = page_with_rows(5);
    for y in 0..5 {
        print(&mut page, y, 0, char::from(b'a' + y as u8), &bold(1)).unwrap();
    }
    page.row_mut(3).set(RowFlag::SoftWrapped, true);
    page.drop_first_rows(3);
    assert_eq!(page.len(), 2);
    assert_eq!(page.cells(0)[0].content(), Content::Char('d'));
    assert!(page.row(0).has(RowFlag::SoftWrapped));
    assert_eq!(page.cells(1)[0].content(), Content::Char('e'));
    assert_eq!(page.styles().references(page.cells(0)[0].style()), 2);
    // The rows past the end are empty, for push_row to hand out again.
    page.push_row();
    assert!(page.cells(2).iter().all(|&cell| cell == Cell::EMPTY));
    page.drop_first_rows(3);
    assert!(page.is_empty());
    assert_eq!(page.styles().len(), 0);
}

#[test]
fn deleted_cells_make_the_others_move_left_with_their_graphemes_and_links() {
    let mut page = Page::new(Capacity::standard(8));
    page.push_row();
    print_graphemes(&mut page, 0, &["a", "b", "c", "e\u{301}"]).unwrap();
    let id = page
        .hyperlinks_mut()
        .acquire(link("", "https://example.com"))
        .unwrap();
    page.set_hyperlink(0, 3, Some(id)).unwrap();
    page.hyperlinks_mut().release(id);
    page.delete_cells(0, 1, 2, 8);
    assert_eq!(row_chars(&page, 0), "ae......");
    assert_eq!(page.grapheme(0, 1), ['\u{301}']);
    assert_eq!(page.hyperlink(0, 1), Some(id));
    assert_eq!(page.hyperlinks().references(id), 1);
    page.delete_cells(0, 0, 8, 8);
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
    assert_eq!(page.graphemes.len(), 0);
}

#[test]
fn deleting_half_of_a_wide_character_empties_it_whole() {
    let mut page = Page::new(Capacity::standard(6));
    page.push_row();
    let wide = Cell::EMPTY
        .with_content(Content::Char('中'))
        .with_width(Width::Wide);
    for x in [0, 2] {
        page.set_cell(0, x, wide);
        page.set_cell(0, x + 1, Cell::EMPTY.with_width(Width::SpacerTail));
    }
    print(&mut page, 0, 4, 'z', &Style::DEFAULT).unwrap();
    // From the second half of the first, to the first half of the second.
    page.delete_cells(0, 1, 2, 6);
    assert_eq!(row_chars(&page, 0), "..z...", "z moved two cells left");
    assert!(
        page.cells(0)
            .iter()
            .all(|cell| cell.width() == Width::Narrow)
    );
}

#[test]
fn cells_move_within_a_range_and_the_rest_of_the_rows_stays() {
    let mut page = page_with_rows(2);
    for x in 0..8 {
        print(&mut page, 0, x, char::from(b'a' + x as u8), &bold(1)).unwrap();
        print(&mut page, 1, x, char::from(b'A' + x as u8), &Style::DEFAULT).unwrap();
    }
    page.row_mut(1).set(RowFlag::SoftWrapped, true);
    page.move_cells(0, 1, 2..5);
    assert_eq!(&row_chars(&page, 1)[..8], "ABcdeFGH");
    assert_eq!(&row_chars(&page, 0)[..8], "ab...fgh");
    assert!(page.row(1).has(RowFlag::SoftWrapped), "the metadata stays");
    assert_eq!(page.styles().references(page.cells(0)[0].style()), 8);
}

#[test]
fn cells_copy_within_a_range_into_another_page() {
    let mut source = page_with_rows(1);
    sample_row(&mut source, 0);
    let mut target = page_with_rows(1);
    for x in 0..4 {
        print(&mut target, 0, x, 'z', &bold(2)).unwrap();
    }
    target
        .copy_cells(0, &source, 0, 1..3, WhenFull::Fail)
        .unwrap();
    assert_eq!(&row_chars(&target, 0)[..4], "zelz");
    assert_eq!(target.grapheme(0, 1), ['\u{301}']);
    assert!(target.hyperlink(0, 2).is_some());
    assert_eq!(target.hyperlink(0, 3), None);
    assert_eq!(target.styles().len(), 2, "bold(1) came in with the l");
}

#[test]
fn wide_characters_across_the_edges_of_a_range_are_emptied() {
    let mut page = Page::new(Capacity::standard(8));
    page.push_row();
    let wide = Cell::EMPTY
        .with_content(Content::Char('中'))
        .with_width(Width::Wide);
    for x in [1, 5] {
        page.set_cell(0, x, wide);
        page.set_cell(0, x + 1, Cell::EMPTY.with_width(Width::SpacerTail));
    }
    page.clear_crossing(0, 2..6);
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
    page.set_cell(0, 3, wide);
    page.set_cell(0, 4, Cell::EMPTY.with_width(Width::SpacerTail));
    page.clear_crossing(0, 2..6);
    assert_eq!(
        page.cells(0)[3].width(),
        Width::Wide,
        "one within the range stays"
    );
}

#[test]
fn an_erase_past_the_text_marks_its_row_dirty_too() {
    let mut page = Page::new(Capacity::standard(80));
    page.push_row();
    page.set_cell(0, 0, Cell::EMPTY.with_content(Content::Char('a')));
    page.row_mut(0).set(RowFlag::Dirty, false);
    // Past the row's extent, where every cell is empty already.
    page.erase(0, 10..20, Cell::EMPTY, false);
    assert!(page.row(0).has(RowFlag::Dirty));
    assert_eq!(page.cells(0)[0].content(), Content::Char('a'));
}

#[test]
fn a_cell_that_only_its_width_fills_is_emptied_with_its_row() {
    let mut page = Page::new(Capacity::standard(80));
    page.push_row();
    // A spacer, which is no longer an empty cell.
    page.set_width(0, 5, Width::SpacerTail);
    assert_ne!(page.cells(0)[5], Cell::EMPTY);
    page.clear_row(0);
    assert!(page.cells(0).iter().all(|&cell| cell == Cell::EMPTY));
}

proptest::proptest! {
    /// Rotating by one in chunks moves the items as the standard rotations
    /// do, and so does rotating by more.
    #[test]
    fn rotations_move_items_as_the_standard_ones_do(
        len in 0_usize..40,
        n in 0_u16..4,
        right: bool,
    ) {
        let n = n.min(len as u16);
        let mut items: Vec<u16> = (0..len as u16).collect();
        let mut expected = items.clone();
        if right {
            expected.rotate_right(usize::from(n));
        } else {
            expected.rotate_left(usize::from(n));
        }
        rotate(&mut items, n, right);
        proptest::prop_assert_eq!(items, expected);
    }
}
