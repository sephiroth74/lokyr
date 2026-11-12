//! The cells of a pane, as an image draws them, of `terminal-read`; an
//! image on the clipboard, of `clipboard-write`; and a file saved through
//! the save panel, of no capability.

use lokyr_plugin_sdk::host::{Bytes, TextStyle};
use lokyr_plugin_sdk::image::Image;
use lokyr_plugin_sdk::message::{NO_WINDOW, SAVING};

use super::*;

/// What the calls of a plugin got of each request, in order.
type Got = Rc<RefCell<Vec<Response>>>;

/// A plugin whose command `taker.go` makes the requests that `asks` gives,
/// given the focused pane and window, and keeps what each got, with the
/// events that it hears.
fn taker(
    asks: impl Fn(host::PaneId, host::WindowId) -> Vec<lokyr_plugin_sdk::message::Request> + 'static,
    got: &Got,
    heard: &Rc<RefCell<Vec<Event>>>,
) -> Box<dyn Plugin> {
    let (got, heard) = (Rc::clone(got), Rc::clone(heard));
    Box::new(Scripted {
        activate: Box::new(|cx| cx.register_command(command("taker.go"))),
        on_command: Box::new(move |cx, _, _| {
            let pane = cx.focused_pane().expect("a focused pane");
            let window = cx.focused_window().expect("a focused window");
            for request in asks(pane, window) {
                let response = cx.request(request);
                got.borrow_mut().push(response);
            }
            Ok(())
        }),
        on_event: Some(Box::new(move |_, events| {
            heard.borrow_mut().extend(events.iter().cloned());
            Ok(())
        })),
    })
}

/// The PNG of a clear pixel.
fn png() -> Bytes {
    Image::blank(1.0).png
}

