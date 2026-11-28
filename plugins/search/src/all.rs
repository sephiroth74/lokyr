//! Find in All Tabs: a panel at the top of the focused window, of a large
//! search field, the find bar's toggles of case, whole words and regexes at
//! its end, and, below a thin line, the list of the matches of every pane
//! of the windows of the session that shows, each a row of its line and of
//! its tab's title, in the windows', the tabs' and the panes' order, then
//! the rows'; [`MOST_FOUND`] at most. It searches as the user types, of the
//! find bar's options, which its toggles share, and of `[plugins.search]
//! case`. The arrows move through the list while the field takes the keys;
//! Return, or a click, brings the match's pane forward, its tab and its
//! window, the match selected, and opens the find bar there of the text,
//! the match its current one; Escape, or a click outside it, closes it.
//! As the host keeps one search of a plugin a pane, which a new one of the
//! pane replaces, it and the find bar do not show together: it closes the
//! bar as it opens, as Escape would, and the bar's opening closes it.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Code, Error, Value};
use lokyr_plugin_sdk::host::{MOST_SEARCHES, PaneId, SearchMatch, TabId, WindowId};
use lokyr_plugin_sdk::plugin::Cx;
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, Layout, Length, ListItem, Node, Shadow, Style, TextSize,
    Token, UiEventKind,
};

use crate::{CASE, IN_FIELD, Options, REGEX, Search, WORD, message, plain, query_of};

/// Opens Find in All Tabs in the focused window, its field taking the
/// keys; open, its field takes them again.
pub(crate) const FIND_ALL: &str = "search.find-all";

/// Moves the list's selection by `delta` items, as the keymap's arrows do
/// while its field has the keys.
pub(crate) const MOVE: &str = "search.move";

/// The panel, whose id names its keymap's contexts, as
/// `search.find-all.focused`, and the keys of its nodes.
pub(crate) const PANEL: &str = "find-all";
pub(crate) const FIELD: &str = "query";
pub(crate) const LIST: &str = "matches";
const NONE: &str = "none";

/// The app's command that brings a window forward by its number.
pub(crate) const GOTO_WINDOW: &str = "window.goto";

/// The most matches that the list holds, past which the searches' others
/// are left out.
pub(crate) const MOST_FOUND: usize = 1000;

/// The most characters of a line that its row shows, past which it ends
/// in an ellipsis.
const MOST_LINE: usize = 200;

/// The rows of a line's logical line that its read takes before its row
/// and after it, as a long line wraps.
const REACH: usize = 4;

/// The panel's width, a row's height, and the most rows that its list
/// shows at once, past which it scrolls, in points.
const WIDTH: f32 = 480.0;
const ROW: f32 = 30.0;
const MOST_ROWS: usize = 10;

/// A pane that the search goes through: its window, its tab and its tab's
/// title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) window: WindowId,
    pub(crate) tab: TabId,
    pub(crate) pane: PaneId,
    pub(crate) title: String,
}

/// A match: the place of its pane among the places, its cells, and its
/// line's text, once read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) place: usize,
    pub(crate) found: SearchMatch,
    pub(crate) line: Option<String>,
}

impl Found {
    /// Where it goes in the list: its pane's place, then its first cell's
    /// row and column.
    fn order(&self) -> (usize, i64, u16) {
        (self.place, self.found.first.1, self.found.first.0)
    }
}

/// Find in All Tabs, while it is open: the panes that it searches, its
/// searches, each of its pane's place, the places whose searches wait for
/// a call of their own, as a call starts a few at most, the matches that
/// they found so far, the field's text, the selected match's place in the
/// list, whether the host refused the text, as of a regex that does not
/// compile, and whether a search of the text started, past which a refusal
/// is the host's bound of a call's searches.
#[derive(Debug, Default)]
pub(crate) struct All {
    pub(crate) places: Vec<Place>,
    pub(crate) searches: BTreeMap<u64, usize>,
    pub(crate) pending: Vec<usize>,
    pub(crate) found: Vec<Found>,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) invalid: bool,
    pub(crate) started: bool,
}

/// What Find in All Tabs shows: its toggles' states, the rows of its
/// matches, each of its line and its tab's title, and whether its text is
/// a regex that does not compile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AllView {
    /// The toggles' states.
    pub options: Options,
    /// The rows, each of its line and its tab's title.
    pub rows: Vec<(String, String)>,
    /// Whether the host refused the text.
    pub invalid: bool,
}

