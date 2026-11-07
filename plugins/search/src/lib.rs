//! Built-in plugin of the search: the find bar, a panel at the focused
//! pane's top right, of a search field of macOS's look, whose magnifying
//! glass lists the recent searches, and whose toggles of case, whole words
//! and regexes sit at its end; the count of the matches, of a width of its
//! own; and the steps through them, each of which selects its match, ready
//! for a copy, and brings it into the view, over the kernel's search
//! service. The search goes from the bottom up, as the newest output is at
//! the bottom: its next match is the one above, its previous the one below.
//! Escape closes the bar, the current match staying selected; the bar
//! opens again of the last search and its options. Like every built-in
//! plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error};
use lokyr_plugin_sdk::host::{PaneId, SearchCase, SearchMatch, SearchQuery};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Edges, Hover, Justify, Layout, Length, Node, PointerShape, Shadow, Style, Token,
    Transition, UiEventKind,
};

lokyr_plugin_sdk::export_plugin!(Search);

/// Opens the find bar at the focused pane, its field taking the keys, of
/// its last search's text, which it searches again.
const OPEN: &str = "search.open";

/// Selects the next match, the one above the current, the last after the
/// first, as `wrap-around` has it; opens the bar while it is closed.
const NEXT: &str = "search.next";

/// Selects the previous match, the one below the current.
const PREVIOUS: &str = "search.previous";

/// The find bar, a panel, and its recent searches' menu.
const BAR: &str = "find";
const RECENT: &str = "find-recent";

/// The keys of the bar's nodes.
const FIELD: &str = "query";
const COUNT: &str = "count";
const CASE: &str = "case";
const REGEX: &str = "regex";
const WORD: &str = "word";
const UP: &str = "up";
const DOWN: &str = "down";
const PAST: &str = "recent";
const CLOSE: &str = "close";

/// The prefix of the key of a recent search's item, of its place.
const PAST_ITEM: &str = "recent-";

/// The most recent searches that the plugin's state keeps.
const MOST_RECENT: usize = 20;

/// The keys of the plugin's state: the recent searches, and the options
/// of the last search.
const STATE_RECENT: &str = "recent";
const STATE_CASE: &str = "case-sensitive";
const STATE_REGEX: &str = "regex";
const STATE_WORD: &str = "whole-word";

/// The width of the search field, its glass, its text and its toggles, in
/// points.
const FIELD_WIDTH: f32 = 260.0;

/// The width of the count, which its text never changes, in points: of
/// `9999 of 9999`, the longest of its numbers, which go short past it,
/// and of its longest text, Italian's `Nessun risultato`.
const COUNT_WIDTH: f32 = 112.0;

/// The side of a button of an icon, and of a toggle, in points, and of
/// those in the field.
const BUTTON: f32 = 22.0;
const IN_FIELD: f32 = 20.0;

/// How long a button's hover and press fade, in seconds.
const FADE: f32 = 0.12;

/// The plugin's catalogs, of its commands' titles and its bar's texts.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/search.ftl")),
    ("it", include_str!("../locales/it/search.ftl")),
];

/// The texts of the bar, in English, where the host gives none.
const ENGLISH: [(&str, &str); 12] = [
    ("find-placeholder", "Find"),
    ("find-none", "No results"),
    ("find-invalid", "Invalid regex"),
    ("find-case", "Match Case"),
    ("find-regex", "Regular Expression"),
    ("find-word", "Whole Words"),
    ("find-next", "Find Next"),
    ("find-previous", "Find Previous"),
    ("find-history", "Recent Searches"),
    ("find-close", "Close"),
    ("find-count-unknown", "{ $count } found"),
    ("find-count", "{ $current } of { $count }"),
];

/// The text of the message `id` of `args`, in the language shown, or
/// English's where the host gives none, its arguments in their places.
fn message(cx: &mut Cx<'_>, id: &str, args: &[(&str, &str)]) -> String {
    let data: Vec<(&str, Data)> = args
        .iter()
        .map(|&(name, value)| (name, Data::Text(value.to_owned())))
        .collect();
    if let Ok(text) = cx.message(id, &data)
        && text != id
    {
        return text;
    }
    let english = ENGLISH
        .iter()
        .find(|(english, _)| *english == id)
        .map_or(id, |(_, text)| *text);
    args.iter().fold(english.to_owned(), |text, (name, value)| {
        text.replace(&format!("{{ ${name} }}"), value)
    })
}