#[test]
fn a_pane_s_screen_and_selection_read_as_their_cells_of_their_styles() {
    use lokyr_plugin_sdk::message::Request as Asked;
    let mut app = App::default();
    let window = window(&mut app, "printf '\\033[1mls\\033[0m -l\\n'; sleep 10");
    let pane = app.workspace.active_pane(window).unwrap();
    text_once(&app, pane, "ls -l");
    // Of no selection, which the look above made.
    app.workspace.pane(pane).unwrap().clear_selection();
    let (got, heard) = (Got::default(), Rc::default());
    let mut plugins = Plugins::new();
    plugins
        .add(
            "taker",
            taker(
                |pane, _| vec![Asked::ScreenCells(pane), Asked::SelectionCells(pane)],
                &got,
                &heard,
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let run = |plugins: &mut Plugins, app: &mut App| -> Vec<Response> {
        plugins
            .run("taker.go", Args::new(), &mut app.context())
            .unwrap();
        got.borrow_mut().drain(..).collect()
    };
    let read = run(&mut plugins, &mut app);
    let [Response::Cells(Some(screen)), Response::Cells(None)] = &read[..] else {
        panic!("{read:?}");
    };
    // A row of the view each, the first of its text and its bold run.
    assert_eq!(screen.len(), 24);
    assert_eq!(screen[0].text, "ls -l");
    let bold = TextStyle {
        bold: true,
        ..TextStyle::default()
    };
    assert_eq!(
        (
            screen[0].runs[0].first,
            screen[0].runs[0].last,
            screen[0].runs[0].style
        ),
        (0, 1, bold)
    );
    // The selection's cells, of the columns that it takes.
    let row = app.workspace.pane(pane).unwrap().viewport().unwrap().top;
    app.workspace.pane(pane).unwrap().select(
        (3, row),
        (4, row),
        lokyr_vt_core::terminal::SelectionShape::Cells,
        lokyr_vt_core::terminal::SelectionUnit::Cell,
    );
    let read = run(&mut plugins, &mut app);
    let Response::Cells(Some(selected)) = &read[1] else {
        panic!("{read:?}");
    };
    assert_eq!(
        selected
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>(),
        ["-l"]
    );
    assert_eq!(selected[0].runs[0].style, TextStyle::default());
}

#[test]
fn a_png_goes_on_the_clipboard_in_place_of_its_text_and_what_is_none_is_refused() {
    use lokyr_plugin_sdk::message::Request as Asked;
    let mut app = App::default();
    window(&mut app, "sleep 10");
    app.clipboard.0 = Some("old".to_owned());
    let (got, heard) = (Got::default(), Rc::default());
    let mut plugins = Plugins::new();
    plugins
        .add(
            "taker",
            taker(
                |_, _| {
                    vec![
                        Asked::SetClipboardImage(Bytes(b"GIF89a".to_vec())),
                        Asked::SetClipboardImage(png()),
                    ]
                },
                &got,
                &heard,
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("taker.go", Args::new(), &mut app.context())
        .unwrap();
    let got: Vec<Response> = got.borrow_mut().drain(..).collect();
    assert!(matches!(&got[0], Response::Failed(error) if error.message.contains("PNG")));
    assert_eq!(got[1], Response::Done);
    assert_eq!(app.clipboard.0, None);
    assert_eq!(app.clipboard.2, Some(png().0));
}

#[test]
fn a_save_asks_the_app_for_the_focused_window_s_panel_one_at_a_time_and_its_end_is_heard() {
    use lokyr_plugin_sdk::message::Request as Asked;
    let mut app = App::default();
    let shown = window(&mut app, "sleep 10");
    let (got, heard) = (Got::default(), Rc::default());
    let save = |window: Option<host::WindowId>, name: &str| Asked::SaveFile {
        window,
        name: name.to_owned(),
        bytes: png(),
    };
    let mut plugins = Plugins::new();
    plugins
        .add(
            "taker",
            taker(
                move |_, window| {
                    vec![
                        save(None, "Lokyr.png"),
                        save(Some(window), "again.png"),
                        save(None, "../x.png"),
                    ]
                },
                &got,
                &heard,
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("taker.go", Args::new(), &mut app.context())
        .unwrap();
    let taken: Vec<Response> = got.borrow_mut().drain(..).collect();
    let Response::Asking(id) = taken[0] else {
        panic!("{taken:?}");
    };
    // A second on the window while the first is asked, refused; a name of
    // a `/` refused.
    assert_eq!(taken[1], Response::Failed(Error::new(Code::Failed, SAVING)));
    assert!(matches!(&taken[2], Response::Failed(error) if error.message.contains("file's name")));
    let requests = plugins.take_requests();
    let saves: Vec<_> = requests
        .iter()
        .filter_map(|request| match request {
            Request::SaveFile {
                plugin,
                id,
                window,
                name,
                bytes,
            } => Some((plugin.clone(), *id, *window, name.clone(), bytes.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        saves,
        [(
            "taker".to_owned(),
            id,
            shown,
            "Lokyr.png".to_owned(),
            png().0
        )]
    );
    // While the app shows its panel, the next call's is refused too.
    app.saving = Some(Box::new(move |window| window == shown));
    plugins
        .run("taker.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        got.borrow()[0],
        Response::Failed(Error::new(Code::Failed, SAVING))
    );
    got.borrow_mut().clear();
    // The save's end, to the plugin alone.
    plugins.saved("taker", id, Some("/tmp/Lokyr.png".to_owned()), None);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(
        *heard.borrow(),
        [Event::Saved {
            id,
            path: Some("/tmp/Lokyr.png".to_owned()),
            error: None,
        }]
    );
    // Without a focused window, nowhere to show it.
    app.saving = None;
    app.focused = None;
    let mut cx_less = Plugins::new();
    let refused = Got::default();
    let kept = Rc::clone(&refused);
    cx_less
        .add(
            "lone",
            scripted(
                |cx| cx.register_command(command("lone.save")),
                move |cx, _, _| {
                    kept.borrow_mut().push(cx.request(Asked::SaveFile {
                        window: None,
                        name: "x.png".to_owned(),
                        bytes: png(),
                    }));
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(cx_less.activate(&mut app.context()).is_empty());
    cx_less
        .run("lone.save", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        refused.borrow()[0],
        Response::Failed(Error::new(Code::Unavailable, NO_WINDOW))
    );
}
