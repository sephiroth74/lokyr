//! Built-in plugin of the plugin manager: the settings tab's page of the
//! plugins, Lokyr's own and those that Sleip installed, each of its name,
//! its version, what it does, its state and, of an installed one, its
//! capabilities and the decisions on them, its state active, in green, or
//! disabled, in yellow, of the theme's colors; a button disables or enables a plugin, as its section of the
//! config, and an installed one's removes it,
//! through the packages' service, which tells it as it goes. Lokyr ›
//! Plugins… and the palette's command open the settings tab at the page.
//! The page follows the plugins as they change, of no wakeup at rest. Like
//! every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Category, Command, Error};
use lokyr_plugin_sdk::host::{Decision, Packages, PluginInfo, PluginState};
use lokyr_plugin_sdk::message::{Data, Event, PLUGINS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontWeight, Layout, Length, Node, ProgressShape,
    ProgressState, Role, Style, TextSize, Token, UiEventKind,
};

lokyr_plugin_sdk::export_plugin!(Manager);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/plugin-manager.ftl")),
    ("it", include_str!("../locales/it/plugin-manager.ftl")),
];

/// Its page of the settings tab.
const PAGE: &str = "plugins";

/// The command that opens the settings tab at the page.
const OPEN: &str = "plugin-manager.open";

/// Its own name, which turns itself off nowhere, as its page would go.
const OWN: &str = "lokyr/plugin-manager";

/// The prefix of the key of a plugin's button that turns it off or on.
const TOGGLE: &str = "toggle:";

/// The prefix of the key of an installed plugin's button that removes it.
const REMOVE: &str = "remove:";

/// The plugin: the jobs of the removals under way, each of its plugin, and
/// the last failure of each plugin's removal, which shows until it is
/// removed again.
#[derive(Default)]
pub struct Manager {
    removing: BTreeMap<u64, String>,
    failed: BTreeMap<String, String>,
    /// The last error that each job's Sleip told.
    errors: BTreeMap<u64, String>,
}

/// The page's view.
fn page() -> ViewId {
    ViewId::Page(PAGE.to_owned())
}

impl Plugin for Manager {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: OPEN.to_owned(),
            title: "Plugins…".to_owned(),
            category: Category::App,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.subscribe(&[PLUGINS_CHANGED])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        cx.show_page(PAGE)
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let mut again = false;
        for event in events {
            match event {
                Event::PluginsChanged | Event::PageShown(_) => again = true,
                Event::Package { id, line } => {
                    if let Some(message) = error_of(line) {
                        self.errors.insert(*id, message);
                    }
                }
                Event::PackagesEnded { id, ok } => {
                    self.ended(*id, *ok);
                    again = true;
                }
                _ => {}
            }
        }
        if again {
            cx.request_render(page())?;
        }
        Ok(())
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if event.kind != UiEventKind::Click {
            return Ok(());
        }
        let key = event.key.0.as_str();
        if let Some(name) = key.strip_prefix(TOGGLE) {
            let on = cx
                .plugins()?
                .iter()
                .find(|plugin| plugin.name == name)
                .is_some_and(|plugin| plugin.state != PluginState::Off);
            cx.set_plugin_enabled(name, !on)?;
        } else if let Some(name) = key.strip_prefix(REMOVE)
            && !self.removing.values().any(|removed| removed == name)
        {
            let job = cx.packages(Packages::Remove {
                plugin: name.to_owned(),
            })?;
            self.failed.remove(name);
            self.removing.insert(job, name.to_owned());
        } else {
            return Ok(());
        }
        cx.request_render(page())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view != page() {
            return None;
        }
        let plugins = cx.plugins().ok()?;
        let text = |cx: &mut Cx<'_>, id: &str| cx.message(id, &[]).unwrap_or_default();
        let (installed, built_in): (Vec<&PluginInfo>, Vec<&PluginInfo>) =
            plugins.iter().partition(|plugin| !plugin.built_in);
        let mut children = vec![
            Node::text(text(cx, "plugins-title"))
                .key("title")
                .role(Role::Heading)
                .style(Style {
                    text_size: Some(TextSize::Large),
                    font_weight: Some(FontWeight::Bold),
                    ..Style::default()
                }),
            Node::text(text(cx, "plugins-intro"))
                .key("intro")
                .style(secondary()),
        ];
        children.push(heading(text(cx, "plugins-installed"), "installed"));
        if installed.is_empty() {
            children.push(
                Node::text(text(cx, "plugins-none-installed"))
                    .key("none-installed")
                    .style(secondary()),
            );
        } else {
            let rows = installed
                .iter()
                .map(|plugin| self.row(cx, plugin))
                .collect();
            children.push(group(rows, "installed-plugins"));
        }
        children.push(heading(text(cx, "plugins-built-in"), "built-in"));
        let rows = built_in.iter().map(|plugin| self.row(cx, plugin)).collect();
        children.push(group(rows, "built-in-plugins"));
        Some(
            Node::scroll(vec![Node::container(children).key("plugins-page").layout(
                Layout {
                    direction: Direction::Column,
                    gap: 10.0,
                    padding: Edges::all(20.0),
                    width: Length::Fraction(1.0),
                    ..Layout::default()
                },
            )])
            .key("plugins-scroll")
            .layout(Layout {
                // Down a column, of the page's height, past which its
                // content scrolls.
                direction: Direction::Column,
                width: Length::Fraction(1.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            }),
        )
    }
}