/// The texts of Find in All Tabs, in the language shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AllTexts {
    /// The field's placeholder.
    pub placeholder: String,
    /// The line of no match.
    pub none: String,
    /// The line of a regex that does not compile.
    pub invalid: String,
    /// What an accessibility client calls its list.
    pub matches: String,
    /// The case toggle's label.
    pub case: String,
    /// The regex toggle's label.
    pub regex: String,
    /// The whole words toggle's label.
    pub word: String,
}

/// The panel's view.
pub(crate) fn panel() -> ViewId {
    ViewId::Panel(PanelId::from(PANEL))
}

/// The panes of the windows of the session that shows, in the windows',
/// the tabs' and the panes' order, each of its tab's title.
fn places(cx: &mut Cx<'_>) -> Result<Vec<Place>, Error> {
    let shown: Option<Vec<WindowId>> = cx
        .sessions()?
        .into_iter()
        .find(|session| session.active)
        .map(|session| session.windows);
    let mut places = Vec::new();
    for window in cx.windows()? {
        if shown
            .as_ref()
            .is_some_and(|shown| !shown.contains(&window.id))
        {
            continue;
        }
        for tab in cx.tabs(window.id)? {
            for &pane in &tab.panes {
                places.push(Place {
                    window: window.id,
                    tab: tab.id,
                    pane,
                    title: tab.title.clone(),
                });
            }
        }
    }
    Ok(places)
}

/// Takes `matches`, of the pane at `place`, into `found`, each at its
/// place in the list's order, [`MOST_FOUND`] at most, of no line yet: a
/// match past the last of a full list is left out, and one before it puts
/// the last out, so that a later pane's matches, which may come first, do
/// not crowd out an earlier one's. Whether any came in.
fn admit(found: &mut Vec<Found>, place: usize, matches: &[SearchMatch]) -> bool {
    let mut came = false;
    for &each in matches {
        let new = Found {
            place,
            found: each,
            line: None,
        };
        if found.len() >= MOST_FOUND && found.last().is_some_and(|last| last.order() <= new.order())
        {
            continue;
        }
        let at = found.partition_point(|kept| kept.order() < new.order());
        found.insert(at, new);
        found.truncate(MOST_FOUND);
        came = true;
    }
    came
}

/// Reads the lines of the matches of `found` of the pane at `place` that
/// have none yet, each row's once; false of a read that fails, as of a pane
/// that another plugin closed meanwhile.
fn read_lines(cx: &mut Cx<'_>, found: &mut [Found], place: (usize, PaneId)) -> bool {
    let mut lines: BTreeMap<i64, String> = BTreeMap::new();
    for each in found
        .iter_mut()
        .filter(|each| each.place == place.0 && each.line.is_none())
    {
        let row = each.found.first.1;
        if let Some(line) = lines.get(&row) {
            each.line = Some(line.clone());
            continue;
        }
        let Ok(read) = cx.line_text(place.1, row, REACH) else {
            return false;
        };
        let line = read
            .map(|line| shortened(line.text.trim()))
            .unwrap_or_default();
        lines.insert(row, line.clone());
        each.line = Some(line);
    }
    true
}

/// `text`, of [`MOST_LINE`] characters at most, past which it ends in an
/// ellipsis.
fn shortened(text: &str) -> String {
    let mut chars = text.chars();
    let mut short: String = chars.by_ref().take(MOST_LINE).collect();
    if chars.next().is_some() {
        short.push('…');
    }
    short
}

/// Ends the searches `searches`.
fn end_searches(cx: &mut Cx<'_>, searches: &BTreeMap<u64, usize>) -> Result<(), Error> {
    for &id in searches.keys() {
        cx.end_search(id)?;
    }
    Ok(())
}

/// Brings the pane `pane` of the tab `tab` of the window `window` forward:
/// its tab, the pane, and its window, unless it has the focus.
fn bring(cx: &mut Cx<'_>, window: WindowId, tab: TabId, pane: PaneId) -> Result<(), Error> {
    cx.activate_tab(tab)?;
    cx.focus_pane(pane)?;
    if cx.focused_window() == Some(window) {
        return Ok(());
    }
    let number = cx
        .windows()?
        .into_iter()
        .find(|each| each.id == window)
        .and_then(|window| window.number);
    if let Some(number) = number {
        let mut args = Args::new();
        args.insert("index".to_owned(), Value::Integer(i64::from(number)));
        cx.run_command(GOTO_WINDOW, args)?;
    }
    Ok(())
}