/// `count`, short past 9999, so that the count keeps its width: `12K`,
/// `3M`.
pub fn short(count: usize) -> String {
    match count {
        0..10_000 => count.to_string(),
        10_000..1_000_000 => format!("{}K", count / 1000),
        _ => format!("{}M", count / 1_000_000),
    }
}

/// The config of `[plugins.search]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Settings {
    /// Whether case counts while the case toggle is off: of a capital
    /// letter alone, smart, or never.
    case: SearchCase,
    /// Whether a step past the last match goes on from the first, and the
    /// other way.
    wrap_around: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            case: SearchCase::Smart,
            wrap_around: true,
        }
    }
}

impl Settings {
    /// The settings of `config`, its section, which the host checked
    /// against the schema and gave its defaults.
    fn of(config: &Data) -> Self {
        let mut settings = Self::default();
        let Data::Map(config) = config else {
            return settings;
        };
        if let Some(Data::Text(case)) = config.get("case")
            && case == "insensitive"
        {
            settings.case = SearchCase::Insensitive;
        }
        if let Some(Data::Boolean(wrap)) = config.get("wrap-around") {
            settings.wrap_around = *wrap;
        }
        settings
    }
}

/// The options of a search, of the bar's toggles, which the plugin's
/// state keeps from one search to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Whether case counts.
    pub sensitive: bool,
    /// Whether the text is a regex.
    pub regex: bool,
    /// Whether matches take whole words alone.
    pub whole_word: bool,
}

/// Which way a step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Up, to the older output: the next match.
    Up,
    /// Down, to the newer output: the previous match.
    Down,
}

/// The find bar of a pane, while it is open.
#[derive(Debug)]
struct Find {
    /// The pane that it searches.
    pane: PaneId,
    /// The field's text, and its revision.
    text: String,
    revision: u64,
    /// The search's options.
    options: Options,
    /// The search under way, or done, of the text, if one is.
    search: Option<u64>,
    /// Its matches told so far, in the rows' order.
    known: Vec<SearchMatch>,
    /// How many it found, those past the kept ones too, and whether it
    /// went to its end.
    count: usize,
    done: bool,
    /// The match selected, if one is.
    current: Option<SearchMatch>,
    /// Whether the host refused the search, as of a regex that does not
    /// compile.
    invalid: bool,
    /// Whether its field, opened anew, waits for its text, which it takes
    /// as it takes the keys, once its view is there.
    filling: bool,
    /// The count's text as the bar showed it last, which a new search
    /// leaves until it tells something sure.
    shown: String,
}

impl Find {
    /// The bar of `pane`, of no text yet, of `options`.
    fn new(pane: PaneId, options: Options) -> Self {
        Self {
            pane,
            text: String::new(),
            revision: 0,
            options,
            search: None,
            known: Vec::new(),
            count: 0,
            done: false,
            current: None,
            invalid: false,
            filling: false,
            shown: String::new(),
        }
    }

    /// The query of its text and its options, of `settings`' case where
    /// its toggle leaves case aside: smart, or insensitive.
    fn query(&self, settings: Settings) -> SearchQuery {
        SearchQuery {
            text: self.text.clone(),
            regex: self.options.regex,
            case: if self.options.sensitive {
                SearchCase::Sensitive
            } else {
                settings.case
            },
            whole_word: self.options.whole_word,
        }
    }

    /// Takes the matches of a part of its search, which keep the rows'
    /// order among those told before, as the parts' rows do not overlap.
    fn take(&mut self, matches: &[SearchMatch]) {
        let Some(first) = matches.first() else {
            return;
        };
        let at = self
            .known
            .partition_point(|known| place(known) < place(first));
        self.known.splice(at..at, matches.iter().copied());
    }

    /// The place among the matches told of the current one, if it is one
    /// of them.
    fn current_index(&self) -> Option<usize> {
        let current = self.current?;
        self.known
            .binary_search_by(|known| place(known).cmp(&place(&current)))
            .ok()
    }
}

/// Where `found` starts, its row's number first, for the rows' order.
fn place(found: &SearchMatch) -> (i64, u16) {
    (found.first.1, found.first.0)
}

/// What the find bar shows: its options and its count's text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BarView {
    /// The toggles' states.
    pub options: Options,
    /// The count's text.
    pub count: String,
}

