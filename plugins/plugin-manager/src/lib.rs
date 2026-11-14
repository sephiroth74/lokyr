//! Built-in plugin of the plugin manager: the settings window's page of the
//! plugins, of a sidebar of its two parts. Installed: Lokyr's own plugins
//! and those that Sleip installed, each of its name, its version, what it
//! does, its state, active, in green, or disabled, in yellow, of the
//! theme's colors, and, of an installed one, its capabilities and the
//! decisions on them; a button disables or enables a plugin, as its section
//! of the config, an installed one's removes it, and one that the registry
//! has newer updates it. Registry: the plugins of the registry, which a
//! field filters as it is typed in, each to install, of its details, its
//! versions and what each asks for. Each change runs through the packages'
//! service, Sleip's, which tells it as it goes, a job under way of a
//! button that cancels it. Lokyr › Plugins… and the palette's command open
//! the settings window at the page, which follows the plugins as they
//! change, of no wakeup at rest; Sleip reads the registry's signed list as
//! the page first shows, and again as its button asks. Like every built-in
//! plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

mod page;
mod registry;

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Category, Command, Error};
use lokyr_plugin_sdk::host::{Packages, PluginState};
use lokyr_plugin_sdk::message::{Data, Event, PLUGINS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::{Node, UiEventKind};

use registry::{Found, Info};

lokyr_plugin_sdk::export_plugin!(Manager);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/plugin-manager.ftl")),
    ("it", include_str!("../locales/it/plugin-manager.ftl")),
];

/// Its page of the settings window.
const PAGE: &str = "plugins";

/// The command that opens the settings window at the page.
const OPEN: &str = "plugin-manager.open";

/// Its own name, which turns itself off nowhere, as its page would go.
const OWN: &str = "lokyr/plugin-manager";

/// The key of the sidebar's list of the page's parts.
const SECTIONS: &str = "sections";

/// The key of the registry's field, which filters its plugins.
const QUERY: &str = "query";

/// The prefix of the key of a plugin's button that turns it off or on.
const TOGGLE: &str = "toggle:";

/// The prefix of the key of an installed plugin's button that removes it.
const REMOVE: &str = "remove:";

/// The prefix of the key of a plugin's button that installs it.
const INSTALL: &str = "install:";

/// The prefix of the key of a plugin's button that updates it.
const UPDATE: &str = "update:";

/// The prefix of the key of a plugin's button that shows its details.
const DETAILS: &str = "details:";

/// The prefix of the key of a job's button that cancels it.
const CANCEL: &str = "cancel:";

/// The key of the details' button back to the registry's plugins.
const BACK: &str = "back";

/// The key of the registry's button that reads it again.
const REFRESH: &str = "refresh";

/// The page's parts, the sidebar's items in their order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Section {
    /// The installed plugins and Lokyr's own.
    #[default]
    Installed,
    /// The registry's plugins.
    Registry,
}

impl Section {
    /// The parts in the sidebar's order.
    const ALL: [Self; 2] = [Self::Installed, Self::Registry];
}

/// What a job of the packages' service does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Work {
    /// Reads every plugin of the registry.
    List,
    /// Reads a plugin's versions.
    Info(String),
    /// Installs a plugin.
    Install(String),
    /// Updates a plugin to the newest version that this Lokyr takes.
    Update(String),
    /// Removes a plugin.
    Remove(String),
}

impl Work {
    /// The plugin that it changes, if it changes one.
    fn changes(&self) -> Option<&str> {
        match self {
            Self::Install(name) | Self::Update(name) | Self::Remove(name) => Some(name),
            Self::List | Self::Info(_) => None,
        }
    }
}

/// The plugin: the part of the page shown; the jobs under way, each of its
/// work, and the error that each told; the last failure of each plugin's
/// change, which shows until it changes again; the registry's plugins,
/// once read, those of the reading under way, its failure and its notes;
/// the words that filter them; and the plugin whose details show.
#[derive(Default)]
pub struct Manager {
    section: Section,
    jobs: BTreeMap<u64, Work>,
    errors: BTreeMap<u64, String>,
    failed: BTreeMap<String, String>,
    found: Option<Vec<Found>>,
    finding: Vec<Found>,
    registry_error: Option<String>,
    notes: Vec<String>,
    query: String,
    detail: Option<(String, Option<Info>)>,
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
                Event::PluginsChanged => again = true,
                // The page shows at its installed plugins, as its sidebar
                // does, the registry read as it first shows.
                Event::PageShown(_) => {
                    self.section = Section::Installed;
                    self.detail = None;
                    if self.found.is_none() {
                        self.read_registry(cx)?;
                    }
                    again = true;
                }
                Event::Package { id, line } => again |= self.line(*id, line),
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
        let key = event.key.0.as_str();
        match (&event.kind, key) {
            (UiEventKind::Select { item } | UiEventKind::Activate { item }, SECTIONS) => {
                let Some(&section) = Section::ALL.get(*item) else {
                    return Ok(());
                };
                if section == self.section {
                    return Ok(());
                }
                self.section = section;
                self.detail = None;
            }
            (UiEventKind::InputChanged { text, .. }, QUERY) => {
                if *text == self.query {
                    return Ok(());
                }
                text.clone_into(&mut self.query);
            }
            (UiEventKind::Click, _) => {
                if !self.clicked(cx, key)? {
                    return Ok(());
                }
            }
            _ => return Ok(()),
        }
        cx.request_render(page())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view != page() {
            return None;
        }
        let plugins = cx.plugins().ok()?;
        Some(page::tree(self, cx, &plugins))
    }
}