/// The texts of Find in All Tabs, in the language shown.
fn texts(cx: &mut Cx<'_>) -> AllTexts {
    let mut text = |id: &str| message(cx, id, &[]);
    AllTexts {
        placeholder: text("find-all-placeholder"),
        none: text("find-none"),
        invalid: text("find-invalid"),
        matches: text("find-all-matches"),
        case: text("find-case"),
        regex: text("find-regex"),
        word: text("find-word"),
    }
}

impl Search {
    /// Opens Find in All Tabs in the focused window, of no text yet, of
    /// the panes of the session's windows; open, its field takes the keys
    /// again.
    pub(crate) fn open_all(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.all.is_some() {
            return cx.focus_node(panel(), FIELD);
        }
        let Some(window) = cx.focused_window() else {
            return Ok(());
        };
        let places = places(cx)?;
        // The find bar, whose search of its pane its own would replace,
        // closed as Escape closes it.
        if self.find.is_some() {
            cx.close_panel(crate::BAR)?;
            self.reopening += 1;
            self.closed(cx)?;
        }
        cx.open_panel(
            PANEL,
            PanelOptions {
                anchor: Anchor::WindowCorner {
                    window,
                    corner: Corner::Top,
                },
                role: Role::Dialog,
                close_outside: true,
                close_escape: true,
                takes_keys: false,
                close_typing: false,
            },
        )?;
        self.all = Some(All {
            places,
            ..All::default()
        });
        Ok(())
    }

    /// Its view, while it is open.
    pub(crate) fn render_all(&self, cx: &mut Cx<'_>) -> Option<Node> {
        let all = self.all.as_ref()?;
        let view = AllView {
            options: self.options,
            rows: all
                .found
                .iter()
                .map(|found| {
                    let title = all
                        .places
                        .get(found.place)
                        .map(|place| place.title.clone())
                        .unwrap_or_default();
                    (found.line.clone().unwrap_or_default(), title)
                })
                .collect(),
            invalid: all.invalid,
        };
        Some(find_all(&view, &texts(cx)))
    }

