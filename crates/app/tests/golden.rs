//! Golden images of the screen: a terminal's snapshot laid out in cells, as
//! a window's frame lays it out, and a view tree of the interface, laid out
//! and painted in the theme's light and dark tokens, drawn by the GPU into
//! an offscreen texture, and compared with the image in `tests/golden/`,
//! within a tolerance for the GPU's and the rasterizer's rounding.
//!
//! `LOKYR_GOLDEN_UPDATE=1 cargo test -p lokyr-app --test golden` writes the
//! images afresh, to look at before they are committed. An image that
//! differs is written next to the target's other files, with a map of the
//! pixels that differ, as the failure tells.

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lokyr_font_coretext::CoreText;
use lokyr_plugin_sdk::host::{Color, PaneId, TabId, TabInfo, WindowId};
use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::view::{
    self, Align, Border, Direction, Edges, Kind, Layout as Box, Length, Node, Shadow, Style,
};
use lokyr_render_core::{CellBatch, CursorShape, DisplayList, Item, Renderer};
use lokyr_render_metal::{Gpu, MetalRenderer};
use lokyr_text_core::{FontConfig, Size};
use lokyr_ui_core::animation::Animations;
use lokyr_ui_core::field::Fields;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::{Layout, Measure, Room};
use lokyr_ui_core::list::Lists;
use lokyr_ui_core::paint::{Fills, Shown, paint, tooltip};
use lokyr_ui_core::pane::{self as cells, CursorLook, Selection, TextLook};
use lokyr_ui_core::theme::Theme;
use lokyr_ui_core::tree::{NodeId, Tree};
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, SelectionShape, SelectionUnit, Snapshot, Terminal};

/// The pixels around the cells, as a window's padding.
const MARGIN: u32 = 4;

/// The font's size, in points.
const POINTS: f64 = 13.0;

/// A pixel differs when one of its components differs by more than this,
/// past the GPU's rounding and the rasterizer's small changes between
/// releases of macOS.
const CHANNEL: u8 = 24;

/// The share of an image's pixels that can differ, in a scene of text: a
/// glyph moved by a pixel changes many more.
const SHARE: f64 = 0.002;

/// The pixels that can differ, at a scale of 1, in a scene of the shapes
/// that the renderer draws itself, without text, as a cursor or a dotted
/// line: as many times the scale's square at another.
const SHAPES: usize = 2;

/// What a scene draws: a screen's columns and rows, what a program writes
/// to it, the cursor's shape where it shows, and the cells that are
/// selected, from one to another.
struct Scene {
    name: &'static str,
    columns: u16,
    rows: u16,
    text: String,
    cursor: Option<CursorShape>,
    selection: Option<((u16, i64), (u16, i64))>,
    /// Whether it draws shapes alone, without text, which the GPU draws
    /// the same wherever it runs.
    shapes: bool,
    /// The share by which its colors fade toward its background, as a pane's
    /// that has not the keyboard.
    fade: Option<f32>,
}

/// The GPU, its renderer and a font of `POINTS` at `scale`, as a window of
/// that scale has them; `None` without a Metal device.
struct Drawing {
    _gpu: Gpu,
    renderer: MetalRenderer,
    font: Font<CoreText>,
}

impl Drawing {
    fn new(scale: f32) -> Option<Self> {
        let Ok(gpu) = Gpu::new() else {
            eprintln!("no Metal device: the golden images are not compared");
            return None;
        };
        let config = FontConfig {
            size: Size::try_from(POINTS).unwrap(),
            ..FontConfig::default()
        };
        // The system's default, which smooths fonts; Han's characters in
        // Simplified Chinese's faces, as the images have them, whatever the
        // system's languages, which choose Japanese's or Chinese's.
        let fonts = CoreText::with_font_smoothing(true).with_language("zh-Hans");
        let font = Font::new(fonts, config, scale).unwrap();
        let (coverage, color) = font.texture_sizes();
        let renderer = MetalRenderer::new(&gpu, coverage, color).unwrap();
        Some(Self {
            _gpu: gpu,
            renderer,
            font,
        })
    }

    /// The cells of `scene`'s screen, from `(x, y)`, in the dark variant of
    /// the built-in theme, as a frame lays them out.
    fn batch(
        &mut self,
        scene: &Scene,
        (x, y): (i32, i32),
    ) -> (CellBatch, lokyr_render_core::Color) {
        let theme = Theme::builtin();
        let variant = theme.variant(true);
        let mut terminal = Terminal::new(scene.columns, scene.rows, Options::default());
        terminal.set_default_colors(variant.colors.clone());
        Parser::new().advance(&mut terminal, scene.text.as_bytes());
        if let Some((from, to)) = scene.selection {
            terminal.select(from, to, SelectionShape::Cells, SelectionUnit::Cell, "");
        }
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        self.font.begin_frame();
        let cell = *self.font.cell();
        let cursor = scene.cursor.map(|shape| CursorLook {
            shape,
            thickness: cell.underline_thickness.max(1),
        });
        let selected = snapshot.selection();
        let contains =
            |column: u16, row: u16| selected.is_some_and(|shown| shown.contains(column, row));
        let selection = selected.map(|_| Selection {
            colors: variant.selection,
            contains: &contains,
        });
        let batch = cells::cells_with(
            &snapshot,
            &cell,
            (x, y),
            selection.as_ref(),
            cursor,
            TextLook::default(),
            &mut self.font,
        );
        (batch, cells::background(&snapshot))
    }

    /// `scenes`, one under the other, drawn offscreen: the image's width,
    /// its height, and its pixels.
    fn draw(&mut self, scenes: &[Scene]) -> (u32, u32, Vec<u8>) {
        let mut items = Vec::new();
        let mut background = None;
        let mut y = MARGIN;
        let mut width = 0;
        for scene in scenes {
            let (mut batch, color) = self.batch(scene, (MARGIN as i32, y as i32));
            if let Some(share) = scene.fade {
                cells::fade(&mut batch, color, share);
            }
            width = width.max(u32::from(scene.columns) * batch.cell_width);
            y += u32::from(scene.rows) * batch.cell_height;
            background.get_or_insert(color);
            items.push(Item::Cells(batch));
        }
        let (width, height) = (width + 2 * MARGIN, y + MARGIN);
        let list = DisplayList {
            background: background.unwrap_or_default(),
            items,
            frame: self.font.atlas_frame(),
        };
        let updates = self.font.texture_updates();
        self.renderer.update_textures(&updates).unwrap();
        let pixels = self.renderer.draw_offscreen(&list, width, height).unwrap();
        (width, height, pixels)
    }
}