/// The texts of the find bar, in the language shown: its field's
/// placeholder, and its toggles' and its buttons' labels, which their
/// tooltips show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarTexts {
    /// The field's placeholder.
    pub placeholder: String,
    /// The case toggle's label.
    pub case: String,
    /// The regex toggle's label.
    pub regex: String,
    /// The whole words toggle's label.
    pub word: String,
    /// The next match's button's label.
    pub next: String,
    /// The previous match's button's label.
    pub previous: String,
    /// The recent searches' glass's label.
    pub recent: String,
    /// The close button's label.
    pub close: String,
}

impl Default for BarTexts {
    fn default() -> Self {
        let english = |id: &str| {
            ENGLISH
                .iter()
                .find(|(english, _)| *english == id)
                .map_or(id, |(_, text)| *text)
                .to_owned()
        };
        Self {
            placeholder: english("find-placeholder"),
            case: english("find-case"),
            regex: english("find-regex"),
            word: english("find-word"),
            next: english("find-next"),
            previous: english("find-previous"),
            recent: english("find-history"),
            close: english("find-close"),
        }
    }
}

/// `node`, a button or a toggle of an icon of `side` points, without a
/// box at rest, as macOS's and the editors' find bars have them: its hover
/// and its press fill it, faded; a toggle `on` keeps its own fill and
/// color.
fn plain(node: Node, side: f32, on: bool, label: &str) -> Node {
    node.label(label)
        .tooltip(label)
        .layout(Layout {
            width: Length::Points(side),
            min_width: Length::Points(side),
            height: Length::Points(side),
            justify: Justify::Center,
            align: Align::Center,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(if on { "toggle.on.bg" } else { "transparent" })),
            color: on.then(|| Token::from("toggle.on.fg")),
            radius: 4.0,
            // The arrow, over a field's I-beam around it.
            pointer: Some(PointerShape::Arrow),
            hover: Some(Hover {
                background: Some(Token::from("button.hover.bg")),
                ..Hover::default()
            }),
            pressed: Some(Hover {
                background: Some(Token::from("button.pressed.bg")),
                ..Hover::default()
            }),
            transition: Some(Transition {
                duration: FADE,
                ..Transition::default()
            }),
            ..Style::default()
        })
}

/// The find bar of `view`, of `texts`: a search field of macOS's look,
/// its magnifying glass, which lists the recent searches, its text, and
/// its toggles of case, whole words and regexes at its end; the count, of
/// a width of its own, so that the bar keeps its width whatever it tells;
/// and the buttons of the next and the previous match and of the close.
pub fn find_bar(view: &BarView, texts: &BarTexts) -> Node {
    let options = view.options;
    let text = Node::field(texts.placeholder.as_str())
        .key(FIELD)
        .layout(Layout {
            grow: 1.0,
            width: Length::Points(0.0),
            ..Layout::default()
        })
        // Of the search field's box, which it lies in, of no box of its own.
        .style(Style {
            background: Some(Token::from("transparent")),
            ..Style::default()
        });
    let field = Node::container(vec![
        plain(
            Node::icon_button("search").key(PAST),
            IN_FIELD,
            false,
            &texts.recent,
        ),
        text,
        plain(
            Node::toggle("case-sensitive", options.sensitive).key(CASE),
            IN_FIELD,
            options.sensitive,
            &texts.case,
        ),
        plain(
            Node::toggle("whole-word", options.whole_word).key(WORD),
            IN_FIELD,
            options.whole_word,
            &texts.word,
        ),
        plain(
            Node::toggle("regex", options.regex).key(REGEX),
            IN_FIELD,
            options.regex,
            &texts.regex,
        ),
    ])
    .layout(Layout {
        width: Length::Points(FIELD_WIDTH),
        min_width: Length::Points(FIELD_WIDTH),
        gap: 1.0,
        padding: Edges::all(2.0),
        align: Align::Center,
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("field.bg")),
        border: Some(Border {
            color: Token::from("field.border"),
            width: 1.0,
        }),
        radius: 6.0,
        // The ring of its text's focus round the whole field, and the
        // I-beam over it, but over its buttons.
        holds_ring: true,
        pointer: Some(PointerShape::Text),
        ..Style::default()
    });
    let count = Node::container(vec![Node::text(view.count.as_str()).key(COUNT).style(
        Style {
            color: Some(Token::from("text.secondary")),
            ..Style::default()
        },
    )])
    .layout(Layout {
        width: Length::Points(COUNT_WIDTH),
        min_width: Length::Points(COUNT_WIDTH),
        justify: Justify::Center,
        align: Align::Center,
        ..Layout::default()
    })
    .style(Style {
        clip: true,
        ..Style::default()
    });
    Node::container(vec![
        field,
        count,
        plain(
            Node::icon_button("arrow-up").key(UP),
            BUTTON,
            false,
            &texts.next,
        ),
        plain(
            Node::icon_button("arrow-down").key(DOWN),
            BUTTON,
            false,
            &texts.previous,
        ),
        plain(
            Node::icon_button("close").key(CLOSE),
            BUTTON,
            false,
            &texts.close,
        ),
    ])
    .layout(Layout {
        gap: 2.0,
        padding: Edges::all(5.0),
        align: Align::Center,
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("panel.bg")),
        border: Some(Border {
            color: Token::from("panel.border"),
            width: 1.0,
        }),
        radius: 8.0,
        shadow: Some(Shadow::default()),
        ..Style::default()
    })
}

