//! The scenarios of the conformance suite, of every request of the plugin
//! API, its failures among them: those that the first-party plugins make,
//! `links`', the reads of a line, its link, its directory, what opens, the
//! decorations and the pointer's events, and `core-actions`', the text's
//! size, the clipboard and the writes to a pane; and the events, the
//! panels, the capabilities, the declarations, the app's requests, the
//! state, the reads of rows, the views, the shell's commands and the
//! pane's view, and the workspace's writes that every plugin meets. Each
//! runs in the world of the suite: a window of a pane whose program, in
//! `/`, wrote [`LINE`] and [`COMMAND`], the focused one, and a
//! clipboard of [`CLIPBOARD`]; the kernel's host runs them in their order,
//! in one world, the workspace's leaving it as it found it, and
//! `core-actions`' last, as its writes change the pane.
//!
//! A window, a tab or a pane that a scenario's requests open, whose id
//! each host gives its own, is a symbol, [`window`], [`tab`] and [`pane`]
//! of the order in which the trace first meets it: the test plugin asks
//! with the host's id of a symbol, and its trace tells each id that it
//! meets by its symbol.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Capability, Category, Code, Command, Error, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{
    Awake, BarPlace, Binding, Bytes, Color, CommandInfo, DecorationLook, Direction, Edge,
    LEAST_DELAY, Layout, Line, LineText, MOST_DELAY, MOST_SEARCHES, MOST_STATE, MOST_TIMERS,
    MOST_WRITES, MenuItem, MenuPlace, Packages, PaneId, QuickLength, QuickPlace, ScreenPoint,
    Scroll, SearchMatch, SearchQuery, SelectionKind, ShellCommand, Side, StyleRun, TabId, TabInfo,
    TextStyle, Underline, Viewport, WindowId, WindowInfo, check_file_name,
};
use lokyr_plugin_sdk::image::{
    CellRow, IMAGES_BOUND, Image, ImageBackground, ImageSpec, MOST_IMAGES,
};
use lokyr_plugin_sdk::message::{
    DELAY_RANGE, DIM_RANGE, Data, Event, Mods, OWN_PANE, Request, Response, SAVING, TIMERS_BOUND,
};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role, ViewId};
use serde::{Deserialize, Serialize};

/// The line that the world's pane's program wrote.
pub const LINE: &str = "see https://lokyr.dev/x now";

/// What the world's pane's program wrote after [`LINE`], as `printf`'s
/// format: a command that the shell would mark, `true`, of no output and
/// an exit status of 0, then a prompt.
pub const COMMAND: &str = concat!(
    "\\033]133;A\\007$ \\033]133;B\\007true\\n",
    "\\033]133;C\\007\\033]133;D;0\\007\\033]133;A\\007$ ",
);

/// The clipboard's text as the world starts.
pub const CLIPBOARD: &str = "old";

/// A pane that no host holds.
const NO_PANE: PaneId = PaneId(999_999);

/// A search that no plugin made.
const NO_SEARCH: u64 = 999_999;

/// A window that no host holds.
const NO_WINDOW: WindowId = WindowId(999_999);

/// A tab that no host holds.
const NO_TAB: TabId = TabId(999_999);

/// A timer that no plugin started.
const NO_TIMER: u64 = 999_999;

/// The id of a job of the packages' service that is none.
const NO_JOB: u64 = 999_999;

/// The first of the symbols, past every id that a host gives: a window's
/// are from it, a tab's from twice it, a pane's from three times it.
pub const SYMBOL: u64 = 1 << 40;

/// The window that the trace meets as the `k`th of those that the
/// scenario opens.
pub const fn window(k: u64) -> WindowId {
    WindowId(SYMBOL + k)
}

/// The tab that the trace meets as the `k`th of those that the scenario
/// opens.
pub const fn tab(k: u64) -> TabId {
    TabId(2 * SYMBOL + k)
}

/// The pane that the trace meets as the `k`th of those that the scenario
/// opens.
pub const fn pane(k: u64) -> PaneId {
    PaneId(3 * SYMBOL + k)
}

/// The timer that the trace meets as the `k`th of those that the scenario
/// starts.
pub const fn timer(k: u64) -> u64 {
    4 * SYMBOL + k
}

/// The world of the scenarios, as the kernel's host made it: its window,
/// its tab and its pane, and the pane's row of [`LINE`], with the line's
/// text and cells as the pane reads them, the commands of [`COMMAND`], the
/// pane's view, its terminal's cursor's cell and its view's cells.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    pub window: WindowId,
    pub tab: TabId,
    pub pane: PaneId,
    pub row: i64,
    pub line: LineText,
    pub commands: Vec<ShellCommand>,
    /// The lines of the commands, in their order.
    pub command_lines: Vec<Option<String>>,
    pub viewport: Viewport,
    pub cursor: (u16, i64),
    /// The cells of the rows that the pane's view shows.
    pub screen: Vec<CellRow>,
}

/// A scenario: what the test plugin holds, and its steps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Scenario {
    /// What it is of.
    pub name: String,
    /// The capabilities that the plugin holds, as its manifest declares
    /// them; `None` for every one, as a first-party plugin's.
    pub capabilities: Option<Vec<Capability>>,
    /// The events that the plugin subscribes to as it activates.
    pub subscriptions: Vec<String>,
    pub steps: Vec<Step>,
}

/// A step of a scenario: a call of the plugin's, the workspace's events,
/// the time that passes, and what the plugin hears after them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Step {
    /// The requests that the call makes, each with the answer that it
    /// takes; none for a step of events alone, which makes no call.
    pub asks: Vec<(Request, Response)>,
    /// The workspace's events, which the host posts after the call.
    pub posts: Vec<Event>,
    /// How long the host's time moves on after them, in milliseconds, as
    /// the plugin's timers fire.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub wait: u64,
    /// The events that the plugin hears after them, as one batch.
    pub heard: Vec<Event>,
}

/// Whether `wait` is none, which a step's JSON leaves out.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(wait: &u64) -> bool {
    *wait == 0
}

/// A step of a call alone.
fn ask(asks: Vec<(Request, Response)>) -> Step {
    Step {
        wait: 0,
        asks,
        posts: Vec::new(),
        heard: Vec::new(),
    }
}

/// A failure of `code` and `message`.
fn failed(code: Code, message: &str) -> Response {
    Response::Failed(Error::new(code, message))
}

/// The scenarios, of `world`, in the order that the kernel's host runs
/// them.
pub fn all(world: &World) -> Vec<Scenario> {
    vec![
        links(world),
        events(world),
        capabilities(world),
        declarations(),
        timers(),
        app_requests(world),
        state_and_rows(world),
        views(world),
        commands_and_viewport(world),
        copy_mode(world),
        workspace(world),
        tabs(world),
        tab_changes(world, true),
        tab_changes(world, false),
        pane_changes(world, true),
        pane_changes(world, false),
        bounds(world),
        images(),
        cells_and_saves(world),
        awake(true),
        awake(false),
        packages(true),
        packages(false),
        core_actions(world),
    ]
}

/// The Mac kept awake, of `keep-awake`, when `allowed`: the system and the
/// display, the system alone, and let sleep; refused of a plugin that
/// lacks the capability.
fn awake(allowed: bool) -> Scenario {
    let answer = || {
        if allowed {
            Response::Done
        } else {
            failed(
                Code::MissingCapability,
                "the plugin `acme/conformer` lacks the capability `keep-awake`",
            )
        }
    };
    Scenario {
        name: format!(
            "the Mac kept awake, {} `keep-awake`",
            if allowed { "of" } else { "without" }
        ),
        capabilities: Some(if allowed {
            vec![Capability::KeepAwake]
        } else {
            Vec::new()
        }),
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (Request::KeepAwake(Awake::Display), answer()),
            (Request::KeepAwake(Awake::System), answer()),
            (Request::KeepAwake(Awake::Off), answer()),
        ])],
    }
}

/// The packages' service, of `packages`, Lokyr's own plugins' capability,
/// when `allowed`: a job of each call, of its id, a name that is none and a
/// search that Sleip would take for an option refused, a job's cancel of
/// any id done, and a job's lines and end to the plugin alone; each
/// refused of a plugin that lacks the capability.
fn packages(allowed: bool) -> Scenario {
    let lacks = || {
        failed(
            Code::MissingCapability,
            "the plugin `acme/conformer` lacks the capability `packages`",
        )
    };
    let answer = |response: Response| if allowed { response } else { lacks() };
    let refused = |code: Code, message: &str| {
        if allowed {
            failed(code, message)
        } else {
            lacks()
        }
    };
    let line = Data::Map(BTreeMap::from([
        ("event".to_owned(), Data::Text("installed".to_owned())),
        ("name".to_owned(), Data::Text("sephiroth74/snap".to_owned())),
        ("version".to_owned(), Data::Text("0.1.0".to_owned())),
    ]));
    let ended = vec![
        Event::Package { id: 0, line },
        Event::PackagesEnded { id: 0, ok: true },
    ];
    let mut steps = vec![ask(vec![
        (
            Request::Packages(Packages::List),
            answer(Response::Asking(0)),
        ),
        (
            Request::Packages(Packages::Search {
                query: "snap".to_owned(),
            }),
            answer(Response::Asking(0)),
        ),
        (
            Request::Packages(Packages::Add {
                plugin: "sephiroth74/snap".to_owned(),
                constraint: Some("^0.1".to_owned()),
            }),
            answer(Response::Asking(0)),
        ),
        (
            Request::Packages(Packages::Add {
                plugin: "Snap".to_owned(),
                constraint: None,
            }),
            refused(
                Code::InvalidName,
                "`Snap` is not a plugin's name, `owner/name`",
            ),
        ),
        (
            Request::Packages(Packages::Search {
                query: "--path".to_owned(),
            }),
            refused(
                Code::WrongKind,
                "a search is a text of 256 bytes at most, of no control character, not starting with `-`",
            ),
        ),
        (Request::CancelPackages(NO_JOB), answer(Response::Done)),
    ])];
    if allowed {
        steps.push(Step {
            wait: 0,
            asks: Vec::new(),
            posts: ended.clone(),
            heard: ended,
        });
    }
    Scenario {
        name: format!(
            "the packages' service, {} `packages`",
            if allowed { "of" } else { "without" }
        ),
        capabilities: Some(if allowed {
            vec![Capability::Packages]
        } else {
            Vec::new()
        }),
        subscriptions: Vec::new(),
        steps,
    }
}

