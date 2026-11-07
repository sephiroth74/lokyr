//! The time that the interface takes of a frame of a list that scrolls, of
//! 100 000 rows and of 1 000: its rows' layout, the plugin's render of the
//! items that the list asks for as it passes them, and the paint, in the
//! system's font, as the gallery's list has it.
//!
//! `cargo test --release -p lokyr-app --test list_frame -- --ignored
//! --nocapture` prints them.

#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use lokyr_font_coretext::CoreText;
use lokyr_plugin_sdk::view::{
    Align, Direction, Kind, Layout as Box, Length, ListItem, Node, Style, UiEventKind,
};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::field::Fields;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::Layout;
use lokyr_ui_core::list::Lists;
use lokyr_ui_core::paint::{Fills, Shown, paint};
use lokyr_ui_core::theme::Theme;
use lokyr_ui_core::tree::Tree;

/// The frames that the measure scrolls through.
const FRAMES: usize = 2000;

/// The list of `count` rows of 22 points, 660 points high, of the items
/// from `first` to `end`: a row of a number, room, and a hash.
fn view(count: usize, (first, end): (usize, usize)) -> Node {
    let row = Node::container(vec![
        Node::text(""),
        Node::spacer(),
        Node::text("").style(Style {
            color: Some("text.secondary".into()),
            ..Style::default()
        }),
    ])
    .layout(Box {
        align: Align::Center,
        padding: lokyr_plugin_sdk::view::Edges::all(4.0),
        ..Box::default()
    });
    let items = (first..end)
        .map(|index| ListItem {
            texts: vec![
                format!("Row {index}"),
                format!("{:08x}", (index as u64).wrapping_mul(2_654_435_761)),
            ],
        })
        .collect();
    let mut list = Node::list(row, count, (first, items))
        .key("rows")
        .layout(Box {
            height: Length::Points(660.0),
            ..Box::default()
        });
    if let Kind::List(list) = &mut list.kind {
        list.row_height = Some(22.0);
    }
    Node::container(vec![list]).layout(Box {
        direction: Direction::Column,
        ..Box::default()
    })
}

/// The first frame's time, which draws the glyphs into the atlas, then the
/// others' mean and the slowest of them, of a list of `count` rows,
/// scrolled 13 points a frame, at a scale of 2.
fn measure(count: usize) -> (Duration, Duration, Duration) {
    let scale = 2.0;
    let mut font = Font::new(CoreText::new(), FontConfig::default(), scale).unwrap();
    let tokens = Theme::builtin().dark.tokens.clone();
    let mut tree = Tree::new();
    let mut layout = Layout::new(scale);
    let changes = tree.update(&view(count, (0, 0))).unwrap();
    layout.update(&tree, &changes);
    let mut lists = Lists::new();
    let fields = Fields::new();
    let (mut first, mut total, mut slowest) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for frame in 0..FRAMES {
        let start = Instant::now();
        layout.compute(&tree, Some(480.0 * scale), None, &mut font);
        let list = tree.children(tree.root().unwrap())[0];
        if frame > 0 {
            lists.scroll(&tree, list, 13.0);
        }
        // The plugin's render of the items asked for, as the gallery's.
        for event in lists.lay_out(&tree, &layout, &mut font) {
            if let UiEventKind::ListRange {
                first,
                count: asked,
            } = event.kind
            {
                let changes = tree.update(&view(count, (first, first + asked))).unwrap();
                layout.update(&tree, &changes);
                layout.compute(&tree, Some(480.0 * scale), None, &mut font);
                lists.lay_out(&tree, &layout, &mut font);
            }
        }
        font.begin_frame();
        let items = paint(
            &tree,
            &layout,
            &mut font,
            &tokens,
            (0.0, 0.0),
            Shown {
                dark: true,
                focused: None,
                fields: &fields,
                hovered: None,
                pressed: None,
                lists: &lists,
                motion: None,
                fills: Fills::All,
            },
        );
        assert!(!items.is_empty());
        let _ = font.texture_updates();
        let took = start.elapsed();
        if frame == 0 {
            first = took;
            continue;
        }
        total += took;
        slowest = slowest.max(took);
    }
    (first, total / (FRAMES - 1) as u32, slowest)
}

#[test]
#[ignore = "a measure, of a release build"]
fn a_frame_of_a_list_of_100_000_rows_takes_what_its_rows_that_show_take() {
    for count in [1_000, 100_000] {
        let (first, mean, slowest) = measure(count);
        let micros = |took: Duration| took.as_secs_f64() * 1e6;
        println!(
            "{count} rows: the first frame {:.0} µs, then {:.1} µs a frame, {:.0} µs at most",
            micros(first),
            micros(mean),
            micros(slowest)
        );
    }
}
