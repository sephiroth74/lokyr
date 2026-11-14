//! The packages' service, for Lokyr's own plugins: each job a command of
//! the bundle's Sleip, of `--json`, on a thread of its own, whose lines and
//! end go to its plugin as the main thread's next turn takes them.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use lokyr_kernel::config;
use lokyr_kernel::plugins::host;
use lokyr_pkg_model::service::{self, Job, Stop, Value};
use lokyr_platform_core::{Alert, AlertStyle, Platform};

use super::App;
use crate::Backend;

/// What a job's thread tells the main thread.
enum Told {
    /// A line of the job of its plugin and id.
    Line(String, u64, Value),
    /// The job's end, and whether it succeeded.
    Ended(String, u64, bool),
}

/// The jobs of the packages' service.
#[derive(Default)]
pub(crate) struct Jobs {
    /// Sleip's binary, if the app has one: the bundle's, beside the app's.
    pub(crate) sleip: Option<PathBuf>,
    /// The variables of Sleip's environment beside the app's.
    pub(crate) env: Vec<(String, String)>,
    /// What stops each job that runs, by its plugin and its id.
    running: BTreeMap<(String, u64), Stop>,
    /// The plugin that each job of a removal takes away, by its plugin and
    /// its id.
    removing: BTreeMap<(String, u64), String>,
    /// The plugins that a removal took away and that the app still runs,
    /// until it takes them away too.
    pub(crate) removed: std::collections::BTreeSet<String>,
    /// What the jobs' threads told since the main thread took it.
    told: Arc<Mutex<Vec<Told>>>,
}

impl std::fmt::Debug for Jobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jobs")
            .field("sleip", &self.sleip)
            .field("running", &self.running.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Jobs {
    /// The jobs of the Sleip beside the app's binary, if there is one, of
    /// the app's own plugin API, as `SLEIP_LOKYR` names the app.
    pub(crate) fn beside_the_app() -> Self {
        let executable = std::env::current_exe().ok();
        let sleip = executable
            .as_ref()
            .and_then(|executable| executable.parent())
            .map(|folder| folder.join("sleip"))
            .filter(|sleip| sleip.is_file());
        let env = executable
            .map(|executable| {
                vec![(
                    "SLEIP_LOKYR".to_owned(),
                    executable.to_string_lossy().into_owned(),
                )]
            })
            .unwrap_or_default();
        Self {
            sleip,
            env,
            ..Self::default()
        }
    }
}

impl<B: Backend> App<B> {
    /// Starts the job `id` of `plugin`'s call `call`, on a thread of its
    /// own, whose lines wake the main thread; without Sleip, its error and
    /// its end of no success.
    pub(super) fn start_packages(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, id): (String, u64),
        call: &host::Packages,
    ) {
        let started = match &self.packages.sleip {
            Some(sleip) => Job::start(sleip, call, &self.packages.env)
                .map_err(|error| format!("Sleip did not start: {error}")),
            None => Err("this Lokyr has no Sleip beside it".to_owned()),
        };
        let mut job = match started {
            Ok(job) => job,
            Err(message) => {
                self.plugins
                    .package_line(&plugin, id, &service::error(&message));
                self.plugins.packages_ended(&plugin, id, false);
                return;
            }
        };
        self.packages
            .running
            .insert((plugin.clone(), id), job.stopper());
        if let host::Packages::Remove { plugin: removed } = call {
            self.packages
                .removing
                .insert((plugin.clone(), id), removed.clone());
        }
        let (told, waker) = (Arc::clone(&self.packages.told), platform.waker());
        let tell = move |what: Told| {
            told.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(what);
            waker.wake();
        };
        std::thread::spawn(move || {
            while let Some(line) = job.next_line() {
                tell(Told::Line(plugin.clone(), id, line));
            }
            let (ok, error) = job.finish();
            if let Some(error) = error {
                tell(Told::Line(plugin.clone(), id, error));
            }
            tell(Told::Ended(plugin, id, ok));
        });
    }

    /// Ends `plugin`'s job `id` of the packages' service, if it runs, whose
    /// end its thread then tells.
    pub(super) fn cancel_packages(&mut self, plugin: &str, id: u64) {
        if let Some(stop) = self.packages.running.get(&(plugin.to_owned(), id)) {
            stop.stop();
        }
    }

    /// Turns the plugin of `namespace` on or off, as a plugin of the
    /// packages' service asks: its section of the config written, which
    /// the config read again applies at once, the plugin stopping or
    /// starting anew; or an alert of why it cannot be.
    pub(super) fn set_plugin_enabled(
        &mut self,
        platform: &mut dyn Platform,
        namespace: &str,
        enabled: bool,
    ) {
        let written = match &self.config_file {
            Some(file) => config::set_plugin_enabled(&file.path, namespace, enabled)
                .map_err(|error| error.to_string()),
            None => Err(self.texts.text("settings-no-home")),
        };
        match written {
            Ok(()) => {
                self.reload(platform);
                // A plugin that a fault stopped, which its section turns on
                // already, starts again.
                if enabled {
                    let (restarted, errors) = self.with_plugins(platform, |plugins, context| {
                        plugins.restart_faulted(namespace, context)
                    });
                    for error in errors {
                        eprintln!("lokyr: {error}");
                    }
                    if restarted {
                        self.after_plugins(platform);
                    }
                }
            }
            Err(why) => {
                eprintln!("lokyr: {why}");
                let alert = Alert {
                    style: AlertStyle::Warning,
                    message: self.texts.text("plugin-enabled-failed-message"),
                    detail: why,
                    buttons: Vec::new(),
                    suppression: None,
                    input: None,
                };
                let window = self.acting.or_else(|| platform.focused_window());
                platform.show_alert(window, &alert);
            }
        }
    }

    /// Takes the key `enabled` of the plugin `name`, removed and no longer
    /// run, out of the config, and its section if nothing else is left of
    /// it, so that no section stays of a plugin no longer there: never
    /// while the app runs it, which turning on would start again.
    /// The plugins' sections in force are the file's since, so that the
    /// next check of them tells nothing of the section gone.
    pub(super) fn forget_enabled(&mut self, name: &str) {
        let namespace = name.rsplit('/').next().unwrap_or(name);
        let Some(file) = &self.config_file else {
            return;
        };
        match config::set_plugin_enabled(&file.path, namespace, true)
            .and_then(|()| config::Config::read(&file.path))
        {
            Ok(read) => self.config.plugins = read.plugins,
            Err(error) => eprintln!("lokyr: {error}"),
        }
    }

    /// Gives the plugins what their jobs' threads told since the last
    /// turn, for their next delivery. A plugin removed takes its key
    /// `enabled` out of the config once the app no longer runs it.
    pub(super) fn take_packages(&mut self) {
        let told = std::mem::take(
            &mut *self
                .packages
                .told
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for what in told {
            match what {
                Told::Line(plugin, id, line) => self.plugins.package_line(&plugin, id, &line),
                Told::Ended(plugin, id, ok) => {
                    self.packages.running.remove(&(plugin.clone(), id));
                    let removed = self.packages.removing.remove(&(plugin.clone(), id));
                    if let (Some(removed), true) = (removed, ok) {
                        if self.plugins.status(&removed).is_some() {
                            self.packages.removed.insert(removed);
                        } else {
                            self.forget_enabled(&removed);
                        }
                    }
                    self.plugins.packages_ended(&plugin, id, ok);
                }
            }
        }
    }
}