/// The cells of a pane, an image on the clipboard and a file's save: the
/// selection's cells, of none, and the view's, of `terminal-read`; a PNG on
/// the clipboard in place of its text, of `clipboard-write`, and what is no
/// PNG refused; a save of no capability, on the focused window, a second on
/// it while the first's panel shows refused, a name that is none and a
/// window not there refused, and the save's end, to the plugin alone.
fn cells_and_saves(world: &World) -> Scenario {
    let png = || Bytes(Image::blank(1.0).png.0);
    let save = |window: Option<WindowId>, name: &str| Request::SaveFile {
        window,
        name: name.to_owned(),
        bytes: png(),
    };
    Scenario {
        name: "the cells, an image on the clipboard and a save".to_owned(),
        capabilities: Some(vec![
            Capability::TerminalRead,
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
        ]),
        subscriptions: Vec::new(),
        steps: vec![
            ask(vec![
                (Request::SelectionCells(world.pane), Response::Cells(None)),
                (
                    Request::ScreenCells(world.pane),
                    Response::Cells(Some(world.screen.clone())),
                ),
                (Request::SetClipboardImage(png()), Response::Done),
                (
                    Request::SetClipboardImage(Bytes(b"GIF89a".to_vec())),
                    failed(Code::Failed, "an image is a PNG, of its signature"),
                ),
            ]),
            ask(vec![
                (Request::ClipboardText, Response::Text(None)),
                (save(None, "Lokyr.png"), Response::Asking(0)),
                (
                    save(Some(world.window), "again.png"),
                    failed(Code::Failed, SAVING),
                ),
                (
                    save(None, "a/b.png"),
                    failed(
                        Code::Failed,
                        &check_file_name("a/b.png").unwrap_err().message,
                    ),
                ),
                (
                    save(Some(NO_WINDOW), "x.png"),
                    failed(Code::NotFound, "there is no window 999999"),
                ),
                // The world's clipboard back, as the scenarios after it find
                // it.
                (
                    Request::SetClipboardText(CLIPBOARD.to_owned()),
                    Response::Done,
                ),
            ]),
            Step {
                wait: 0,
                asks: Vec::new(),
                posts: vec![Event::Saved {
                    id: 0,
                    path: Some("/tmp/Lokyr.png".to_owned()),
                    error: None,
                }],
                heard: vec![Event::Saved {
                    id: 0,
                    path: Some("/tmp/Lokyr.png".to_owned()),
                    error: None,
                }],
            },
        ],
    }
}

/// The images of cells that a plugin asks for, of no capability: each at
/// the scale asked for or the display's, [`MOST_IMAGES`] a call at most,
/// and those out of their bounds refused, as every host checks them alike.
fn images() -> Scenario {
    let spec = |scale: Option<f32>| ImageSpec {
        rows: vec![
            CellRow {
                text: "ls -la".to_owned(),
                runs: vec![StyleRun {
                    first: 0,
                    last: 1,
                    style: TextStyle {
                        foreground: Some(Color::Palette(2)),
                        bold: true,
                        ..TextStyle::default()
                    },
                }],
            },
            CellRow {
                text: "日本 é".to_owned(),
                runs: Vec::new(),
            },
        ],
        title: Some("~ — zsh".to_owned()),
        buttons: true,
        rounded: true,
        shadow: true,
        padding: 32.0,
        background: ImageBackground::Rgb(40, 44, 52),
        scale,
        keep: false,
        min_columns: Some(80),
        min_rows: Some(4),
    };
    let rendered = |scale: Option<f32>| {
        (
            Request::RenderImage(spec(scale)),
            Response::Image(Image::blank(scale.unwrap_or(2.0))),
        )
    };
    let mut asks: Vec<(Request, Response)> = (0..MOST_IMAGES)
        .map(|at| rendered((at == 1).then_some(3.0)))
        .collect();
    asks.push((
        Request::RenderImage(spec(None)),
        failed(Code::Failed, IMAGES_BOUND),
    ));
    let refused = |image: ImageSpec| {
        let message = image.check().unwrap_err().message;
        (Request::RenderImage(image), failed(Code::Failed, &message))
    };
    let wrong = vec![
        refused(ImageSpec::default()),
        refused(ImageSpec {
            rows: vec![CellRow {
                text: "a\u{1b}[31mb".to_owned(),
                runs: Vec::new(),
            }],
            ..spec(None)
        }),
        refused(ImageSpec {
            padding: 300.0,
            ..spec(None)
        }),
        refused(ImageSpec {
            scale: Some(8.0),
            ..spec(None)
        }),
        refused(ImageSpec {
            min_columns: Some(2000),
            ..spec(None)
        }),
        rendered(Some(1.0)),
        // Kept, by an id that the trace tells as 0, and let go.
        (
            Request::RenderImage(ImageSpec {
                keep: true,
                ..spec(Some(2.0))
            }),
            Response::Image(Image {
                id: Some(0),
                ..Image::blank(2.0)
            }),
        ),
        (Request::DropImage(1), Response::Done),
        (Request::DropImage(999), Response::Done),
    ];
    Scenario {
        name: "the images of cells, of no capability, bounded".to_owned(),
        capabilities: Some(Vec::new()),
        subscriptions: Vec::new(),
        steps: vec![ask(asks), ask(wrong)],
    }
}

/// The changes of a tab that the plugin hears, of a plugin that reads the
/// processes, `reads`, or not: each change of its title or its color one,
/// as its look changes, none of a title given again; and to a plugin that
/// does not read the processes, the changes of what it can read alone, two
/// of one call one, as it learnt the tab as it activated.
fn tab_changes(world: &World, reads: bool) -> Scenario {
    let t = world.tab;
    let title = |title: Option<&str>| Request::SetTabTitle {
        tab: t,
        title: title.map(str::to_owned),
    };
    let color = |color: Option<Color>| Request::SetTabColor { tab: t, color };
    let changed = || Event::Workspace(WorkspaceEvent::TabChanged { tab: t });
    let changes = |two: usize| -> Vec<Event> {
        (0..if reads { two } else { 1 })
            .map(|_| changed())
            .collect()
    };
    let step = |asks: Vec<(Request, Response)>, heard: Vec<Event>| Step {
        wait: 0,
        asks,
        posts: Vec::new(),
        heard,
    };
    Scenario {
        name: format!(
            "a tab's changes, to a plugin that {} the processes",
            if reads { "reads" } else { "does not read" }
        ),
        capabilities: (!reads).then(|| vec![Capability::PaneView]),
        subscriptions: vec!["tab-changed".to_owned()],
        steps: vec![
            step(
                vec![(title(Some("named")), Response::Done)],
                vec![changed()],
            ),
            step(vec![(title(Some("named")), Response::Done)], Vec::new()),
            step(
                vec![
                    (title(Some("other")), Response::Done),
                    (color(Some(Color::Rgb(200, 30, 40))), Response::Done),
                ],
                changes(2),
            ),
            step(
                vec![(title(None), Response::Done), (color(None), Response::Done)],
                changes(2),
            ),
        ],
    }
}

/// The changes of a pane that the plugin hears, of a plugin that reads the
/// processes, `reads`, or not: each one; and to a plugin that does not
/// read them, none of a pane whose program's title, all that it reads of
/// it, did not change, as of its folder or its program.
fn pane_changes(world: &World, reads: bool) -> Scenario {
    let changed = || {
        Event::Workspace(WorkspaceEvent::PaneChanged {
            pane: world.pane,
            tab: world.tab,
        })
    };
    let step = || Step {
        wait: 0,
        asks: Vec::new(),
        posts: vec![changed()],
        heard: if reads { vec![changed()] } else { Vec::new() },
    };
    Scenario {
        name: format!(
            "a pane's changes, to a plugin that {} the processes",
            if reads { "reads" } else { "does not read" }
        ),
        capabilities: (!reads).then(|| vec![Capability::PaneView]),
        subscriptions: vec!["pane-changed".to_owned()],
        steps: vec![step(), step()],
    }
}

