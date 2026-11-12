use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Value as Arg};
use lokyr_plugin_sdk::host::{
    Binding, Host, LineText, PaneId, Scroll, SelectionKind, TabId, WindowId,
};
use lokyr_plugin_sdk::message::{Call, Data};
use lokyr_plugin_sdk::plugin::Cx;
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role};

use super::*;
use crate::vm::{Failure, Vm};
use crate::{Callback, Deadlines, ENTRY, Execution, Package, through_luau};

/// A host that keeps the requests that it takes, and answers them with
/// its answers in their order, then as done.
#[derive(Default)]
struct Recording {
    asked: Vec<Request>,
    answers: VecDeque<Response>,
}

impl Host for Recording {
    fn handle(&mut self, call: Call) -> Response {
        self.asked.push(call.request);
        self.answers.pop_front().unwrap_or(Response::Done)
    }
}

/// Runs `body` as a plugin's `activate`, of `cx`, on a host that answers
/// with `answers`: its result, and the requests that the host took.
fn run(body: &str, answers: Vec<Response>) -> (Result<(), Failure>, Vec<Request>) {
    let entry = format!("return {{ activate = function(cx) {body} end }}");
    let package = Arc::new(Package::new([(ENTRY.to_owned(), entry)]));
    let vm = Vm::new(
        &package,
        "acme/test",
        Execution::Native,
        Deadlines::default(),
    )
    .unwrap();
    let mut host = Recording {
        asked: Vec::new(),
        answers: answers.into(),
    };
    let ran = {
        let mut cx = Cx::new(&mut host, "acme/test");
        vm.call(Callback::Activate, &mut cx, |_| Ok(Vec::new()))
            .map(drop)
    };
    (ran, host.asked)
}

/// The message of the error of `body`'s run.
fn failure(body: &str) -> String {
    match run(body, Vec::new()).0 {
        Err(Failure::Code(error)) => error.to_string(),
        other => panic!("{body} ran as {other:?}"),
    }
}

