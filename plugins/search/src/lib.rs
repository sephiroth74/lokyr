//! Built-in plugin of the search: the find bar, a panel at the focused
//! pane's top right, of a search field of macOS's look, whose magnifying
//! glass lists the recent searches, and whose toggles of case, whole words
//! and regexes sit at its end; the count of the matches, of a width of its
//! own; the matches highlighted over the pane's cells, the current one
//! apart; and the steps through them, each of which makes its match the
//! current one and brings it into the view, over the kernel's search
//! service. The search goes from the bottom up, as the newest output is at
//! the bottom: its next match is the one above, its previous the one below.
//! Escape closes the bar, its highlights going and the current match
//! selected, ready for a copy; the bar opens again of the last search and
//! its options. Find in All Tabs, Command-Shift-F, searches every pane of
//! the session's windows of the bar's options as the user types, each match
//! a row of its line and its tab's title; Return brings its pane forward,
//! the match selected, and opens the bar there of the text, the match the
//! current one. Like every built-in plugin, it depends only on the plugin
//! SDK.

#![forbid(unsafe_code)]

mod all;

pub use all::{AllTexts, AllView, find_all};

use std::borrow::Cow;
use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind as ParamKind, Param, Value,
};
use lokyr_plugin_sdk::host::{PaneId, SearchCase, SearchMatch, SearchQuery};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Edges, Hover, Justify, Layout, Length, Node, PointerShape, Shadow, Style,
    TextSize, Token, Transition, UiEventKind,
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
const FILTER: &str = "filter";
const UP: &str = "up";
const DOWN: &str = "down";
const PAST: &str = "recent";
const CLOSE: &str = "close";
const CLEAR: &str = "clear";

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
const STATE_FILTER: &str = "filter";

/// The width of the search field, its glass, its text and its toggles, in
/// points: of 260 for its text beside three toggles, and of the filter's.
const FIELD_WIDTH: f32 = 281.0;

/// The width of the count, which its text never changes, in points: of
/// `9999 of 9999`, the longest of its numbers, which go short past it,
/// and of its longest text, Italian's `Nessun risultato`, in the small
/// text.
const COUNT_WIDTH: f32 = 96.0;

/// The most characters of a recent search that its item of the menu
/// shows, past which it ends in an ellipsis, so that a long one keeps the
/// menu narrow.
const RECENT_LABEL: usize = 40;

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

/// The texts of the bar and of Find in All Tabs, in English, where the
/// host gives none.
const ENGLISH: [(&str, &str); 16] = [
    ("find-placeholder", "Find"),
    ("find-none", "No results"),
    ("find-invalid", "Invalid regex"),
    ("find-case", "Match Case"),
    ("find-regex", "Regular Expression"),
    ("find-word", "Whole Words"),
    ("find-filter", "Show Matching Lines Only"),
    ("find-next", "Find Next"),
    ("find-previous", "Find Previous"),
    ("find-history", "Recent Searches"),
    ("find-close", "Close"),
    ("find-clear", "Clear"),
    ("find-count-unknown", "{ $count } found"),
    ("find-count", "{ $current } of { $count }"),
    ("find-all-placeholder", "Find in All Tabs"),
    ("find-all-matches", "Matches"),
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

/// `text` as an item of a menu shows it, of `most` characters at most: cut
/// before a character that starts a cluster of its own, as a mark, a
/// joiner or a selector leans on the one before it, and ended in an
/// ellipsis, when it has more.
pub fn label(text: &str, most: usize) -> Cow<'_, str> {
    if most == 0 || text.chars().count() <= most {
        return Cow::Borrowed(text);
    }
    // The room of the ellipsis.
    let kept = most - 1;
    let mut cut = None;
    let mut joined = false;
    for (byte, char) in text.char_indices().take(kept + 1) {
        if !joined && !leans(char) {
            cut = Some(byte);
        }
        joined = char == '\u{200D}';
    }
    let cut = cut
        .filter(|&cut| cut > 0)
        .or_else(|| text.char_indices().nth(kept).map(|(byte, _)| byte))
        .unwrap_or(text.len());
    Cow::Owned(format!("{}\u{2026}", text[..cut].trim_end()))
}