/// The scenes of the text: Latin, its styles, CJK, emoji, combining marks,
/// box drawing, blocks and powerline symbols, which the terminal draws
/// itself.
fn text() -> Vec<Scene> {
    vec![Scene {
        name: "text",
        columns: 40,
        rows: 8,
        text: concat!(
            "The quick brown fox, 0O 1lI| {}[]()\r\n",
            "\x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[1;3mbold italic\x1b[0m \x1b[2mdim\x1b[0m\r\n",
            "日本語のテキスト|한국어|中文\r\n",
            "😀|👍🏽|🇮🇹|❤️|1️⃣ e\u{301} n\u{303} o\u{308}\u{301}\r\n",
            "┌─┬─┐ ╔═╗ ╭─╮ ░▒▓█ ▀▄▌▐ ▁▂▃▄▅▆▇█\r\n",
            "└─┴─┘ ╚═╝ ╰─╯ ⣿⡇⠁ \u{e0b0}\u{e0b1}\u{e0b2}\u{e0b3}\u{e0b4}\u{e0b6}\r\n",
            "\x1b[30;44m ~/lokyr \x1b[34;42m\u{e0b0}\x1b[30m main \x1b[32;49m\u{e0b0}\x1b[0m",
        )
        .to_owned(),
        cursor: None,
        selection: None,
        shapes: false,
        fade: None,
    }]
}

/// The palette, 256 colors, true colors, reverse video and dim text.
fn colors() -> Vec<Scene> {
    let mut text = String::new();
    for base in [40, 100] {
        for n in 0..8 {
            text.push_str(&format!("\x1b[{}m   ", base + n));
        }
        text.push_str("\x1b[0m\r\n");
    }
    for n in 0..8 {
        text.push_str(&format!("\x1b[{}mAa \x1b[1mAa\x1b[0m ", 30 + n));
    }
    text.push_str("\r\n");
    for n in (16..232).step_by(6) {
        text.push_str(&format!("\x1b[48;5;{n}m "));
    }
    text.push_str("\x1b[0m\r\n");
    for n in 0..36_u32 {
        let (r, g, b) = (255 - n * 7, n * 7, 128);
        text.push_str(&format!("\x1b[48;2;{r};{g};{b}m "));
    }
    text.push_str("\x1b[0m\r\n");
    text.push_str(
        "\x1b[7m reverse \x1b[0m \x1b[31;7m red reverse \x1b[0m \x1b[2;32mdim green\x1b[0m",
    );
    vec![Scene {
        name: "colors",
        columns: 48,
        rows: 6,
        text,
        cursor: None,
        selection: None,
        shapes: false,
        fade: None,
    }]
}

/// Every underline's style, colored underlines, the strike and the
/// overline, under descenders and wide characters too.
fn decorations() -> Vec<Scene> {
    vec![Scene {
        name: "decorations",
        columns: 40,
        rows: 4,
        text: concat!(
            "\x1b[4msingle\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[4:4mdotted\x1b[0m\r\n",
            "\x1b[4:5mdashed\x1b[0m \x1b[9mstrike\x1b[0m \x1b[53moverline\x1b[0m \x1b[4;9;53mall\x1b[0m\r\n",
            "\x1b[4:3;58;5;196mred curly\x1b[0m \x1b[4;58;2;0;160;255mblue line\x1b[0m\r\n",
            "\x1b[4mgjpqy\x1b[0m \x1b[4m日本\x1b[0m \x1b[9;31mred strike\x1b[0m",
        )
        .to_owned(),
        cursor: None,
        selection: None,
        shapes: false,
        fade: None,
    }]
}

/// The cursor's shapes, each on a screen of its own: a block over a
/// letter, a bar, an underline, and the hollow block of a window without
/// the focus, over a wide character.
fn cursors() -> Vec<Scene> {
    [
        ("block", CursorShape::Block, "abc\x1b[D"),
        ("bar", CursorShape::Bar, "abc\x1b[D"),
        ("underline", CursorShape::Underline, "abc\x1b[D"),
        ("hollow", CursorShape::Hollow, "a日\x1b[2D"),
    ]
    .into_iter()
    .map(|(_, shape, text)| Scene {
        name: "cursor",
        columns: 8,
        rows: 1,
        text: text.to_owned(),
        cursor: Some(shape),
        selection: None,
        shapes: false,
        fade: None,
    })
    .collect()
}

/// A selection across two rows, over colored text and a wide character.
fn selection() -> Vec<Scene> {
    vec![Scene {
        name: "selection",
        columns: 30,
        rows: 3,
        text: "plain \x1b[31mred\x1b[0m \x1b[42mon green\x1b[0m\r\n日本語 and more\r\nafter"
            .to_owned(),
        cursor: None,
        selection: Some(((2, 0), (5, 1))),
        shapes: false,
        fade: None,
    }]
}

/// A pane's text, and the same faded toward its background, as a pane's
/// that has not the keyboard, of the splits' default fade: an emoji of its
/// own colors too, faded as an opacity over its cell.
fn dimmed() -> Vec<Scene> {
    let text = "plain \x1b[31mred\x1b[0m \x1b[1;32mbold green\x1b[0m \x1b[44m on blue \x1b[0m 🍎";
    [None, Some(0.15)]
        .into_iter()
        .map(|fade| Scene {
            name: "dim",
            columns: 36,
            rows: 1,
            text: text.to_owned(),
            cursor: None,
            selection: None,
            shapes: false,
            fade,
        })
        .collect()
}

