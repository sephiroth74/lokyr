//! Built-in plugin of the search: the find bar, a panel at the focused
//! pane's top right, of its field and its history, the toggles of case,
//! regexes and whole words, the count of the matches, and the steps
//! through them, each of which selects its match, ready for a copy, and
//! brings it into the view, over the kernel's search service. The search
//! goes from the bottom up, as the newest output is at the bottom: its
//! next match is the one above, its previous the one below. Escape closes
//! the bar, the current match staying selected. Like every built-in
//! plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error};
use lokyr_plugin_sdk::host::{PaneId, SearchCase, SearchMatch, SearchQuery};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Edges, Layout, Length, Node, Shadow, Style, Token, UiEventKind,
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

/// The key of the recent searches in the plugin's state.
const STATE_RECENT: &str = "recent";

/// The width of the bar's field, in points.
const FIELD_WIDTH: f32 = 180.0;

/// The plugin's catalogs, of its commands' titles and its bar's texts.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/search.ftl")),
    ("it", include_str!("../locales/it/search.ftl")),
];

/// The texts of the bar, in English, where the host gives none.
const ENGLISH: [(&str, &str); 13] = [
    ("find-placeholder", "Find"),
    ("find-none", "No results"),
    ("find-searching", "Searching…"),
    ("find-invalid", "Not a valid regex"),
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
fn message(cx: &mut Cx<'_>, id: &str, args: &[(&str, usize)]) -> String {
    let data: Vec<(&str, Data)> = args
        .iter()
        .map(|&(name, value)| {
            (
                name,
                Data::Integer(i64::try_from(value).unwrap_or(i64::MAX)),
            )
        })
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
        text.replace(&format!("{{ ${name} }}"), &value.to_string())
    })
}

/// The config of `[plugins.search]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Settings {
    /// Whether case counts as the bar opens.
    case: SearchCase,
    /// Whether the text is a regex as the bar opens.
    regex: bool,
    /// Whether a step past the last match goes on from the first, and the
    /// other way.
    wrap_around: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            case: SearchCase::Smart,
            regex: false,
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
        settings.case = match config.get("case") {
            Some(Data::Text(case)) if case == "sensitive" => SearchCase::Sensitive,
            Some(Data::Text(case)) if case == "insensitive" => SearchCase::Insensitive,
            _ => SearchCase::Smart,
        };
        if let Some(Data::Boolean(regex)) = config.get("regex") {
            settings.regex = *regex;
        }
        if let Some(Data::Boolean(wrap)) = config.get("wrap-around") {
            settings.wrap_around = *wrap;
        }
        settings
    }
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
    /// Whether case counts, of its toggle; otherwise the config's.
    sensitive: bool,
    /// Whether the text is a regex.
    regex: bool,
    /// Whether matches take whole words alone.
    whole_word: bool,
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
}

impl Find {
    /// The bar of `pane`, of no text yet, of `settings`' options.
    fn new(pane: PaneId, settings: Settings) -> Self {
        Self {
            pane,
            text: String::new(),
            revision: 0,
            sensitive: settings.case == SearchCase::Sensitive,
            regex: settings.regex,
            whole_word: false,
            search: None,
            known: Vec::new(),
            count: 0,
            done: false,
            current: None,
            invalid: false,
            filling: false,
        }
    }