impl Manager {
    /// A click on the button of `key`: whether it did something.
    fn clicked(&mut self, cx: &mut Cx<'_>, key: &str) -> Result<bool, Error> {
        if let Some(name) = key.strip_prefix(TOGGLE) {
            let on = cx
                .plugins()?
                .iter()
                .find(|plugin| plugin.name == name)
                .is_some_and(|plugin| plugin.state != PluginState::Off);
            cx.set_plugin_enabled(name, !on)?;
        } else if let Some(name) = key.strip_prefix(REMOVE) {
            self.change(cx, Work::Remove(name.to_owned()))?;
        } else if let Some(name) = key.strip_prefix(INSTALL) {
            self.change(cx, Work::Install(name.to_owned()))?;
        } else if let Some(name) = key.strip_prefix(UPDATE) {
            self.change(cx, Work::Update(name.to_owned()))?;
        } else if let Some(name) = key.strip_prefix(DETAILS) {
            let job = cx.packages(Packages::Info {
                plugin: name.to_owned(),
            })?;
            self.failed.remove(name);
            self.jobs.insert(job, Work::Info(name.to_owned()));
            self.detail = Some((name.to_owned(), None));
        } else if let Some(job) = key.strip_prefix(CANCEL) {
            let Ok(job) = job.parse::<u64>() else {
                return Ok(false);
            };
            if !self.jobs.contains_key(&job) {
                return Ok(false);
            }
            cx.cancel_packages(job)?;
        } else if key == BACK {
            self.detail = None;
        } else if key == REFRESH {
            self.read_registry(cx)?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// Starts the job of `work`, a change of a plugin's, unless one of the
    /// plugin's runs already.
    fn change(&mut self, cx: &mut Cx<'_>, work: Work) -> Result<(), Error> {
        let Some(name) = work.changes().map(str::to_owned) else {
            return Ok(());
        };
        if self.changing(&name).is_some() {
            return Ok(());
        }
        let call = match &work {
            Work::Remove(_) => Packages::Remove {
                plugin: name.clone(),
            },
            // The newest version that this Lokyr takes, as `sleip add` of
            // no constraint installs it.
            _ => Packages::Add {
                plugin: name.clone(),
                constraint: None,
            },
        };
        let job = cx.packages(call)?;
        self.failed.remove(&name);
        self.jobs.insert(job, work);
        Ok(())
    }

    /// Reads every plugin of the registry, unless a reading runs already.
    fn read_registry(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.reading() {
            return Ok(());
        }
        let job = cx.packages(Packages::Search {
            query: String::new(),
        })?;
        self.finding.clear();
        self.notes.clear();
        self.registry_error = None;
        self.jobs.insert(job, Work::List);
        Ok(())
    }

    /// The job and the work of the change of the plugin `name` under way,
    /// if one is.
    fn changing(&self, name: &str) -> Option<(u64, &Work)> {
        self.jobs
            .iter()
            .find(|(_, work)| work.changes() == Some(name))
            .map(|(&job, work)| (job, work))
    }

    /// Whether the registry is being read.
    fn reading(&self) -> bool {
        self.jobs.values().any(|work| *work == Work::List)
    }

    /// The job `id`'s line `line`: whether the page shows something new
    /// of it.
    fn line(&mut self, id: u64, line: &Data) -> bool {
        if let Some(message) = registry::message_of(line, "error") {
            self.errors.insert(id, message);
            return false;
        }
        match self.jobs.get(&id) {
            Some(Work::List) => {
                if let Some(found) = registry::found(line) {
                    self.finding.push(found);
                } else if let Some(note) = registry::message_of(line, "note") {
                    self.notes.push(note);
                }
                false
            }
            Some(Work::Info(name)) => {
                let Some(info) = registry::info(line) else {
                    return false;
                };
                match &mut self.detail {
                    Some((shown, read)) if shown == name => {
                        *read = Some(info);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The job `id` ended, `ok` or not: the registry's plugins read, or its
    /// failure; a change that failed tells why, a cancelled one nothing.
    fn ended(&mut self, id: u64, ok: bool) {
        let error = self.errors.remove(&id);
        let Some(work) = self.jobs.remove(&id) else {
            return;
        };
        match work {
            Work::List if ok => self.found = Some(std::mem::take(&mut self.finding)),
            Work::List => {
                self.finding.clear();
                self.registry_error = Some(error.unwrap_or_default());
            }
            Work::Info(name) | Work::Install(name) | Work::Update(name) | Work::Remove(name) => {
                if let (false, Some(error)) = (ok, error) {
                    self.failed.insert(name, error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