/// The shapes that the renderer draws itself, without text: the cursor's
/// four shapes on empty cells, and every line on spaces, each style of
/// underline, the strike and the overline.
fn shapes() -> Vec<Scene> {
    let mut scenes: Vec<Scene> = [
        CursorShape::Block,
        CursorShape::Bar,
        CursorShape::Underline,
        CursorShape::Hollow,
    ]
    .into_iter()
    .map(|shape| Scene {
        name: "shapes",
        columns: 8,
        // A row of its own, and an empty one after it, which keeps it apart
        // from the next.
        rows: 2,
        text: "  \x1b[D".to_owned(),
        cursor: Some(shape),
        selection: None,
        shapes: true,
        fade: None,
    })
    .collect();
    let lines = ["4", "4:2", "4:3", "4:4", "4:5", "9", "53"]
        .map(|style| format!("\x1b[{style}m{}\x1b[0m\r\n", " ".repeat(12)))
        .concat();
    scenes.push(Scene {
        name: "shapes",
        columns: 12,
        rows: 7,
        text: lines.trim_end_matches("\r\n").to_owned(),
        cursor: None,
        selection: None,
        shapes: true,
        fade: None,
    });
    scenes
}

/// The width of the view tree's image, in points.
const VIEW_WIDTH: f32 = 480.0;

/// A tab of the bar, of a terminal's icon, its title and its close button,
/// in its colors of the active tab or of another.
fn tab(title: &str, active: bool) -> Node {
    let (background, color) = if active {
        ("tab.active.bg", "tab.active.fg")
    } else {
        ("tab.inactive.bg", "tab.inactive.fg")
    };
    Node::container(vec![
        Node::icon("terminal"),
        Node::text(title),
        Node::icon("close").style(Style {
            color: Some("text.secondary".into()),
            ..Style::default()
        }),
    ])
    .key(title)
    .layout(Box {
        gap: 6.0,
        padding: Edges {
            top: 4.0,
            right: 8.0,
            bottom: 4.0,
            left: 10.0,
        },
        align: Align::Center,
        ..Box::default()
    })
    .style(Style {
        background: Some(background.into()),
        radius: 6.0,
        color: Some(color.into()),
        ..Style::default()
    })
}

/// A toggle of a find bar, of its icon, its background shown when `on`.
fn toggle(icon: &str, on: bool) -> Node {
    Node::container(vec![Node::icon(icon)])
        .key(icon)
        .layout(Box {
            padding: Edges::all(2.0),
            ..Box::default()
        })
        .style(Style {
            background: on.then(|| "button.bg".into()),
            radius: 4.0,
            ..Style::default()
        })
}

/// A text marked as a match of a search, the current one or another.
fn mark(text: &str, token: &str) -> Node {
    Node::container(vec![Node::text(text)]).style(Style {
        background: Some(token.into()),
        radius: 2.0,
        ..Style::default()
    })
}

/// The view tree of the images: a tab bar of two tabs and a button; a find
/// bar floating in its panel, of a border, rounded corners and a shadow, of
/// a field of the focus's border that clips its text, its count and its
/// toggles; a line of the search's matches; and a box that clips a title
/// too long, beside a badge of the accent's color, whose corners show
/// apart from the background.
fn view_tree() -> Node {
    let secondary = || Style {
        color: Some("text.secondary".into()),
        ..Style::default()
    };
    let bar = Node::container(vec![
        tab("~/lokyr", true),
        tab("cargo test", false),
        Node::spacer(),
        Node::icon("add").style(secondary()),
    ])
    .layout(Box {
        gap: 4.0,
        padding: Edges::all(4.0),
        align: Align::Center,
        ..Box::default()
    })
    .style(Style {
        background: Some("tab.bar.bg".into()),
        ..Style::default()
    });
    let field = Node::container(vec![Node::text("fn main\\(")])
        .layout(Box {
            padding: Edges {
                top: 3.0,
                right: 6.0,
                bottom: 3.0,
                left: 6.0,
            },
            width: Length::Points(160.0),
            ..Box::default()
        })
        .style(Style {
            background: Some("field.bg".into()),
            border: Some(Border {
                color: "field.focus".into(),
                width: 2.0,
            }),
            radius: 5.0,
            clip: true,
            ..Style::default()
        });
    let find = Node::container(vec![
        Node::icon("search").style(secondary()),
        field,
        Node::text("3 of 12").style(secondary()),
        toggle("case-sensitive", false),
        toggle("regex", true),
        toggle("whole-word", false),
        Node::icon("chevron-up"),
        Node::icon("chevron-down"),
        Node::icon("close"),
    ])
    .layout(Box {
        gap: 6.0,
        padding: Edges::all(8.0),
        align: Align::Center,
        ..Box::default()
    })
    .style(Style {
        background: Some("panel.bg".into()),
        border: Some(Border {
            color: "panel.border".into(),
            width: 1.0,
        }),
        radius: 8.0,
        shadow: Some(Shadow::default()),
        ..Style::default()
    });
    let line = Node::container(vec![
        Node::text("let"),
        mark("main", "search.match"),
        Node::text("= run(||"),
        mark("main", "search.match.current"),
        Node::text(")"),
    ])
    .layout(Box {
        gap: 4.0,
        ..Box::default()
    });
    let clipped = Node::container(vec![Node::text("a title that its box clips at its edge")])
        .layout(Box {
            width: Length::Points(150.0),
            padding: Edges::all(4.0),
            ..Box::default()
        })
        .style(Style {
            border: Some(Border {
                color: "separator".into(),
                width: 1.0,
            }),
            radius: 4.0,
            clip: true,
            ..Style::default()
        });
    let badge = Node::container(vec![Node::text("beta")])
        .layout(Box {
            padding: Edges {
                top: 2.0,
                right: 8.0,
                bottom: 2.0,
                left: 8.0,
            },
            ..Box::default()
        })
        .style(Style {
            background: Some("accent".into()),
            radius: 9.0,
            color: Some("panel.bg".into()),
            ..Style::default()
        });
    let last = Node::container(vec![clipped, badge]).layout(Box {
        gap: 12.0,
        align: Align::Center,
        ..Box::default()
    });
    Node::container(vec![bar, find, line, last]).layout(Box {
        direction: Direction::Column,
        gap: 14.0,
        padding: Edges::all(16.0),
        ..Box::default()
    })
}

