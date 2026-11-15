//! The themes page's Theme Editor: every token of the theme in force,
//! or of one that it previews, chosen of its menu of the themes or of a
//! card's Edit, by groups, each of its color
//! over a backdrop, its name, its value and where it comes from, filtered as
//! its field says, the tokens that a saved file changed lit for a while; a
//! token of a file opens at its line, any other copies its line to paste.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::host::{
    NoticeLevel, ThemeFormat, ThemeInfo, ThemeToken, ThemeTokens, TokenOrigin,
};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::Cx;
use lokyr_plugin_sdk::theme::ThemeVariant;
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontWeight, Kind, Layout, Length, Node, Role, Style, TextSize,
    Token, UiEventKind,
};

use super::{code, message, search_box, secondary, themes_page, words};

mod parts;

/// The filter's field, and its button that empties it.
const FILTER: &str = "editor-filter";
const FILTER_CLEAR: &str = "editor-filter-clear";

/// The button of the theme shown, and its menu of the themes.
const THEME: &str = "editor-theme";
const THEMES_MENU: &str = "editor-themes";

/// The menu's item of each theme, by its name.
const THEME_ITEM: &str = "editor-theme:";

/// The button beside the menu of a theme that cannot be edited as it is:
/// a held or built-in one, which it duplicates, or an `.itermcolors` one,
/// which it converts.
const LOCK: &str = "editor-lock";

/// A token's button, which opens its file or copies its line, by its name.
const ACT: &str = "editor-act:";

/// A token's row, by its name.
const ROW: &str = "editor-token:";

/// How long the tokens that a change of the theme changed stay lit.
const LIT: Duration = Duration::from_secs(3);

/// The width of the tokens' column.
const COLUMN: f32 = 300.0;

/// The state of the Editor.
#[derive(Debug, Default)]
pub(crate) struct Editor {
    /// Whether it shows, in the themes page.
    shown: bool,
    /// The theme that it previews, of the menu, in place of the one in
    /// force.
    previewed: Option<String>,
    /// The words of its filter.
    filter: String,
    /// The tokens as it read them last.
    tokens: Option<ThemeTokens>,
    /// The tokens that the last change of the theme changed, lit.
    lit: BTreeSet<String>,
    /// The timer that ends their light.
    lit_timer: Option<u64>,
    /// The part of the interface or the token chosen of a click, which
    /// marks the tokens of the part, or the parts of the token.
    picked: Option<Pick>,
    /// The question of a copy of a read-only theme, whose answer, the
    /// copy's name, it shows.
    copying: Option<u64>,
    /// The question of an `.itermcolors` theme's conversion.
    converting: Option<u64>,
    /// The `.itermcolors` theme whose conversion it asked last as it
    /// showed it, asked again of the lock alone until another shows.
    offered: Option<String>,
}

/// Why the theme shown cannot be edited as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Locked {
    /// It is held or built in: a copy edits it.
    ReadOnly,
    /// It is an `.itermcolors` file's: converted, it edits.
    Itermcolors,
}

/// What a click chose: a part of the interface's specimen, or a token of
/// the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pick {
    Part(String),
    Token(String),
}

impl Editor {
    /// Shows it, in the themes page, of no filter: of the theme `theme`, or
    /// of the one in force of none; an `.itermcolors` one's conversion
    /// asked.
    pub(crate) fn show(&mut self, cx: &mut Cx<'_>, theme: Option<&str>) -> Result<(), Error> {
        self.shown = true;
        self.filter.clear();
        self.lit.clear();
        self.picked = None;
        self.offered = None;
        self.tokens = cx.theme_tokens(None)?;
        if let Some(name) = theme {
            self.show_theme(cx, name)?;
        }
        self.offer_conversion(cx)
    }

    /// Hides it, as the page shows an appearance's themes or hides: a
    /// theme that it previews goes, the one in force back.
    pub(crate) fn hide(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.shown = false;
        if self.previewed.take().is_some() {
            cx.preview_theme(None)?;
        }
        Ok(())
    }

