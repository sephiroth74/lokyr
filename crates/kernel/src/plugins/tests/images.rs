//! The images of cells that plugins ask the app for: checked, then
//! rendered by the app, a call's bounded, none of a look, and its failure
//! told.

use std::sync::{Arc, Mutex};

use lokyr_plugin_sdk::image::{CellRow, ImageSpec as Spec};

use super::*;

/// The image of `text`'s one row, at `scale`.
fn spec(text: &str, scale: Option<f32>) -> Spec {
    Spec {
        rows: vec![CellRow {
            text: text.to_owned(),
            runs: Vec::new(),
        }],
        title: Some("zsh".to_owned()),
        buttons: true,
        scale,
        ..Spec::default()
    }
}

/// What a painter's calls got: each image's size and scale, or its error's
/// code.
type Got = Arc<Mutex<Vec<Result<(u32, f32), Code>>>>;

/// A plugin whose `painter.many` renders `count` images, whose
/// `painter.wrong` renders one of a control character, and whose look
/// renders one.
struct Painting {
    count: usize,
    got: Got,
}

impl Painting {
    fn paint(&self, cx: &mut Cx<'_>, image: Spec) {
        let got = cx
            .render_image(image)
            .map(|image| (image.width, image.scale))
            .map_err(|error| error.code);
        self.got.lock().unwrap().push(got);
    }
}

impl Plugin for Painting {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("painter.many"))?;
        cx.register_command(command("painter.wrong"))
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        if name == "painter.wrong" {
            self.paint(cx, spec("a\x1b[31mb", None));
            return Ok(());
        }
        for at in 0..self.count {
            let scale = (at % 2 == 1).then_some(3.0);
            self.paint(cx, spec("ls -la", scale));
        }
        Ok(())
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        self.paint(cx, spec("x", None));
        true
    }
}

#[test]
fn a_plugin_s_images_are_rendered_by_the_app_a_call_s_bounded_and_none_of_a_look() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let got = Got::default();
    plugins
        .add(
            "painter",
            Box::new(Painting {
                count: MOST_IMAGES + 1,
                got: Arc::clone(&got),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let taken =
        |got: &Got| -> Vec<Result<(u32, f32), Code>> { got.lock().unwrap().drain(..).collect() };
    // Each rendered by the app, at its scale or the display's, past the
    // bound refused before the app renders it.
    plugins
        .run("painter.many", Args::new(), &mut app.context())
        .unwrap();
    let rendered = taken(&got);
    assert_eq!(rendered.len(), MOST_IMAGES + 1);
    assert_eq!(rendered[..2], [Ok((1, 2.0)), Ok((1, 3.0))]);
    assert!(rendered[..MOST_IMAGES].iter().all(Result::is_ok));
    assert_eq!(rendered[MOST_IMAGES], Err(Code::Failed));
    assert_eq!(app.images.asked.len(), MOST_IMAGES);
    assert_eq!(app.images.asked[0], spec("ls -la", None));
    // The next call has its own.
    plugins
        .run("painter.many", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        taken(&got).iter().filter(|got| got.is_ok()).count(),
        MOST_IMAGES
    );
    // A look renders none.
    assert!(
        plugins
            .applies("painter.many", &Args::new(), &mut app.context())
            .unwrap()
    );
    assert_eq!(taken(&got), [Err(Code::Failed)]);
    // An image of a control character, refused before the app sees it.
    app.images.asked.clear();
    plugins
        .run("painter.wrong", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(taken(&got), [Err(Code::Failed)]);
    assert!(app.images.asked.is_empty());
    // The app's failure, told as what the image needs is not there.
    app.images.failure = Some("the app does not draw".to_owned());
    plugins
        .run("painter.many", Args::new(), &mut app.context())
        .unwrap();
    assert!(
        taken(&got)[..MOST_IMAGES]
            .iter()
            .all(|got| *got == Err(Code::Unavailable))
    );
}

#[test]
fn a_plugin_on_its_thread_renders_its_images_through_the_main_thread() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let manifest = Manifest::parse(
        "[plugin]\nname = \"acme/painter\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    let got = Got::default();
    let made = Arc::clone(&got);
    plugins
        .add_installed(
            manifest,
            Box::new(move || {
                Box::new(Painting {
                    count: MOST_IMAGES + 1,
                    got: Arc::clone(&made),
                })
            }),
            (&[], &[]),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let served = |plugins: &mut Plugins, app: &mut App| {
        let start = Instant::now();
        while plugins.waiting() {
            let told = plugins.serve(&mut app.context());
            assert!(told.commands.is_empty(), "{:?}", told.commands);
            assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    served(&mut plugins, &mut app);
    plugins
        .run("painter.many", Args::new(), &mut app.context())
        .unwrap();
    served(&mut plugins, &mut app);
    let rendered: Vec<_> = got.lock().unwrap().drain(..).collect();
    assert_eq!(rendered.len(), MOST_IMAGES + 1);
    assert!(rendered[..MOST_IMAGES].iter().all(Result::is_ok));
    assert_eq!(rendered[MOST_IMAGES], Err(Code::Failed));
    assert_eq!(app.images.asked.len(), MOST_IMAGES);
}

/// A plugin whose `painter.huge` renders an image of the most cells, and
/// whose `painter.titled` one of a title past its bound.
struct Huge {
    got: Got,
}

impl Plugin for Huge {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("painter.huge"))?;
        cx.register_command(command("painter.titled"))
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let image = match name {
            "painter.huge" => Spec {
                min_columns: Some(lokyr_plugin_sdk::image::MOST_COLUMNS),
                min_rows: Some(1000),
                ..spec("x", None)
            },
            _ => Spec {
                title: Some("t".repeat(300)),
                ..spec("x", None)
            },
        };
        let got = cx
            .render_image(image)
            .map(|image| (image.width, image.scale))
            .map_err(|error| error.code);
        self.got.lock().unwrap().push(got);
        Ok(())
    }
}

#[test]
fn a_plugin_s_images_take_its_budget_of_pixels_which_comes_back_in_time() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let got = Got::default();
    plugins
        .add(
            "painter",
            Box::new(Huge {
                got: Arc::clone(&got),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let huge = |plugins: &mut Plugins, app: &mut App| {
        plugins
            .run("painter.huge", Args::new(), &mut app.context())
            .unwrap();
        got.lock().unwrap().pop().unwrap()
    };
    // A million cells cost 128 million pixels, of a budget of 32: drawn,
    // then owed.
    assert!(huge(&mut plugins, &mut app).is_ok());
    assert_eq!(huge(&mut plugins, &mut app), Err(Code::Failed));
    assert_eq!(app.images.asked.len(), 1, "refused before the app draws it");
    // Given back at 4 million a second: 24 seconds owe still, 25 do not.
    app.wait(Duration::from_secs(24));
    assert_eq!(huge(&mut plugins, &mut app), Err(Code::Failed));
    app.wait(Duration::from_secs(1));
    assert!(huge(&mut plugins, &mut app).is_ok());
}

#[test]
fn an_image_past_its_bounds_reaches_the_app_cut_to_them() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let got = Got::default();
    plugins
        .add(
            "painter",
            Box::new(Huge {
                got: Arc::clone(&got),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("painter.titled", Args::new(), &mut app.context())
        .unwrap();
    assert!(got.lock().unwrap()[0].is_ok());
    let title = app.images.asked[0].title.clone().unwrap();
    assert_eq!(
        title.chars().count(),
        lokyr_plugin_sdk::image::MOST_TITLE_CHARS
    );
    assert!(title.ends_with('…'));
}