/// The buttons, the toggles and the separators of the images, in their
/// states: a button at rest, one pressed, one disabled, one of an icon
/// alone, which has the focus, one of an icon and a label; a toggle of an
/// icon off, one on, one off under the pointer, a disabled one on, one of
/// a label off; upright separators between the groups, and a flat one
/// under them; room below for the tooltip of the toggle under the pointer.
fn widgets_tree() -> Node {
    let button = |key: &str, label: &str, icon: &str, disabled: bool| {
        Node::new(Kind::Button(view::Button {
            label: label.to_owned(),
            icon: icon.to_owned(),
            disabled,
        }))
        .key(key)
    };
    let toggle = |key: &str, label: &str, icon: &str, (on, disabled): (bool, bool)| {
        Node::new(Kind::Toggle(view::Toggle {
            label: label.to_owned(),
            icon: icon.to_owned(),
            on,
            disabled,
        }))
        .key(key)
    };
    let row = |children| {
        Node::container(children).layout(Box {
            gap: 8.0,
            align: Align::Center,
            ..Box::default()
        })
    };
    let buttons = row(vec![
        button("replace", "Replace", "", false),
        button("pressed", "Replace", "", false),
        button("all", "Replace All", "", true),
        Node::separator(),
        button("add", "", "add", false),
        button("settings", "Settings", "settings", false),
    ]);
    let toggles = row(vec![
        toggle("case", "", "case-sensitive", (false, false)),
        toggle("regex", "", "regex", (true, false)),
        toggle("word", "", "whole-word", (false, false)).tooltip("Whole Word"),
        Node::separator(),
        toggle("pin", "", "pin", (true, true)),
        toggle("wrap", "Wrap Around", "", (false, false)),
    ]);
    let panel = Node::container(vec![buttons, toggles, Node::separator()])
        .layout(Box {
            direction: Direction::Column,
            gap: 10.0,
            padding: Edges::all(12.0),
            ..Box::default()
        })
        .style(Style {
            background: Some("panel.bg".into()),
            border: Some(Border {
                color: "panel.border".into(),
                width: 1.0,
            }),
            radius: 8.0,
            ..Style::default()
        });
    Node::container(vec![panel]).layout(Box {
        direction: Direction::Column,
        padding: Edges {
            top: 16.0,
            right: 16.0,
            bottom: 48.0,
            left: 16.0,
        },
        ..Box::default()
    })
}

/// The progresses of the images, of the states that the GPU draws, the
/// indeterminate one's moving part the window server's: bars normal, in
/// error and paused, of their pills, and one of its own colors, square and
/// 10 points high; rings of each, of lines 2, 3 and 4 points thick.
fn progress_tree() -> Node {
    use lokyr_plugin_sdk::view::{Progress, ProgressShape, ProgressState};
    let states = [
        (ProgressState::Normal, 0.4),
        (ProgressState::Error, 0.7),
        (ProgressState::Paused, 0.55),
    ];
    let mut bars: Vec<Node> = states
        .into_iter()
        .map(|(state, value)| {
            Node::new(Kind::Progress(Progress {
                state,
                value,
                ..Progress::default()
            }))
        })
        .collect();
    bars.push(
        Node::new(Kind::Progress(Progress {
            value: 0.65,
            radius: Some(0.0),
            ..Progress::default()
        }))
        .layout(Box {
            height: Length::Points(10.0),
            ..Box::default()
        })
        .style(Style {
            background: Some("field.border".into()),
            color: Some("accent".into()),
            ..Style::default()
        }),
    );
    let rings: Vec<Node> = states
        .into_iter()
        .zip([(16.0, 2.0), (24.0, 3.0), (32.0, 4.0)])
        .map(|((state, value), (side, thickness))| {
            Node::new(Kind::Progress(Progress {
                state,
                value,
                shape: ProgressShape::Ring,
                thickness,
                ..Progress::default()
            }))
            .layout(Box {
                width: Length::Points(side),
                height: Length::Points(side),
                ..Box::default()
            })
        })
        .collect();
    let rings = Node::container(rings).layout(Box {
        gap: 16.0,
        align: Align::Center,
        ..Box::default()
    });
    bars.push(rings);
    // Dots of the states' values, and an indeterminate one's track.
    let dots: Vec<Node> = states
        .into_iter()
        .chain([(ProgressState::Indeterminate, 0.0)])
        .zip([10.0, 16.0, 24.0, 16.0])
        .map(|((state, value), side)| {
            Node::new(Kind::Progress(Progress {
                state,
                value,
                shape: ProgressShape::Dots,
                ..Progress::default()
            }))
            .layout(Box {
                width: Length::Points(side),
                height: Length::Points(side),
                ..Box::default()
            })
        })
        .collect();
    bars.push(Node::container(dots).layout(Box {
        gap: 16.0,
        align: Align::Center,
        ..Box::default()
    }));
    Node::container(bars).layout(Box {
        direction: Direction::Column,
        gap: 12.0,
        padding: Edges::all(16.0),
        ..Box::default()
    })
}

/// The node of `key` in `tree`.
fn find(tree: &Tree, key: &str) -> NodeId {
    tree.by_key(&key.into()).unwrap()
}

/// Draws the view tree at `scale`, in the dark variant or the light one,
/// and compares the image with its golden one.
fn check_view(dark: bool, scale: u32) {
    check_tree("view", &view_tree(), (dark, scale), |_| {
        (None, None, None, None)
    });
}

/// Draws the widgets' tree at `scale`, in the dark variant or the light
/// one, of the focus on the button of an icon, the pointer over the
/// toggle off and pressing a button, and the toggle's tooltip, and
/// compares the image with its golden one.
fn check_widgets(dark: bool, scale: u32) {
    check_tree("widgets", &widgets_tree(), (dark, scale), |tree| {
        (
            Some(find(tree, "add")),
            Some(find(tree, "word")),
            Some(find(tree, "pressed")),
            Some("Whole Word"),
        )
    });
}