    /// Takes `event`; returns whether it took it.
    pub(crate) fn on_event(&mut self, cx: &mut Cx<'_>, event: &Event) -> Result<bool, Error> {
        match event {
            // The theme resolved anew: its tokens read again, those that
            // changed lit.
            Event::ThemeChanged if self.shown => {
                let tokens = cx.theme_tokens(self.previewed.as_deref())?;
                let changed = changed(self.tokens.as_ref(), tokens.as_ref());
                self.tokens = tokens;
                if !changed.is_empty() {
                    self.lit = changed;
                    if let Some(timer) = self.lit_timer.take() {
                        cx.cancel_timer(timer)?;
                    }
                    self.lit_timer = Some(cx.start_timer(LIT, false)?);
                }
                cx.request_render(themes_page())?;
                Ok(true)
            }
            Event::Timer { id } if self.lit_timer == Some(*id) => {
                self.lit_timer = None;
                self.lit.clear();
                if self.shown {
                    cx.request_render(themes_page())?;
                }
                Ok(true)
            }
            Event::PanelClosed(PanelId(panel)) if panel == THEMES_MENU => Ok(true),
            // The copy of a read-only theme made, shown in its place.
            Event::Answered { id, text } if self.copying == Some(*id) => {
                self.copying = None;
                if let (Some(name), true) = (text, self.shown) {
                    self.choose(cx, name)?;
                }
                Ok(true)
            }
            // An `.itermcolors` theme converted: its tokens read anew, of
            // its new file.
            Event::Answered { id, text } if self.converting == Some(*id) => {
                self.converting = None;
                if text.is_some() && self.shown {
                    self.tokens = cx.theme_tokens(self.previewed.as_deref())?;
                    cx.request_render(themes_page())?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Takes `event` of the page or of its menu; returns whether it took it.
    pub(crate) fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<bool, Error> {
        let key = event.key.0.as_str();
        if event.view == ViewId::Panel(PanelId::from(THEMES_MENU)) {
            if !matches!(event.kind, UiEventKind::Click | UiEventKind::Toggle { .. }) {
                return Ok(true);
            }
            let _ = cx.close_panel(THEMES_MENU);
            if let Some(name) = key.strip_prefix(THEME_ITEM) {
                self.choose(cx, name)?;
            }
            return Ok(true);
        }
        if event.view != themes_page() || !self.shown {
            return Ok(false);
        }
        match (&event.kind, key) {
            (UiEventKind::InputChanged { text, .. }, FILTER) => {
                if *text != self.filter {
                    text.clone_into(&mut self.filter);
                    cx.request_render(themes_page())?;
                }
            }
            (UiEventKind::Click, FILTER_CLEAR) => {
                cx.set_text(themes_page(), FILTER, "", u64::MAX)?;
                self.filter.clear();
                cx.request_render(themes_page())?;
            }
            (UiEventKind::Click, LOCK) => self.unlock(cx)?,
            (UiEventKind::Click, THEME) => cx.open_panel(
                THEMES_MENU,
                PanelOptions {
                    anchor: Anchor::Node {
                        view: themes_page(),
                        key: THEME.into(),
                    },
                    role: PanelRole::Menu,
                    close_outside: true,
                    close_escape: true,
                    takes_keys: false,
                    close_typing: false,
                },
            )?,
            (UiEventKind::Click, _) if key.starts_with(ACT) => {
                self.act(cx, &key[ACT.len()..])?;
            }
            // A part, or a token's row: chosen, or not again of a second
            // click.
            (UiEventKind::Click, _) if key.starts_with(parts::PART) || key.starts_with(ROW) => {
                let pick = match key.strip_prefix(parts::PART) {
                    Some(id) => Pick::Part(id.to_owned()),
                    None => Pick::Token(key[ROW.len()..].to_owned()),
                };
                self.picked = (self.picked.as_ref() != Some(&pick)).then_some(pick);
                cx.request_render(themes_page())?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Shows the theme `name`, an `.itermcolors` one's conversion asked.
    pub(crate) fn choose(&mut self, cx: &mut Cx<'_>, name: &str) -> Result<(), Error> {
        self.show_theme(cx, name)?;
        self.offer_conversion(cx)
    }

    /// Shows the theme `name`: the one in force as it is, any other
    /// previewed in its place.
    fn show_theme(&mut self, cx: &mut Cx<'_>, name: &str) -> Result<(), Error> {
        let in_force = cx.themes()?.current;
        let chosen = (name != in_force).then(|| name.to_owned());
        self.preview(cx, chosen)
    }

    /// Why the theme shown cannot be edited as it is, if it cannot, of the
    /// themes' list: none of a theme that the list lacks yet.
    fn locked(&self, cx: &mut Cx<'_>) -> Option<Locked> {
        let name = self.shown_theme()?;
        let themes = cx.themes().ok()?;
        let theme = themes.themes.iter().find(|theme| theme.name == name)?;
        if theme.is_read_only() {
            Some(Locked::ReadOnly)
        } else if theme.format == ThemeFormat::Itermcolors {
            Some(Locked::Itermcolors)
        } else {
            None
        }
    }

    /// Asks the conversion of the theme shown, if it is an `.itermcolors`
    /// one that it did not ask of as it showed, and no question waits.
    fn offer_conversion(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let name = self.shown_theme().map(str::to_owned);
        if self.locked(cx) != Some(Locked::Itermcolors) {
            self.offered = None;
            return Ok(());
        }
        if self.offered == name || self.converting.is_some() {
            return Ok(());
        }
        self.offered = name;
        self.unlock(cx)
    }

    /// Makes the theme shown one that it edits: a copy of a read-only one,
    /// its name asked, or an `.itermcolors` one converted, the user
    /// agreeing; of one question at a time.
    fn unlock(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.copying.is_some() || self.converting.is_some() {
            return Ok(());
        }
        let Some(name) = self.shown_theme().map(str::to_owned) else {
            return Ok(());
        };
        match self.locked(cx) {
            Some(Locked::ReadOnly) => self.copying = Some(cx.copy_theme(&name)?),
            Some(Locked::Itermcolors) => self.converting = Some(cx.convert_theme(&name)?),
            None => {}
        }
        Ok(())
    }

    /// Previews the theme `chosen`, or the one in force again of none, and
    /// reads its tokens.
    fn preview(&mut self, cx: &mut Cx<'_>, chosen: Option<String>) -> Result<(), Error> {
        if chosen == self.previewed {
            return Ok(());
        }
        cx.preview_theme(chosen.as_deref())?;
        self.previewed = chosen;
        self.lit.clear();
        self.tokens = cx.theme_tokens(self.previewed.as_deref())?;
        cx.request_render(themes_page())
    }

    /// Opens the file of the color `name` at its line, or copies its line
    /// to paste into a theme's file: a token's `"name" = "value"`, of its
    /// rule's text or of its color, a key of `[terminal]`'s `name =
    /// "#rrggbb"`, and one of the palette's colors its color alone.
    fn act(&self, cx: &mut Cx<'_>, name: &str) -> Result<(), Error> {
        let Some(tokens) = self.tokens.as_ref() else {
            return Ok(());
        };
        let terminal = tokens.terminal.iter().position(|token| token.name == name);
        let Some(token) = terminal
            .map(|at| &tokens.terminal[at])
            .or_else(|| tokens.tokens.iter().find(|token| token.name == name))
        else {
            return Ok(());
        };
        let color = token.color.to_string();
        let line = match (&token.origin, terminal) {
            (TokenOrigin::File { path, line, .. }, _) => {
                return cx.open_at(path, (*line).max(1), None);
            }
            (_, Some(_)) if in_palette(name) => format!("\"{color}\""),
            (_, Some(_)) => format!("{name} = \"{color}\""),
            (TokenOrigin::Rule { text }, None) => format!("\"{name}\" = \"{text}\""),
            (TokenOrigin::Theme { .. }, None) => format!("\"{name}\" = \"{color}\""),
        };
        copy(cx, &line)
    }

    /// Its part of the themes page: the tokens' column at its left; the
    /// specimens of the terminal and of the interface at its right.
    pub(crate) fn tree(&self, cx: &mut Cx<'_>) -> Node {
        let tokens = self.tokens.as_ref();
        let title = self
            .shown_theme()
            .map_or_else(|| message(cx, "editor-no-theme", &[]), str::to_owned);
        let mut button =
            Node::button(title.as_str())
                .key(THEME)
                .label(message(cx, "editor-theme", &[]));
        if let Kind::Button(own) = &mut button.kind {
            "chevron-down".clone_into(&mut own.icon);
        }
        let mut chooser = vec![
            Node::text(message(cx, "editor-theme", &[])).style(secondary()),
            button,
        ];
        // A theme that cannot be edited as it is: the lock that unlocks it.
        let why = match self.locked(cx) {
            Some(Locked::ReadOnly) => Some("editor-read-only"),
            Some(Locked::Itermcolors) => Some("editor-itermcolors"),
            None => None,
        };
        if let Some(why) = why {
            let why = message(cx, why, &[]);
            chooser.push(
                Node::icon_button("lock")
                    .key(LOCK)
                    .label(why.clone())
                    .tooltip(why),
            );
        }
        let chooser = Node::container(chooser).layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            ..Layout::default()
        });
        let filter = search_box(
            cx,
            (FILTER, FILTER_CLEAR, "editor-filter-row"),
            ("editor-filter", "editor-filter-clear"),
            !self.filter.is_empty(),
        )
        .layout(Layout {
            direction: Direction::Row,
            gap: 2.0,
            padding: Edges::all(2.0),
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        });
        let wanted = words(&self.filter);
        // The tokens that a pick marks: its own, or the part's.
        let marked: BTreeSet<String> = match &self.picked {
            Some(Pick::Token(name)) => BTreeSet::from([name.clone()]),
            Some(Pick::Part(id)) => parts::tokens_of(id)
                .iter()
                .map(|&name| name.to_owned())
                .collect(),
            None => BTreeSet::new(),
        };
        let marks = (&self.lit, &marked);
        let rows = tokens.map_or_else(Vec::new, |tokens| {
            let mut rows = token_rows(cx, (&tokens.terminal, true), &wanted, marks);
            rows.extend(token_rows(cx, (&tokens.tokens, false), &wanted, marks));
            rows
        });
        let list = Node::scroll(vec![Node::container(rows).key("editor-tokens").layout(
            Layout {
                direction: Direction::Column,
                gap: 2.0,
                width: Length::Fraction(1.0),
                ..Layout::default()
            },
        )])
        .key("editor-list")
        .layout(Layout {
            direction: Direction::Column,
            grow: 1.0,
            width: Length::Fraction(1.0),
            height: Length::Points(0.0),
            ..Layout::default()
        });
        let column = Node::container(vec![chooser, filter, list])
            .key("editor-column")
            .role(Role::Group)
            .layout(Layout {
                direction: Direction::Column,
                gap: 10.0,
                width: Length::Points(COLUMN),
                min_width: Length::Points(COLUMN),
                height: Length::Fraction(1.0),
                padding: Edges::all(10.0),
                ..Layout::default()
            });
        let heading = |cx: &mut Cx<'_>, id: &str| {
            Node::text(message(cx, id, &[]))
                .role(Role::Heading)
                .style(Style {
                    font_weight: Some(FontWeight::Bold),
                    ..Style::default()
                })
        };
        let words: BTreeMap<&str, String> = parts::WORDS
            .iter()
            .map(|&id| (id, message(cx, id, &[])))
            .collect();
        let word = |id: &str| words.get(id).cloned().unwrap_or_else(|| id.to_owned());
        // The parts that a pick marks: its own, or the token's.
        let chosen = |id: &str| match &self.picked {
            Some(Pick::Part(part)) => part == id,
            Some(Pick::Token(name)) => parts::tokens_of(id).contains(&name.as_str()),
            None => false,
        };
        let mut right = vec![
            heading(cx, "editor-terminal"),
            // Of its own size, of the terminal's cells.
            Node::terminal_specimen()
                .key("editor-terminal")
                .label(message(cx, "editor-terminal", &[])),
            heading(cx, "editor-interface"),
        ];
        right.extend(parts::cards(&chosen, &word));
        let specimens = Node::scroll(vec![Node::container(right).layout(Layout {
            direction: Direction::Column,
            gap: 12.0,
            width: Length::Fraction(1.0),
            padding: Edges::all(10.0),
            ..Layout::default()
        })])
        .key("editor-specimens")
        .layout(Layout {
            direction: Direction::Column,
            grow: 1.0,
            width: Length::Points(0.0),
            min_width: Length::Points(0.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        });
        Node::container(vec![column, specimens])
            .key("editor-page")
            .layout(Layout {
                direction: Direction::Row,
                grow: 1.0,
                width: Length::Points(0.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            })
    }

    /// The tree of `view`, where it is its menu's.
    pub(crate) fn render(&self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Option<Node>> {
        if *view == ViewId::Panel(PanelId::from(THEMES_MENU)) {
            return Some(self.menu(cx));
        }
        None
    }

    /// The name of the theme that it shows: the one that it previews, or
    /// the one in force.
    fn shown_theme(&self) -> Option<&str> {
        self.previewed
            .as_deref()
            .or_else(|| self.tokens.as_ref().map(|tokens| tokens.theme.as_str()))
    }

    /// The native menu of the themes, the one shown ticked: the user's own,
    /// of their folder, then a line and a submenu of each appearance of
    /// the themes that the app holds or builds in, the light one first.
    fn menu(&self, cx: &mut Cx<'_>) -> Option<Node> {
        let themes = cx.themes().ok()?;
        let shown = self.shown_theme();
        let item = |theme: &ThemeInfo| {
            let mut item = Node::toggle("", shown == Some(theme.name.as_str()))
                .key(format!("{THEME_ITEM}{}", theme.name).as_str());
            if let Kind::Toggle(toggle) = &mut item.kind {
                toggle.label.clone_from(&theme.name);
            }
            item
        };
        let mut items: Vec<Node> = themes
            .themes
            .iter()
            .filter(|theme| !theme.is_read_only())
            .map(item)
            .collect();
        if !items.is_empty() {
            items.push(Node::separator());
        }
        for (variant, title) in [
            (ThemeVariant::Light, "themes-light"),
            (ThemeVariant::Dark, "themes-dark"),
        ] {
            let mut group = vec![Node::text(message(cx, title, &[]))];
            group.extend(
                themes
                    .themes
                    .iter()
                    .filter(|theme| theme.is_read_only() && theme.variant == variant)
                    .map(item),
            );
            if group.len() > 1 {
                items.push(Node::container(group));
            }
        }
        Some(Node::container(items).key("editor-themes-menu"))
    }
}

/// Copies `line` to paste, and tells it, as the notices' pace lets it.
fn copy(cx: &mut Cx<'_>, line: &str) -> Result<(), Error> {
    let line = line.to_owned();
    cx.set_clipboard_text(&line)?;
    let told = message(cx, "editor-copied", &[("line", line)]);
    // Copied all the same where notices come too fast to tell each.
    let _ = cx.notify(&told, NoticeLevel::Info, None);
    Ok(())
}

/// The colors of `now`, its terminal's and its tokens, whose color or
/// origin differs from `before`'s, or that `before` had not; none of a
/// first read.
fn changed(before: Option<&ThemeTokens>, now: Option<&ThemeTokens>) -> BTreeSet<String> {
    let (Some(before), Some(now)) = (before, now) else {
        return BTreeSet::new();
    };
    if before.theme != now.theme {
        return BTreeSet::new();
    }
    let old: BTreeMap<&str, &ThemeToken> = before
        .terminal
        .iter()
        .chain(&before.tokens)
        .map(|token| (token.name.as_str(), token))
        .collect();
    now.terminal
        .iter()
        .chain(&now.tokens)
        .filter(|token| old.get(token.name.as_str()) != Some(token))
        .map(|token| token.name.clone())
        .collect()
}

/// The names of the palette's 16 colors, as a theme's expressions write
/// them.
const PALETTE: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-magenta",
    "bright-cyan",
    "bright-white",
];

/// Whether `name` is one of the palette's colors.
fn in_palette(name: &str) -> bool {
    PALETTE.contains(&name)
}

/// The group of the token `name`: its name's first part, Lokyr's own by
/// their groups and a plugin's of its namespace.
fn group(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// The rows of `tokens` that the filter's `words` keep, all of them, in
/// their order, each group under its heading, those of `lit` lit and those
/// of `marked` marked, as a pick marks them: of the
/// terminal's colors where `terminal` says so, its keys under `[terminal]`
/// and its palette's under `[terminal] palette`, as a theme's file writes
/// them.
fn token_rows(
    cx: &mut Cx<'_>,
    (tokens, terminal): (&[ThemeToken], bool),
    words: &[String],
    (lit, marked): (&BTreeSet<String>, &BTreeSet<String>),
) -> Vec<Node> {
    let mut rows = Vec::new();
    let mut last: Option<&str> = None;
    for token in tokens {
        let origin = origin_text(cx, &token.origin);
        let value = token.color.to_string();
        let haystack = format!("{} {value} {origin}", token.name).to_lowercase();
        if !words.iter().all(|word| haystack.contains(word.as_str())) {
            continue;
        }
        let heading = match (terminal, in_palette(&token.name)) {
            (true, false) => "[terminal]",
            (true, true) => "[terminal] palette",
            (false, _) => group(&token.name),
        };
        if last != Some(heading) {
            last = Some(heading);
            rows.push(
                Node::text(heading)
                    .key(format!("editor-group:{heading}").as_str())
                    .role(Role::Heading)
                    .style(Style {
                        font_weight: Some(FontWeight::Bold),
                        ..Style::default()
                    })
                    .layout(Layout {
                        padding: Edges {
                            top: 8.0,
                            right: 0.0,
                            bottom: 2.0,
                            left: 0.0,
                        },
                        ..Layout::default()
                    }),
            );
        }
        let shade = if lit.contains(&token.name) {
            Some("pane.drop")
        } else if marked.contains(&token.name) {
            Some("list.selection.inactive.bg")
        } else {
            None
        };
        rows.push(token_row(cx, token, (&value, &origin), shade));
    }
    rows
}

/// What `origin` tells, as a row shows it: a file's name and line, a
/// theme's name, or the rule.
fn origin_text(cx: &mut Cx<'_>, origin: &TokenOrigin) -> String {
    match origin {
        TokenOrigin::File { path, line, .. } => {
            let file = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path);
            format!("{file}:{line}")
        }
        TokenOrigin::Theme { theme } => theme.clone(),
        TokenOrigin::Rule { text } => message(cx, "editor-rule", &[("rule", text.clone())]),
    }
}

/// The row of `token`: its color over the terminal's background, of its
/// value as a tooltip, its name and its origin, and its button, which
/// opens its file or copies its line; of `shade`'s background, as a change
/// lit it or a pick marks it.
fn token_row(
    cx: &mut Cx<'_>,
    token: &ThemeToken,
    (value, origin): (&str, &str),
    shade: Option<&str>,
) -> Node {
    let swatch = Node::container(vec![
        Node::container(Vec::new())
            .layout(Layout {
                width: Length::Fraction(1.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from(
                    format!("#{}", value.trim_start_matches('#')).as_str(),
                )),
                radius: 3.0,
                ..Style::default()
            }),
    ])
    .layout(Layout {
        width: Length::Points(28.0),
        min_width: Length::Points(28.0),
        height: Length::Points(18.0),
        padding: Edges::all(2.0),
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("terminal.bg")),
        border: Some(Border {
            color: Token::from("separator"),
            width: 1.0,
        }),
        radius: 4.0,
        ..Style::default()
    })
    // Its value as the pointer rests on it.
    .key(format!("editor-swatch:{}", token.name).as_str())
    .tooltip(value.to_owned());
    let file = matches!(token.origin, TokenOrigin::File { .. });
    let (icon, tip) = if file {
        ("edit", message(cx, "editor-open", &[]))
    } else {
        ("copy", message(cx, "editor-copy", &[]))
    };
    let small = Style {
        text_size: Some(TextSize::Small),
        ..secondary()
    };
    Node::container(vec![
        swatch,
        Node::container(vec![
            Node::text(token.name.as_str()).style(code()),
            Node::text(origin).style(small),
        ])
        .layout(Layout {
            direction: Direction::Column,
            gap: 1.0,
            grow: 1.0,
            min_width: Length::Points(0.0),
            ..Layout::default()
        }),
        Node::icon_button(icon)
            .key(format!("{ACT}{}", token.name).as_str())
            .label(tip.as_str())
            .tooltip(tip),
    ])
    .key(format!("editor-token:{}", token.name).as_str())
    .role(Role::Group)
    .label(format!("{} {value} {origin}", token.name))
    .layout(Layout {
        direction: Direction::Row,
        gap: 10.0,
        align: Align::Center,
        width: Length::Fraction(1.0),
        padding: Edges::all(4.0),
        ..Layout::default()
    })
    .style(Style {
        background: shade.map(Token::from),
        radius: 6.0,
        ..Style::default()
    })
}

#[cfg(test)]
mod tests;