/// The bounds of a call: its searches, [`MOST_SEARCHES`] at most, each of
/// the pane's ending the one before, whose event the plugin never hears;
/// and its writes and its requests of the app's together,
/// [`MOST_WRITES`] at most.
fn bounds(world: &World) -> Scenario {
    let search = || Request::Search {
        pane: world.pane,
        query: SearchQuery {
            text: "now".to_owned(),
            ..SearchQuery::default()
        },
    };
    let mut searches: Vec<(Request, Response)> = (0..MOST_SEARCHES)
        .map(|_| (search(), Response::Searching(0)))
        .collect();
    searches.push((
        search(),
        failed(
            Code::Failed,
            &format!("a call starts {MOST_SEARCHES} searches at most"),
        ),
    ));
    let half = MOST_WRITES / 2;
    let mut writes: Vec<(Request, Response)> = (0..half)
        .map(|_| {
            (
                Request::SetTabTitle {
                    tab: world.tab,
                    title: None,
                },
                Response::Done,
            )
        })
        .collect();
    let open = || Request::Open("https://lokyr.dev/x".to_owned());
    writes.extend((half..MOST_WRITES).map(|_| (open(), Response::Done)));
    let past = format!(
        "a call writes and asks the app {MOST_WRITES} times at most, which apply as it returns"
    );
    writes.push((open(), failed(Code::Failed, &past)));
    writes.push((
        Request::SetTabTitle {
            tab: world.tab,
            title: None,
        },
        failed(Code::Failed, &past),
    ));
    Scenario {
        name: "a call's searches and its writes with its requests of the app's, bounded".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            Step {
                wait: 0,
                asks: searches,
                posts: Vec::new(),
                heard: vec![Event::Searched {
                    id: 0,
                    pane: world.pane,
                    matches: vec![SearchMatch {
                        first: (24, world.row),
                        last: (26, world.row),
                    }],
                    count: 1,
                    done: true,
                }],
            },
            ask(writes),
            ask(vec![(Request::EndSearch(NO_SEARCH), Response::Done)]),
        ],
    }
}

/// What a tab bar's plugin asks of a tab: a title and a color of its own,
/// which a read of the tabs gives, and back; a text that the user is asked
/// for, whose answer comes to the plugin alone; its bar's place, which a
/// later call reads; and what of them is refused.
fn tabs(world: &World) -> Scenario {
    let (w, t, p) = (world.window, world.tab, world.pane);
    let info = |title: &str, color| TabInfo {
        id: t,
        window: w,
        title: title.to_owned(),
        color,
        active: true,
        focused: p,
        panes: vec![p],
        zoomed: None,
        activity: false,
        bell: false,
        progress: None,
        printing: false,
        running: false,
        alternate: false,
        secure: false,
        agent: None,
        agent_message: None,
    };
    let red = Color::Rgb(200, 30, 40);
    let answer = Event::Answered {
        id: 0,
        text: Some("work".to_owned()),
    };
    Scenario {
        name: "a tab's own title and color, and a question".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::SetTabTitle {
                            tab: t,
                            title: Some("named".to_owned()),
                        },
                        Response::Done,
                    ),
                    (
                        Request::SetTabColor {
                            tab: t,
                            color: Some(red),
                        },
                        Response::Done,
                    ),
                    (
                        Request::SetTabColor {
                            tab: t,
                            color: Some(Color::Palette(1)),
                        },
                        failed(
                            Code::Failed,
                            "a tab's color is an RGB one, not one of the palette's",
                        ),
                    ),
                    (
                        Request::SetTabTitle {
                            tab: NO_TAB,
                            title: None,
                        },
                        failed(Code::NotFound, "there is no tab 999999"),
                    ),
                    (
                        Request::AskText {
                            window: None,
                            message: "The tab's title".to_owned(),
                            detail: String::new(),
                            text: "named".to_owned(),
                        },
                        Response::Asking(0),
                    ),
                    (
                        Request::AskText {
                            window: Some(NO_WINDOW),
                            message: String::new(),
                            detail: String::new(),
                            text: String::new(),
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                ],
                posts: vec![answer.clone()],
                heard: vec![answer],
            },
            ask(vec![
                (
                    Request::Tabs(w),
                    Response::Tabs(vec![info("named", Some(red))]),
                ),
                (
                    Request::SetTabTitle {
                        tab: t,
                        title: None,
                    },
                    Response::Done,
                ),
                (
                    Request::SetTabColor {
                        tab: t,
                        color: None,
                    },
                    Response::Done,
                ),
            ]),
            ask(vec![(
                Request::Tabs(w),
                Response::Tabs(vec![info("", None)]),
            )]),
            // Its tab bar's place, which a later call reads, a column's
            // width within its limits.
            ask(vec![
                (
                    Request::SetTabBarPlace(BarPlace::Left { width: 200.0 }),
                    Response::Done,
                ),
                (
                    Request::SetTabBarPlace(BarPlace::Right { width: 10.0 }),
                    failed(
                        Code::Failed,
                        "a tab bar's column is from 60 to 600 points wide, not 10",
                    ),
                ),
                (
                    Request::TabBarPlace,
                    Response::TabBarPlace(BarPlace::Titlebar),
                ),
            ]),
            ask(vec![
                (
                    Request::TabBarPlace,
                    Response::TabBarPlace(BarPlace::Left { width: 200.0 }),
                ),
                (Request::SetTabBarPlace(BarPlace::Titlebar), Response::Done),
                // A window's own place, its title in its titlebar, and
                // none of a window not there.
                (
                    Request::SetWindowTabBarPlace {
                        window: w,
                        place: Some(BarPlace::Title),
                    },
                    Response::Done,
                ),
                (
                    Request::SetWindowTabBarPlace {
                        window: NO_WINDOW,
                        place: None,
                    },
                    failed(Code::NotFound, "there is no window 999999"),
                ),
                (
                    Request::SetWindowTabBarPlace {
                        window: w,
                        place: None,
                    },
                    Response::Done,
                ),
                // The panes' fade, of a share, and the keyboard that follows
                // the pointer, and back.
                (Request::SetInactiveDim(0.15), Response::Done),
                (
                    Request::SetInactiveDim(2.0),
                    failed(Code::Failed, DIM_RANGE),
                ),
                (Request::SetFocusFollowsMouse(true), Response::Done),
                (Request::SetInactiveDim(0.0), Response::Done),
                (Request::SetFocusFollowsMouse(false), Response::Done),
            ]),
            // A drag of its column's edge, told to it alone.
            Step {
                wait: 0,
                asks: Vec::new(),
                posts: vec![Event::TabBarResized { width: 240.0 }],
                heard: vec![Event::TabBarResized { width: 240.0 }],
            },
            // The keys of a command of its arguments, as the platform writes
            // them, and none of others.
            ask(vec![
                (
                    Request::KeysOf {
                        command: "tabs.goto".to_owned(),
                        args: Args::from([("index".to_owned(), Value::Integer(1))]),
                    },
                    Response::Text(Some("⌘1".to_owned())),
                ),
                (
                    Request::KeysOf {
                        command: "tabs.goto".to_owned(),
                        args: Args::from([("index".to_owned(), Value::Integer(2))]),
                    },
                    Response::Text(None),
                ),
                // A message that its catalogs lack, none on either host.
                (
                    Request::Message {
                        id: "tab-untitled".to_owned(),
                        args: BTreeMap::from([("index".to_owned(), Data::Integer(2))]),
                    },
                    Response::Text(None),
                ),
            ]),
        ],
    }
}

