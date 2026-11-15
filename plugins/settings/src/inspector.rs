//! The settings window's page of the Theme Inspector: every token of the
//! theme in force, or of one that it previews, by groups, each of its color
//! over a backdrop, its name, its value and where it comes from, filtered as
//! its field says, the tokens that a saved file changed lit for a while; a
//! token of a file opens at its line, any other copies its line to paste.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::host::{NoticeLevel, ThemeToken, ThemeTokens, TokenOrigin};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::Cx;
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontWeight, Layout, Length, Node, Role, Style, TextSize,
    Token, UiEventKind,
};

use super::{code, message, search_box, secondary, words};

/// The page's id.
pub(crate) const INSPECTOR: &str = "inspector";

/// The filter's field, and its button that empties it.
const FILTER: &str = "inspector-filter";
const FILTER_CLEAR: &str = "inspector-filter-clear";

/// The button of the theme shown, and its menu of the themes.
const THEME: &str = "inspector-theme";
const THEMES_MENU: &str = "inspector-themes";

/// The menu's item of the theme in force, and of each theme, by its place.
const IN_FORCE: &str = "inspector-in-force";
const THEME_ITEM: &str = "inspector-theme:";

/// A token's button, which opens its file or copies its line, by its name.
const ACT: &str = "inspector-act:";

/// How long the tokens that a change of the theme changed stay lit.
const LIT: Duration = Duration::from_secs(3);

/// The width of the tokens' column.
const COLUMN: f32 = 460.0;

/// The page's view.
pub(crate) fn inspector_page() -> ViewId {
    ViewId::Page(INSPECTOR.to_owned())
}

/// The state of the page.
#[derive(Debug, Default)]
pub(crate) struct Inspector {
    /// Whether it shows.
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
}

