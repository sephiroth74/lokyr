//! The images of cells that the plugins ask the app for: rendered by the
//! windows' renderer, at the focused window's scale or the one asked for,
//! and refused of an app that draws nothing or of an image too large.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Code, Command};
use lokyr_kernel::plugins::image::{CellRow, ImageSpec, MOST_COLUMNS, MOST_ROWS};
use lokyr_kernel::plugins::view::Node;
use lokyr_kernel::plugins::{Cx, Plugin};
use lokyr_text_core::FontConfig;

use super::*;

/// What the plugin of [`Imager`] got of each image: its size, its scale
/// and its PNG's header, or its error's code.
type Got = Rc<RefCell<Vec<Result<(u32, u32, f32, Vec<u8>), Code>>>>;

/// A plugin whose `imager.snap` renders an image of two rows, whose
/// `imager.retina` renders one at a scale of 3, and whose `imager.huge`
/// renders one too large.
struct Imager {
    got: Got,
}

const MANIFEST: &str = "[plugin]\nname = \"lokyr/imager\"\nkind = \"builtin\"\nlicense = \"MIT\"\n";

impl Plugin for Imager {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in [
            "imager.snap",
            "imager.retina",
            "imager.huge",
            "imager.least",
        ] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let row = |text: &str| CellRow {
            text: text.to_owned(),
            runs: Vec::new(),
        };
        let image = match name {
            "imager.huge" => ImageSpec {
                rows: vec![row(&"x".repeat(usize::from(MOST_COLUMNS))); MOST_ROWS],
                scale: Some(4.0),
                ..ImageSpec::default()
            },
            _ => ImageSpec {
                rows: vec![row("ls -la"), row("日本語")],
                title: Some("zsh".to_owned()),
                buttons: true,
                scale: (name == "imager.retina").then_some(3.0),
                min_columns: (name == "imager.least").then_some(80),
                min_rows: (name == "imager.least").then_some(10),
                ..ImageSpec::default()
            },
        };
        let got = cx
            .render_image(image)
            .map(|image| {
                let header = image.png.0[..24].to_vec();
                (image.width, image.height, image.scale, header)
            })
            .map_err(|error| error.code);
        self.got.borrow_mut().push(got);
        Ok(())
    }
}

/// The app of the plugin of [`Imager`], drawing with `drawing`, of a
/// window focused, on a display of a scale of 2, whose frames drew.
fn imager(drawing: Option<Drawing>) -> (Fake, App, WindowId, Got) {
    let got = Got::default();
    let plugin = Imager {
        got: Rc::clone(&got),
    };
    let mut fake = Fake::new(2.0);
    let mut app = App::new(drawing, unpadded(), None)
        .with_program(quiet())
        .with_builtin(MANIFEST, Box::new(plugin));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    for _ in 0..4 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    (fake, app, window, got)
}

/// The width and the height that `header`, a PNG's first bytes, tells.
fn png_size(header: &[u8]) -> (u32, u32) {
    assert_eq!(&header[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&header[12..16], b"IHDR");
    let number = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
    (number(16), number(20))
}

#[test]
fn a_plugin_s_image_is_rendered_by_the_windows_renderer_at_the_focused_window_s_scale() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let (mut fake, mut app, _, got) = imager(Some(drawing));
    for name in ["imager.snap", "imager.retina", "imager.least"] {
        fake.send_app(&mut app, AppEvent::Command(name.to_owned()));
    }
    let got: Vec<_> = got.borrow_mut().drain(..).collect();
    let [
        Ok((width, height, scale, header)),
        Ok((retina_width, _, retina, _)),
        Ok((least_width, least_height, _, _)),
    ] = &got[..]
    else {
        panic!("{got:?}");
    };
    // Of 80 columns and 10 rows at least, wider and taller than its text.
    assert!(least_width > width && least_height > height, "{got:?}");
    // At the focused window's scale, a PNG of its size.
    assert_eq!(*scale, 2.0);
    assert_eq!(png_size(header), (*width, *height));
    // Wider than its cells as its bar's buttons and title need, and of the
    // bar, the padding and two rows.
    assert!(*width > 0 && *height > 0);
    // At the scale asked for, its pixels of the points' half again.
    assert_eq!(*retina, 3.0);
    assert!(retina_width > width, "{retina_width} {width}");
}

