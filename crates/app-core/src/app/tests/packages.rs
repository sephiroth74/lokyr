//! The packages' service, for Lokyr's own plugins: a job of the bundle's
//! Sleip, its lines and its end to its plugin, a job cancelled, a Lokyr of
//! no Sleip, and the capability refused to an installed plugin.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Command};
use lokyr_kernel::plugins::host::Packages;
use lokyr_kernel::plugins::{Cx, Event, Plugin};
use lokyr_pkg_model::lock::Lock;
use lokyr_pkg_model::store::Store;

use super::installed::{grant_open, manifest, notices, opened, shelf, stored};
use super::*;

/// A plugin of Lokyr's own of the packages' service: `manager.list` lists
/// the installed plugins, `manager.search` searches, and `manager.cancel`
/// cancels its last job; the jobs' events kept.
struct Manager {
    last: Option<u64>,
    heard: Rc<RefCell<Vec<Event>>>,
}

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/manager\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[capabilities]\npackages = true\n",
);

impl Plugin for Manager {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["manager.list", "manager.search", "manager.cancel"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        match name {
            "manager.list" => self.last = Some(cx.packages(Packages::List)?),
            "manager.search" => {
                self.last = Some(cx.packages(Packages::Search {
                    query: "slow".to_owned(),
                })?);
            }
            _ => cx.cancel_packages(self.last.unwrap_or_default())?,
        }
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.heard.borrow_mut().extend(events.iter().cloned());
        Ok(())
    }
}