impl Inspector {
    /// Takes `event`; returns whether it took it.
    pub(crate) fn on_event(&mut self, cx: &mut Cx<'_>, event: &Event) -> Result<bool, Error> {
        match event {
            Event::PageShown(page) if page == INSPECTOR => {
                self.shown = true;
                self.filter.clear();
                self.lit.clear();
                self.tokens = cx.theme_tokens(None)?;
                cx.request_render(inspector_page())?;
                Ok(true)
            }
            Event::PageHidden(page) if page == INSPECTOR => {
                self.shown = false;
                if self.previewed.take().is_some() {
                    cx.preview_theme(None)?;
                }
                Ok(true)
            }
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
                cx.request_render(inspector_page())?;
                Ok(true)
            }
            Event::Timer { id } if self.lit_timer == Some(*id) => {
                self.lit_timer = None;
                self.lit.clear();
                if self.shown {
                    cx.request_render(inspector_page())?;
                }
                Ok(true)
            }
            Event::PanelClosed(PanelId(panel)) if panel == THEMES_MENU => Ok(true),
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
            let chosen = if key == IN_FORCE {
                None
            } else {
                let Some(at) = key
                    .strip_prefix(THEME_ITEM)
                    .and_then(|at| at.parse::<usize>().ok())
                else {
                    return Ok(true);
                };
                cx.themes()?.themes.get(at).map(|theme| theme.name.clone())
            };
            self.preview(cx, chosen)?;
            return Ok(true);
        }
        if event.view != inspector_page() {
            return Ok(false);
        }
        match (&event.kind, key) {
            (UiEventKind::InputChanged { text, .. }, FILTER) => {
                if *text != self.filter {
                    text.clone_into(&mut self.filter);
                    cx.request_render(inspector_page())?;
                }
            }
            (UiEventKind::Click, FILTER_CLEAR) => {
                cx.set_text(inspector_page(), FILTER, "", u64::MAX)?;
                self.filter.clear();
                cx.request_render(inspector_page())?;
            }
            (UiEventKind::Click, THEME) => cx.open_panel(
                THEMES_MENU,
                PanelOptions {
                    anchor: Anchor::Node {
                        view: inspector_page(),
                        key: THEME.into(),
                    },
                    role: PanelRole::Menu,
                    close_outside: true,
                    close_escape: true,
                    takes_keys: false,
                    close_typing: false,
                },
            )?,
            (UiEventKind::Click, _) => {
                if let Some(name) = key.strip_prefix(ACT) {
                    self.act(cx, name)?;
                }
            }
            _ => {}
        }
        Ok(true)
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
        cx.request_render(inspector_page())
    }

    /// Opens the file of the token `name` at its line, or copies its line,
    /// `"name" = "value"`, of its rule's text or of its color, to paste into
    /// a theme's file.
    fn act(&self, cx: &mut Cx<'_>, name: &str) -> Result<(), Error> {
        let Some(token) = self
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.tokens.iter().find(|token| token.name == name))
        else {
            return Ok(());
        };
        match &token.origin {
            TokenOrigin::File { path, line, .. } => cx.open_at(path, (*line).max(1), None),
            TokenOrigin::Rule { text } => copy(cx, name, text),
            TokenOrigin::Theme { .. } => copy(cx, name, &token.color.to_string()),
        }
    }

    /// The page, where it shows: the tokens' column at its left; the
    /// specimens of the terminal and of the interface at its right.
    fn tree(&self, cx: &mut Cx<'_>) -> Option<Node> {
        if !self.shown {
            return None;
        }
        let tokens = self.tokens.as_ref();
        let title = match (&self.previewed, tokens) {
            (Some(name), _) => name.clone(),
            (None, Some(tokens)) => {
                message(cx, "inspector-in-force", &[("theme", tokens.theme.clone())])
            }
            (None, None) => message(cx, "inspector-no-theme", &[]),
        };
        let chooser = Node::container(vec![
            Node::text(message(cx, "inspector-theme", &[])).style(secondary()),
            Node::button(title.as_str())
                .key(THEME)
                .label(message(cx, "inspector-theme", &[])),
        ])
        .layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            ..Layout::default()
        });
        let filter = search_box(
            cx,
            (FILTER, FILTER_CLEAR, "inspector-filter-row"),
            ("inspector-filter", "inspector-filter-clear"),
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
        let rows = tokens.map_or_else(Vec::new, |tokens| {
            token_rows(cx, &tokens.tokens, &wanted, &self.lit)
        });
        let list = Node::scroll(vec![Node::container(rows).key("inspector-tokens").layout(
            Layout {
                direction: Direction::Column,
                gap: 2.0,
                width: Length::Fraction(1.0),
                ..Layout::default()
            },
        )])
        .key("inspector-list")
        .layout(Layout {
            direction: Direction::Column,
            grow: 1.0,
            width: Length::Fraction(1.0),
            height: Length::Points(0.0),
            ..Layout::default()
        });
        let column = Node::container(vec![chooser, filter, list])
            .key("inspector-column")
            .role(Role::Group)
            .layout(Layout {
                direction: Direction::Column,
                gap: 10.0,
                width: Length::Points(COLUMN),
                min_width: Length::Points(COLUMN),
                height: Length::Fraction(1.0),
                padding: Edges::all(16.0),
                ..Layout::default()
            });
        let specimens = Node::container(vec![
            Node::text(message(cx, "inspector-specimens", &[])).style(secondary()),
        ])
        .key("inspector-specimens")
        .layout(Layout {
            direction: Direction::Column,
            gap: 12.0,
            grow: 1.0,
            height: Length::Fraction(1.0),
            padding: Edges::all(16.0),
            ..Layout::default()
        });
        Some(
            Node::container(vec![column, specimens])
                .key("inspector-page")
                .layout(Layout {
                    direction: Direction::Row,
                    width: Length::Fraction(1.0),
                    height: Length::Fraction(1.0),
                    ..Layout::default()
                }),
        )
    }

    /// The tree of `view`, where it is the page's or its menu's.
    pub(crate) fn render(&self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Option<Node>> {
        if *view == inspector_page() {
            return Some(self.tree(cx));
        }
        if *view == ViewId::Panel(PanelId::from(THEMES_MENU)) {
            return Some(self.menu(cx));
        }
        None
    }

    /// The menu of the themes: the one in force, then each theme of the
    /// list, ticked where it shows.
    fn menu(&self, cx: &mut Cx<'_>) -> Option<Node> {
        let themes = cx.themes().ok()?;
        let mut items = vec![
            Node::toggle(
                message(cx, "inspector-theme-in-force", &[]).as_str(),
                self.previewed.is_none(),
            )
            .key(IN_FORCE),
        ];
        items.extend(themes.themes.iter().enumerate().map(|(at, theme)| {
            Node::toggle(
                theme.name.as_str(),
                self.previewed.as_deref() == Some(theme.name.as_str()),
            )
            .key(format!("{THEME_ITEM}{at}").as_str())
        }));
        Some(Node::scroll(items).layout(Layout {
            direction: Direction::Column,
            height: Length::Points(360.0),
            ..Layout::default()
        }))
    }
}