/// Whether `char` leans on the character before it, in the same cluster:
/// a combining mark, a joiner, a variation selector, a skin tone or a
/// tag.
fn leans(char: char) -> bool {
    matches!(
        char,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{200C}'..='\u{200D}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{1F3FB}'..='\u{1F3FF}'
            | '\u{E0020}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
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
    /// Whether every match that shows is highlighted, besides the current
    /// one.
    highlight_all: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            case: SearchCase::Smart,
            wrap_around: true,
            highlight_all: true,
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
        if let Some(Data::Boolean(all)) = config.get("highlight-all") {
            settings.highlight_all = *all;
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
    /// Whether the pane's view shows the matches' lines alone.
    pub filter: bool,
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
    /// Whether its search told a part, as its matches show from then on.
    told: bool,
    /// Its matches told so far, in the rows' order.
    known: Vec<SearchMatch>,
    /// How many it found, those past the kept ones too, and whether it
    /// went to its end.
    count: usize,
    done: bool,
    /// The current match, highlighted apart, which the bar's closing
    /// selects, if one is.
    current: Option<SearchMatch>,
    /// Its place among the matches as the host told it at the last step,
    /// until a part comes.
    place: Option<usize>,
    /// Whether the host refused the search, as of a regex that does not
    /// compile.
    invalid: bool,
    /// Whether its field, opened anew, waits for its text, which it takes
    /// as it takes the keys, once its view is there.
    filling: bool,
    /// The count's text as the bar showed it last, which a new search
    /// leaves until it tells something sure.
    shown: String,
    /// The first cell of the match that its search makes the current one,
    /// as the command that opened it asked, until a part tells it.
    wanted: Option<(u16, i64)>,
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
            told: false,
            known: Vec::new(),
            count: 0,
            done: false,
            current: None,
            place: None,
            invalid: false,
            filling: false,
            shown: String::new(),
            wanted: None,
        }
    }

    /// The query of its text and its options, of `settings`' case where
    /// its toggle leaves case aside.
    fn query(&self, settings: Settings) -> SearchQuery {
        query_of(&self.text, self.options, settings)
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
        self.place = None;
    }

    /// The place among the matches of the current one, if it is one of
    /// them: as the host told it at the last step, or among those told.
    fn current_index(&self) -> Option<usize> {
        if self.place.is_some() {
            return self.place;
        }
        let current = self.current?;
        self.known
            .binary_search_by(|known| place(known).cmp(&place(&current)))
            .ok()
    }
}

/// The query of `text` and `options`, of `settings`' case where the case
/// toggle leaves case aside: smart, or insensitive.
fn query_of(text: &str, options: Options, settings: Settings) -> SearchQuery {
    SearchQuery {
        text: text.to_owned(),
        regex: options.regex,
        case: if options.sensitive {
            SearchCase::Sensitive
        } else {
            settings.case
        },
        whole_word: options.whole_word,
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
    /// Whether its field holds text, which its clear button empties.
    pub filled: bool,
    /// Whether its pane shows the alternate screen, as a full-screen
    /// program's, of no scrollback, whose rows the filter shows none of:
    /// its toggle hidden.
    pub alternate: bool,
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
    /// The filter toggle's label.
    pub filter: String,
    /// The next match's button's label.
    pub next: String,
    /// The previous match's button's label.
    pub previous: String,
    /// The recent searches' glass's label.
    pub recent: String,
    /// The close button's label.
    pub close: String,
    /// The label of the button that empties its field.
    pub clear: String,
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
            filter: english("find-filter"),
            next: english("find-next"),
            previous: english("find-previous"),
            recent: english("find-history"),
            close: english("find-close"),
            clear: english("find-clear"),
        }
    }
}

