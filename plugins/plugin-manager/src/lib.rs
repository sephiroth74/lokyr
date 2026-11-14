//! Built-in plugin of the plugin manager: the settings window's page of the
//! plugins, of a sidebar of its two parts. Installed: Lokyr's own plugins
//! and those that Sleip installed, each of its name, its version, what it
//! does, its state, active, in green, or disabled, in yellow, of the
//! theme's colors, and, of an installed one, its capabilities and the
//! decisions on them; a button disables or enables a plugin, as its section
//! of the config, an installed one's removes it, and one that the registry
//! has newer updates it. Registry: the plugins of the registry, a loader
//! while they are read, which a field filters as it is typed in, and whose
//! Search, or Return, reads again, each to install, of its details, its
//! versions and what each asks for. Each change runs through the packages'
//! service, Sleip's, which tells it as it goes, a job under way of a
//! button that cancels it, a change done showing as busy until Lokyr takes
//! it. Only a plugin installed from the registry updates from it, as Sleip's
//! list of the installed plugins tells where each came from. Lokyr ›
//! Plugins… and the palette's command open the settings window at the page,
//! which follows the plugins as they change while it shows, of no wakeup at
//! rest; Sleip reads the registry's signed list as the page first shows, or
//! its registry's part, and again at a search. Like every built-in plugin,
//! it depends only on the plugin SDK.

#![forbid(unsafe_code)]

mod page;
mod registry;

use std::collections::{BTreeMap, BTreeSet};

use lokyr_plugin_sdk::command::{Args, Category, Command, Error};
use lokyr_plugin_sdk::host::Packages;
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

/// The key of the registry's button that searches it, reading it again.
const SEARCH: &str = "search";

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

/// The start of the source of a plugin installed from a registry, as
/// Sleip's list writes it.
const FROM_REGISTRY: &str = "registry+";

/// What a change's job told of the lockfile: each plugin that it holds
/// anew, of its version, or that it lost, of none; and whether it changed.
type Lockfile = (Vec<(String, Option<String>)>, bool);

/// What a job of the packages' service does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Work {
    /// Reads every plugin of the registry.
    List,
    /// Reads where each installed plugin came from.
    Installed,
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
            Self::List | Self::Installed | Self::Info(_) => None,
        }
    }
}