/// The plugin of the search.
#[derive(Debug, Default)]
pub struct Search {
    settings: Settings,
    find: Option<Find>,
    /// The options of the last search, which the next takes.
    options: Options,
    /// The recent searches' texts, the latest first.
    recent: Vec<String>,
    /// The closings of its bar that it asked for, to open it anew, whose
    /// events it leaves.
    reopening: usize,
}

impl Plugin for Search {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if let Ok(Data::Map(state)) = cx.state() {
            if let Some(Data::List(recent)) = state.get(STATE_RECENT) {
                self.recent = recent
                    .iter()
                    .filter_map(|text| match text {
                        Data::Text(text) if !text.is_empty() => Some(text.clone()),
                        _ => None,
                    })
                    .take(MOST_RECENT)
                    .collect();
            }
            let on = |key: &str| matches!(state.get(key), Some(Data::Boolean(true)));
            self.options = Options {
                sensitive: on(STATE_CASE),
                regex: on(STATE_REGEX),
                whole_word: on(STATE_WORD),
            };
        }
        self.settings = Settings::of(&cx.config()?);
        for (name, title) in [
            (OPEN, "Find…"),
            (NEXT, "Find Next"),
            (PREVIOUS, "Find Previous"),
        ] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category: Category::Find,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn config_schema(&self) -> Option<&'static str> {
        Some(include_str!("../search.schema.json"))
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        match name {
            OPEN => self.open(cx),
            NEXT => self.step(cx, Step::Up),
            PREVIOUS => self.step(cx, Step::Down),
            _ => Ok(()),
        }
    }

    /// The bar, and its recent searches' menu.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view == bar() {
            let find = self.find.as_mut()?;
            // The count of before stays while the search tells nothing sure,
            // as at each key typed, so that it does not flicker.
            if let Some(count) = count(cx, find) {
                find.shown = count;
            }
            let view = BarView {
                options: find.options,
                count: find.shown.clone(),
            };
            let texts = texts(cx);
            return Some(find_bar(&view, &texts));
        }
        if *view == ViewId::Panel(PanelId::from(RECENT)) {
            return Some(Node::container(
                self.recent
                    .iter()
                    .enumerate()
                    .map(|(at, text)| {
                        Node::button(text.as_str()).key(format!("{PAST_ITEM}{at}").as_str())
                    })
                    .collect(),
            ));
        }
        None
    }

    /// The field's text searches as it changes, Return steps up, and
    /// Shift-Return down; the toggles search again, and their options go
    /// to the next search; the buttons step or close the bar, and its
    /// glass shows the recent searches, one of which searches again.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        if event.view == ViewId::Panel(PanelId::from(RECENT)) {
            let chosen = key
                .strip_prefix(PAST_ITEM)
                .and_then(|at| at.parse::<usize>().ok())
                .and_then(|at| self.recent.get(at).cloned());
            if let (Some(text), UiEventKind::Click) = (chosen, &event.kind)
                && let Some(find) = &mut self.find
            {
                find.text.clone_from(&text);
                cx.set_text(bar(), FIELD, &text, find.revision)?;
                return self.search(cx);
            }
            return Ok(());
        }
        if event.view != bar() {
            return Ok(());
        }
        let Some(find) = &mut self.find else {
            return Ok(());
        };
        match (&event.kind, key) {
            // Opened anew, its field takes its text as it takes the keys.
            (UiEventKind::Focus, FIELD) if find.filling => {
                find.filling = false;
                if find.text.is_empty() {
                    return Ok(());
                }
                cx.set_text(bar(), FIELD, &find.text, find.revision)
            }
            (UiEventKind::InputChanged { text, revision }, FIELD) => {
                find.text.clone_from(text);
                find.revision = *revision;
                self.search(cx)
            }
            (UiEventKind::Submit { text, shift }, FIELD) => {
                let text = text.clone();
                self.remember(cx, &text);
                self.step(cx, if *shift { Step::Down } else { Step::Up })
            }
            (UiEventKind::Toggle { on }, CASE | REGEX | WORD) => {
                match key {
                    CASE => find.options.sensitive = *on,
                    REGEX => find.options.regex = *on,
                    _ => find.options.whole_word = *on,
                }
                self.options = find.options;
                self.keep(cx);
                self.search(cx)
            }
            (UiEventKind::Click, UP) => self.step(cx, Step::Up),
            (UiEventKind::Click, DOWN) => self.step(cx, Step::Down),
            (UiEventKind::Click, PAST) if !self.recent.is_empty() => cx.open_panel(
                RECENT,
                PanelOptions {
                    anchor: Anchor::Node {
                        view: bar(),
                        key: PAST.into(),
                    },
                    role: Role::Menu,
                    close_outside: true,
                    close_escape: true,
                },
            ),
            // Its closing, which the plugin hears, ends the search.
            (UiEventKind::Click, CLOSE) => cx.close_panel(BAR),
            _ => Ok(()),
        }
    }

    /// A part of the search comes, whose first match, of the nearest part
    /// to the view, is selected if none is; the bar's closing ends the
    /// search; a change of the config takes its options.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            match event {
                Event::Searched {
                    id,
                    matches,
                    count,
                    done,
                    ..
                } => self.searched(cx, *id, matches, (*count, *done))?,
                Event::PanelClosed(id) if *id == PanelId::from(BAR) => {
                    if self.reopening > 0 {
                        self.reopening -= 1;
                    } else {
                        self.closed(cx)?;
                    }
                }
                Event::ConfigChanged => self.settings = Settings::of(&cx.config()?),
                _ => {}
            }
        }
        Ok(())
    }
}