#[test]
fn an_image_is_refused_of_an_app_that_draws_nothing_and_past_the_most_pixels() {
    let (mut fake, mut app, _, got) = imager(None);
    fake.send_app(&mut app, AppEvent::Command("imager.snap".to_owned()));
    assert_eq!(
        got.borrow_mut().drain(..).collect::<Vec<_>>(),
        [Err(Code::Unavailable)]
    );
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let (mut fake, mut app, _, got) = imager(Some(drawing));
    fake.send_app(&mut app, AppEvent::Command("imager.huge".to_owned()));
    assert_eq!(
        got.borrow_mut().drain(..).collect::<Vec<_>>(),
        [Err(Code::Unavailable)]
    );
}

/// What the plugin of [`Keeper`] got of its kept images, their ids.
type Kept = Rc<RefCell<Vec<u64>>>;

/// A plugin whose `keeper.keep` renders an image that the host keeps and
/// opens a panel of its node, whose `keeper.drop` lets its last image go.
struct Keeper {
    kept: Kept,
}

impl Plugin for Keeper {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["keeper.keep", "keeper.drop"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        use lokyr_kernel::plugins::ui::{Anchor, Corner, PanelOptions, Role};
        if name == "keeper.drop" {
            let last = self.kept.borrow().last().copied();
            return last.map_or(Ok(()), |id| cx.drop_image(id));
        }
        let image = cx.render_image(ImageSpec {
            rows: vec![CellRow {
                text: "ls".to_owned(),
                runs: Vec::new(),
            }],
            keep: true,
            ..ImageSpec::default()
        })?;
        self.kept
            .borrow_mut()
            .push(image.id.expect("a kept image's id"));
        let window = cx.focused_window().expect("a focused window");
        cx.open_panel(
            "preview",
            PanelOptions {
                anchor: Anchor::WindowCorner {
                    window,
                    corner: Corner::TopLeft,
                },
                role: Role::Dialog,
                close_outside: false,
                close_escape: true,
                takes_keys: false,
            },
        )
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &lokyr_kernel::plugins::ui::ViewId) -> Option<Node> {
        use lokyr_kernel::plugins::view::{Layout as Box, Length};
        let id = *self.kept.borrow().last()?;
        Some(Node::image(id).layout(Box {
            width: Length::Points(120.0),
            height: Length::Points(60.0),
            ..Box::default()
        }))
    }
}

#[test]
fn a_kept_image_s_node_draws_its_picture_and_the_picture_goes_as_it_is_let_go() {
    let kept = Kept::default();
    let plugin = Keeper {
        kept: Rc::clone(&kept),
    };
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::new(2.0);
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(quiet())
        .with_builtin(
            "[plugin]\nname = \"lokyr/keeper\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
            Box::new(plugin),
        );
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let frames = |fake: &mut Fake, app: &mut App| {
        for _ in 0..4 {
            fake.send(app, window, WindowEvent::Frame);
        }
    };
    frames(&mut fake, &mut app);
    fake.send_app(&mut app, AppEvent::Command("keeper.keep".to_owned()));
    frames(&mut fake, &mut app);
    let pictures = |app: &App| -> Vec<lokyr_render_core::PictureId> {
        app.windows[&window]
            .shown
            .as_ref()
            .map(|shown| {
                shown
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Picture { picture, .. } => Some(*picture),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let first = app.pictures["lokyr/keeper"][&kept.borrow()[0]].id;
    assert_eq!(pictures(&app), [first]);
    // Four more: the oldest let go past the most.
    for _ in 0..4 {
        fake.send_app(&mut app, AppEvent::Command("keeper.keep".to_owned()));
    }
    assert_eq!(app.pictures["lokyr/keeper"].len(), 4);
    let dropped = |app: &App| app.drawing.as_ref().unwrap().renderer.dropped.clone();
    assert_eq!(dropped(&app), [first]);
    // Let go, its node draws nothing.
    fake.send_app(&mut app, AppEvent::Command("keeper.drop".to_owned()));
    frames(&mut fake, &mut app);
    assert_eq!(app.pictures["lokyr/keeper"].len(), 3);
    assert_eq!(dropped(&app).len(), 2);
    assert!(pictures(&app).is_empty());
    // The plugin taken away, its images go.
    assert!(app.with_plugins(&mut fake, |plugins, cx| plugins.remove("lokyr/keeper", cx)));
    app.after_plugins(&mut fake);
    assert!(!app.pictures.contains_key("lokyr/keeper"));
    assert_eq!(dropped(&app).len(), 5);
}