/// Copies the line of the token `name` of `value` to paste, and tells it,
/// as the notices' pace lets it.
fn copy(cx: &mut Cx<'_>, name: &str, value: &str) -> Result<(), Error> {
    let line = format!("\"{name}\" = \"{value}\"");
    cx.set_clipboard_text(&line)?;
    let told = message(cx, "inspector-copied", &[("line", line)]);
    // Copied all the same where notices come too fast to tell each.
    let _ = cx.notify(&told, NoticeLevel::Info, None);
    Ok(())
}

/// The tokens of `now` whose color or origin differs from `before`'s, or
/// that `before` had not; none of a first read.
fn changed(before: Option<&ThemeTokens>, now: Option<&ThemeTokens>) -> BTreeSet<String> {
    let (Some(before), Some(now)) = (before, now) else {
        return BTreeSet::new();
    };
    if before.theme != now.theme {
        return BTreeSet::new();
    }
    let old: BTreeMap<&str, &ThemeToken> = before
        .tokens
        .iter()
        .map(|token| (token.name.as_str(), token))
        .collect();
    now.tokens
        .iter()
        .filter(|token| old.get(token.name.as_str()) != Some(token))
        .map(|token| token.name.clone())
        .collect()
}

/// The group of the token `name`: its name's first part, Lokyr's own by
/// their groups and a plugin's of its namespace.
fn group(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// The rows of `tokens` that the filter's `words` keep, all of them, in
/// their order, each group under its heading, those of `lit` lit.
fn token_rows(
    cx: &mut Cx<'_>,
    tokens: &[ThemeToken],
    words: &[String],
    lit: &BTreeSet<String>,
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
        let heading = group(&token.name);
        if last != Some(heading) {
            last = Some(heading);
            rows.push(
                Node::text(heading)
                    .key(format!("inspector-group:{heading}").as_str())
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
        rows.push(token_row(
            cx,
            token,
            (&value, &origin),
            lit.contains(&token.name),
        ));
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
        TokenOrigin::Rule { text } => message(cx, "inspector-rule", &[("rule", text.clone())]),
    }
}

/// The row of `token`: its color over the terminal's background, its name,
/// its value and its origin, and its button, which opens its file or copies
/// its line; lit as a change made it.
fn token_row(
    cx: &mut Cx<'_>,
    token: &ThemeToken,
    (value, origin): (&str, &str),
    lit: bool,
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
    });
    let file = matches!(token.origin, TokenOrigin::File { .. });
    let (icon, tip) = if file {
        ("edit", message(cx, "inspector-open", &[]))
    } else {
        ("copy", message(cx, "inspector-copy", &[]))
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
        Node::text(value).style(code()),
        Node::icon_button(icon)
            .key(format!("{ACT}{}", token.name).as_str())
            .label(tip.as_str())
            .tooltip(tip),
    ])
    .key(format!("inspector-token:{}", token.name).as_str())
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
        background: lit.then(|| Token::from("pane.drop")),
        radius: 6.0,
        ..Style::default()
    })
}

#[cfg(test)]
mod tests;