/// What `links` asks: the focused pane, its line, its link and its
/// directory, what opens, the underline of a link, and the pointer.
fn links(world: &World) -> Scenario {
    let (pane, row) = (world.pane, world.row);
    let look = DecorationLook {
        underline: Some(Underline::Single),
        underline_color: None,
        background: None,
        pointing: true,
    };
    let pointer = Event::Pointer {
        pane,
        at: Some((6, row)),
        mods: Mods {
            command: true,
            ..Mods::default()
        },
    };
    Scenario {
        name: "links".to_owned(),
        capabilities: None,
        subscriptions: vec!["pointer".to_owned()],
        steps: vec![
            ask(vec![
                (Request::FocusedWindow, Response::Window(Some(world.window))),
                (Request::FocusedPane, Response::Pane(Some(pane))),
                (
                    Request::LineText {
                        pane,
                        row,
                        reach: 32,
                    },
                    Response::Line(Some(world.line.clone())),
                ),
                (
                    Request::LineText {
                        pane,
                        row: row + 1000,
                        reach: 32,
                    },
                    Response::Line(None),
                ),
                (
                    Request::LinkAt {
                        pane,
                        column: 6,
                        row,
                    },
                    Response::Text(None),
                ),
                (
                    Request::WorkingDirectory(pane),
                    Response::Text(Some("/".to_owned())),
                ),
                // A file's line, which the app opens in the editor; none of
                // a URL, nor of a line 0.
                (
                    Request::OpenAt {
                        target: "/".to_owned(),
                        line: 3,
                        column: Some(2),
                    },
                    Response::Done,
                ),
                (
                    Request::OpenAt {
                        target: "https://lokyr.dev".to_owned(),
                        line: 1,
                        column: None,
                    },
                    failed(
                        Code::Failed,
                        "`https://lokyr.dev` at line 1 is no file's line, which counts from 1",
                    ),
                ),
                (
                    Request::OpenAt {
                        target: "/".to_owned(),
                        line: 0,
                        column: None,
                    },
                    failed(
                        Code::Failed,
                        "`/` at line 0 is no file's line, which counts from 1",
                    ),
                ),
                (
                    Request::Opens("https://lokyr.dev/x".to_owned()),
                    Response::Boolean(true),
                ),
                (
                    Request::Opens("javascript:x".to_owned()),
                    Response::Boolean(false),
                ),
                // A file of another machine, which OSC 7's rule tells.
                (
                    Request::Opens("file://elsewhere.invalid/tmp".to_owned()),
                    Response::Boolean(false),
                ),
                (
                    Request::Open("file://elsewhere.invalid/tmp".to_owned()),
                    failed(
                        Code::Failed,
                        "a `file:` URL of another machine, `elsewhere.invalid`, does not open",
                    ),
                ),
                (
                    Request::Open("javascript:x".to_owned()),
                    failed(
                        Code::Failed,
                        "`javascript:` does not open: only http, https, mailto, file",
                    ),
                ),
                (
                    Request::Open("https://lokyr.dev/x".to_owned()),
                    Response::Done,
                ),
                (
                    Request::Decorate {
                        pane,
                        from: (4, row),
                        to: (22, row),
                        look,
                    },
                    Response::Decoration(0),
                ),
                (
                    Request::Decorate {
                        pane,
                        from: (22, row),
                        to: (4, row),
                        look,
                    },
                    failed(
                        Code::Failed,
                        "a decoration's cells are not the pane's, or not in reading order",
                    ),
                ),
                (
                    Request::Undecorate { pane, id: 999_999 },
                    failed(
                        Code::NotFound,
                        &format!("the plugin has no decoration 999999 of the pane {}", pane.0),
                    ),
                ),
                (
                    Request::MarkGutter {
                        pane,
                        row,
                        color: (255, 0, 0),
                    },
                    Response::Decoration(0),
                ),
                (
                    Request::MarkGutter {
                        pane,
                        row: 900_000,
                        color: (255, 0, 0),
                    },
                    failed(
                        Code::Failed,
                        &format!("the pane {} holds no row 900000", pane.0),
                    ),
                ),
                (
                    Request::UnmarkGutter { pane, id: 999_999 },
                    failed(
                        Code::NotFound,
                        &format!(
                            "the plugin has no mark 999999 of the pane {}'s gutter",
                            pane.0
                        ),
                    ),
                ),
                (
                    Request::LineText {
                        pane: NO_PANE,
                        row,
                        reach: 0,
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
            ]),
            Step {
                wait: 0,
                asks: Vec::new(),
                posts: vec![pointer.clone()],
                heard: vec![pointer],
            },
        ],
    }
}

/// What every plugin meets of its events and its panels: an event of its
/// own namespace, which it hears as the call returns, and another's,
/// refused; a panel opened once, and closed once, whose closing it hears;
/// and the workspace's events that it subscribed to.
fn events(world: &World) -> Scenario {
    let ping = Event::Custom {
        name: "conformer.ping".to_owned(),
        schema: "conformer.ping/1".to_owned(),
        value: Data::Integer(1),
    };
    let menu = || Request::OpenPanel {
        id: PanelId::from("menu"),
        options: PanelOptions {
            anchor: Anchor::Pointer(world.window),
            role: Role::Menu,
            close_outside: true,
            close_escape: true,
            takes_keys: false,
            close_typing: false,
        },
    };
    let created = Event::Workspace(WorkspaceEvent::TabCreated {
        tab: tab(0),
        window: world.window,
        index: 1,
    });
    Scenario {
        name: "events and panels".to_owned(),
        capabilities: None,
        subscriptions: vec!["conformer.ping".to_owned(), "tab-created".to_owned()],
        steps: vec![
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::Emit {
                            name: "conformer.ping".to_owned(),
                            schema: "conformer.ping/1".to_owned(),
                            value: Data::Integer(1),
                        },
                        Response::Done,
                    ),
                    (
                        Request::Emit {
                            name: "other.ping".to_owned(),
                            schema: "other.ping/1".to_owned(),
                            value: Data::Nil,
                        },
                        failed(
                            Code::InvalidName,
                            "`other.ping` is not an event of the plugin's, `conformer.event`",
                        ),
                    ),
                    (
                        Request::Emit {
                            name: "conformer.ping".to_owned(),
                            schema: String::new(),
                            value: Data::Nil,
                        },
                        failed(
                            Code::InvalidName,
                            "the event `conformer.ping` names no schema",
                        ),
                    ),
                    (
                        Request::Emit {
                            name: "conformer.a b".to_owned(),
                            schema: "conformer.a/1".to_owned(),
                            value: Data::Nil,
                        },
                        failed(
                            Code::InvalidName,
                            "`conformer.a b` is not an event of the plugin's, `conformer.event`",
                        ),
                    ),
                    (menu(), Response::Done),
                    (
                        menu(),
                        failed(
                            Code::AlreadyRegistered,
                            "the plugin has a panel `menu` open",
                        ),
                    ),
                    (Request::ClosePanel(PanelId::from("menu")), Response::Done),
                    (
                        Request::ClosePanel(PanelId::from("menu")),
                        failed(Code::NotFound, "the plugin has no panel `menu` open"),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![Event::PanelClosed(PanelId::from("menu")), ping],
            },
            // The workspace's events that it did not subscribe to go.
            Step {
                wait: 0,
                asks: Vec::new(),
                posts: vec![
                    created.clone(),
                    Event::Workspace(WorkspaceEvent::TabClosed {
                        tab: tab(0),
                        window: world.window,
                    }),
                ],
                heard: vec![created],
            },
        ],
    }
}

/// What a plugin of the clipboard's reading alone meets: its read, and the
/// others refused, of the capabilities that it lacks.
fn capabilities(world: &World) -> Scenario {
    let lacks = |capability: &str| {
        failed(
            Code::MissingCapability,
            &format!("the plugin `acme/conformer` lacks the capability `{capability}`"),
        )
    };
    Scenario {
        name: "capabilities".to_owned(),
        capabilities: Some(vec![Capability::ClipboardRead]),
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (
                Request::ClipboardText,
                Response::Text(Some(CLIPBOARD.to_owned())),
            ),
            (Request::SelectionText(world.pane), lacks("terminal-read")),
            (
                Request::SendText {
                    pane: world.pane,
                    text: "x".to_owned(),
                },
                lacks("pty-write"),
            ),
            (Request::Open("https://lokyr.dev".to_owned()), lacks("open")),
            (
                Request::SetClipboardText("x".to_owned()),
                lacks("clipboard-write"),
            ),
            (
                Request::OpenQuickWindow {
                    place: QuickPlace::default(),
                    directory: None,
                },
                lacks("global-hotkey"),
            ),
            (
                Request::Bind(Binding {
                    keys: "global:ctrl+`".to_owned(),
                    command: "conformer.ask".to_owned(),
                    args: Args::new(),
                    when: None,
                }),
                lacks("global-hotkey"),
            ),
            // The theme and the font that the user sees, and the config's.
            (
                Request::PreviewTheme(Some("Lokyr Light".to_owned())),
                lacks("appearance-write"),
            ),
            (
                Request::SetTheme {
                    variant: lokyr_plugin_sdk::theme::ThemeVariant::Light,
                    name: "Lokyr Light".to_owned(),
                },
                lacks("appearance-write"),
            ),
            (
                Request::PreviewFont(Some("Menlo".to_owned())),
                lacks("appearance-write"),
            ),
            (
                Request::SetFont("Menlo".to_owned()),
                lacks("appearance-write"),
            ),
            // The commands that the shell ran, and their events; the view,
            // which tells nothing of the text, to every plugin.
            (
                Request::ShellCommands {
                    pane: world.pane,
                    top: 0,
                    bottom: 100,
                },
                lacks("history-read"),
            ),
            (
                Request::CommandLine {
                    pane: world.pane,
                    command: 1,
                },
                lacks("history-read"),
            ),
            (
                Request::Subscribe(vec![
                    "viewport-changed".to_owned(),
                    "command-ended".to_owned(),
                ]),
                lacks("history-read"),
            ),
            (
                Request::Subscribe(vec!["viewport-changed".to_owned()]),
                Response::Done,
            ),
            (
                Request::Viewport(world.pane),
                Response::Viewport(world.viewport),
            ),
            // The windows, the tabs and the panes; what the panes show; what
            // a plugin draws over them; and the pointer.
            (
                Request::NewTab {
                    window: world.window,
                    index: None,
                    directory: None,
                },
                lacks("workspace-write"),
            ),
            (Request::Quit, lacks("workspace-write")),
            (Request::FocusPane(world.pane), lacks("workspace-write")),
            (Request::SelectAll(world.pane), lacks("pane-view")),
            (Request::ClearScrollback(world.pane), lacks("pane-view")),
            (
                Request::Undecorate {
                    pane: world.pane,
                    id: 1,
                },
                lacks("decorate"),
            ),
            (
                Request::Subscribe(vec!["pointer".to_owned()]),
                lacks("pointer-read"),
            ),
        ])],
    }
}