impl Manager {
    /// The job `id` ended, `ok` or not: its plugin's removal is no longer
    /// under way, and one that failed tells why.
    fn ended(&mut self, id: u64, ok: bool) {
        let error = self.errors.remove(&id);
        if let Some(name) = self.removing.remove(&id)
            && !ok
        {
            self.failed.insert(name, error.unwrap_or_default());
        }
    }

    /// The row of `plugin`: its name and version, what it does, its state,
    /// an installed one's capabilities, and its buttons, or its removal
    /// under way, or the failure of the last.
    fn row(&self, cx: &mut Cx<'_>, plugin: &PluginInfo) -> Node {
        let name = &plugin.name;
        let mut title = vec![Node::text(name.clone()).style(Style {
            font_weight: Some(FontWeight::Semibold),
            ..Style::default()
        })];
        if let Some(version) = &plugin.version {
            title.push(Node::text(version.clone()).style(secondary()));
        }
        let mut about = vec![row_of(title, 6.0)];
        if let Some(description) = &plugin.description {
            about.push(Node::text(description.clone()).style(small()));
        }
        // Active, whether it runs or waits for what starts it, or disabled,
        // turned off or stopped; a fault tells why on a line of its own.
        let active = matches!(plugin.state, PluginState::Active | PluginState::Waiting);
        let state = message(
            cx,
            if active {
                "plugin-state-active"
            } else {
                "plugin-state-disabled"
            },
            &[],
        );
        about.push(
            Node::text(state)
                .key(format!("state:{name}").as_str())
                .style(Style {
                    color: Some(Token::from(if active {
                        "text.success"
                    } else {
                        "text.warning"
                    })),
                    text_size: Some(TextSize::Small),
                    ..Style::default()
                }),
        );
        if let (PluginState::Faulted, Some(fault)) = (plugin.state, &plugin.fault) {
            let fault = message(cx, "plugin-fault", &[("fault", fault.clone())]);
            about.push(Node::text(fault).style(Style {
                color: Some(Token::from("progress.error")),
                text_size: Some(TextSize::Small),
                ..Style::default()
            }));
        }
        // An installed plugin's capabilities, as the user decided them; a
        // built-in one holds them all.
        if !plugin.built_in && !plugin.capabilities.is_empty() {
            let decided: Vec<String> = plugin
                .capabilities
                .iter()
                .map(|decided| {
                    let id = match decided.decision {
                        Decision::Granted => "plugin-decision-granted",
                        Decision::Refused => "plugin-decision-refused",
                        Decision::Undecided => "plugin-decision-undecided",
                    };
                    message(cx, id, &[("capability", decided.capability.clone())])
                })
                .collect();
            let capabilities = message(
                cx,
                "plugin-capabilities",
                &[("capabilities", decided.join("; "))],
            );
            about.push(Node::text(capabilities).style(small()));
        }
        if let Some(failure) = self.failed.get(name) {
            let failure = message(cx, "plugin-remove-failed", &[("message", failure.clone())]);
            about.push(Node::text(failure).style(Style {
                color: Some(Token::from("progress.error")),
                text_size: Some(TextSize::Small),
                ..Style::default()
            }));
        }
        let about = Node::container(about).layout(Layout {
            direction: Direction::Column,
            gap: 3.0,
            grow: 1.0,
            ..Layout::default()
        });
        let mut actions = Vec::new();
        if self.removing.values().any(|removed| removed == name) {
            actions.push(
                Node::progress(ProgressShape::Ring, ProgressState::Indeterminate, 0.0).layout(
                    Layout {
                        width: Length::Points(14.0),
                        height: Length::Points(14.0),
                        ..Layout::default()
                    },
                ),
            );
            actions.push(Node::text(message(cx, "plugin-removing", &[])).style(small()));
        } else {
            if name != OWN {
                let id = if plugin.state == PluginState::Off {
                    "plugin-enable"
                } else {
                    "plugin-disable"
                };
                actions.push(
                    Node::button(message(cx, id, &[])).key(format!("{TOGGLE}{name}").as_str()),
                );
            }
            if !plugin.built_in {
                actions.push(
                    Node::button(message(cx, "plugin-remove", &[]))
                        .key(format!("{REMOVE}{name}").as_str()),
                );
            }
        }
        Node::container(vec![about, row_of(actions, 8.0)])
            .key(format!("plugin:{name}").as_str())
            .role(Role::Group)
            .label(name.clone())
            .layout(Layout {
                direction: Direction::Row,
                gap: 12.0,
                align: Align::Center,
                padding: Edges {
                    top: 10.0,
                    right: 12.0,
                    bottom: 10.0,
                    left: 12.0,
                },
                ..Layout::default()
            })
    }
}