/// A Sleip of a shell's script, of the test's `name`: `list` tells its
/// arguments, a plugin and its end; `search` waits.
fn sleip(name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let folder = shelf(name);
    let path = folder.join("sleip");
    std::fs::write(
        &path,
        concat!(
            "#!/bin/sh\n",
            "case \"$1\" in\n",
            "list) echo \"{\\\"event\\\":\\\"args\\\",\\\"args\\\":\\\"$*\\\"}\"\n",
            "  echo '{\"event\":\"plugin\",\"name\":\"acme/hello\",\"version\":\"0.1.0\"}' ;;\n",
            "search) exec sleep 30 ;;\n",
            "esac\n",
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The texts of `line`'s field `field`, a line of Sleip's.
fn field(line: &lokyr_kernel::plugins::Data, field: &str) -> Option<String> {
    use lokyr_kernel::plugins::Data;
    match line {
        Data::Map(map) => match map.get(field) {
            Some(Data::Text(text)) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn a_plugin_of_lokyr_s_own_runs_sleip_s_commands_and_hears_their_lines_and_ends() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let plugin = Manager {
        last: None,
        heard: Rc::clone(&heard),
    };
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None)
        .with_program(quiet())
        .with_sleip(Some(sleip("sleip")))
        .with_builtin(MANIFEST, Box::new(plugin));
    fake.start(&mut app);
    let command = |fake: &mut Fake, app: &mut App, name: &str| {
        fake.send_app(app, AppEvent::Command(name.to_owned()));
    };
    let ended = |heard: &Rc<RefCell<Vec<Event>>>| {
        heard.borrow().iter().find_map(|event| match event {
            Event::PackagesEnded { ok, .. } => Some(*ok),
            _ => None,
        })
    };
    // The list's lines, then its end, its command of `--json`.
    command(&mut fake, &mut app, "manager.list");
    run_until(&mut fake, &mut app, |_, _| ended(&heard).is_some());
    let lines: Vec<_> = heard
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Package { line, .. } => Some(line.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(field(&lines[0], "args").as_deref(), Some("list --json"));
    assert_eq!(field(&lines[1], "name").as_deref(), Some("acme/hello"));
    assert_eq!(ended(&heard), Some(true));
    // A job cancelled as it waits ends of no success.
    heard.borrow_mut().clear();
    command(&mut fake, &mut app, "manager.search");
    command(&mut fake, &mut app, "manager.cancel");
    run_until(&mut fake, &mut app, |_, _| ended(&heard).is_some());
    assert_eq!(ended(&heard), Some(false));
    // A Lokyr of no Sleip tells its error, then the end.
    heard.borrow_mut().clear();
    app.packages.sleip = None;
    command(&mut fake, &mut app, "manager.list");
    run_until(&mut fake, &mut app, |_, _| ended(&heard).is_some());
    assert_eq!(ended(&heard), Some(false));
    let error = heard.borrow().iter().find_map(|event| match event {
        Event::Package { line, .. } => field(line, "message"),
        _ => None,
    });
    assert_eq!(error.as_deref(), Some("this Lokyr has no Sleip beside it"));
}

#[test]
fn an_installed_plugin_of_the_packages_capability_runs_without_it_as_a_notice_tells() {
    let _luau = LUAU.lock().unwrap_or_else(PoisonError::into_inner);
    let folder = shelf("first-party");
    let store = Store::new(folder.join("store"));
    let asker = manifest("asker", &["startup"], "^0.1", "packages = true\n");
    // Refused, of its message, which it tells by a page that it opens.
    let entry = r#"
        return {
            activate = function(cx)
                local id, message = cx.packages("list")
                if id == nil and message ~= nil then
                    assert(cx.open("https://lokyr.dev/refused"))
                end
            end,
        }
    "#;
    let lock = folder.join("plugins.lock");
    Lock {
        api: lokyr_kernel::plugins::API_VERSION.to_owned(),
        packages: std::collections::BTreeMap::from([stored(&store, "asker", &asker, entry)]),
    }
    .write(&lock)
    .unwrap();
    grant_open(&folder, &["asker"]);
    // URLs that open without a question, as the plugin opens one.
    let config = Config::parse(
        "[links]\nurls = \"open\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet()).with_installed(lock, store);
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |fake, _| opened(fake, "refused"));
    let told = notices(&mut app);
    assert!(
        told.iter()
            .any(|text| text.contains("Lokyr's own plugins alone")),
        "{told:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// A plugin of Lokyr's own of the packages' service that turns
/// `lokyr/noted` off, `switcher.off`, and on, `switcher.on`, and reads the
/// plugins, `switcher.read`, into `read`.
struct Switcher {
    read: Rc<RefCell<Vec<Vec<lokyr_kernel::plugins::host::PluginInfo>>>>,
}

const SWITCHER: &str = concat!(
    "[plugin]\nname = \"lokyr/switcher\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[capabilities]\npackages = true\n",
);

impl Plugin for Switcher {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["switcher.off", "switcher.on", "switcher.read"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        match name {
            "switcher.off" => cx.set_plugin_enabled("lokyr/noted", false),
            "switcher.on" => cx.set_plugin_enabled("lokyr/noted", true),
            _ => {
                let plugins = cx.plugins()?;
                self.read.borrow_mut().push(plugins);
                Ok(())
            }
        }
    }
}

/// A plugin of a command of its own, `noted.go`, which the switcher turns
/// off and on.
struct Noted;

impl Plugin for Noted {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "noted.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }
}

fn make_noted() -> Box<dyn Plugin + Send> {
    Box::new(Noted)
}

const NOTED: &str = "[plugin]\nname = \"lokyr/noted\"\nkind = \"builtin\"\nlicense = \"MIT\"\n";

#[test]
fn a_plugin_of_lokyr_s_own_turns_another_off_and_on_in_the_config_and_reads_its_state() {
    use lokyr_kernel::plugins::host::PluginState;
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-plugin-enabled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "# mine\n[font]\nsize = 14\n").unwrap();
    let read = Rc::new(RefCell::new(Vec::new()));
    let mut fake = Fake::default();
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_builtin(
            SWITCHER,
            Box::new(Switcher {
                read: Rc::clone(&read),
            }),
        )
        .with_registration(plugins::Registration {
            manifest: NOTED,
            make: make_noted,
        });
    fake.start(&mut app);
    let command = |fake: &mut Fake, app: &mut App, name: &str| {
        fake.send_app(app, AppEvent::Command(name.to_owned()));
    };
    let noted = |read: &Rc<RefCell<Vec<Vec<lokyr_kernel::plugins::host::PluginInfo>>>>| {
        read.borrow()
            .last()
            .and_then(|plugins| plugins.iter().find(|info| info.name == "lokyr/noted"))
            .map(|info| (info.namespace.clone(), info.state, info.built_in))
    };
    command(&mut fake, &mut app, "switcher.read");
    assert_eq!(
        noted(&read),
        Some(("noted".to_owned(), PluginState::Active, true))
    );
    assert!(app.commands.get("noted.go").is_some());
    // Turned off: its section written, the rest of the file kept, and it
    // stops at once.
    command(&mut fake, &mut app, "switcher.off");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\n[font]\nsize = 14\n\n[plugins.noted]\nenabled = false\n"
    );
    assert!(app.commands.get("noted.go").is_none());
    command(&mut fake, &mut app, "switcher.read");
    assert_eq!(
        noted(&read).map(|(_, state, _)| state),
        Some(PluginState::Off)
    );
    // Turned on again: a new one, its key taken away, as on is its
    // default, and its section with it, of nothing else.
    command(&mut fake, &mut app, "switcher.on");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\n[font]\nsize = 14\n"
    );
    assert!(app.commands.get("noted.go").is_some());
    command(&mut fake, &mut app, "switcher.read");
    assert_eq!(
        noted(&read).map(|(_, state, _)| state),
        Some(PluginState::Active)
    );
    assert!(fake.alerts().is_empty());
    let _ = std::fs::remove_dir_all(&directory);
    // Without a config file, an alert tells why nothing changes.
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None)
        .with_program(quiet())
        .with_builtin(SWITCHER, Box::new(Switcher { read }))
        .with_registration(plugins::Registration {
            manifest: NOTED,
            make: make_noted,
        });
    fake.start(&mut app);
    command(&mut fake, &mut app, "switcher.off");
    let told: Vec<String> = fake
        .alerts()
        .iter()
        .map(|shown| shown.alert.message.clone())
        .collect();
    assert_eq!(told, ["Lokyr cannot turn the plugin on or off"]);
    assert!(app.commands.get("noted.go").is_some());
}