/// What a mode of the keyboard's over the scrollback asks: the terminal's
/// cursor's cell, a cursor of the plugin's over a cell and back, and the
/// selection taken away; its selections are `core-actions`', which leaves
/// the pane as it does.
fn copy_mode(world: &World) -> Scenario {
    let row = world.row;
    let no_pane = || failed(Code::NotFound, "there is no pane 999999");
    Scenario {
        name: "copy mode".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (
                Request::CursorCell(world.pane),
                Response::Cell(Some(world.cursor)),
            ),
            (Request::CursorCell(NO_PANE), no_pane()),
            (
                Request::ShowCursor {
                    pane: world.pane,
                    at: Some((4, row)),
                },
                Response::Done,
            ),
            (
                Request::ShowCursor {
                    pane: world.pane,
                    at: None,
                },
                Response::Done,
            ),
            (
                Request::ShowCursor {
                    pane: NO_PANE,
                    at: None,
                },
                no_pane(),
            ),
            (Request::Deselect(world.pane), Response::Done),
            (Request::Deselect(NO_PANE), no_pane()),
        ])],
    }
}

/// The commands that the world's pane's shell marked, read whole and by
/// the rows that they meet, the pane's view, and their events.
fn commands_and_viewport(world: &World) -> Scenario {
    let ids = |ids: &[u64]| -> Response {
        Response::ShellCommands(
            world
                .commands
                .iter()
                .filter(|command| ids.contains(&command.id))
                .copied()
                .collect(),
        )
    };
    let commands = |top: i64, bottom: i64| Request::ShellCommands {
        pane: world.pane,
        top,
        bottom,
    };
    // `true`'s prompt is on the row after the line's, its end and the
    // prompt after it on the row after that.
    let (first, second) = (world.commands[0].id, world.commands[1].id);
    let row = world.row;
    let events = vec![
        Event::ViewportChanged {
            pane: world.pane,
            viewport: world.viewport,
        },
        Event::CommandStarted {
            pane: world.pane,
            command: second,
        },
        Event::CommandEnded {
            pane: world.pane,
            command: second,
            status: Some(1),
        },
    ];
    Scenario {
        name: "commands and viewport".to_owned(),
        capabilities: None,
        subscriptions: vec![
            "viewport-changed".to_owned(),
            "command-started".to_owned(),
            "command-ended".to_owned(),
        ],
        steps: vec![Step {
            wait: 0,
            asks: vec![
                (
                    commands(-1_000_000, 1_000_000),
                    Response::ShellCommands(world.commands.clone()),
                ),
                (commands(row, row + 1), ids(&[])),
                (commands(row + 1, row + 2), ids(&[first])),
                (commands(row, row + 3), ids(&[first, second])),
                (commands(row + 2, row + 3), ids(&[second])),
                (commands(row + 5, row + 5), ids(&[])),
                // Their lines, as the long command's notification reads them.
                (
                    Request::CommandLine {
                        pane: world.pane,
                        command: first,
                    },
                    Response::Text(world.command_lines[0].clone()),
                ),
                (
                    Request::CommandLine {
                        pane: world.pane,
                        command: second,
                    },
                    Response::Text(world.command_lines[1].clone()),
                ),
                (
                    Request::CommandLine {
                        pane: world.pane,
                        command: 999_999,
                    },
                    Response::Text(None),
                ),
                (
                    Request::CommandLine {
                        pane: NO_PANE,
                        command: first,
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
                (
                    Request::ShellCommands {
                        pane: NO_PANE,
                        top: 0,
                        bottom: 1,
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
                (
                    Request::Viewport(world.pane),
                    Response::Viewport(world.viewport),
                ),
                (
                    Request::Viewport(NO_PANE),
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
            ],
            posts: events.clone(),
            heard: events,
        }],
    }
}

/// What `core-actions` asks: the text's size, the clipboard, whose write a
/// read of the same call does not see, and the writes to the focused pane,
/// the selections of each kind among them.
fn core_actions(world: &World) -> Scenario {
    let pane = world.pane;
    let select = |kind| Request::SelectCells {
        pane,
        first: (0, world.row),
        last: (3, world.row),
        kind,
    };
    Scenario {
        name: "core-actions".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            ask(vec![
                (Request::FontSize, Response::FontSize(13.0)),
                (Request::SetFontSize(Some(14.0)), Response::Done),
                (Request::FocusedPane, Response::Pane(Some(pane))),
                (Request::SelectionText(pane), Response::Text(None)),
                // A selection of each kind, as a mode of the keyboard's
                // makes them, which the select-all after takes the place of.
                (select(SelectionKind::Words), Response::Done),
                (select(SelectionKind::Lines), Response::Done),
                (select(SelectionKind::Block), Response::Done),
                (
                    Request::ClipboardText,
                    Response::Text(Some(CLIPBOARD.to_owned())),
                ),
                (Request::SetClipboardText("new".to_owned()), Response::Done),
                (
                    Request::ClipboardText,
                    Response::Text(Some(CLIPBOARD.to_owned())),
                ),
                (
                    Request::Paste {
                        pane,
                        text: "true".to_owned(),
                    },
                    Response::Done,
                ),
                (
                    Request::SendText {
                        pane,
                        text: "y".to_owned(),
                    },
                    Response::Done,
                ),
                (
                    Request::Scroll {
                        pane,
                        scroll: Scroll::Up(1),
                    },
                    Response::Done,
                ),
                (
                    Request::Scroll {
                        pane,
                        scroll: Scroll::Row(0),
                    },
                    Response::Done,
                ),
                (Request::SelectAll(pane), Response::Done),
                (Request::ClearScrollback(pane), Response::Done),
                (Request::Clear(pane), Response::Done),
                (
                    Request::Scroll {
                        pane: NO_PANE,
                        scroll: Scroll::Top,
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
                (
                    Request::Config,
                    Response::Data(Data::Map(Default::default())),
                ),
                (Request::State, Response::Data(Data::Nil)),
            ]),
            ask(vec![(
                Request::ClipboardText,
                Response::Text(Some("new".to_owned())),
            )]),
        ],
    }
}

/// The test plugin's one command, as its activation registers it.
pub fn ask_command() -> Command {
    Command {
        name: "conformer.ask".to_owned(),
        title: "Ask".to_owned(),
        category: Category::View,
        capabilities: Vec::new(),
        params: Vec::new(),
        when: None,
    }
}

/// What every plugin meets of its timers, of the host's time that the
/// steps move on: one that fires once, one that repeats, once however many
/// times it fired since the plugin last heard of it, one of no delay, which
/// waits the least, a delay past a day refused; a timer cancelled, which
/// tells nothing more, and one that no plugin started; the most timers of
/// a plugin, past which one is refused until one is cancelled. Their events
/// come to the plugin alone, which subscribed to none.
fn timers() -> Scenario {
    let start = |delay: u64, repeats: bool| Request::StartTimer { delay, repeats };
    let fired = |ids: &[u64]| -> Vec<Event> {
        ids.iter().map(|&k| Event::Timer { id: timer(k) }).collect()
    };
    let step = |wait: u64, heard: Vec<Event>| Step {
        wait,
        asks: Vec::new(),
        posts: Vec::new(),
        heard,
    };
    // As many as a plugin has, then one more, refused.
    let mut most: Vec<(Request, Response)> = (0..MOST_TIMERS as u64)
        .map(|k| (start(MOST_DELAY, false), Response::Timer(timer(3 + k))))
        .collect();
    most.push((start(10, false), failed(Code::Unavailable, TIMERS_BOUND)));
    let next = 3 + MOST_TIMERS as u64;
    Scenario {
        name: "timers".to_owned(),
        capabilities: Some(Vec::new()),
        subscriptions: Vec::new(),
        steps: vec![
            ask(vec![
                (start(100, false), Response::Timer(timer(0))),
                (start(40, true), Response::Timer(timer(1))),
                (start(0, false), Response::Timer(timer(2))),
                (
                    start(MOST_DELAY + 1, false),
                    failed(Code::Failed, DELAY_RANGE),
                ),
                (Request::CancelTimer(NO_TIMER), Response::Done),
            ]),
            // The timer of no delay, at the least.
            step(LEAST_DELAY, fired(&[2])),
            // The one that repeats, at 40 ms.
            step(40 - LEAST_DELAY, fired(&[1])),
            // At 140 ms, the one of 100 ms, and the one that repeats once,
            // which fired at 80 and at 120.
            step(100, fired(&[0, 1])),
            // The one that repeats cancelled, and the others fired once:
            // nothing more.
            Step {
                wait: 1000,
                asks: vec![(Request::CancelTimer(timer(1)), Response::Done)],
                posts: Vec::new(),
                heard: Vec::new(),
            },
            ask(most),
            // One cancelled, another can start.
            Step {
                wait: LEAST_DELAY,
                asks: vec![
                    (Request::CancelTimer(timer(3)), Response::Done),
                    (start(LEAST_DELAY, false), Response::Timer(timer(next))),
                ],
                posts: Vec::new(),
                heard: fired(&[next]),
            },
        ],
    }
}

/// What every plugin meets of its declarations: a command, a binding,
/// subscriptions and menus' items, each refused of a wrong name, keys,
/// context or arguments, and a command registered twice.
fn declarations() -> Scenario {
    let bind = |keys: &str, command: &str, when: Option<&str>| {
        Request::Bind(Binding {
            keys: keys.to_owned(),
            command: command.to_owned(),
            args: Args::new(),
            when: when.map(str::to_owned),
        })
    };
    let invalid = |message: &str| failed(Code::InvalidBinding, message);
    let ticked = |command: &str, args: Args, ticked: bool| {
        Request::AddMenuItem(MenuItem {
            place: MenuPlace::Bar(Category::View),
            command: command.to_owned(),
            args,
            title: Some("menu-ask".to_owned()),
            submenu: None,
            ticked,
        })
    };
    let item = |command: &str, args: Args| ticked(command, args, false);
    let other = Command {
        name: "conformer.other".to_owned(),
        ..ask_command()
    };
    Scenario {
        name: "declarations".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (Request::RegisterCommand(other), Response::Done),
            (
                Request::Commands,
                Response::Commands(
                    ["conformer.ask", "conformer.other"]
                        .map(|name| CommandInfo {
                            name: name.to_owned(),
                            title: "Ask".to_owned(),
                            category: Category::View,
                            params: Vec::new(),
                            plugin: Some("conformer".to_owned()),
                            keys: None,
                            when: None,
                            enabled: Some(true),
                        })
                        .to_vec(),
                ),
            ),
            (
                Request::RegisterCommand(ask_command()),
                failed(
                    Code::AlreadyRegistered,
                    "`conformer.ask` is registered already",
                ),
            ),
            (
                Request::RegisterCommand(Command {
                    name: "Conformer.x".to_owned(),
                    ..ask_command()
                }),
                failed(
                    Code::InvalidName,
                    "`Conformer.x` is not a command's name, `namespace.action`",
                ),
            ),
            (
                bind("cmd+shift+k", "conformer.ask", Some("pane.focused")),
                Response::Done,
            ),
            (
                bind("cmd+shfit+k", "conformer.ask", None),
                invalid("`shfit` is not a modifier, in `cmd+shfit+k`"),
            ),
            (
                bind("cmd+k", "conformer ask", None),
                invalid("`conformer ask` is not a command's name, `namespace.action`"),
            ),
            (
                bind("cmd+k", "conformer.ask", Some("pane.focused &&")),
                invalid("the expression ends early"),
            ),
            (item("conformer.ask", Args::new()), Response::Done),
            // The same item ticked: its tick changed.
            (ticked("conformer.ask", Args::new(), true), Response::Done),
            // In a submenu, another item.
            (
                Request::AddMenuItem(MenuItem {
                    place: MenuPlace::Bar(Category::View),
                    command: "conformer.ask".to_owned(),
                    args: Args::new(),
                    title: Some("menu-ask".to_owned()),
                    submenu: Some("menu-more".to_owned()),
                    ticked: false,
                }),
                Response::Done,
            ),
            (
                item("conformer.none", Args::new()),
                failed(Code::NotFound, "no command `conformer.none`"),
            ),
            (
                item("conformer ask", Args::new()),
                failed(
                    Code::InvalidName,
                    "`conformer ask` is not a command's name, `namespace.action`",
                ),
            ),
            (
                item(
                    "conformer.ask",
                    Args::from([("depth".to_owned(), Value::Integer(1))]),
                ),
                failed(
                    Code::UnknownArgument,
                    "`conformer.ask` takes no argument `depth`",
                ),
            ),
            (bind("global:ctrl+`", "conformer.ask", None), Response::Done),
            (
                bind("global:ctrl+~", "conformer.ask", None),
                invalid(
                    "`~` takes Shift on the US layout, by whose keys a global key goes: write `ctrl+shift+``, in `ctrl+~`",
                ),
            ),
            (
                bind("global:ctrl+`", "conformer.ask", Some("pane.focused")),
                invalid("a global key holds everywhere, without `when`, in `global:ctrl+``"),
            ),
            (
                Request::Subscribe(vec!["window-opened".to_owned(), "window-focus".to_owned()]),
                Response::Done,
            ),
            (
                Request::Subscribe(vec!["tab-opened".to_owned()]),
                failed(
                    Code::InvalidName,
                    "`tab-opened` is no event: one of the workspace's, as `tab-created`, or a plugin's, `namespace.event`",
                ),
            ),
        ])],
    }
}

/// What every plugin asks of the app: a command run, a window opened or
/// closed, the app quit and the text's size, and what of them is refused.
fn app_requests(world: &World) -> Scenario {
    Scenario {
        name: "the app's requests".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (Request::Themes, Response::Themes(super::themes())),
            // The system's monospaced families, a preview and its end, the
            // choice, and a family that is not there or not monospaced.
            (
                Request::Fonts,
                Response::Fonts(lokyr_plugin_sdk::fake::fonts()),
            ),
            (
                Request::PreviewFont(Some("Menlo".to_owned())),
                Response::Done,
            ),
            (
                Request::PreviewFont(Some("menlo".to_owned())),
                Response::Done,
            ),
            (Request::PreviewFont(None), Response::Done),
            (
                Request::PreviewFont(Some("Helvetica".to_owned())),
                failed(
                    Code::NotFound,
                    "there is no monospaced font family `Helvetica`",
                ),
            ),
            (Request::SetFont("Monaco".to_owned()), Response::Done),
            // The system's day, whether a notice shows, and whether the
            // state lasts, as the harness gives the kernel's host a folder.
            (Request::Day, Response::Day(lokyr_plugin_sdk::fake::DAY)),
            (Request::NoticeShows, Response::Boolean(false)),
            (Request::KeepsState, Response::Boolean(true)),
            // No agent runs in the scenarios' panes.
            (Request::Agents, Response::Agents(Vec::new())),
            (
                Request::SetFont("Comic Sans".to_owned()),
                failed(
                    Code::NotFound,
                    "there is no monospaced font family `Comic Sans`",
                ),
            ),
            (
                Request::PreviewTheme(Some("Dracula".to_owned())),
                Response::Done,
            ),
            (Request::PreviewTheme(None), Response::Done),
            (
                Request::PreviewTheme(Some("../x".to_owned())),
                failed(Code::Failed, "`../x` is no theme's name"),
            ),
            (
                Request::SetTheme {
                    variant: lokyr_plugin_sdk::theme::ThemeVariant::Dark,
                    name: "Lokyr Dark".to_owned(),
                },
                Response::Done,
            ),
            // A theme that is not there, or of the other appearance, which
            // the config would keep.
            (
                Request::SetTheme {
                    variant: lokyr_plugin_sdk::theme::ThemeVariant::Dark,
                    name: "Dracula".to_owned(),
                },
                failed(Code::NotFound, "there is no dark theme `Dracula`"),
            ),
            (
                Request::SetTheme {
                    variant: lokyr_plugin_sdk::theme::ThemeVariant::Dark,
                    name: "Lokyr Light".to_owned(),
                },
                failed(Code::NotFound, "there is no dark theme `Lokyr Light`"),
            ),
            (
                Request::RunCommand {
                    name: "conformer.ask".to_owned(),
                    args: Args::new(),
                },
                Response::Done,
            ),
            (
                Request::RunCommand {
                    name: "app.none".to_owned(),
                    args: Args::new(),
                },
                failed(Code::NotFound, "no command `app.none`"),
            ),
            (
                Request::RunCommand {
                    name: "conformer.ask".to_owned(),
                    args: Args::from([("n".to_owned(), Value::Integer(1))]),
                },
                failed(
                    Code::UnknownArgument,
                    "`conformer.ask` takes no argument `n`",
                ),
            ),
            (Request::OpenWindow { directory: None }, Response::Done),
            (
                Request::OpenQuickWindow {
                    place: QuickPlace::default(),
                    directory: Some("~".to_owned()),
                },
                Response::Window(Some(window(0))),
            ),
            (
                Request::OpenQuickWindow {
                    place: QuickPlace {
                        primary: QuickLength::Percent(0.0),
                        ..QuickPlace::default()
                    },
                    directory: None,
                },
                failed(
                    Code::Failed,
                    "a quick window's length is a percentage above 0 and at most 100, or from 1 to 100000 points",
                ),
            ),
            (
                Request::OpenQuickWindow {
                    place: QuickPlace {
                        animation: 2001,
                        ..QuickPlace::default()
                    },
                    directory: None,
                },
                failed(
                    Code::Failed,
                    "a quick window slides from 0 to 2000 milliseconds, not 2001",
                ),
            ),
            (
                Request::ShowQuickWindow {
                    window: NO_WINDOW,
                    place: None,
                },
                failed(Code::NotFound, "there is no quick window 999999"),
            ),
            (
                Request::OpenWindow {
                    directory: Some("/tmp".to_owned()),
                },
                Response::Done,
            ),
            (
                Request::OpenWindow {
                    directory: Some("tmp".to_owned()),
                },
                failed(
                    Code::Failed,
                    "`tmp` is not a path that is absolute or from `~`",
                ),
            ),
            (Request::CloseWindow(world.window), Response::Done),
            (
                Request::CloseWindow(NO_WINDOW),
                failed(Code::NotFound, "there is no window 999999"),
            ),
            (Request::Quit, Response::Done),
            (
                Request::SetFontSize(Some(1000.0)),
                failed(
                    Code::Failed,
                    "the size, in points, is from 4 to 200, not 1000",
                ),
            ),
            (Request::SetFontSize(None), Response::Done),
        ])],
    }
}