/// Draws `root` at `scale`, in the dark variant or the light one, of the
/// node with the focus, the one under the pointer, the one pressed and a
/// tooltip under that one, as `states` gives them of the tree, and
/// compares the image with its golden one, `{name}-{appearance}-{scale}x`.
fn check_tree(
    name: &str,
    root: &Node,
    (dark, scale): (bool, u32),
    states: impl Fn(
        &Tree,
    ) -> (
        Option<NodeId>,
        Option<NodeId>,
        Option<NodeId>,
        Option<&'static str>,
    ),
) {
    let Some(mut drawing) = Drawing::new(scale as f32) else {
        return;
    };
    let variant = Theme::builtin().variant(dark).clone();
    let mut tree = Tree::new();
    let changes = tree.update(root).unwrap();
    let mut layout = Layout::new(scale as f32);
    layout.update(&tree, &changes);
    let width = VIEW_WIDTH * scale as f32;
    layout.compute(&tree, Some(width), None, &mut drawing.font);
    let frame = layout.frame(&tree, tree.root().unwrap()).unwrap();
    let (width, height) = (frame.width as u32, frame.height as u32);
    let (focused, hovered, pressed, tip) = states(&tree);
    drawing.font.begin_frame();
    let mut items = paint(
        &tree,
        &layout,
        &mut drawing.font,
        &variant.tokens,
        (0.0, 0.0),
        Shown {
            dark,
            focused,
            fields: &Fields::new(),
            hovered,
            pressed,
            lists: &Lists::new(),
            motion: None,
            fills: Fills::All,
        },
    );
    // The tooltip, as the pointer rested at the middle of the node under
    // it.
    if let (Some(text), Some(node)) = (tip, hovered) {
        let over = layout.frame(&tree, node).unwrap();
        let at = (over.x + over.width / 2.0, over.y + over.height / 2.0);
        items.extend(tooltip(
            &mut drawing.font,
            &variant.tokens,
            text,
            (at, (frame.width, frame.height)),
            (scale as f32, dark),
        ));
    }
    let list = DisplayList {
        background: lokyr_render_core::Color::from_rgb8(
            variant.colors.background.r,
            variant.colors.background.g,
            variant.colors.background.b,
        ),
        items,
        frame: drawing.font.atlas_frame(),
    };
    let updates = drawing.font.texture_updates();
    drawing.renderer.update_textures(&updates).unwrap();
    let pixels = drawing
        .renderer
        .draw_offscreen(&list, width, height)
        .unwrap();
    let appearance = if dark { "dark" } else { "light" };
    let most = (f64::from(width * height) * SHARE) as usize;
    compare(
        &format!("{name}-{appearance}-{scale}x"),
        (width, height, &pixels),
        most,
    );
}

/// The directory of the golden images.
fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Writes `pixels`, RGBA, `width` by `height`, as a PNG at `path`.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}

/// The PNG at `path`: its width, its height and its RGBA pixels.
fn read_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).ok()?));
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    pixels.truncate(info.buffer_size());
    Some((info.width, info.height, pixels))
}

/// How many of the pixels of two images of the same size differ by more
/// than `CHANNEL` in one of their components, and a map of them, red on
/// the expected image faded.
fn differences(expected: &[u8], actual: &[u8]) -> (usize, Vec<u8>) {
    let mut count = 0;
    let mut map = Vec::with_capacity(expected.len());
    for (want, got) in expected
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0)
    {
        let differs = want.iter().zip(got).any(|(a, b)| a.abs_diff(*b) > CHANNEL);
        if differs {
            count += 1;
            map.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            map.extend(want[..3].iter().map(|c| c / 4));
            map.push(255);
        }
    }
    (count, map)
}

/// Draws `scenes` at `scale`, and compares the image with its golden one.
fn check(scenes: &[Scene], scale: u32) {
    let Some(mut drawing) = Drawing::new(scale as f32) else {
        return;
    };
    let (width, height, pixels) = drawing.draw(scenes);
    let most = if scenes[0].shapes {
        SHAPES * (scale * scale) as usize
    } else {
        (f64::from(width * height) * SHARE) as usize
    };
    let name = format!("{}-{scale}x", scenes[0].name);
    compare(&name, (width, height, &pixels), most);
}

/// Compares the image `name` drawn, its width, its height and its pixels,
/// with its golden one, of which `most` pixels can differ; or writes it as
/// the golden one, when asked.
fn compare(name: &str, (width, height, pixels): (u32, u32, &[u8]), most: usize) {
    let path = golden().join(format!("{name}.png"));
    if std::env::var_os("LOKYR_GOLDEN_UPDATE").is_some() {
        write_png(&path, width, height, pixels);
        return;
    }
    let failed = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden");
    let actual = failed.join(format!("{name}.png"));
    let Some((want_width, want_height, expected)) = read_png(&path) else {
        write_png(&actual, width, height, pixels);
        panic!(
            "{} is not there, or is not an RGBA image: the image drawn is {}; \
             LOKYR_GOLDEN_UPDATE=1 writes the golden images",
            path.display(),
            actual.display()
        );
    };
    if (want_width, want_height) != (width, height) {
        write_png(&actual, width, height, pixels);
        panic!(
            "{name} is {width} by {height} pixels, where {} is {want_width} by {want_height}: \
             the image drawn is {}",
            path.display(),
            actual.display()
        );
    }
    let (count, map) = differences(&expected, pixels);
    if count > most {
        write_png(&actual, width, height, pixels);
        let map_path = failed.join(format!("{name}-differences.png"));
        write_png(&map_path, width, height, &map);
        panic!(
            "{name}: {count} pixels differ from {}, more than {most}: the image drawn is {}, \
             and the pixels that differ are red in {}",
            path.display(),
            actual.display(),
            map_path.display()
        );
    }
}

#[test]
fn text_draws_as_its_golden_image() {
    for scale in [1, 2] {
        check(&text(), scale);
    }
}

#[test]
fn colors_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&colors(), scale);
    }
}

#[test]
fn decorations_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&decorations(), scale);
    }
}

#[test]
fn the_cursor_s_shapes_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&cursors(), scale);
    }
}

#[test]
fn the_shapes_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&shapes(), scale);
    }
}

#[test]
fn a_pane_without_the_keyboard_draws_faded_as_its_golden_image() {
    for scale in [1, 2] {
        check(&dimmed(), scale);
    }
}

#[test]
fn panes_headers_draw_as_their_golden_images() {
    // The header of the pane that has the keyboard, then another's.
    let root = Node::container(vec![
        lokyr_splits::title_header("src — vim", true),
        lokyr_splits::title_header("~ — cargo test", false),
    ])
    .layout(Box {
        direction: Direction::Column,
        width: Length::Points(320.0),
        gap: 8.0,
        padding: Edges::all(8.0),
        ..Box::default()
    });
    for dark in [false, true] {
        for scale in [1, 2] {
            check_tree("pane-headers", &root, (dark, scale), |_| {
                (None, None, None, None)
            });
        }
    }
}

