//! The images of cells that the plugins ask the app for: rendered by the
//! windows' renderer, at the focused window's scale or the one asked for,
//! and refused of an app that draws nothing or of an image too large.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Code, Command};
use lokyr_kernel::plugins::image::{CellRow, ImageSpec, MOST_COLUMNS, MOST_ROWS};
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
        for name in ["imager.snap", "imager.retina", "imager.huge"] {
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
    for name in ["imager.snap", "imager.retina"] {
        fake.send_app(&mut app, AppEvent::Command(name.to_owned()));
    }
    let got: Vec<_> = got.borrow_mut().drain(..).collect();
    let [
        Ok((width, height, scale, header)),
        Ok((retina_width, _, retina, _)),
    ] = &got[..]
    else {
        panic!("{got:?}");
    };
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