    /// The query of its text and its options, of `settings`' case where
    /// its toggle leaves case aside: smart, or insensitive.
    fn query(&self, settings: Settings) -> SearchQuery {
        let case = match (self.sensitive, settings.case) {
            (true, _) => SearchCase::Sensitive,
            (false, SearchCase::Smart) => SearchCase::Smart,
            (false, _) => SearchCase::Insensitive,
        };
        SearchQuery {
            text: self.text.clone(),
            regex: self.regex,
            case,
            whole_word: self.whole_word,
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

/// The plugin of the search.
#[derive(Debug, Default)]
pub struct Search {
    settings: Settings,
    find: Option<Find>,
    /// The recent searches' texts, the latest first.
    recent: Vec<String>,
    /// The closings of its bar that it asked for, to open it anew, whose
    /// events it leaves.
    reopening: usize,
}

impl Plugin for Search {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if let Ok(Data::Map(state)) = cx.state()
            && let Some(Data::List(recent)) = state.get(STATE_RECENT)
        {
            self.recent = recent
                .iter()
                .filter_map(|text| match text {
                    Data::Text(text) if !text.is_empty() => Some(text.clone()),
                    _ => None,
                })
                .take(MOST_RECENT)
                .collect();
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
            return self.find.as_ref().map(|find| self.bar_tree(cx, find));
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
    /// Shift-Return down; the toggles search again; the buttons step, show
    /// the recent searches or close the bar; a recent search chosen
    /// searches again.
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
                    CASE => find.sensitive = *on,
                    REGEX => find.regex = *on,
                    _ => find.whole_word = *on,
                }
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

impl Search {
    /// Opens the bar at the focused pane, of its last search's text, which
    /// it searches again; open already, its field takes the keys again, as
    /// it opens anew, at the focused pane, its search ending if it moves to
    /// another.
    fn open(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        if let Some(find) = &self.find {
            cx.close_panel(BAR)?;
            self.reopening += 1;
            if find.pane != pane {
                self.closed(cx)?;
            }
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
        match &mut self.find {
            Some(find) => {
                find.revision = 0;
                find.filling = true;
                Ok(())
            }
            None => {
                let mut find = Find::new(pane, self.settings);
                find.text = self.recent.first().cloned().unwrap_or_default();
                find.filling = true;
                self.find = Some(find);
                self.search(cx)
            }
        }
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
        let state = Data::Map(BTreeMap::from([(
            STATE_RECENT.to_owned(),
            Data::List(self.recent.iter().cloned().map(Data::Text).collect()),
        )]));
        // Without a state directory, they stay for this run.
        let _ = cx.set_state(state);
    }

    /// The bar's tree: its field, the count, its toggles and its buttons,
    /// of its texts in the language shown.
    fn bar_tree(&self, cx: &mut Cx<'_>, find: &Find) -> Node {
        let count = if find.text.is_empty() {
            String::new()
        } else if find.invalid {
            message(cx, "find-invalid", &[])
        } else if find.count == 0 {
            if find.done {
                message(cx, "find-none", &[])
            } else {
                message(cx, "find-searching", &[])
            }
        } else {
            let told = match find.current_index() {
                Some(at) => message(
                    cx,
                    "find-count",
                    &[("current", at + 1), ("count", find.count)],
                ),
                None => message(cx, "find-count-unknown", &[("count", find.count)]),
            };
            if find.done {
                told
            } else {
                format!("{told}…")
            }
        };
        let mut text = |id: &str| message(cx, id, &[]);
        let labelled = |node: Node, label: String| node.label(label.as_str()).tooltip(label);
        let field = Node::field(text("find-placeholder"))
            .key(FIELD)
            .layout(Layout {
                width: Length::Points(FIELD_WIDTH),
                min_width: Length::Points(FIELD_WIDTH),
                ..Layout::default()
            });
        let count = Node::text(count).key(COUNT).style(Style {
            color: Some(Token::from("text.secondary")),
            ..Style::default()
        });
        Node::container(vec![
            field,
            count,
            labelled(
                Node::toggle("case-sensitive", find.sensitive).key(CASE),
                text("find-case"),
            ),
            labelled(
                Node::toggle("regex", find.regex).key(REGEX),
                text("find-regex"),
            ),
            labelled(
                Node::toggle("whole-word", find.whole_word).key(WORD),
                text("find-word"),
            ),
            labelled(Node::icon_button("arrow-up").key(UP), text("find-next")),
            labelled(
                Node::icon_button("arrow-down").key(DOWN),
                text("find-previous"),
            ),
            labelled(Node::icon_button("history").key(PAST), text("find-history")),
            labelled(Node::icon_button("close").key(CLOSE), text("find-close")),
        ])
        .layout(Layout {
            gap: 4.0,
            padding: Edges::all(6.0),
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
}

#[cfg(test)]
mod tests;