#[test]
fn the_find_bar_draws_as_its_golden_images() {
    use lokyr_search_plugin::{BarTexts, BarView, Options, find_bar};
    // Of no option, of no result, its field's placeholder showing; then
    // of case and regex, a count, the pointer over the up arrow.
    let quiet = find_bar(
        &BarView {
            count: "No results".to_owned(),
            ..BarView::default()
        },
        &BarTexts::default(),
    );
    let busy = find_bar(
        &BarView {
            options: Options {
                sensitive: true,
                regex: true,
                whole_word: false,
            },
            count: "3 of 12".to_owned(),
        },
        &BarTexts::default(),
    );
    let room = |bar: Node| {
        Node::container(vec![bar]).layout(Box {
            width: Length::Points(470.0),
            padding: Edges::all(8.0),
            ..Box::default()
        })
    };
    for dark in [false, true] {
        for scale in [1, 2] {
            check_tree("find-bar", &room(quiet.clone()), (dark, scale), |_| {
                (None, None, None, None)
            });
            check_tree(
                "find-bar-options",
                &room(busy.clone()),
                (dark, scale),
                |tree| (None, tree.by_key(&"up".into()), None, None),
            );
        }
    }
}

#[test]
fn a_selection_draws_as_its_golden_image() {
    for scale in [1, 2] {
        check(&selection(), scale);
    }
}

#[test]
fn a_view_tree_draws_as_its_golden_images_light_and_dark() {
    for dark in [false, true] {
        for scale in [1, 2] {
            check_view(dark, scale);
        }
    }
}

#[test]
fn buttons_toggles_separators_and_a_tooltip_draw_as_their_golden_images() {
    for dark in [false, true] {
        for scale in [1, 2] {
            check_widgets(dark, scale);
        }
    }
}

#[test]
fn progresses_draw_as_their_golden_images() {
    for dark in [false, true] {
        for scale in [1, 2] {
            check_tree("progress", &progress_tree(), (dark, scale), |_| {
                (None, None, None, None)
            });
        }
    }
}

/// The tabs of a tab bar's images: a tab of its program's color, the
/// active one, and one of a title too long for its room.
fn bar_tabs() -> Vec<TabInfo> {
    let tab = |id: u64, title: &str, active: bool| TabInfo {
        id: TabId(id),
        window: WindowId(1),
        title: title.to_owned(),
        color: None,
        active,
        panes: vec![PaneId(id)],
        focused: PaneId(id),
        zoomed: None,
        activity: false,
        bell: false,
        progress: None,
        printing: false,
        running: false,
        alternate: false,
        secure: false,
        agent: None,
    };
    let mut colored = tab(1, "lokyr — vim", false);
    colored.color = Some(Color::Rgb(0x3a, 0x7b, 0xe6));
    vec![
        colored,
        tab(2, "-zsh", true),
        tab(
            3,
            "~/Documents/git/sephiroth74/rust/lokyr — cargo test",
            false,
        ),
    ]
}

/// The room of a titlebar of tabs over a sidebar's top, its height and
/// the width of the window's buttons, in points.
const SIDEBAR_ROOM: (f32, f32) = (40.0, 84.0);

/// The tabs of a bar's image: the three of [`bar_tabs`], of the keys of
/// the default bindings; one alone, which the config shows a bar for;
/// twelve, more than the keys' nine; the three, of keys of the user's
/// bindings, one of none; four of their marks; or one alone, whose bar
/// hides, of its title in the titlebar.
#[derive(Clone, Copy)]
enum BarTabs {
    Three,
    One,
    Many,
    Bound,
    Marked,
    Title,
    TitleProgress,
}