/// The plugin: whether its page shows, and the part shown; the jobs under
/// way, each of its work, the error that each told, what each changed of
/// the lockfile, and those cancelled; the changes done that Lokyr has not
/// taken yet, each of its work and of the version that it awaits, none of
/// a plugin removed; the last failure of each plugin's change, which shows
/// until it changes again; where each installed plugin came from, as
/// Sleip's list tells; the registry's plugins, once read, those of the
/// reading under way, its failure and its notes; the words that filter
/// them; and the plugin whose details show, and the failure of their read.
#[derive(Default)]
pub struct Manager {
    shown: bool,
    section: Section,
    jobs: BTreeMap<u64, Work>,
    errors: BTreeMap<u64, String>,
    lockfile: BTreeMap<u64, Lockfile>,
    cancelled: BTreeSet<u64>,
    settling: BTreeMap<String, (Work, Option<String>)>,
    failed: BTreeMap<String, String>,
    sources: BTreeMap<String, String>,
    found: Option<Vec<Found>>,
    finding: Vec<Found>,
    registry_error: Option<String>,
    notes: Vec<String>,
    query: String,
    detail: Option<(String, Option<Info>)>,
    info_failed: Option<(String, String)>,
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
                Event::PluginsChanged => {
                    self.settle(cx)?;
                    again = true;
                }
                // The page shows at its installed plugins, as its sidebar
                // does, the registry read as it first shows, and where each
                // installed plugin came from read anew.
                Event::PageShown(_) => {
                    self.shown = true;
                    self.section = Section::Installed;
                    self.detail = None;
                    if self.found.is_none() {
                        self.read_registry(cx)?;
                    }
                    self.read_sources(cx)?;
                    again = true;
                }
                Event::PageHidden(_) => self.shown = false,
                Event::Package { id, line } => again |= self.line(*id, line),
                Event::PackagesEnded { id, ok } => {
                    if self.ended(*id, *ok) {
                        self.read_sources(cx)?;
                    }
                    again = true;
                }
                _ => {}
            }
        }
        // Its view is there while its page shows alone.
        if again && self.shown {
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
                // The registry not read yet, as of no showing heard, read
                // as its part shows.
                if section == Section::Registry && self.found.is_none() {
                    self.read_registry(cx)?;
                }
            }
            (UiEventKind::InputChanged { text, .. }, QUERY) => {
                if *text == self.query {
                    return Ok(());
                }
                text.clone_into(&mut self.query);
            }
            // Return in the field searches, as its button does.
            (UiEventKind::Submit { text, .. }, QUERY) => {
                text.clone_into(&mut self.query);
                self.read_registry(cx)?;
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
            // Active, or waiting for what starts it, turns off; turned off,
            // or stopped by a fault, turns on, and starts again.
            let on = cx
                .plugins()?
                .iter()
                .find(|plugin| plugin.name == name)
                .is_some_and(|plugin| page::is_active(plugin.state));
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
            self.info_failed = None;
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
            self.cancelled.insert(job);
        } else if key == BACK {
            self.detail = None;
        } else if key == SEARCH {
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
        if self.changing(&name).is_some() || self.settling.contains_key(&name) {
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

    /// Reads where each installed plugin came from, unless a reading runs
    /// already.
    fn read_sources(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.jobs.values().any(|work| *work == Work::Installed) {
            return Ok(());
        }
        let job = cx.packages(Packages::List)?;
        self.jobs.insert(job, Work::Installed);
        Ok(())
    }

    /// Whether the installed plugin `name` came from the registry, which
    /// updates it, as Sleip's list told; not of a folder, a repository or a
    /// plugin under development, whose source an update would replace.
    fn of_registry(&self, name: &str) -> bool {
        self.sources
            .get(name)
            .is_some_and(|source| source.starts_with(FROM_REGISTRY))
    }

    /// The changes done that Lokyr took, as the plugins' list tells them:
    /// a plugin there at the version that it awaits, or gone.
    fn settle(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.settling.is_empty() {
            return Ok(());
        }
        let plugins = cx.plugins()?;
        self.settling.retain(|name, (_, version)| {
            let now = plugins
                .iter()
                .find(|plugin| plugin.name == *name && !plugin.built_in);
            match (now, version) {
                (None, None) => false,
                (Some(plugin), Some(version)) => plugin.version.as_deref() != Some(version),
                _ => true,
            }
        });
        Ok(())
    }

    /// The work of the change of the plugin `name` that is done and that
    /// Lokyr has not taken yet, if one is.
    fn settling(&self, name: &str) -> Option<&Work> {
        self.settling.get(name).map(|(work, _)| work)
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
            Some(Work::Installed) => {
                if let Some((name, source)) = registry::listed(line) {
                    self.sources.insert(name, source);
                }
                false
            }
            Some(Work::Install(_) | Work::Update(_) | Work::Remove(_)) => {
                let (changes, changed) = self.lockfile.entry(id).or_insert((Vec::new(), true));
                if let Some(change) = registry::changed(line) {
                    changes.push(change);
                } else if let Some(done) = registry::done_changed(line) {
                    *changed = done;
                }
                false
            }
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
    /// failure; where the installed plugins came from; a plugin's versions,
    /// or why not; a change done, busy until Lokyr takes what it changed of
    /// the lockfile, or one that failed, of why, a cancelled one nothing. A
    /// failure that tells no error tells so, of an empty text. Whether it
    /// changed the installed plugins.
    fn ended(&mut self, id: u64, ok: bool) -> bool {
        let error = self.errors.remove(&id).unwrap_or_default();
        let cancelled = self.cancelled.remove(&id);
        let (changes, changed) = self.lockfile.remove(&id).unwrap_or((Vec::new(), true));
        let Some(work) = self.jobs.remove(&id) else {
            return false;
        };
        match work {
            Work::List if ok => self.found = Some(std::mem::take(&mut self.finding)),
            Work::List => {
                self.finding.clear();
                self.registry_error = Some(error);
            }
            Work::Installed => {}
            Work::Info(name) => {
                if !ok {
                    self.info_failed = Some((name, error));
                }
            }
            Work::Install(ref name) | Work::Update(ref name) | Work::Remove(ref name) => {
                if ok {
                    // Each plugin that the lockfile holds anew, or lost, its
                    // dependencies too, busy until Lokyr takes it.
                    if changed {
                        for (plugin, version) in changes {
                            self.settling.insert(plugin, (work.clone(), version));
                        }
                    }
                    return true;
                }
                if !cancelled {
                    self.failed.insert(name.clone(), error);
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