#[test]
fn every_request_of_the_sdk_is_a_function_of_cx_by_its_rust_name() {
    let Err(Traced::Variants(variants)) = Request::deserialize(Tracing) else {
        panic!("a request is a variant");
    };
    assert_eq!(kinds().len(), variants.len());
    let names: Vec<&str> = kinds().iter().map(|kind| kind.name.as_str()).collect();
    for name in [
        "register_command",
        "focused_pane",
        "line_text",
        "set_clipboard_text",
        "open_panel",
        "set_state",
    ] {
        assert!(names.contains(&name), "{name}");
    }
    let line_text = kinds()
        .iter()
        .find(|kind| kind.name == "line_text")
        .unwrap();
    assert_eq!(line_text.params, Params::Fields(&["pane", "row", "reach"]));
    let focused = kinds()
        .iter()
        .find(|kind| kind.name == "focused_pane")
        .unwrap();
    assert_eq!(focused.params, Params::None);
    // Each a function of `cx`, as a plugin finds it.
    let body = format!(
        "for _, name in {{ {} }} do assert(type(cx[name]) == 'function', name) end",
        names
            .iter()
            .map(|name| format!("{name:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(run(&body, Vec::new()).0.is_ok());
}

#[test]
fn a_call_makes_its_request_of_its_arguments_in_their_order() {
    let (ran, asked) = run(
        r#"
        cx.line_text(4, -2, 32)
        cx.focused_pane()
        cx.register_command({ name = "acme.go", title = "Go", category = "view" })
        cx.bind({ keys = "cmd+shift+f", command = "acme.go", when = "pane.focused" })
        cx.open_panel("find", {
            anchor = { pane_corner = { pane = 3, corner = "top-right" } },
            role = "dialog",
            close_escape = true,
        })
        cx.scroll(4, { up = 3 })
        cx.scroll(4, "page-up")
        cx.emit("acme.progress", "acme.progress/1", { done = 3, items = { "a", "b" }, ok = true })
        cx.new_tab(1)
        cx.new_tab(1, nil, "~/src")
        cx.run_command("acme.go")
        cx.run_command("acme.go", { n = 3, name = "x", on = true, ratio = 0.5 })
        cx.message("hello", { name = "Ada", count = 2 })
        cx.select_cells(4, { 0, -1 }, { 5, -1 })
        cx.set_state({})
        cx.close_tab(7)
        cx.emit("acme.ping", "acme.ping/1", nil)
        cx.close_tab(7, nil)
        "#,
        Vec::new(),
    );
    assert!(ran.is_ok(), "{ran:?}");
    let text = |text: &str| Data::Text(text.to_owned());
    assert_eq!(
        asked,
        vec![
            Request::LineText {
                pane: PaneId(4),
                row: -2,
                reach: 32,
            },
            Request::FocusedPane,
            Request::RegisterCommand(Command {
                name: "acme.go".to_owned(),
                title: "Go".to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            }),
            Request::Bind(Binding {
                keys: "cmd+shift+f".to_owned(),
                command: "acme.go".to_owned(),
                args: Args::new(),
                when: Some("pane.focused".to_owned()),
            }),
            Request::OpenPanel {
                id: PanelId::from("find"),
                options: PanelOptions {
                    anchor: Anchor::PaneCorner {
                        pane: PaneId(3),
                        corner: Corner::TopRight,
                    },
                    role: Role::Dialog,
                    close_outside: false,
                    close_escape: true,
                    takes_keys: false,
                    close_typing: false,
                },
            },
            Request::Scroll {
                pane: PaneId(4),
                scroll: Scroll::Up(3),
            },
            Request::Scroll {
                pane: PaneId(4),
                scroll: Scroll::PageUp,
            },
            Request::Emit {
                name: "acme.progress".to_owned(),
                schema: "acme.progress/1".to_owned(),
                value: Data::Map(BTreeMap::from([
                    ("done".to_owned(), Data::Integer(3)),
                    ("items".to_owned(), Data::List(vec![text("a"), text("b")])),
                    ("ok".to_owned(), Data::Boolean(true)),
                ])),
            },
            Request::NewTab {
                window: WindowId(1),
                index: None,
                directory: None,
            },
            Request::NewTab {
                window: WindowId(1),
                index: None,
                directory: Some("~/src".to_owned()),
            },
            Request::RunCommand {
                name: "acme.go".to_owned(),
                args: Args::new(),
            },
            Request::RunCommand {
                name: "acme.go".to_owned(),
                args: Args::from([
                    ("n".to_owned(), Arg::Integer(3)),
                    ("name".to_owned(), Arg::String("x".to_owned())),
                    ("on".to_owned(), Arg::Boolean(true)),
                    ("ratio".to_owned(), Arg::Number(0.5)),
                ]),
            },
            Request::Message {
                id: "hello".to_owned(),
                args: BTreeMap::from([
                    ("name".to_owned(), text("Ada")),
                    ("count".to_owned(), Data::Integer(2)),
                ]),
            },
            Request::SelectCells {
                pane: PaneId(4),
                first: (0, -1),
                last: (5, -1),
                kind: SelectionKind::default(),
            },
            Request::SetState(Data::Map(BTreeMap::new())),
            Request::CloseTab {
                tab: TabId(7),
                ask: false,
            },
            // A plain value's `nil` is its value; an optional field's its
            // default.
            Request::Emit {
                name: "acme.ping".to_owned(),
                schema: "acme.ping/1".to_owned(),
                value: Data::Nil,
            },
            Request::CloseTab {
                tab: TabId(7),
                ask: false,
            },
        ]
    );
}

#[test]
fn an_argument_of_another_shape_raises_an_error_that_names_it() {
    let error = failure(r#"cx.line_text(4, "x", 32)"#);
    assert!(
        error.contains("bad argument to `cx.line_text`: argument 2 (`row`): expected a whole number, not a text"),
        "{error}"
    );
    let error = failure("cx.line_text(4, -1, 32, 5)");
    assert!(error.contains("it takes 3 arguments, not 4"), "{error}");
    let error = failure("cx.focused_pane(1)");
    assert!(error.contains("it takes 0 arguments, not 1"), "{error}");
    let error = failure("cx.selection_text()");
    assert!(
        error.contains("argument 1: expected a whole number, not nil"),
        "{error}"
    );
    let error = failure(
        r#"cx.open_panel("find", { anchor = { pane_corner = { pane = 3, corner = "top" } }, rol = "dialog" })"#,
    );
    assert!(
        error.contains("argument 2 (`options`): no field `rol`"),
        "{error}"
    );
    let error = failure("cx.line_text(4)");
    assert!(error.contains("missing field `row`"), "{error}");
    // A plain value deeper than the host takes.
    let error = failure("local t = {} for _ = 1, 64 do t = { t } end cx.set_state(t)");
    assert!(
        error.contains("argument 1: a plain value nests 64 levels of tables at most"),
        "{error}"
    );
    // The host never took them.
    assert!(run("cx.line_text(4, 'x', 32)", Vec::new()).1.is_empty());
}

#[test]
fn an_answer_is_the_response_s_value_true_or_nil_the_message_and_the_code() {
    let answers = vec![
        Response::Done,
        Response::Pane(Some(PaneId(3))),
        Response::Pane(None),
        Response::Line(Some(LineText {
            text: "日x".to_owned(),
            cells: vec![(0, -1), (2, -1)],
        })),
        Response::Tab {
            tab: TabId(5),
            pane: PaneId(6),
        },
        Response::Data(Data::Map(BTreeMap::from([(
            "recent".to_owned(),
            Data::List(vec![Data::Text("a".to_owned())]),
        )]))),
        Response::Failed(Error::new(Code::NotFound, "there is no pane 9")),
        Response::Failed(Error::new(Code::MissingCapability, "it lacks `pty-write`")),
    ];
    let (ran, _) = run(
        r#"
        assert(cx.set_clipboard_text("x") == true)
        assert(cx.focused_pane() == 3)
        assert(cx.focused_pane() == nil)
        local line = cx.line_text(1, -1, 0)
        assert(line.text == "日x" and line.cells[2][1] == 2 and line.cells[2][2] == -1)
        local tab = cx.new_tab(1)
        assert(tab.tab == 5 and tab.pane == 6)
        local state = cx.state()
        assert(state.recent[1] == "a")
        local none, message, code = cx.selection_text(9)
        assert(none == nil and message == "there is no pane 9" and code == 101)
        -- `assert` raises its message, after the place of the call.
        local ok, raised = pcall(function()
            assert(cx.paste(1, "x"))
        end)
        local lacks = ": it lacks `pty-write`"
        assert(not ok and raised:sub(-#lacks) == lacks, tostring(raised))
        "#,
        answers,
    );
    assert!(ran.is_ok(), "{ran:?}");
}

#[test]
fn every_kind_of_answer_comes_back_as_its_response() {
    let responses = [
        Response::Done,
        Response::Window(Some(WindowId(2))),
        Response::Window(None),
        Response::Text(Some("x".to_owned())),
        Response::Boolean(false),
        Response::FontSize(13.5),
        Response::Reading(4),
        Response::Cell(Some((3, -2))),
        Response::Grid {
            columns: 80,
            rows: 24,
        },
        Response::Tab {
            tab: TabId(1),
            pane: PaneId(2),
        },
        Response::Data(Data::List(vec![Data::Integer(1), Data::Number(0.5)])),
        Response::Failed(Error::new(Code::Fault, "on purpose")),
    ];
    for response in responses {
        through_luau(&Request::FocusedPane, &response).unwrap();
        through_luau(
            &Request::LineText {
                pane: PaneId(1),
                row: 0,
                reach: 3,
            },
            &response,
        )
        .unwrap();
    }
}

#[test]
fn an_image_is_asked_of_a_table_and_its_png_comes_back_as_a_string() {
    use lokyr_plugin_sdk::host::{Color, StyleRun, TextStyle};
    use lokyr_plugin_sdk::image::{CellRow, Image, ImageBackground, ImageSpec};
    let answers = vec![Response::Image(Image::blank(2.0))];
    let (ran, asked) = run(
        r#"
        local image = cx.render_image({
            rows = {
                { text = "ls", runs = {
                    { first = 0, last = 1, style = {
                        foreground = { palette = 2 }, bold = true, faint = false,
                        italic = false, blink = false, inverse = false,
                        invisible = false, strikethrough = false, overline = false,
                    } },
                } },
                { text = "日本" },
            },
            title = "zsh",
            buttons = true,
            background = { rgb = { 1, 2, 3 } },
            scale = 2,
        })
        assert(type(image.png) == "string", typeof(image.png))
        assert(#image.png == 68 and image.png:sub(2, 4) == "PNG")
        assert(image.width == 1 and image.height == 1 and image.scale == 2)
        "#,
        answers,
    );
    assert!(ran.is_ok(), "{ran:?}");
    let style = TextStyle {
        foreground: Some(Color::Palette(2)),
        bold: true,
        ..TextStyle::default()
    };
    assert_eq!(
        asked,
        [Request::RenderImage(ImageSpec {
            rows: vec![
                CellRow {
                    text: "ls".to_owned(),
                    runs: vec![StyleRun {
                        first: 0,
                        last: 1,
                        style,
                    }],
                },
                CellRow {
                    text: "日本".to_owned(),
                    runs: Vec::new(),
                },
            ],
            title: Some("zsh".to_owned()),
            buttons: true,
            background: ImageBackground::Rgb(1, 2, 3),
            scale: Some(2.0),
            ..ImageSpec::default()
        })]
    );
    through_luau(
        &Request::RenderImage(ImageSpec::default()),
        &Response::Image(Image::blank(1.0)),
    )
    .unwrap();
}

#[test]
fn a_save_and_a_clipboard_image_take_their_bytes_of_a_string_and_cells_come_back_as_tables() {
    use lokyr_plugin_sdk::host::{Bytes, StyleRun, TextStyle};
    use lokyr_plugin_sdk::image::CellRow;
    let cells = vec![CellRow {
        text: "ls".to_owned(),
        runs: vec![StyleRun {
            first: 0,
            last: 1,
            style: TextStyle {
                bold: true,
                ..TextStyle::default()
            },
        }],
    }];
    let answers = vec![
        Response::Cells(Some(cells)),
        Response::Done,
        Response::Asking(7),
    ];
    let (ran, asked) = run(
        r#"
        local rows = cx.selection_cells(1)
        assert(rows[1].text == "ls" and rows[1].runs[1].style.bold == true)
        local png = "\137PNG\r\n\26\n"
        assert(cx.set_clipboard_image(png) == true)
        assert(cx.save_file(nil, "Lokyr.png", png) == 7)
        "#,
        answers,
    );
    assert!(ran.is_ok(), "{ran:?}");
    let png = Bytes(b"\x89PNG\r\n\x1a\n".to_vec());
    assert_eq!(
        asked,
        [
            Request::SelectionCells(PaneId(1)),
            Request::SetClipboardImage(png.clone()),
            Request::SaveFile {
                window: None,
                name: "Lokyr.png".to_owned(),
                bytes: png,
            },
        ]
    );
}