/// The find bar's view.
fn bar() -> ViewId {
    ViewId::Panel(PanelId::from(BAR))
}

/// The bar's texts, in the language shown.
fn texts(cx: &mut Cx<'_>) -> BarTexts {
    let mut text = |id: &str| message(cx, id, &[]);
    BarTexts {
        placeholder: text("find-placeholder"),
        case: text("find-case"),
        regex: text("find-regex"),
        word: text("find-word"),
        next: text("find-next"),
        previous: text("find-previous"),
        recent: text("find-history"),
        close: text("find-close"),
    }
}

/// The count of `find`'s matches, as its bar tells it, if its search
/// tells something sure: no result of no text, or once it went to its end
/// of none; a refused query; the current match's place among them, of
/// short numbers past 9999, `…` while the search goes on. `None` while it
/// has found nothing yet, and goes on: the count of before stays.
fn count(cx: &mut Cx<'_>, find: &Find) -> Option<String> {
    if find.invalid {
        return Some(message(cx, "find-invalid", &[]));
    }
    if find.count == 0 {
        return (find.text.is_empty() || find.done).then(|| message(cx, "find-none", &[]));
    }
    let count = short(find.count);
    let told = match find.current_index() {
        Some(at) => {
            let current = short(at + 1);
            message(
                cx,
                "find-count",
                &[("current", &current), ("count", &count)],
            )
        }
        None => message(cx, "find-count-unknown", &[("count", &count)]),
    };
    Some(if find.done {
        told
    } else {
        format!("{told}…")
    })
}

impl Search {
    /// Opens the bar at the focused pane, of its last search's text, which
    /// it searches again, of its options; open already at the pane, its
    /// field takes the keys again, if it has them not; at another pane, it
    /// moves there, its search ending.
    fn open(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        if let Some(find) = &self.find {
            if find.pane == pane {
                return cx.focus_node(bar(), FIELD);
            }
            cx.close_panel(BAR)?;
            self.reopening += 1;
            self.closed(cx)?;
        }
        cx.open_panel(
            BAR,
            PanelOptions {
                anchor: Anchor::PaneCorner {
                    pane,
                    corner: Corner::TopRight,
                },
                role: Role::Dialog,
                close_outside: false,
                close_escape: true,
            },
        )?;
        // Its field, of no revision yet, takes its text as it takes the
        // keys, once the bar's view is there.
        let mut find = Find::new(pane, self.options);
        find.text = self.recent.first().cloned().unwrap_or_default();
        find.filling = true;
        self.find = Some(find);
        self.search(cx)
    }