    /// The field's text searches every pane as it changes, and Return goes
    /// to the selected match; the toggles search again, and their options
    /// go to the find bar's next search; a click on the list goes to its
    /// match, and the keys move its selection alone while the list has the
    /// keyboard.
    pub(crate) fn on_all_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let Some(all) = &mut self.all else {
            return Ok(());
        };
        let key = event.key.0.as_str();
        match (&event.kind, key) {
            (UiEventKind::InputChanged { text, .. }, FIELD) => {
                all.query.clone_from(text);
                self.search_all(cx)?;
                cx.request_render(panel())
            }
            (UiEventKind::Submit { .. }, FIELD) => self.go(cx),
            (UiEventKind::Toggle { on }, CASE | REGEX | WORD) => {
                match key {
                    CASE => self.options.sensitive = *on,
                    REGEX => self.options.regex = *on,
                    _ => self.options.whole_word = *on,
                }
                self.keep(cx);
                self.search_all(cx)?;
                cx.request_render(panel())
            }
            (UiEventKind::Select { item }, LIST) => {
                all.selected = (*item).min(all.found.len().saturating_sub(1));
                Ok(())
            }
            (UiEventKind::Activate { item }, LIST) => {
                all.selected = (*item).min(all.found.len().saturating_sub(1));
                self.go(cx)
            }
            _ => Ok(()),
        }
    }

    /// Takes a part of the search `id`, if it is one of Find in All Tabs':
    /// its matches in the list, of their lines; the search ends at its last
    /// part, or as its pane's lines no longer read, its matches going; and,
    /// once the list is full, those of the places past its last match's,
    /// which can bring none before it, end, and the places that wait go.
    /// Whether it was one of them; the searches that wait start once the
    /// delivery's parts are taken.
    pub(crate) fn searched_all(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        pane: PaneId,
        (matches, done): (&[SearchMatch], bool),
    ) -> Result<bool, Error> {
        let Some(all) = &mut self.all else {
            return Ok(false);
        };
        let Some(&place) = all.searches.get(&id) else {
            return Ok(false);
        };
        let mut ended = done;
        if admit(&mut all.found, place, matches) && !read_lines(cx, &mut all.found, (place, pane)) {
            all.found
                .retain(|each| each.place != place || each.line.is_some());
            ended = true;
        }
        if ended {
            all.searches.remove(&id);
            cx.end_search(id)?;
        }
        if all.found.len() >= MOST_FOUND
            && let Some(last) = all.found.last().map(|found| found.place)
        {
            all.pending.retain(|&at| at <= last);
            let past: Vec<u64> = all
                .searches
                .iter()
                .filter(|&(_, &at)| at > last)
                .map(|(&id, _)| id)
                .collect();
            for id in past {
                all.searches.remove(&id);
                cx.end_search(id)?;
            }
        }
        all.selected = all.selected.min(all.found.len().saturating_sub(1));
        Ok(true)
    }

    /// The parts of a delivery taken: the searches that wait start, as many
    /// as a call starts, and the list shows them.
    pub(crate) fn searched_all_end(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.all.is_none() {
            return Ok(());
        }
        self.start_pending(cx)?;
        cx.request_render(panel())
    }

    /// Closes it, of no choice, its searches ending, as the find bar opens.
    pub(crate) fn close_all(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(all) = self.all.take() else {
            return Ok(());
        };
        cx.close_panel(PANEL)?;
        self.all_closing += 1;
        end_searches(cx, &all.searches)
    }

    /// Its panel closed: its searches end, unless it closed as it went to
    /// a match or as the find bar opened.
    pub(crate) fn closed_all(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.all_closing > 0 {
            self.all_closing -= 1;
            return Ok(());
        }
        if let Some(all) = self.all.take() {
            end_searches(cx, &all.searches)?;
        }
        Ok(())
    }

    /// Moves the selection by `delta` places, within the list.
    pub(crate) fn step_all(&mut self, cx: &mut Cx<'_>, delta: i64) -> Result<(), Error> {
        let Some(all) = &mut self.all else {
            return Ok(());
        };
        let Some(last) = all.found.len().checked_sub(1) else {
            return Ok(());
        };
        let at = i64::try_from(all.selected)
            .unwrap_or(0)
            .saturating_add(delta);
        all.selected = usize::try_from(at.max(0)).unwrap_or(0).min(last);
        cx.select(panel(), LIST, all.selected)
    }

    /// Searches every pane of its text, the searches before ending, their
    /// matches going: none of no text.
    fn search_all(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(all) = &mut self.all else {
            return Ok(());
        };
        end_searches(cx, &all.searches)?;
        all.searches.clear();
        all.found.clear();
        all.selected = 0;
        all.pending.clear();
        all.invalid = false;
        all.started = false;
        if all.query.is_empty() {
            return Ok(());
        }
        // The last place first out, as they wait.
        all.pending = (0..all.places.len()).rev().collect();
        self.start_pending(cx)
    }

    /// Starts the searches that wait, of its text and the find bar's
    /// options, as many as a call starts at most, the others waiting for
    /// the next call, which a search's part brings.
    fn start_pending(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let (options, settings) = (self.options, self.settings);
        let Some(all) = &mut self.all else {
            return Ok(());
        };
        let query = query_of(&all.query, options, settings);
        for _ in 0..MOST_SEARCHES {
            let Some(at) = all.pending.pop() else {
                break;
            };
            let Some(place) = all.places.get(at) else {
                continue;
            };
            match cx.search(place.pane, query.clone()) {
                Ok(id) => {
                    all.searches.insert(id, at);
                    all.started = true;
                }
                // Past a search of the text that started, the host's bound
                // of a call's searches: the place waits for the next call.
                Err(error) if error.code == Code::Failed && all.started => {
                    all.pending.push(at);
                    break;
                }
                // A regex that does not compile, which no pane takes.
                Err(error) if error.code == Code::Failed => {
                    all.invalid = true;
                    all.pending.clear();
                    break;
                }
                // A pane that went meanwhile is searched no more.
                Err(_) => {}
            }
        }
        Ok(())
    }

    /// Goes to the selected match: its panel closes, its searches end, the
    /// match's pane comes forward, its tab and its window, the match
    /// selected, and the find bar opens there of the text, the match its
    /// current one; of none, it stays.
    fn go(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(all) = &self.all else {
            return Ok(());
        };
        let Some(found) = all.found.get(all.selected).cloned() else {
            return Ok(());
        };
        let Some(place) = all.places.get(found.place).cloned() else {
            return Ok(());
        };
        let query = all.query.clone();
        self.close_all(cx)?;
        bring(cx, place.window, place.tab, place.pane)?;
        cx.select_cells(place.pane, found.found.first, found.found.last)?;
        self.open_at(cx, place.pane, Some(query), Some(found.found.first))
    }
}