/// What every plugin meets of its state and of the reads of rows: a state
/// kept, which a read of the same call does not see, and one refused; a
/// read of rows, whose rows come as an event; the styles of a row that the
/// pane does not hold, and of a pane not there.
fn state_and_rows(world: &World) -> Scenario {
    let kept = Data::Map(BTreeMap::from([("a".to_owned(), Data::Integer(1))]));
    Scenario {
        name: "state and rows".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            Step {
                wait: 0,
                asks: vec![
                    (Request::SetState(kept.clone()), Response::Done),
                    (Request::State, Response::Data(Data::Nil)),
                    (
                        Request::SetState(Data::Text("x".repeat(MOST_STATE))),
                        failed(
                            Code::Failed,
                            &format!(
                                "a plugin's state is {MOST_STATE} bytes at most as JSON, not {}",
                                MOST_STATE + 2
                            ),
                        ),
                    ),
                    (
                        Request::ReadLines {
                            pane: world.pane,
                            first: world.row,
                            count: 1,
                        },
                        Response::Reading(0),
                    ),
                    (
                        Request::Styles {
                            pane: world.pane,
                            row: 1000,
                        },
                        Response::Styles(None),
                    ),
                    (
                        Request::Styles {
                            pane: NO_PANE,
                            row: 0,
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![Event::Lines {
                    id: 0,
                    pane: world.pane,
                    lines: vec![Line {
                        row: world.row,
                        text: LINE.to_owned(),
                        wrapped: false,
                    }],
                    complete: true,
                }],
            },
            // A search of the pane's terminal, whose one match, of the
            // program's line, comes as an event; and what is refused.
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::Search {
                            pane: world.pane,
                            query: SearchQuery {
                                text: "now".to_owned(),
                                ..SearchQuery::default()
                            },
                        },
                        Response::Searching(0),
                    ),
                    (
                        Request::Search {
                            pane: world.pane,
                            query: SearchQuery::default(),
                        },
                        failed(Code::Failed, "a search of no text finds nothing"),
                    ),
                    (
                        Request::Search {
                            pane: NO_PANE,
                            query: SearchQuery {
                                text: "now".to_owned(),
                                ..SearchQuery::default()
                            },
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                    (
                        Request::SearchMatches {
                            search: NO_SEARCH,
                            first: 0,
                            count: 10,
                        },
                        failed(Code::NotFound, "there is no search 999999"),
                    ),
                    (
                        Request::ShowMatches {
                            search: NO_SEARCH,
                            all: true,
                            current: None,
                        },
                        failed(Code::NotFound, "there is no search 999999"),
                    ),
                    (
                        Request::CurrentMatch(NO_SEARCH),
                        failed(Code::NotFound, "there is no search 999999"),
                    ),
                    (Request::EndSearch(NO_SEARCH), Response::Done),
                    (
                        Request::SelectCells {
                            pane: NO_PANE,
                            first: (0, 0),
                            last: (1, 0),
                            kind: SelectionKind::Cells,
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![Event::Searched {
                    id: 0,
                    pane: world.pane,
                    matches: vec![SearchMatch {
                        first: (24, world.row),
                        last: (26, world.row),
                    }],
                    count: 1,
                    done: true,
                }],
            },
            ask(vec![(Request::State, Response::Data(kept))]),
        ],
    }
}

/// The test plugin's panel, of the id `menu`.
pub const MENU: &str = "menu";

/// What every plugin meets of its views: a panel opened, whose render it
/// gives, a field's text set and a list's selection moved in its tree,
/// renders asked for, and what of them is refused, of a node of another
/// kind, a node not there, a view not its own, a panel of no id or of an
/// anchor not there.
fn views(world: &World) -> Scenario {
    let menu = ViewId::Panel(PanelId::from(MENU));
    let options = |anchor| PanelOptions {
        anchor,
        role: Role::Menu,
        close_outside: true,
        close_escape: true,
        takes_keys: false,
        close_typing: false,
    };
    let set_text = |key: &str| Request::SetText {
        view: menu.clone(),
        key: key.into(),
        text: "x".to_owned(),
        revision: 0,
    };
    let select = |key: &str| Request::Select {
        view: menu.clone(),
        key: key.into(),
        item: 1,
    };
    let status_bar = ViewId::StatusBar(world.window);
    Scenario {
        name: "views".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::OpenPanel {
                            id: PanelId::from(MENU),
                            options: options(Anchor::Pointer(world.window)),
                        },
                        Response::Done,
                    ),
                    (
                        Request::OpenPanel {
                            id: PanelId::from(""),
                            options: options(Anchor::Pointer(world.window)),
                        },
                        failed(Code::InvalidName, "a panel's id is not empty"),
                    ),
                    (
                        Request::OpenPanel {
                            id: PanelId::from("other"),
                            options: options(Anchor::Pointer(NO_WINDOW)),
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                    (
                        Request::OpenPanel {
                            id: PanelId::from("other"),
                            options: options(Anchor::Cursor(NO_PANE)),
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                    // An overlay across a pane's top, as a command's header.
                    (
                        Request::OpenPanel {
                            id: PanelId::from("header"),
                            options: PanelOptions {
                                anchor: Anchor::PaneTop(world.pane),
                                role: Role::Overlay,
                                close_outside: false,
                                close_escape: false,
                                takes_keys: false,
                                close_typing: false,
                            },
                        },
                        Response::Done,
                    ),
                    (Request::ClosePanel(PanelId::from("header")), Response::Done),
                    (
                        Request::OpenPanel {
                            id: PanelId::from("other"),
                            options: options(Anchor::PaneTop(NO_PANE)),
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![Event::PanelClosed(PanelId::from("header"))],
            },
            // Its render came as it opened.
            ask(vec![
                (set_text("query"), Response::Done),
                (select("results"), Response::Done),
                (
                    set_text("results"),
                    failed(Code::NotFound, "`results` is not a field"),
                ),
                (
                    select("query"),
                    failed(Code::NotFound, "`query` is not a list"),
                ),
                (
                    set_text("none"),
                    failed(
                        Code::NotFound,
                        &format!("the view {menu:?} has no node `none`"),
                    ),
                ),
                (Request::RequestRender(menu.clone()), Response::Done),
                // The keyboard's focus given to a node, of none not there.
                (
                    Request::FocusNode {
                        view: menu.clone(),
                        key: "query".into(),
                    },
                    Response::Done,
                ),
                (
                    Request::FocusNode {
                        view: menu.clone(),
                        key: "none".into(),
                    },
                    failed(
                        Code::NotFound,
                        &format!("the view {menu:?} has no node `none`"),
                    ),
                ),
                // A node shown, of a render that the view may have yet to
                // show, and none of a view that it has not rendered.
                (
                    Request::Reveal {
                        view: menu.clone(),
                        key: "results".into(),
                    },
                    Response::Done,
                ),
                (
                    Request::Reveal {
                        view: status_bar.clone(),
                        key: "x".into(),
                    },
                    failed(
                        Code::NotFound,
                        &format!("the plugin has rendered no view {status_bar:?}"),
                    ),
                ),
                (
                    Request::RequestRender(status_bar.clone()),
                    failed(
                        Code::NotFound,
                        &format!("the plugin has no view {status_bar:?}"),
                    ),
                ),
            ]),
            Step {
                wait: 0,
                asks: vec![(Request::ClosePanel(PanelId::from(MENU)), Response::Done)],
                posts: Vec::new(),
                heard: vec![Event::PanelClosed(PanelId::from(MENU))],
            },
        ],
    }
}

/// What every plugin meets of the workspace's writes, of the tabs, the
/// splits and the windows that its calls open, which a later write of the
/// same call finds, and of their events; what of them is refused; and the
/// workspace left as the scenario found it.
fn workspace(world: &World) -> Scenario {
    let (w, t, p) = (world.window, world.tab, world.pane);
    let ws = |event| Event::Workspace(event);
    let info = |id, active, focused, panes: Vec<PaneId>, zoomed| TabInfo {
        id,
        window: w,
        title: String::new(),
        color: None,
        active,
        focused,
        panes,
        zoomed,
        activity: false,
        bell: false,
        progress: None,
        printing: false,
        running: false,
        alternate: false,
        secure: false,
        agent: None,
        agent_message: None,
    };
    let split = |ratio| Layout::Split {
        direction: Direction::Vertical,
        ratio,
        first: Box::new(Layout::Pane(pane(0))),
        second: Box::new(Layout::Pane(pane(1))),
    };
    let names = WorkspaceEvent::NAMES.iter().map(|&name| name.to_owned());
    Scenario {
        name: "the workspace".to_owned(),
        capabilities: None,
        subscriptions: names.collect(),
        steps: vec![
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::NewTab {
                            window: w,
                            index: None,
                            directory: Some("~/x".to_owned()),
                        },
                        Response::Tab {
                            tab: tab(0),
                            pane: pane(0),
                        },
                    ),
                    (
                        Request::Split {
                            pane: pane(0),
                            direction: Direction::Vertical,
                            side: Side::Second,
                        },
                        Response::Pane(Some(pane(1))),
                    ),
                    (
                        Request::SendText {
                            pane: pane(1),
                            text: "x".to_owned(),
                        },
                        Response::Done,
                    ),
                    (Request::ActivateTab(tab(0)), Response::Done),
                    (Request::FocusPane(pane(1)), Response::Done),
                    (
                        Request::Zoom {
                            tab: tab(0),
                            pane: Some(pane(0)),
                        },
                        Response::Done,
                    ),
                    (
                        Request::NewTab {
                            window: NO_WINDOW,
                            index: None,
                            directory: None,
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                    (
                        Request::NewTab {
                            window: w,
                            index: None,
                            directory: Some("x".to_owned()),
                        },
                        failed(
                            Code::Failed,
                            "`x` is not a path that is absolute or from `~`",
                        ),
                    ),
                    (
                        Request::CloseTab {
                            tab: NO_TAB,
                            ask: false,
                        },
                        failed(Code::NotFound, "there is no tab 999999"),
                    ),
                    (
                        Request::ClosePane {
                            pane: NO_PANE,
                            ask: true,
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                    (
                        Request::MoveTab {
                            tab: tab(0),
                            window: NO_WINDOW,
                            index: 0,
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                    (
                        Request::ResizePane {
                            pane: p,
                            edge: Edge::Right,
                            by: 2.0,
                        },
                        failed(Code::Failed, "a resize of 2 is not a share from -1 to 1"),
                    ),
                    (
                        Request::SetRatio {
                            tab: t,
                            path: Vec::new(),
                            ratio: Some(3.0),
                        },
                        failed(Code::Failed, "a split's ratio is a share from 0 to 1"),
                    ),
                    // Of no change: a pane of no edge, a tab of one pane.
                    (
                        Request::ResizePane {
                            pane: p,
                            edge: Edge::Right,
                            by: 0.1,
                        },
                        Response::Done,
                    ),
                    (Request::Equalize(t), Response::Done),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::TabCreated {
                        tab: tab(0),
                        window: w,
                        index: 1,
                    }),
                    ws(WorkspaceEvent::PaneCreated {
                        pane: pane(0),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::PaneCreated {
                        pane: pane(1),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::TabActivated {
                        tab: tab(0),
                        window: w,
                    }),
                    ws(WorkspaceEvent::PaneFocused {
                        pane: pane(1),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                ],
            },
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::Windows,
                        Response::Windows(vec![WindowInfo {
                            id: w,
                            tabs: vec![t, tab(0)],
                            active: tab(0),
                            number: None,
                            keys: None,
                            bell: false,
                            quick: false,
                            hidden: false,
                        }]),
                    ),
                    (
                        Request::Tabs(w),
                        Response::Tabs(vec![
                            info(t, false, p, vec![p], None),
                            info(tab(0), true, pane(1), vec![pane(0), pane(1)], Some(pane(0))),
                        ]),
                    ),
                    (Request::Layout(tab(0)), Response::Layout(split(None))),
                    (Request::FocusedPane, Response::Pane(Some(pane(1)))),
                    (
                        Request::SetRatio {
                            tab: tab(0),
                            path: Vec::new(),
                            ratio: Some(0.3),
                        },
                        Response::Done,
                    ),
                    (
                        Request::PaneGrid(pane(1)),
                        Response::Grid {
                            columns: 80,
                            rows: 24,
                        },
                    ),
                    // The title of a pane not there, as a pane's title of
                    // its program's folder is the kernel's alone to know.
                    (
                        Request::PaneTitle(NO_PANE),
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                    // Swapped, and back.
                    (
                        Request::SwapPanes {
                            pane: pane(0),
                            with: pane(1),
                        },
                        Response::Done,
                    ),
                    (
                        Request::SwapPanes {
                            pane: pane(1),
                            with: pane(0),
                        },
                        Response::Done,
                    ),
                    (
                        Request::MoveTab {
                            tab: tab(0),
                            window: w,
                            index: 0,
                        },
                        Response::Done,
                    ),
                    (
                        Request::TabToNewWindow {
                            tab: tab(0),
                            at: Some(ScreenPoint { x: 40.0, y: 60.0 }),
                        },
                        Response::Done,
                    ),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::TabMoved {
                        tab: tab(0),
                        from: w,
                        window: w,
                        index: 0,
                    }),
                    ws(WorkspaceEvent::WindowOpened { window: window(0) }),
                    ws(WorkspaceEvent::TabMoved {
                        tab: tab(0),
                        from: w,
                        window: window(0),
                        index: 0,
                    }),
                    ws(WorkspaceEvent::TabActivated { tab: t, window: w }),
                ],
            },
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::Windows,
                        Response::Windows(vec![
                            WindowInfo {
                                id: w,
                                tabs: vec![t],
                                active: t,
                                number: None,
                                keys: None,
                                bell: false,
                                quick: false,
                                hidden: false,
                            },
                            WindowInfo {
                                id: window(0),
                                tabs: vec![tab(0)],
                                active: tab(0),
                                number: None,
                                keys: None,
                                bell: false,
                                quick: false,
                                hidden: false,
                            },
                        ]),
                    ),
                    (Request::FocusedPane, Response::Pane(Some(p))),
                    (Request::Layout(tab(0)), Response::Layout(split(Some(0.3)))),
                    (Request::PaneToNewWindow(pane(1)), Response::Done),
                    (
                        Request::PaneToNewTab {
                            pane: pane(0),
                            window: w,
                            index: 5,
                        },
                        Response::Done,
                    ),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::PaneFocused {
                        pane: pane(0),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::WindowOpened { window: window(1) }),
                    ws(WorkspaceEvent::TabCreated {
                        tab: tab(1),
                        window: window(1),
                        index: 0,
                    }),
                    ws(WorkspaceEvent::PaneMoved {
                        pane: pane(1),
                        from: tab(0),
                        tab: tab(1),
                    }),
                    ws(WorkspaceEvent::TabMoved {
                        tab: tab(0),
                        from: window(0),
                        window: w,
                        index: 1,
                    }),
                    ws(WorkspaceEvent::WindowClosed { window: window(0) }),
                ],
            },
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::CloseTab {
                            tab: tab(1),
                            ask: false,
                        },
                        Response::Done,
                    ),
                    // Of a shell at its prompt, as the user confirms it.
                    (
                        Request::CloseTab {
                            tab: tab(0),
                            ask: true,
                        },
                        Response::Done,
                    ),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::PaneClosed {
                        pane: pane(1),
                        tab: tab(1),
                    }),
                    ws(WorkspaceEvent::TabClosed {
                        tab: tab(1),
                        window: window(1),
                    }),
                    ws(WorkspaceEvent::WindowClosed { window: window(1) }),
                    ws(WorkspaceEvent::PaneClosed {
                        pane: pane(0),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::TabClosed {
                        tab: tab(0),
                        window: w,
                    }),
                ],
            },
            // A new tab into the first's pane, below it, whole, half and
            // half: the tab goes, the first showing still; and none into a
            // pane of its own.
            Step {
                wait: 0,
                asks: vec![
                    (
                        Request::NewTab {
                            window: w,
                            index: None,
                            directory: None,
                        },
                        Response::Tab {
                            tab: tab(2),
                            pane: pane(2),
                        },
                    ),
                    (
                        Request::TabIntoPane {
                            tab: tab(2),
                            pane: p,
                            edge: Edge::Bottom,
                        },
                        Response::Done,
                    ),
                    (
                        Request::TabIntoPane {
                            tab: t,
                            pane: p,
                            edge: Edge::Left,
                        },
                        failed(Code::Failed, OWN_PANE),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::TabCreated {
                        tab: tab(2),
                        window: w,
                        index: 1,
                    }),
                    ws(WorkspaceEvent::PaneCreated {
                        pane: pane(2),
                        tab: tab(2),
                    }),
                    ws(WorkspaceEvent::PaneMoved {
                        pane: pane(2),
                        from: tab(2),
                        tab: t,
                    }),
                    ws(WorkspaceEvent::TabClosed {
                        tab: tab(2),
                        window: w,
                    }),
                    ws(WorkspaceEvent::LayoutChanged { tab: t }),
                    ws(WorkspaceEvent::PaneFocused {
                        pane: pane(2),
                        tab: t,
                    }),
                ],
            },
            ask(vec![(
                Request::Layout(t),
                Response::Layout(Layout::Split {
                    direction: Direction::Vertical,
                    ratio: Some(0.5),
                    first: Box::new(Layout::Pane(p)),
                    second: Box::new(Layout::Pane(pane(2))),
                }),
            )]),
            // Its pane closed, the first as it was, for the next scenarios.
            Step {
                wait: 0,
                asks: vec![(
                    Request::ClosePane {
                        pane: pane(2),
                        ask: false,
                    },
                    Response::Done,
                )],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::PaneClosed {
                        pane: pane(2),
                        tab: t,
                    }),
                    ws(WorkspaceEvent::LayoutChanged { tab: t }),
                    ws(WorkspaceEvent::PaneFocused { pane: p, tab: t }),
                ],
            },
            ask(vec![(
                Request::Windows,
                Response::Windows(vec![WindowInfo {
                    id: w,
                    tabs: vec![t],
                    active: t,
                    number: None,
                    keys: None,
                    bell: false,
                    quick: false,
                    hidden: false,
                }]),
            )]),
        ],
    }
}

/// `scenario` as JSON, as a front end of another language takes it.
pub fn json(scenario: &Scenario) -> String {
    serde_json::to_string_pretty(scenario).expect("a scenario writes as JSON")
}

/// The scenario of `text`, its JSON.
pub fn from_json(text: &str) -> Scenario {
    serde_json::from_str(text).expect("a scenario reads from JSON")
}