/// A bar's tabs, the keys of each, and the config's keys past the look's.
type ShownTabs = (Vec<TabInfo>, Vec<Option<String>>, Vec<(&'static str, Data)>);

impl BarTabs {
    /// The tabs, the keys of each, and the config's keys past the look's.
    fn shown(self) -> ShownTabs {
        let keys = |keys: &[&str]| keys.iter().map(|keys| Some((*keys).to_owned())).collect();
        match self {
            Self::Three => (bar_tabs(), keys(&["⌘1", "⌘2", "⌘3"]), Vec::new()),
            // A single tab whose bar hides, of its title in the titlebar,
            // of the protected input and a program that runs and prints.
            Self::Title => {
                let mut tab = bar_tabs().swap_remove(1);
                tab.id = TabId(1);
                tab.secure = true;
                tab.running = true;
                tab.printing = true;
                (vec![tab], keys(&["⌘1"]), Vec::new())
            }
            // A single tab's title of its program's progress, 40 percent,
            // in a ring.
            Self::TitleProgress => {
                use lokyr_plugin_sdk::host::TabProgress;
                use lokyr_plugin_sdk::view::ProgressState;
                let mut tab = bar_tabs().swap_remove(1);
                tab.id = TabId(1);
                tab.running = true;
                tab.printing = true;
                tab.progress = Some(TabProgress {
                    state: ProgressState::Normal,
                    value: Some(0.4),
                });
                (vec![tab], keys(&["⌘1"]), Vec::new())
            }
            Self::One => {
                let mut tab = bar_tabs().swap_remove(1);
                tab.id = TabId(1);
                (
                    vec![tab],
                    keys(&["⌘1"]),
                    vec![("hide-when-single", Data::Boolean(false))],
                )
            }
            Self::Many => {
                let titles = [
                    "-zsh",
                    "vim",
                    "htop",
                    "lokyr — cargo test",
                    "ssh pi",
                    "-zsh",
                    "man ls",
                    "git log",
                    "~/Downloads",
                    "python3",
                    "-zsh",
                    "tail -f lokyr.log",
                ];
                let tabs = titles
                    .iter()
                    .enumerate()
                    .map(|(at, title)| {
                        let id = at as u64 + 1;
                        TabInfo {
                            id: TabId(id),
                            window: WindowId(1),
                            title: (*title).to_owned(),
                            color: (id == 2).then_some(Color::Rgb(0xd9, 0x4a, 0x38)),
                            active: id == 5,
                            panes: vec![PaneId(id)],
                            focused: PaneId(id),
                            zoomed: None,
                            activity: false,
                            bell: false,
                            progress: None,
                            printing: false,
                            running: false,
                            alternate: false,
                            secure: false,
                            agent: None,
                        }
                    })
                    .collect();
                let keys = (1..=titles.len())
                    .map(|at| (at <= 9).then(|| format!("⌘{at}")))
                    .collect();
                (tabs, keys, Vec::new())
            }
            Self::Bound => {
                let keys = vec![Some("⌃1".to_owned()), Some("⌥⌘2".to_owned()), None];
                (bar_tabs(), keys, Vec::new())
            }
            Self::Marked => {
                use lokyr_plugin_sdk::host::{AgentState, TabProgress};
                use lokyr_plugin_sdk::view::ProgressState;
                let mut tabs = bar_tabs();
                // The first: output and a bell while it did not show, and
                // its program's progress, 40 percent.
                tabs[0].activity = true;
                tabs[0].bell = true;
                tabs[0].progress = Some(TabProgress {
                    state: ProgressState::Normal,
                    value: Some(0.4),
                });
                // The active one: the protected input, of a program that
                // runs and prints.
                tabs[1].secure = true;
                tabs[1].running = true;
                tabs[1].printing = true;
                // The third: an agent that waits, whose task failed.
                tabs[2].agent = Some(AgentState::Waiting);
                tabs[2].progress = Some(TabProgress {
                    state: ProgressState::Error,
                    value: Some(0.7),
                });
                // A fourth: an agent at work, of no known end.
                let mut working = bar_tabs().swap_remove(1);
                working.id = TabId(4);
                working.title = "claude".to_owned();
                working.active = false;
                working.agent = Some(AgentState::Working);
                working.progress = Some(TabProgress {
                    state: ProgressState::Paused,
                    value: Some(0.55),
                });
                tabs.push(working);
                let keys = vec![
                    Some("⌘1".to_owned()),
                    Some("⌘2".to_owned()),
                    Some("⌘3".to_owned()),
                    Some("⌘4".to_owned()),
                ];
                (tabs, keys, Vec::new())
            }
        }
    }
}

/// A tab bar's image: its name, its config's `style` and `position`, the
/// room that a window gives it, its width and its height, in points, its
/// height its own without one, and its tabs.
type Bar = (
    &'static str,
    &'static str,
    &'static str,
    (f32, Option<f32>),
    BarTabs,
);

/// The images of the tab bar: each of its looks at each of its places, as
/// `[plugins.tabs]` asks for them, of their width and, in the titlebar and
/// down a column, of their height, in points, the pointer over the third
/// tab; a single tab, many, and the keys of the user's bindings.
const BARS: [Bar; 25] = [
    (
        "standard-top",
        "standard",
        "top",
        (640.0, None),
        BarTabs::Three,
    ),
    (
        "standard-bottom",
        "standard",
        "bottom",
        (640.0, None),
        BarTabs::Three,
    ),
    (
        "unified",
        "unified",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Three,
    ),
    (
        "seamless-top",
        "seamless",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Three,
    ),
    (
        "seamless-bottom",
        "seamless",
        "bottom",
        (640.0, None),
        BarTabs::Three,
    ),
    (
        "standard-left",
        "standard",
        "left",
        (200.0, Some(180.0)),
        BarTabs::Three,
    ),
    (
        "unified-left",
        "unified",
        "left",
        (200.0, Some(180.0)),
        BarTabs::Three,
    ),
    (
        "seamless-left",
        "seamless",
        "left",
        (200.0, Some(180.0)),
        BarTabs::Three,
    ),
    (
        "unified-single",
        "unified",
        "top",
        (640.0, Some(40.0)),
        BarTabs::One,
    ),
    (
        "seamless-left-single",
        "seamless",
        "left",
        (200.0, Some(120.0)),
        BarTabs::One,
    ),
    (
        "unified-many",
        "unified",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Many,
    ),
    (
        "seamless-many",
        "seamless",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Many,
    ),
    (
        "standard-many",
        "standard",
        "top",
        (640.0, None),
        BarTabs::Many,
    ),
    (
        "unified-left-many",
        "unified",
        "left",
        (200.0, Some(420.0)),
        BarTabs::Many,
    ),
    (
        "unified-bound",
        "unified",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Bound,
    ),
    (
        "unified-bottom",
        "unified",
        "bottom",
        (640.0, None),
        BarTabs::Three,
    ),
    (
        "unified-right",
        "unified",
        "right",
        (200.0, Some(180.0)),
        BarTabs::Three,
    ),
    (
        "seamless-right",
        "seamless",
        "right",
        (200.0, Some(180.0)),
        BarTabs::Three,
    ),
    // At the top with no titlebar, as in full screen, of its own height.
    (
        "seamless-full-screen",
        "seamless",
        "top",
        (640.0, None),
        BarTabs::Three,
    ),
    // The tabs' marks: what came while they did not show, what runs, the
    // protected input, agents' states and programs' progress.
    (
        "unified-marks",
        "unified",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Marked,
    ),
    (
        "seamless-marks",
        "seamless",
        "top",
        (640.0, Some(40.0)),
        BarTabs::Marked,
    ),
    (
        "unified-left-marks",
        "unified",
        "left",
        (200.0, Some(200.0)),
        BarTabs::Marked,
    ),
    // A single tab's title, across a titlebar of the system's height; of
    // its program's progress.
    (
        "unified-title",
        "unified",
        "top",
        (640.0, Some(28.0)),
        BarTabs::Title,
    ),
    (
        "unified-title-progress",
        "unified",
        "top",
        (640.0, Some(28.0)),
        BarTabs::TitleProgress,
    ),
    // More rows than the column holds, which scroll.
    (
        "standard-left-scrolled",
        "standard",
        "left",
        (200.0, Some(200.0)),
        BarTabs::Many,
    ),
];

/// Draws the tab bar of `style` at `position`, laid out as a window lays
/// it out in a room of `size` points, its height its own without one, at
/// `scale`, in the dark variant or the light one, the pointer over the
/// third tab, which shows its close button, and compares the image with
/// its golden one, `tabs-{name}-{appearance}-{scale}x`.
fn check_tab_bar((name, style, position, (width, height), shown): Bar, (dark, scale): (bool, u32)) {
    let Some(mut drawing) = Drawing::new(scale as f32) else {
        return;
    };
    let variant = Theme::builtin().variant(dark).clone();
    let (tabs, keys, more) = shown.shown();
    let mut config = BTreeMap::from([
        ("style".to_owned(), Data::Text(style.to_owned())),
        ("position".to_owned(), Data::Text(position.to_owned())),
    ]);
    config.extend(more.into_iter().map(|(key, value)| (key.to_owned(), value)));
    let config = Data::Map(config);
    let (root, place) = lokyr_tabs::tab_bar(&tabs, &keys, Some("⌥⌘1"), &config);
    let mut tree = Tree::new();
    let changes = tree.update(&root.unwrap()).unwrap();
    let pixels = scale as f32;
    let mut layout = Layout::new(pixels);
    layout.update(&tree, &changes);
    // A sidebar under a titlebar of tabs, and the window's buttons at its
    // left, as macOS's are; a title between their room on either side.
    if matches!(
        place,
        lokyr_plugin_sdk::host::BarPlace::Sidebar { .. } | lokyr_plugin_sdk::host::BarPlace::Title
    ) {
        let room = Room {
            titlebar: SIDEBAR_ROOM.0 * pixels,
            buttons: SIDEBAR_ROOM.1 * pixels,
        };
        layout.set_room(&tree, room);
    }
    layout.compute(
        &tree,
        Some(width * pixels),
        height.map(|height| height * pixels),
        &mut drawing.font,
    );
    let frame = layout.frame(&tree, tree.root().unwrap()).unwrap();
    // Down a column, each row of its height, however many.
    if matches!(position, "left" | "right") {
        for tab in &tabs {
            let key = view::Key::from(format!("tab-{}", tab.id.0).as_str());
            let row = tree.by_key(&key).and_then(|id| layout.frame(&tree, id));
            assert_eq!(row.map(|row| row.height), Some(28.0 * pixels), "{name}");
        }
    }
    // In the titlebar, the tabs and the button of a new tab take its
    // presses, the window's keys and the room around them the window's.
    if place == lokyr_plugin_sdk::host::BarPlace::Titlebar && height.is_some() {
        let areas = lokyr_ui_core::chrome::titlebar_areas(&tree, &layout);
        // Of many tabs, those that the bar's scroll shows alone.
        if matches!(shown, BarTabs::Many) {
            assert!(areas.len() < tabs.len() + 1, "{name}: {areas:?}");
        } else {
            assert_eq!(areas.len(), tabs.len() + 1, "{name}: {areas:?}");
        }
        assert!(areas.iter().all(|area| area.x > 0.0), "{name}: {areas:?}");
    }
    let (width, height) = (frame.width as u32, frame.height as u32);
    // The pointer over the third tab, its hover shown whole.
    let now = Instant::now();
    let mut animations = Animations::new();
    animations.laid_out(&tree, &layout, now, true);
    let hovered = view::Key::from("tab-3");
    animations.hovered([&hovered], now);
    drawing.font.begin_frame();
    let items = paint(
        &tree,
        &layout,
        &mut drawing.font,
        &variant.tokens,
        (0.0, 0.0),
        Shown {
            dark,
            focused: None,
            fields: &Fields::new(),
            hovered: tree.by_key(&hovered),
            pressed: None,
            lists: &Lists::new(),
            motion: Some((&animations, now)),
            fills: Fills::All,
        },
    );
    let list = DisplayList {
        background: lokyr_render_core::Color::from_rgb8(
            variant.colors.background.r,
            variant.colors.background.g,
            variant.colors.background.b,
        ),
        items,
        frame: drawing.font.atlas_frame(),
    };
    let updates = drawing.font.texture_updates();
    drawing.renderer.update_textures(&updates).unwrap();
    let pixels = drawing
        .renderer
        .draw_offscreen(&list, width, height)
        .unwrap();
    let appearance = if dark { "dark" } else { "light" };
    // Twelve tabs cut each title near a cluster's end, which a release of
    // macOS's fonts measures a little apart: a cut a cluster sooner or
    // later moves a glyph, of a share of their own.
    let share = if matches!(shown, BarTabs::Many) {
        4.0 * SHARE
    } else {
        SHARE
    };
    let most = (f64::from(width * height) * share) as usize;
    compare(
        &format!("tabs-{name}-{appearance}-{scale}x"),
        (width, height, &pixels),
        most,
    );
}

#[test]
fn a_title_of_either_direction_is_shortened_within_its_room() {
    let Some(mut drawing) = Drawing::new(2.0) else {
        return;
    };
    let font = &mut drawing.font;
    let room = 80.0;
    for text in [
        "~/Documents/git/sephiroth74/rust/lokyr — cargo test",
        "שלום עולם, זוהי כותרת ארוכה מאוד של לשונית",
        "ab שלום עולם cd ef gh ij kl mn",
        "🇮🇹 👨‍👩‍👧 a family 🧑🏽‍💻 at work, of flags",
    ] {
        let shortened = font.interface_shortened(text, room);
        // Of an ellipsis, after a prefix of the text that ends at a
        // cluster's end, within the room.
        let prefix = shortened.strip_suffix('…').expect("an ellipsis");
        assert!(
            text.starts_with(prefix) && prefix.len() < text.len(),
            "{text}: {shortened}"
        );
        let width = Measure::text(font, &shortened).0;
        assert!(width <= room + 0.5, "{text}: {shortened}, {width}");
        assert!(!prefix.is_empty(), "{text}: {shortened}");
        // The next frame's is the same, of no shaping again.
        assert!(std::rc::Rc::ptr_eq(
            &shortened,
            &font.interface_shortened(text, room)
        ));
    }
}

#[test]
fn the_tab_bar_draws_as_its_golden_images_of_each_look_and_place() {
    for bar in BARS {
        for dark in [false, true] {
            for scale in [1, 2] {
                check_tab_bar(bar, (dark, scale));
            }
        }
    }
}

#[test]
fn a_pixel_differs_past_the_tolerance_alone() {
    let expected = [10, 20, 30, 255, 0, 0, 0, 255];
    let close = [10 + CHANNEL, 20, 30 - CHANNEL, 255, 0, 0, 0, 255];
    assert_eq!(differences(&expected, &close).0, 0);
    let far = [10, 20, 31 + CHANNEL, 255, 0, 0, 0, 255];
    let (count, map) = differences(&expected, &far);
    assert_eq!(count, 1);
    assert_eq!(map, [255, 0, 0, 255, 0, 0, 0, 255]);
}