/// The view of Find in All Tabs of `view`, of `texts`: its field, a search
/// field, large, of no box and no ring, the find bar's toggles at its end,
/// a thin line, and the list of the matches, each of its line and its
/// tab's title at its end, of a menu's look, or a line of none.
pub fn find_all(view: &AllView, texts: &AllTexts) -> Node {
    let options = view.options;
    let field = Node::container(vec![
        Node::field(texts.placeholder.as_str())
            .key(FIELD)
            .search()
            .layout(Layout {
                grow: 1.0,
                width: Length::Points(0.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("transparent")),
                text_size: Some(TextSize::Large),
                hides_ring: true,
                ..Style::default()
            }),
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
        gap: 1.0,
        align: Align::Center,
        padding: Edges {
            top: 10.0,
            right: 14.0,
            bottom: 10.0,
            left: 14.0,
        },
        ..Layout::default()
    });
    let secondary = |text: &str| {
        Node::text(text).style(Style {
            color: Some(Token::from("text.secondary")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
    };
    // A row: its line, and its tab's title at its end.
    let row = Node::container(vec![Node::text(""), Node::spacer(), secondary("")]).layout(Layout {
        gap: 8.0,
        align: Align::Center,
        padding: Edges {
            top: 4.0,
            right: 16.0,
            bottom: 4.0,
            left: 16.0,
        },
        ..Layout::default()
    });
    let items = view
        .rows
        .iter()
        .map(|(line, title)| ListItem {
            texts: vec![line.clone(), title.clone()],
            ..ListItem::default()
        })
        .collect();
    let shown = view.rows.len().min(MOST_ROWS);
    let mut list = Node::list(row, view.rows.len(), (0, items))
        .key(LIST)
        .label(texts.matches.as_str())
        .layout(Layout {
            width: Length::Fraction(1.0),
            height: Length::Points(shown as f32 * ROW),
            ..Layout::default()
        });
    if let lokyr_plugin_sdk::view::Kind::List(list) = &mut list.kind {
        list.row_height = Some(ROW);
        list.menu = true;
    }
    let below = if view.rows.is_empty() {
        let none = if view.invalid {
            &texts.invalid
        } else {
            &texts.none
        };
        Node::container(vec![list, secondary(none).key(NONE)]).layout(Layout {
            direction: Direction::Column,
            padding: Edges {
                top: 10.0,
                right: 20.0,
                bottom: 12.0,
                left: 20.0,
            },
            ..Layout::default()
        })
    } else {
        Node::container(vec![list]).layout(Layout {
            direction: Direction::Column,
            padding: Edges {
                top: 6.0,
                right: 0.0,
                bottom: 6.0,
                left: 0.0,
            },
            ..Layout::default()
        })
    };
    Node::container(vec![field, Node::separator(), below])
        .layout(Layout {
            direction: Direction::Column,
            width: Length::Points(WIDTH),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 10.0,
            shadow: Some(Shadow::default()),
            clip: true,
            ..Style::default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_line_ends_in_an_ellipsis() {
        assert_eq!(shortened("short"), "short");
        let long = "x".repeat(MOST_LINE + 5);
        let short = shortened(&long);
        assert_eq!(short.chars().count(), MOST_LINE + 1);
        assert!(short.ends_with('…'));
    }

    #[test]
    fn a_later_pane_s_matches_do_not_crowd_out_an_earlier_one_s() {
        let at = |row: i64| SearchMatch {
            first: (0, row),
            last: (5, row),
        };
        let mut found = Vec::new();
        let most = i64::try_from(MOST_FOUND).unwrap();
        let later: Vec<SearchMatch> = (0..most).map(at).collect();
        assert!(admit(&mut found, 1, &later));
        assert_eq!(found.len(), MOST_FOUND);
        // The earlier pane's, which came after them, go before them, the
        // later's last ones out, in the rows' order.
        assert!(admit(&mut found, 0, &[at(7), at(3)]));
        assert_eq!(found.len(), MOST_FOUND);
        let place = |found: &Found| (found.place, found.found.first.1);
        assert_eq!(place(&found[0]), (0, 3));
        assert_eq!(place(&found[1]), (0, 7));
        assert_eq!(place(&found[2]), (1, 0));
        assert_eq!(place(found.last().unwrap()), (1, most - 3));
        // Past the last of a full list, left out.
        assert!(!admit(&mut found, 2, &[at(0)]));
        assert!(!admit(&mut found, 1, &[at(most)]));
        assert!(found.iter().all(|found| found.line.is_none()));
    }
}