/// The message `id` of the plugin's catalogs, of the texts `args`.
fn message(cx: &mut Cx<'_>, id: &str, args: &[(&str, String)]) -> String {
    let args: Vec<(&str, Data)> = args
        .iter()
        .map(|(name, value)| (*name, Data::Text(value.clone())))
        .collect();
    cx.message(id, &args).unwrap_or_else(|_| id.to_owned())
}

/// The message of an `error` line of Sleip's, if `line` is one.
fn error_of(line: &Data) -> Option<String> {
    let Data::Map(line) = line else {
        return None;
    };
    match (line.get("event"), line.get("message")) {
        (Some(Data::Text(event)), Some(Data::Text(message))) if event == "error" => {
            Some(message.clone())
        }
        _ => None,
    }
}

/// A row of `children`, `gap` points apart, in the middle of its height.
fn row_of(children: Vec<Node>, gap: f32) -> Node {
    Node::container(children).layout(Layout {
        direction: Direction::Row,
        gap,
        align: Align::Center,
        ..Layout::default()
    })
}

/// A section's heading, `text`, of the key `key`.
fn heading(text: String, key: &str) -> Node {
    Node::text(text)
        .key(key)
        .role(Role::Heading)
        .style(Style {
            font_weight: Some(FontWeight::Semibold),
            color: Some(Token::from("text.secondary")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
        .layout(Layout {
            padding: Edges {
                top: 8.0,
                right: 0.0,
                bottom: 0.0,
                left: 4.0,
            },
            ..Layout::default()
        })
}

/// A group of `rows`, of the key `key`, in a rounded box, a line between
/// each two, as macOS's settings group their rows.
fn group(rows: Vec<Node>, key: &str) -> Node {
    let mut children = Vec::new();
    for (at, row) in rows.into_iter().enumerate() {
        if at > 0 {
            children.push(Node::separator());
        }
        children.push(row);
    }
    Node::container(children)
        .key(key)
        .role(Role::Group)
        .layout(Layout {
            direction: Direction::Column,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 8.0,
            ..Style::default()
        })
}

/// The style of a secondary text.
fn secondary() -> Style {
    Style {
        color: Some(Token::from("text.secondary")),
        ..Style::default()
    }
}

/// The style of a small secondary text.
fn small() -> Style {
    Style {
        text_size: Some(TextSize::Small),
        ..secondary()
    }
}

#[cfg(test)]
mod tests;