    /// Searches the bar's text anew, of its options, the search before
    /// ending: none of no text; a regex that does not compile tells so.
    fn search(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let settings = self.settings;
        let Some(find) = &mut self.find else {
            return Ok(());
        };
        find.known.clear();
        find.count = 0;
        find.done = false;
        find.current = None;
        find.invalid = false;
        if find.text.is_empty() {
            if let Some(before) = find.search.take() {
                cx.end_search(before)?;
            }
        } else {
            match cx.search(find.pane, find.query(settings)) {
                Ok(id) => find.search = Some(id),
                Err(error) if error.code == Code::Failed => {
                    find.invalid = true;
                    if let Some(before) = find.search.take() {
                        cx.end_search(before)?;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        cx.request_render(bar())
    }

    /// Takes a part of the search `id`, if it is the bar's: its matches,
    /// the count, and whether it went to its end; the first match is the
    /// last of the first part of any, of the view's rows first, the
    /// nearest to the bottom.
    fn searched(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        matches: &[SearchMatch],
        (count, done): (usize, bool),
    ) -> Result<(), Error> {
        let Some(find) = &mut self.find else {
            return Ok(());
        };
        if find.search != Some(id) {
            return Ok(());
        }
        find.take(matches);
        find.count = count;
        find.done = done;
        if find.current.is_none()
            && let Some(last) = matches.last()
        {
            find.current = Some(*last);
            cx.select_cells(find.pane, last.first, last.last)?;
        }
        cx.request_render(bar())
    }

    /// Steps to the next match, up, or to the previous one, down, of those
    /// told, which it selects; past the last, as the config's
    /// `wrap-around` has it. A closed bar opens instead.
    fn step(&mut self, cx: &mut Cx<'_>, toward: Step) -> Result<(), Error> {
        let wrap = self.settings.wrap_around;
        let Some(find) = &mut self.find else {
            return self.open(cx);
        };
        let Some(last) = find.known.len().checked_sub(1) else {
            return Ok(());
        };
        let at = match (find.current_index(), toward) {
            (None, _) => last,
            (Some(0), Step::Up) if wrap => last,
            (Some(0), Step::Up) => 0,
            (Some(at), Step::Up) => at - 1,
            (Some(at), Step::Down) if at == last && wrap => 0,
            (Some(at), Step::Down) => (at + 1).min(last),
        };
        let found = find.known[at];
        find.current = Some(found);
        cx.select_cells(find.pane, found.first, found.last)?;
        cx.request_render(bar())
    }

    /// The bar closed: its search ends, its current match staying
    /// selected, and its text goes to the recent searches.
    fn closed(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(find) = self.find.take() else {
            return Ok(());
        };
        if let Some(search) = find.search {
            cx.end_search(search)?;
        }
        self.remember(cx, &find.text);
        // Its menu goes with it.
        let _ = cx.close_panel(RECENT);
        Ok(())
    }

    /// Puts `text` first among the recent searches, which the plugin's
    /// state keeps, but an empty one.
    fn remember(&mut self, cx: &mut Cx<'_>, text: &str) {
        if text.is_empty() {
            return;
        }
        self.recent.retain(|kept| kept != text);
        self.recent.insert(0, text.to_owned());
        self.recent.truncate(MOST_RECENT);
        self.keep(cx);
    }

    /// Keeps the recent searches and the last options in the plugin's
    /// state.
    fn keep(&self, cx: &mut Cx<'_>) {
        let state = Data::Map(BTreeMap::from([
            (
                STATE_RECENT.to_owned(),
                Data::List(self.recent.iter().cloned().map(Data::Text).collect()),
            ),
            (STATE_CASE.to_owned(), Data::Boolean(self.options.sensitive)),
            (STATE_REGEX.to_owned(), Data::Boolean(self.options.regex)),
            (
                STATE_WORD.to_owned(),
                Data::Boolean(self.options.whole_word),
            ),
        ]));
        // Without a state directory, they stay for this run.
        let _ = cx.set_state(state);
    }
}

#[cfg(test)]
mod tests;