/// The button that empties the find bar's field, `filled` or not, of
/// `label`: its cross knocked out of a disc in the secondary text's color,
/// as macOS's search fields', while the field holds text; else room as
/// wide, so that what comes before it does not move.
fn clear(filled: bool, label: &str) -> Node {
    if !filled {
        return Node::spacer().layout(Layout {
            width: Length::Points(IN_FIELD),
            min_width: Length::Points(IN_FIELD),
            grow: 0.0,
            ..Layout::default()
        });
    }
    let button = plain(
        Node::icon_button("clear-circle").key(CLEAR),
        IN_FIELD,
        false,
        label,
    );
    let style = Style {
        color: Some(Token::from("text.secondary")),
        ..button.style.clone()
    };
    button.style(style)
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

/// The find bar of `view`, of `texts`: a search field of macOS's look, a
/// box of rounded corners of its magnifying glass, which lists the recent
/// searches, its text, its toggles of case, whole words and regexes, and at
/// its end the button that empties it while it holds text, of its room
/// kept, so that the toggles do not move; the count, of a width of its own,
/// so that the bar keeps its width whatever it tells; and the buttons of
/// the next and the previous match and of the close.
pub fn find_bar(view: &BarView, texts: &BarTexts) -> Node {
    let options = view.options;
    let text = Node::field(texts.placeholder.as_str())
        .key(FIELD)
        .layout(Layout {
            grow: 1.0,
            width: Length::Points(0.0),
            ..Layout::default()
        })
        // Of the search field's box, which it lies in, of no box of its own,
        // its text small as macOS's search fields' in a toolbar.
        .style(Style {
            background: Some(Token::from("transparent")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        });
    let mut parts = vec![
        plain(
            Node::icon_button("magnifying-glass").key(PAST),
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
    ];
    // The filter, of the primary screen's rows, does nothing on the
    // alternate screen's.
    if !view.alternate {
        parts.push(plain(
            Node::toggle("filter", options.filter).key(FILTER),
            IN_FIELD,
            options.filter,
            &texts.filter,
        ));
    }
    parts.push(clear(view.filled, &texts.clear));
    let field = Node::container(parts)
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
            // Of a text field's rounded corners, rather than a search field's
            // capsule, within the bar's own rounded box.
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
            text_size: Some(TextSize::Small),
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
    /// Find in All Tabs, while it is open.
    all: Option<all::All>,
    /// The closings of Find in All Tabs that it asked for, as it went to a
    /// match, whose events it leaves, as the command may open it anew.
    all_closing: usize,
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
                filter: on(STATE_FILTER),
            };
        }
        self.settings = Settings::of(&cx.config()?);
        for (name, title) in [
            (OPEN, "Find…"),
            (NEXT, "Find Next"),
            (PREVIOUS, "Find Previous"),
        ] {
            // The bar's text, and the first cell of the match that its
            // search makes the current one, of a column of 0 or more, as
            // Find in All Tabs opens it.
            let params = if name == OPEN {
                vec![
                    Param {
                        name: "text".to_owned(),
                        kind: ParamKind::String,
                        default: Some(Value::String(String::new())),
                    },
                    Param {
                        name: "column".to_owned(),
                        kind: ParamKind::Integer,
                        default: Some(Value::Integer(-1)),
                    },
                    Param {
                        name: "row".to_owned(),
                        kind: ParamKind::Integer,
                        default: Some(Value::Integer(0)),
                    },
                ]
            } else {
                Vec::new()
            };
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category: Category::Find,
                // What finding does of a plugin that runs it: it reads the
                // terminal, draws the matches and selects the current one.
                capabilities: vec![
                    Capability::TerminalRead,
                    Capability::Decorate,
                    Capability::PaneView,
                ],
                params,
                when: None,
            })?;
        }
        cx.register_command(Command {
            name: all::FIND_ALL.to_owned(),
            title: "Find in All Tabs…".to_owned(),
            category: Category::Find,
            // It reads every pane, and brings the match's forward, its tab and
            // its window, before the find bar opens there.
            capabilities: vec![
                Capability::TerminalRead,
                Capability::Decorate,
                Capability::PaneView,
                Capability::WorkspaceWrite,
            ],
            params: Vec::new(),
            when: None,
        })?;
        cx.register_command(Command {
            name: all::MOVE.to_owned(),
            title: "Move in Find in All Tabs".to_owned(),
            category: Category::Find,
            capabilities: Vec::new(),
            params: vec![Param {
                name: "delta".to_owned(),
                kind: ParamKind::Integer,
                default: None,
            }],
            when: Some(format!("search.{}.focused", all::PANEL)),
        })?;
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("../search.schema.json").into())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        match name {
            OPEN => {
                let text = match args.get("text") {
                    Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
                    _ => None,
                };
                let wanted = match (args.get("column"), args.get("row")) {
                    (Some(Value::Integer(column)), Some(Value::Integer(row))) => {
                        u16::try_from(*column).ok().map(|column| (column, *row))
                    }
                    _ => None,
                };
                self.open(cx, text, wanted)
            }
            NEXT => self.step(cx, Step::Up),
            PREVIOUS => self.step(cx, Step::Down),
            all::FIND_ALL => self.open_all(cx),
            all::MOVE => {
                let delta = match args.get("delta") {
                    Some(Value::Integer(delta)) => *delta,
                    _ => 0,
                };
                self.step_all(cx, delta)
            }
            _ => Ok(()),
        }
    }

    /// The bar, its recent searches' menu, and Find in All Tabs.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view == all::panel() {
            return self.render_all(cx);
        }
        if *view == bar() {
            let find = self.find.as_mut()?;
            // The count of before stays while the search tells nothing sure,
            // as at each key typed, so that it does not flicker.
            if let Some(count) = count(cx, find) {
                find.shown = count;
            }
            let alternate = cx
                .viewport(find.pane)
                .is_ok_and(|viewport| viewport.alternate);
            let view = BarView {
                options: find.options,
                count: find.shown.clone(),
                filled: !find.text.is_empty(),
                alternate,
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
                        Node::button(label(text, RECENT_LABEL))
                            .key(format!("{PAST_ITEM}{at}").as_str())
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
        if event.view == all::panel() {
            return self.on_all_event(cx, event);
        }
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
            // Opened anew, its field takes its text as it takes the keys,
            // selected, so that typing replaces it.
            (UiEventKind::Focus, FIELD) if find.filling => {
                find.filling = false;
                if find.text.is_empty() {
                    return Ok(());
                }
                cx.set_text(bar(), FIELD, &find.text, find.revision)?;
                cx.focus_node(bar(), FIELD)
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
            // The pane's view of the matches' lines alone, or as it was, of
            // the search there is, which no new search needs.
            (UiEventKind::Toggle { on }, FILTER) => {
                find.options.filter = *on;
                self.options = find.options;
                if let (true, Some(search)) = (find.told, find.search) {
                    cx.filter_matches(search, *on)?;
                }
                self.keep(cx);
                cx.request_render(bar())
            }
            // Emptied by its button, as typing would empty it.
            (UiEventKind::Click, CLEAR) => {
                find.text.clear();
                cx.set_text(bar(), FIELD, "", find.revision)?;
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
                    takes_keys: false,
                    close_typing: false,
                },
            ),
            // Its closing, which the plugin hears, ends the search.
            (UiEventKind::Click, CLOSE) => cx.close_panel(BAR),
            _ => Ok(()),
        }
    }

    /// A part of the search comes, whose first match, of the nearest part
    /// to the view, is the current one if none is, or of one of Find in All
    /// Tabs' searches; the bar's closing ends the search, and Find in All
    /// Tabs' its own; a change of the config takes its options, the matches
    /// shown as they say.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            match event {
                Event::Searched {
                    id,
                    pane,
                    matches,
                    count,
                    done,
                    ..
                } => {
                    if !self.searched_all(cx, *id, *pane, matches)? {
                        self.searched(cx, *id, matches, (*count, *done))?;
                    }
                }
                Event::PanelClosed(id) if *id == PanelId::from(all::PANEL) => {
                    self.closed_all(cx)?
                }
                Event::PanelClosed(id) if *id == PanelId::from(BAR) => {
                    if self.reopening > 0 {
                        self.reopening -= 1;
                    } else {
                        self.closed(cx)?;
                    }
                }
                Event::ConfigChanged => {
                    self.settings = Settings::of(&cx.config()?);
                    // Its matches shown as it now says, its current one
                    // where it is now.
                    if let Some(find) = &self.find
                        && let (Some(search), true) = (find.search, find.told)
                    {
                        let now = cx.current_match(search)?.current.map(|(_, found)| found);
                        cx.show_matches(search, self.settings.highlight_all, now)?;
                    }
                }
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
        filter: text("find-filter"),
        next: text("find-next"),
        previous: text("find-previous"),
        recent: text("find-history"),
        close: text("find-close"),
        clear: text("find-clear"),
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
    /// field takes the keys again, its text selected; at another pane, it
    /// moves there, its search ending.
    fn open(
        &mut self,
        cx: &mut Cx<'_>,
        text: Option<String>,
        wanted: Option<(u16, i64)>,
    ) -> Result<(), Error> {
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        self.open_at(cx, pane, text, wanted)
    }

    /// Opens the bar at `pane`, as [`Search::open`] does at the focused
    /// one: of `text`, of the last options, searched anew in place of its
    /// own, where given, the match at `wanted` the current one.
    fn open_at(
        &mut self,
        cx: &mut Cx<'_>,
        pane: PaneId,
        text: Option<String>,
        wanted: Option<(u16, i64)>,
    ) -> Result<(), Error> {
        // Find in All Tabs, whose search of the pane the bar's would
        // replace, closed.
        self.close_all(cx)?;
        if let Some(find) = &mut self.find {
            if find.pane == pane {
                // Of a text given, searched anew in place of its own, of the
                // last options, as Find in All Tabs searched, its field filled
                // as it takes the keys, or at once where its view is there.
                if let Some(text) = text {
                    find.options = self.options;
                    find.text.clone_from(&text);
                    find.wanted = wanted;
                    find.filling = true;
                    find.revision += 1;
                    let _ = cx.set_text(bar(), FIELD, &text, find.revision);
                    self.search(cx)?;
                }
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
                takes_keys: false,
                close_typing: false,
            },
        )?;
        // Its field, of no revision yet, takes its text as it takes the
        // keys, once the bar's view is there.
        let mut find = Find::new(pane, self.options);
        find.text = text.unwrap_or_else(|| self.recent.first().cloned().unwrap_or_default());
        find.wanted = wanted;
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
        find.told = false;
        find.count = 0;
        find.done = false;
        find.current = None;
        find.place = None;
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
    /// nearest to the bottom. The search's matches show from its first
    /// part, those of the search before staying until then, so that the
    /// view does not lose them as the text changes.
    fn searched(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        matches: &[SearchMatch],
        (count, done): (usize, bool),
    ) -> Result<(), Error> {
        let all = self.settings.highlight_all;
        let Some(find) = &mut self.find else {
            return Ok(());
        };
        if find.search != Some(id) {
            return Ok(());
        }
        find.take(matches);
        find.count = count;
        find.done = done;
        let mut show = !find.told;
        find.told = true;
        // The match that the command that opened the bar asked for, as its
        // part comes; else the last of the first part of any.
        if let Some(wanted) = find.wanted
            && let Some(found) = matches.iter().find(|found| found.first == wanted)
        {
            find.current = Some(*found);
            find.wanted = None;
            show = true;
        } else if find.current.is_none()
            && let Some(last) = matches.last()
        {
            find.current = Some(*last);
            show = true;
        }
        if show {
            cx.show_matches(id, all, find.current)?;
            // Its lines alone from its first part, as the filter shows.
            if find.options.filter {
                cx.filter_matches(id, true)?;
            }
        }
        cx.request_render(bar())
    }

    /// Steps to the next match, up, or to the previous one, down, of those
    /// that the host keeps, from the current one where it is now, which a
    /// resize moves and the scrollback's limit takes, and highlights it as
    /// the current one, the view scrolling to it; past the last, as the
    /// config's `wrap-around` has it; from the last of all, of none. A
    /// closed bar opens instead.
    fn step(&mut self, cx: &mut Cx<'_>, toward: Step) -> Result<(), Error> {
        let (wrap, all) = (self.settings.wrap_around, self.settings.highlight_all);
        let Some(find) = &mut self.find else {
            return self.open(cx, None, None);
        };
        let Some(search) = find.search else {
            return Ok(());
        };
        let now = cx.current_match(search)?;
        let Some(last) = now.kept.checked_sub(1) else {
            return Ok(());
        };
        let at = match (now.current.map(|(at, _)| at), toward) {
            (None, _) => last,
            (Some(0), Step::Up) if wrap => last,
            (Some(0), Step::Up) => 0,
            (Some(at), Step::Up) => at - 1,
            (Some(at), Step::Down) if at == last && wrap => 0,
            (Some(at), Step::Down) => (at + 1).min(last),
        };
        let Some(&found) = cx.search_matches(search, at, 1)?.matches.first() else {
            return Ok(());
        };
        find.current = Some(found);
        find.place = Some(at);
        cx.show_matches(search, all, Some(found))?;
        cx.request_render(bar())
    }

    /// The bar closed: its search ends, its highlights going, its current
    /// match selected where it is now, which a copy takes, but of one that
    /// left the scrollback, and its text goes to the recent searches.
    fn closed(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(find) = self.find.take() else {
            return Ok(());
        };
        // The view as it was, at the current match, before the search ends.
        if let (true, true, Some(search)) = (find.options.filter, find.told, find.search) {
            cx.filter_matches(search, false)?;
        }
        if let (Some(_), Some(search)) = (find.current, find.search)
            && let Some((_, now)) = cx.current_match(search)?.current
        {
            cx.select_cells(find.pane, now.first, now.last)?;
        }
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
            (STATE_FILTER.to_owned(), Data::Boolean(self.options.filter)),
        ]));
        // Without a state directory, they stay for this run.
        let _ = cx.set_state(state);
    }
}

#[cfg(test)]
mod tests;
