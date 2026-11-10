//! Sleip's commands, run as a user runs them, on an installation of the
//! tests' own, in a home folder of its own: each of text and of JSON, and
//! two at once, which wait on the store's lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::Kind;
use lokyr_pkg_model::store::{Store, digest_of};
use serde_json::Value;

/// A home folder of a test's own, of an installation.
struct Sandbox {
    home: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let home = std::env::temp_dir().join(format!("sleip-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".config/lokyr")).unwrap();
        // The app's plugin API, as its bundle's resource tells it.
        std::fs::write(home.join("api-version"), "0.1.0\n").unwrap();
        Self { home }
    }

    fn config(&self) -> PathBuf {
        self.home.join(".config/lokyr")
    }

    fn store(&self) -> Store {
        Store::of_user(&self.home, Some(&self.home.join(".local/share")))
    }

    /// The command line `args` of Sleip, in the sandbox.
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sleip"));
        command
            .args(args)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_DATA_HOME", self.home.join(".local/share"))
            .env("XDG_CACHE_HOME", self.home.join(".cache"))
            .env("SLEIP_LOKYR", self.home.join("api-version"))
            .env("PATH", "/usr/bin:/bin");
        command
    }

    fn sleip(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    /// Stores the package of the plugin `acme/<name>` of `version`, of the
    /// plugin API `api` and of the capabilities `capabilities`: its entry
    /// of the lockfile.
    fn package(&self, name: &str, version: &str, api: &str) -> (String, Locked) {
        let unpacked = self.home.join(format!("unpacked-{name}-{version}"));
        std::fs::create_dir_all(unpacked.join("src")).unwrap();
        std::fs::write(
            unpacked.join("plugin.toml"),
            format!(
                "[plugin]\nname = \"acme/{name}\"\nversion = \"{version}\"\nkind = \"luau\"\napi = \"{api}\"\nlicense = \"MIT\"\n[capabilities]\nopen = true\n"
            ),
        )
        .unwrap();
        std::fs::write(unpacked.join("src/init.luau"), "return {}\n").unwrap();
        let digest = digest_of(&unpacked).unwrap();
        let store = self.store();
        std::fs::create_dir_all(store.root()).unwrap();
        std::fs::rename(&unpacked, store.folder(&digest)).unwrap();
        (
            format!("acme/{name}"),
            Locked {
                version: version.to_owned(),
                kind: Kind::Luau,
                source: Source::Registry("https://pkg.lokyr.dev".to_owned()),
                checksum: Some(digest),
                capabilities: vec![lokyr_pkg_model::capability_of("open").unwrap()],
                dependencies: BTreeMap::new(),
            },
        )
    }

    fn lock(&self, packages: BTreeMap<String, Locked>) {
        Lock {
            api: "0.1.0".to_owned(),
            packages,
        }
        .write(&self.config().join("plugins.lock"))
        .unwrap();
    }

    fn locked(&self) -> Lock {
        Lock::read(&self.config().join("plugins.lock"))
            .unwrap()
            .unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// The standard output of `output`, as text.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The events of `output`'s NDJSON, each line one.
fn events(output: &Output) -> Vec<Value> {
    stdout(output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{line}: {error}")))
        .collect()
}

#[test]
fn the_help_the_version_and_a_wrong_command_line() {
    let sandbox = Sandbox::new("help");
    let help = sandbox.sleip(&["--help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("doctor"));
    let version = sandbox.sleip(&["--version"]);
    assert_eq!(
        stdout(&version),
        format!("sleip {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(stdout(&sandbox.sleip(&["help", "remove"])).starts_with("Usage: sleip remove"));
    for wrong in [&[][..], &["fly"], &["remove"], &["list", "--fast"]] {
        let output = sandbox.sleip(wrong);
        assert_eq!(output.status.code(), Some(2), "{wrong:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("sleip: "));
    }
}

#[test]
fn list_tells_each_installed_plugin_of_text_and_of_json() {
    let sandbox = Sandbox::new("list");
    assert_eq!(
        stdout(&sandbox.sleip(&["list"])),
        "No plugin is installed.\n"
    );
    assert!(stdout(&sandbox.sleip(&["--json", "list"])).is_empty());
    let mut packages = BTreeMap::from([sandbox.package("one", "1.0.0", "^0.1")]);
    let dev = sandbox.home.join("dev");
    packages.insert(
        "acme/two".to_owned(),
        Locked {
            version: "0.2.0".to_owned(),
            kind: Kind::Luau,
            source: Source::Dev(dev.display().to_string()),
            checksum: None,
            capabilities: Vec::new(),
            dependencies: BTreeMap::new(),
        },
    );
    sandbox.lock(packages);
    let text = sandbox.sleip(&["list"]);
    assert!(text.status.success());
    assert_eq!(
        stdout(&text),
        "acme/one 1.0.0 (registry): open\nacme/two 0.2.0 (dev)\n"
    );
    let json = events(&sandbox.sleip(&["list", "--json"]));
    assert_eq!(json.len(), 2);
    assert_eq!(json[0]["event"], "plugin");
    assert_eq!(json[0]["name"], "acme/one");
    assert_eq!(json[0]["version"], "1.0.0");
    assert_eq!(json[0]["kind"], "luau");
    assert_eq!(json[0]["source"], "registry+https://pkg.lokyr.dev");
    assert_eq!(json[0]["capabilities"], serde_json::json!(["open"]));
    assert_eq!(json[1]["source"], format!("dev+{}", dev.display()));
    // A lockfile that does not read tells it.
    std::fs::write(
        sandbox.config().join("plugins.lock"),
        "version = 9\napi = \"0.1.0\"\n",
    )
    .unwrap();
    let broken = sandbox.sleip(&["--json", "list"]);
    assert_eq!(broken.status.code(), Some(1));
    assert_eq!(events(&broken)[0]["event"], "error");
}

#[test]
fn remove_takes_a_plugin_out_of_both_lists_its_comments_kept_unless_another_needs_it() {
    let sandbox = Sandbox::new("remove");
    let (base, locked_base) = sandbox.package("base", "1.0.0", "^0.1");
    let (top, mut locked_top) = sandbox.package("top", "1.0.0", "^0.1");
    locked_top
        .dependencies
        .insert(base.clone(), "1.0.0".to_owned());
    sandbox.lock(BTreeMap::from([
        (base.clone(), locked_base.clone()),
        (top.clone(), locked_top),
    ]));
    let list = sandbox.config().join("plugins.toml");
    std::fs::write(
        &list,
        "# My plugins.\n[plugins]\n\"acme/top\" = \"^1.0\" # the one I use\n# Needed by top.\n\"acme/base\" = \"^1.0\"\n",
    )
    .unwrap();
    // Needed by another, it stays.
    let needed = sandbox.sleip(&["remove", "acme/base"]);
    assert_eq!(needed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&needed.stderr).contains("needed by `acme/top`"));
    let removed = sandbox.sleip(&["--json", "remove", "acme/top"]);
    assert!(removed.status.success(), "{removed:?}");
    assert_eq!(
        events(&removed),
        [serde_json::json!({ "event": "removed", "name": "acme/top" })]
    );
    assert_eq!(
        std::fs::read_to_string(&list).unwrap(),
        "# My plugins.\n[plugins]\n# Needed by top.\n\"acme/base\" = \"^1.0\"\n"
    );
    assert_eq!(
        sandbox.locked().packages.keys().collect::<Vec<_>>(),
        [&base]
    );
    // Its package stays until `gc`.
    assert_eq!(sandbox.store().digests().unwrap().len(), 2);
    // `--locked` keeps the lockfile as it is.
    let kept = sandbox.sleip(&["remove", "acme/base", "--locked"]);
    assert_eq!(kept.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&kept.stderr).contains("`--locked`"));
    assert!(sandbox.locked().packages.contains_key(&base));
    let text = sandbox.sleip(&["remove", "acme/base"]);
    assert_eq!(
        stdout(&text),
        "Removed acme/base; its package stays in the store until `sleip gc`.\n"
    );
    assert!(sandbox.locked().packages.is_empty());
    let none = sandbox.sleip(&["remove", "acme/base"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&none.stderr).contains("`acme/base` is not installed"));
}

#[test]
fn gc_removes_the_packages_that_no_lockfile_names_and_what_a_stopped_install_left() {
    let sandbox = Sandbox::new("gc");
    let kept = sandbox.package("kept", "1.0.0", "^0.1");
    let digest = kept.1.checksum.unwrap();
    sandbox.package("old", "1.0.0", "^0.1");
    sandbox.package("older", "0.9.0", "^0.1");
    sandbox.lock(BTreeMap::from([kept]));
    let leftover = sandbox.store().root().join(".unpack-1-0");
    std::fs::create_dir_all(&leftover).unwrap();
    std::fs::write(leftover.join("x"), "half").unwrap();
    // A lockfile that does not read removes nothing.
    let lock = sandbox.config().join("plugins.lock");
    let text = std::fs::read_to_string(&lock).unwrap();
    std::fs::write(&lock, "version = 9\napi = \"0.1.0\"\n").unwrap();
    assert_eq!(sandbox.sleip(&["gc"]).status.code(), Some(1));
    assert_eq!(sandbox.store().digests().unwrap().len(), 3);
    std::fs::write(&lock, text).unwrap();
    let collected = sandbox.sleip(&["--json", "gc"]);
    assert!(collected.status.success(), "{collected:?}");
    let json = events(&collected);
    assert_eq!(
        json.iter()
            .filter(|event| event["event"] == "collected")
            .count(),
        3
    );
    assert_eq!(json.last().unwrap()["event"], "done");
    assert_eq!(json.last().unwrap()["collected"], 3);
    assert_eq!(sandbox.store().digests().unwrap(), [digest]);
    assert!(!leftover.exists());
    assert_eq!(stdout(&sandbox.sleip(&["gc"])), "Nothing to remove.\n");
}

#[test]
fn doctor_tells_each_check_and_fails_of_a_problem() {
    let sandbox = Sandbox::new("doctor");
    let healthy = sandbox.package("good", "1.0.0", "^0.1");
    sandbox.package("unused", "1.0.0", "^0.1");
    sandbox.lock(BTreeMap::from([healthy.clone()]));
    let fine = sandbox.sleip(&["doctor"]);
    assert!(fine.status.success(), "{}", stdout(&fine));
    let text = stdout(&fine);
    assert!(
        text.contains("ok     plugin acme/good: 1.0.0 of "),
        "{text}"
    );
    assert!(
        text.contains("ok     app: the plugin API 0.1.0, of "),
        "{text}"
    );
    assert!(
        text.contains("note   unused: 1 packages of the store"),
        "{text}"
    );
    assert!(text.ends_with("No problem found.\n"), "{text}");
    // A package changed, a plugin of another API, and no app.
    let folder = sandbox.store().folder(&healthy.1.checksum.unwrap());
    std::fs::write(folder.join("src/init.luau"), "return { evil = true }\n").unwrap();
    let other = sandbox.package("other", "1.0.0", "^9.0");
    sandbox.lock(BTreeMap::from([healthy, other]));
    let ill = sandbox.sleip(&["--json", "doctor"]);
    assert_eq!(ill.status.code(), Some(1));
    let json = events(&ill);
    let errors: Vec<&Value> = json
        .iter()
        .filter(|event| event["event"] == "check" && event["level"] == "error")
        .collect();
    assert_eq!(errors.len(), 2, "{json:?}");
    assert!(
        errors[0]["detail"]
            .as_str()
            .unwrap()
            .contains("changed since it was stored")
    );
    assert!(
        errors[1]["detail"]
            .as_str()
            .unwrap()
            .contains("the plugin API ^9.0")
    );
    assert_eq!(json.last().unwrap()["problems"], 2);
    let lost = sandbox
        .command(&["doctor"])
        .env("SLEIP_LOKYR", sandbox.home.join("nowhere"))
        .output()
        .unwrap();
    assert_eq!(lost.status.code(), Some(1));
    assert!(stdout(&lost).contains("error  app: Lokyr is not found"));
}

#[test]
fn sleip_reads_the_api_of_the_bundle_that_it_is_in_through_a_link_to_it() {
    let sandbox = Sandbox::new("bundled");
    let contents = sandbox.home.join("Lokyr.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();
    std::fs::write(contents.join("Resources/api-version"), "0.1.0\n").unwrap();
    let bundled = contents.join("MacOS/sleip");
    std::fs::copy(env!("CARGO_BIN_EXE_sleip"), &bundled).unwrap();
    // A folder of the `PATH` of a link to it, as a user makes one.
    let bin = sandbox.home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(&bundled, bin.join("sleip")).unwrap();
    for sleip in [bundled, bin.join("sleip")] {
        let output = Command::new(&sleip)
            .arg("doctor")
            .env("HOME", &sandbox.home)
            .env("XDG_CONFIG_HOME", sandbox.home.join(".config"))
            .env("XDG_DATA_HOME", sandbox.home.join(".local/share"))
            .env("XDG_CACHE_HOME", sandbox.home.join(".cache"))
            .env_remove("SLEIP_LOKYR")
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        let text = stdout(&output);
        let resource = contents
            .join("Resources/api-version")
            .canonicalize()
            .unwrap();
        assert!(
            text.contains(&format!(
                "ok     app: the plugin API 0.1.0, of {}",
                resource.display()
            )),
            "{}: {text}",
            sleip.display()
        );
    }
}

#[test]
fn two_sleips_at_once_wait_on_the_store_s_lock() {
    let sandbox = Sandbox::new("lock");
    sandbox.lock(BTreeMap::new());
    // Another Sleip holds the lock, as this test does.
    let path = sandbox.store().root().with_file_name("store.lock");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let held = std::fs::File::create(&path).unwrap();
    held.lock().unwrap();
    let mut waiting = sandbox
        .command(&["--json", "gc"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // It tells that it waits, however long it takes to start, and waits.
    let mut lines =
        std::io::BufRead::lines(std::io::BufReader::new(waiting.stdout.take().unwrap()));
    let first: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(first["event"], "waiting");
    std::thread::sleep(Duration::from_millis(200));
    assert!(waiting.try_wait().unwrap().is_none(), "it did not wait");
    drop(held);
    let rest: Vec<Value> = lines
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert!(waiting.wait().unwrap().success());
    assert_eq!(rest.last().unwrap()["event"], "done");
    // Free, it takes the lock at once, of no wait.
    let free = sandbox.sleip(&["--json", "gc"]);
    assert_eq!(events(&free)[0]["event"], "done");
    let _: &Path = &path;
}

#[test]
fn new_makes_a_plugin_that_reads_packs_and_runs() {
    use lokyr_plugin_host_luau::{Execution, LuauPlugin, Package};
    use lokyr_plugin_sdk::fake::FakeHost;
    use lokyr_plugin_sdk::host::PaneId;
    use lokyr_plugin_sdk::ui::{PanelId, ViewId};

    let sandbox = Sandbox::new("new");
    let work = sandbox.home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let made = sandbox
        .command(&["--json", "new", "acme/greeter"])
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
    let json = events(&made);
    assert_eq!(json[0]["event"], "created");
    assert_eq!(json[0]["name"], "acme/greeter");
    assert_eq!(json[0]["files"].as_array().unwrap().len(), 8);
    let folder = work.join("greeter");
    let read = |path: &str| std::fs::read_to_string(folder.join(path)).unwrap();
    // Its manifest, of its name, its command and the plugin API.
    let manifest = lokyr_pkg_model::manifest::Manifest::parse(&read("plugin.toml")).unwrap();
    assert_eq!(manifest.name, "acme/greeter");
    assert_eq!(manifest.contributes.commands[0].name, "greeter.say");
    assert_eq!(manifest.activation, ["command:greeter.say"]);
    assert!(manifest.takes_api(lokyr_plugin_sdk::API_VERSION));
    // Nothing of the template's name left.
    for path in [
        "plugin.toml",
        "src/init.luau",
        "locales/en/greeter.ftl",
        "tests/greeter.luau",
        "README.md",
    ] {
        let text = read(path);
        for left in ["acme/hello", "hello.", "hello-", "# hello"] {
            assert!(!text.contains(left), "{path} holds `{left}`");
        }
    }
    assert!(read("lokyr.d.luau").contains("export type Cx"));
    // Its package packs, of its layout.
    lokyr_pkg_model::archive::pack(&folder).unwrap();
    // It runs, its notice of its catalog's text.
    let catalog = read("locales/en/greeter.ftl");
    let notice_text = catalog
        .lines()
        .find_map(|line| line.strip_prefix("greeter-notice = "))
        .unwrap();
    let mut plugin = LuauPlugin::new(
        "acme/greeter",
        Package::new([("src/init.luau".to_owned(), read("src/init.luau"))]),
        Execution::Native,
    );
    let mut host = FakeHost::new("acme/greeter");
    host.pane(PaneId(1));
    host.focus(Some(PaneId(1)));
    host.set_message("greeter-notice", &[], Some(notice_text));
    host.activate(&mut plugin).unwrap();
    host.command(
        &mut plugin,
        "greeter.say",
        lokyr_plugin_sdk::command::Args::new(),
    )
    .unwrap();
    let notice = PanelId::from("notice");
    assert!(host.panels().contains_key(&notice));
    let tree = host.render(&mut plugin, &ViewId::Panel(notice)).unwrap();
    assert_eq!(
        tree.children[0].kind,
        lokyr_plugin_sdk::view::Kind::Text(notice_text.to_owned())
    );
    // A folder there already, and names that are not a plugin's.
    let again = sandbox
        .command(&["new", "acme/greeter"])
        .current_dir(&work)
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&again.stderr).contains("there already"));
    for wrong in ["greeter", "Acme/greeter", "acme/9lives", "lokyr/mine"] {
        let output = sandbox
            .command(&["new", wrong])
            .current_dir(&work)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{wrong}");
    }
}

/// The plugin `acme/<name>` that `sleip new` makes in the sandbox, its
/// folder.
fn made(sandbox: &Sandbox, name: &str) -> PathBuf {
    let work = sandbox.home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let output = sandbox
        .command(&["new", &format!("acme/{name}")])
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    work.join(name)
}

/// `sleip check` in `folder`, of `args` before it.
fn checked(sandbox: &Sandbox, folder: &Path, args: &[&str]) -> Output {
    let mut line = args.to_vec();
    line.push("check");
    sandbox.command(&line).current_dir(folder).output().unwrap()
}

/// Edits the file `path` of `folder`, its text of `edit`.
fn edit(folder: &Path, path: &str, edit: impl FnOnce(String) -> String) {
    let file = folder.join(path);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&file, edit(text)).unwrap();
}

#[test]
fn check_passes_the_plugin_that_new_makes() {
    let sandbox = Sandbox::new("check-new");
    let folder = made(&sandbox, "good");
    let text = checked(&sandbox, &folder, &[]);
    assert!(text.status.success(), "{text:?}");
    assert_eq!(stdout(&text), "No problem found.\n");
    let json = checked(&sandbox, &folder, &["--json"]);
    assert!(json.status.success(), "{json:?}");
    let json = events(&json);
    assert_eq!(json.len(), 1, "{json:?}");
    assert_eq!(json[0]["event"], "done");
    assert_eq!(json[0]["problems"], 0);
}

#[test]
fn check_tells_each_problem_of_its_file_and_line() {
    let sandbox = Sandbox::new("check-problems");
    // Each case: what breaks the plugin, and the problems told, each its
    // place and its kind.
    type Case = (&'static str, fn(&Path), &'static [&'static str]);
    let cases: [Case; 11] = [
        (
            "syntax",
            |folder| {
                edit(folder, "plugin.toml", |text| {
                    text.replacen("[plugin]", "[plugin", 1)
                });
            },
            &["plugin.toml:1:"],
        ),
        (
            "withheld",
            |folder| {
                edit(folder, "plugin.toml", |text| {
                    format!(
                        "{text}\n[capabilities]\nnet = [\"api.github.com\"]\nprocess-spawn = true\n"
                    )
                });
            },
            &[
                "plugin.toml:18:1: Capability: ",
                "plugin.toml:19:1: Capability: ",
            ],
        ),
        (
            "api",
            |folder| {
                edit(folder, "plugin.toml", |text| {
                    text.replace("api = \"^0.1\"", "api = \"^9.0\"")
                });
            },
            &["plugin.toml:5:1: Api: it asks for the plugin API `^9.0`"],
        ),
        (
            "catalog",
            |folder| {
                edit(folder, "locales/en/catalog.ftl", |text| {
                    format!("{text}broken\n")
                });
                edit(folder, "locales/it/catalog.ftl", |_| {
                    "ok = Bene\n\noops =\n".to_owned()
                });
            },
            &[
                "locales/en/catalog.ftl:5:",
                "locales/it/catalog.ftl:3:1: Catalog: ",
            ],
        ),
        (
            "types",
            |folder| {
                edit(folder, "src/init.luau", |text| {
                    text.replacen(
                        "--!strict\n",
                        "--!strict\nlocal wrong: number = \"text\"\n",
                        1,
                    )
                });
            },
            &["src/init.luau:", ": TypeError: "],
        ),
        (
            "parse",
            |folder| {
                edit(folder, "src/init.luau", |text| {
                    format!("{text}\nlocal = 1\n")
                });
            },
            &["src/init.luau:", ": SyntaxError: "],
        ),
        (
            "module",
            |folder| {
                edit(folder, "src/init.luau", |text| {
                    format!("local view = require(\"./ui/view\")\nlocal _ = view\n{text}")
                });
                edit(folder, "src/ui/view.luau", |_| {
                    "--!strict\nlocal count: number = 0\ncount = \"many\"\nreturn {}\n".to_owned()
                });
            },
            &["src/ui/view.luau:3:", ": TypeError: "],
        ),
        (
            "missing",
            |folder| {
                edit(folder, "src/init.luau", |text| {
                    format!("local gone = require(\"./gone\")\nlocal _ = gone\n{text}")
                });
            },
            &["src/init.luau:1:"],
        ),
        (
            "uses",
            |folder| {
                edit(folder, "plugin.toml", |text| {
                    format!("{text}\n[uses]\ncommands = [\"copy\"]\n")
                });
            },
            &["plugin.toml:18:", ": Manifest: "],
        ),
        (
            "taken",
            |folder| {
                edit(folder, "src/init.luau", |text| {
                    text.replacen("--!strict\n", "--!strict\nprint(\"x\")\n", 1)
                });
            },
            &["src/init.luau:2:", ": TypeError: Unknown global 'print'"],
        ),
        (
            "package",
            |folder| {
                std::fs::rename(folder.join("src/init.luau"), folder.join("src/main.luau"))
                    .unwrap();
            },
            &[".: Package: ", "`src/init.luau`"],
        ),
    ];
    for (name, breaks, told) in cases {
        let folder = made(&sandbox, name);
        breaks(&folder);
        let output = checked(&sandbox, &folder, &[]);
        let text = stdout(&output);
        assert_eq!(output.status.code(), Some(1), "{name}: {text}");
        for each in told {
            assert!(text.contains(each), "{name}: no `{each}` in\n{text}");
        }
        assert!(text.trim_end().ends_with("found."), "{name}: {text}");
    }
}

#[test]
fn check_notes_what_is_not_wrong_and_tells_json() {
    let sandbox = Sandbox::new("check-notes");
    // A command of no title in English, and the API's types not this
    // Sleip's: notes, which fail nothing.
    let folder = made(&sandbox, "noted");
    edit(&folder, "locales/en/noted.ftl", |text| {
        text.lines()
            .filter(|line| !line.starts_with("command-noted-say"))
            .collect::<Vec<_>>()
            .join("\n")
    });
    edit(&folder, "lokyr.d.luau", |text| {
        format!("{text}\n-- edited\n")
    });
    let output = checked(&sandbox, &folder, &["--json"]);
    assert!(output.status.success(), "{output:?}");
    let json = events(&output);
    let notes: Vec<_> = json
        .iter()
        .filter(|event| event["event"] == "note")
        .collect();
    assert_eq!(notes.len(), 2, "{json:?}");
    assert_eq!(notes[0]["file"], "locales/en");
    assert!(
        notes[0]["message"]
            .as_str()
            .unwrap()
            .contains("command-noted-say")
    );
    assert_eq!(notes[1]["file"], "lokyr.d.luau");
    assert_eq!(json.last().unwrap()["problems"], 0);
    // A problem, of JSON: its file, line, column, kind and message.
    edit(&folder, "src/init.luau", |text| {
        text.replacen(
            "--!strict\n",
            "--!strict\nlocal wrong: number = \"text\"\n",
            1,
        )
    });
    let output = checked(&sandbox, &folder, &["--json"]);
    assert_eq!(output.status.code(), Some(1));
    let json = events(&output);
    let problem = json
        .iter()
        .find(|event| event["event"] == "problem")
        .unwrap();
    assert_eq!(problem["file"], "src/init.luau");
    assert_eq!(problem["kind"], "TypeError");
    assert!(problem["line"].as_u64().unwrap() > 1);
    assert!(problem["column"].as_u64().unwrap() >= 1);
    assert!(!problem["message"].as_str().unwrap().is_empty());
    assert_eq!(json.last().unwrap()["event"], "done");
    assert_eq!(json.last().unwrap()["problems"], 1);
    // No catalog in English: a note, the commands' titles left alone.
    let bare = made(&sandbox, "bare");
    std::fs::remove_dir_all(bare.join("locales")).unwrap();
    let output = checked(&sandbox, &bare, &[]);
    assert!(output.status.success(), "{output:?}");
    let text = stdout(&output);
    assert!(
        text.starts_with("locales: note: no catalog in English"),
        "{text}"
    );
    assert_eq!(text.lines().count(), 2, "{text}");
    // No plugin in the folder.
    let empty = sandbox.home.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let output = checked(&sandbox, &empty, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).starts_with("plugin.toml: Manifest: it cannot be read"));
}

#[test]
fn gc_of_a_config_folder_of_no_lockfile_removes_nothing_of_the_shared_store() {
    let sandbox = Sandbox::new("gc-elsewhere");
    let kept = sandbox.package("kept", "1.0.0", "^0.1");
    sandbox.lock(BTreeMap::from([kept]));
    // Another config folder, as a shell's `XDG_CONFIG_HOME` gives it.
    let elsewhere = sandbox.home.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let output = sandbox
        .command(&["gc"])
        .env("XDG_CONFIG_HOME", &elsewhere)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("there is no lockfile"));
    assert_eq!(sandbox.store().digests().unwrap().len(), 1);
}

#[test]
fn remove_takes_out_the_dependencies_that_no_other_plugin_needs() {
    let sandbox = Sandbox::new("remove-needless");
    let (base, locked_base) = sandbox.package("base", "1.0.0", "^0.1");
    let (shared, locked_shared) = sandbox.package("shared", "1.0.0", "^0.1");
    let (top, mut locked_top) = sandbox.package("top", "1.0.0", "^0.1");
    let (other, mut locked_other) = sandbox.package("other", "1.0.0", "^0.1");
    let (stray, locked_stray) = sandbox.package("stray", "1.0.0", "^0.1");
    locked_top.dependencies.extend([
        (base.clone(), "1.0.0".to_owned()),
        (shared.clone(), "1.0.0".to_owned()),
    ]);
    locked_other
        .dependencies
        .insert(shared.clone(), "1.0.0".to_owned());
    sandbox.lock(BTreeMap::from([
        (base.clone(), locked_base),
        (shared.clone(), locked_shared),
        (top.clone(), locked_top),
        (other.clone(), locked_other),
        (stray.clone(), locked_stray),
    ]));
    std::fs::write(
        sandbox.config().join("plugins.toml"),
        "[plugins]\n\"acme/top\" = \"^1.0\"\n\"acme/other\" = \"^1.0\"\n",
    )
    .unwrap();
    let removed = sandbox.sleip(&["--json", "remove", "acme/top"]);
    assert!(removed.status.success(), "{removed:?}");
    assert_eq!(
        events(&removed),
        [
            serde_json::json!({ "event": "removed", "name": "acme/top" }),
            serde_json::json!({ "event": "removed", "name": "acme/base", "needed": false }),
        ]
    );
    // What another needs stays, and what the lockfile holds of no plugin's
    // need, the user's own.
    let left: Vec<String> = sandbox.locked().packages.keys().cloned().collect();
    assert_eq!(left, [other, shared, stray]);
}

#[test]
fn new_keeps_a_name_of_the_template_s_word_and_its_plugin_passes_its_check() {
    let sandbox = Sandbox::new("new-names");
    for (name, namespace) in [("hello/world", "world"), ("acme/say-hello", "say-hello")] {
        let work = sandbox.home.join("names");
        std::fs::create_dir_all(&work).unwrap();
        let made = sandbox
            .command(&["new", name])
            .current_dir(&work)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        let folder = work.join(namespace);
        let manifest = lokyr_pkg_model::manifest::Manifest::parse(
            &std::fs::read_to_string(folder.join("plugin.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.name, name);
        assert_eq!(
            manifest.contributes.commands[0].name,
            format!("{namespace}.say")
        );
        let checked = sandbox
            .command(&["check"])
            .current_dir(&folder)
            .output()
            .unwrap();
        assert!(checked.status.success(), "{name}: {}", stdout(&checked));
    }
}

#[test]
fn a_closed_standard_output_ends_no_command_in_a_panic() {
    let sandbox = Sandbox::new("closed-pipe");
    let packages = (0..20)
        .map(|at| sandbox.package(&format!("p{at}"), "1.0.0", "^0.1"))
        .collect();
    sandbox.lock(packages);
    // A reader that stopped reading, as `head` does.
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    for args in [
        &["list"][..],
        &["--json", "list"],
        &["--help"],
        &["--version"],
    ] {
        let output = sandbox
            .command(args)
            .stdout(Stdio::from(writer.try_clone().unwrap()))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
    }
}

#[test]
fn test_runs_the_tests_of_a_plugin_and_tells_a_failure_of_its_file_and_line() {
    let sandbox = Sandbox::new("test");
    let folder = made(&sandbox, "tested");
    let tested = |args: &[&str]| {
        let mut line = args.to_vec();
        line.push("test");
        sandbox
            .command(&line)
            .current_dir(&folder)
            .output()
            .unwrap()
    };
    // The template's test passes.
    let output = tested(&[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout(&output),
        "ok      tests/tested.luau the_command_shows_its_notice_which_goes_by_itself\n1 passed.\n"
    );
    // A failing test, of its file and line, beside one that passes.
    edit(&folder, "tests/more.luau", |_| {
        "--!strict\nlocal test = {}\n\nfunction test.fails(host: TestHost)\n    host.activate()\n    assert(#host.panels() == 1, \"a panel at once\")\nend\n\nfunction test.passes(host: TestHost)\n    host.activate()\n    host.command(\"tested.say\")\n    assert(host.panels()[1] == \"notice\")\nend\n\nreturn test\n".to_owned()
    });
    let output = tested(&[]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let text = stdout(&output);
    assert!(
        text.contains("FAILED  tests/more.luau:6 fails: a panel at once\n"),
        "{text}"
    );
    assert!(text.contains("ok      tests/more.luau passes\n"), "{text}");
    assert!(text.ends_with("2 passed, 1 failed.\n"), "{text}");
    let json = events(&tested(&["--json"]));
    let failed = json
        .iter()
        .find(|event| event["event"] == "test" && event["ok"] == false)
        .unwrap();
    assert_eq!(failed["file"], "tests/more.luau");
    assert_eq!(failed["line"], 6);
    assert_eq!(failed["message"], "a panel at once");
    assert_eq!(json.last().unwrap()["failed"], 1);
    // Its tests' types checked too, of `TestHost`'s.
    edit(&folder, "tests/more.luau", |text| {
        text.replace(
            "host.activate()\n    assert(#host",
            "host.nope()\n    assert(#host",
        )
    });
    let checked = checked(&sandbox, &folder, &[]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(
        stdout(&checked).contains("tests/more.luau:5:"),
        "{}",
        stdout(&checked)
    );
}

#[test]
fn dev_links_a_plugin_checks_it_at_each_save_and_gives_the_installed_one_back_as_it_ends() {
    use std::io::BufRead;

    let sandbox = Sandbox::new("dev");
    // An installed plugin of the name that `sleip dev` takes the place of.
    let installed = sandbox.package("devved", "1.0.0", "^0.1");
    sandbox.lock(BTreeMap::from([installed.clone()]));
    let folder = made(&sandbox, "devved");
    let mut dev = sandbox
        .command(&["--json", "dev"])
        .current_dir(&folder)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = std::io::BufReader::new(dev.stdout.take().unwrap()).lines();
    let mut next = |wanted: &str| -> Value {
        loop {
            let line = lines.next().expect("its output ended").unwrap();
            let event: Value = serde_json::from_str(&line).unwrap();
            if event["event"] == wanted {
                return event;
            }
        }
    };
    assert_eq!(next("checked")["problems"], 0);
    let linked = next("linked");
    assert_eq!(linked["name"], "acme/devved");
    assert_eq!(linked["replaced"], "1.0.0");
    // Its folder in the lockfile, of no checksum, as the app reloads it.
    let entry = sandbox.locked().packages["acme/devved"].clone();
    let canonical = std::fs::canonicalize(&folder).unwrap();
    assert_eq!(entry.source, Source::Dev(canonical.display().to_string()));
    assert_eq!(entry.checksum, None);
    // A save of a fault: checked again, the fault told.
    edit(&folder, "src/init.luau", |text| {
        text.replacen(
            "--!strict\n",
            "--!strict\nlocal wrong: number = \"text\"\n",
            1,
        )
    });
    loop {
        let checked = next("checked");
        if checked["problems"] != 0 {
            break;
        }
    }
    // Interrupted, as Ctrl-C does: the installed plugin back.
    let killed = Command::new("kill")
        .args(["-INT", &dev.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert_eq!(next("unlinked")["name"], "acme/devved");
    assert!(dev.wait().unwrap().success());
    assert_eq!(sandbox.locked().packages["acme/devved"], installed.1);
}

#[test]
fn dev_takes_away_the_lockfile_that_it_made_as_it_ends() {
    use std::io::BufRead;

    let sandbox = Sandbox::new("dev-made");
    let folder = made(&sandbox, "fresh");
    let lock = sandbox.config().join("plugins.lock");
    assert!(!lock.exists());
    let mut dev = sandbox
        .command(&["--json", "dev"])
        .current_dir(&folder)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = std::io::BufReader::new(dev.stdout.take().unwrap()).lines();
    let mut next = |wanted: &str| loop {
        let line = lines.next().expect("its output ended").unwrap();
        let event: Value = serde_json::from_str(&line).unwrap();
        if event["event"] == wanted {
            return event;
        }
    };
    next("linked");
    assert!(sandbox.locked().packages.contains_key("acme/fresh"));
    let killed = Command::new("kill")
        .args(["-INT", &dev.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    next("unlinked");
    assert!(dev.wait().unwrap().success());
    assert!(!lock.exists());
}

/// Runs git of `args` in `folder`, of an author of the tests.
fn git(folder: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Tests", "-c", "user.email=tests@lokyr.dev"])
        .args(args)
        .current_dir(folder)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Sets the version of the plugin of `folder` to `version`.
fn versioned(folder: &Path, version: &str) {
    edit(folder, "plugin.toml", |text| {
        let at = text.find("version = \"").unwrap() + "version = \"".len();
        let end = at + text[at..].find('"').unwrap();
        format!("{}{version}{}", &text[..at], &text[end..])
    });
}

#[test]
fn add_installs_a_plugin_of_a_folder_and_update_takes_its_changes() {
    let sandbox = Sandbox::new("add-path");
    let folder = made(&sandbox, "local");
    let canonical = std::fs::canonicalize(&folder).unwrap();
    let output = sandbox.sleip(&["--json", "add", "--path", &folder.display().to_string()]);
    assert!(output.status.success(), "{output:?}");
    let json = events(&output);
    let installed = json
        .iter()
        .find(|event| event["event"] == "installed")
        .unwrap();
    assert_eq!(installed["name"], "acme/local");
    assert_eq!(installed["version"], "0.1.0");
    let entry = sandbox.locked().packages["acme/local"].clone();
    assert_eq!(entry.source, Source::Path(canonical.display().to_string()));
    let digest = entry.checksum.unwrap();
    assert_eq!(sandbox.store().digests().unwrap(), [digest]);
    let list = std::fs::read_to_string(sandbox.config().join("plugins.toml")).unwrap();
    assert!(list.contains("\"acme/local\""), "{list}");
    assert!(
        list.contains(&format!("path = \"{}\"", folder.display())),
        "{list}"
    );
    // Again, nothing changes; of a relative path too, which the list
    // takes of its whole path.
    assert_eq!(
        stdout(&sandbox.sleip(&["add", "--path", &folder.display().to_string()])),
        "Nothing changed.\n"
    );
    let relative = sandbox
        .command(&["add", "--path", "."])
        .current_dir(&folder)
        .output()
        .unwrap();
    assert!(relative.status.success(), "{relative:?}");
    let list = std::fs::read_to_string(sandbox.config().join("plugins.toml")).unwrap();
    assert!(
        list.contains(&format!("path = \"{}\"", canonical.display())),
        "{list}"
    );
    assert!(sandbox.sleip(&["list"]).status.success());
    // A new version of its folder: `--locked` keeps the lockfile, `update`
    // takes it.
    versioned(&folder, "0.2.0");
    let locked = sandbox.sleip(&["--locked", "update"]);
    assert_eq!(locked.status.code(), Some(1));
    assert_eq!(sandbox.locked().packages["acme/local"].version, "0.1.0");
    let updated = sandbox.sleip(&["update", "acme/local"]);
    assert!(updated.status.success(), "{updated:?}");
    let entry = &sandbox.locked().packages["acme/local"];
    assert_eq!(entry.version, "0.2.0");
    assert_ne!(entry.checksum, Some(digest));
    // A name that its folder does not hold.
    let wrong = sandbox.sleip(&["add", "acme/other", "--path", &folder.display().to_string()]);
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("not `acme/other`"));
}

#[test]
fn add_installs_a_plugin_of_a_git_repository_at_a_tag_or_a_commit() {
    let sandbox = Sandbox::new("add-git");
    let folder = made(&sandbox, "remote");
    git(&folder, &["init", "--quiet"]);
    git(&folder, &["add", "."]);
    git(&folder, &["commit", "--quiet", "-m", "first"]);
    git(&folder, &["tag", "v0.1.0"]);
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let url = format!("file://{}", folder.display());
    let output = sandbox.sleip(&["add", "--git", &url, "--tag", "v0.1.0"]);
    assert!(output.status.success(), "{output:?}");
    let entry = sandbox.locked().packages["acme/remote"].clone();
    assert_eq!(
        entry.source,
        Source::Git {
            url: url.clone(),
            commit: first.clone()
        }
    );
    // A later commit, by its short hash: its version.
    versioned(&folder, "0.2.0");
    git(&folder, &["commit", "--quiet", "-am", "second"]);
    let second = git(&folder, &["rev-parse", "HEAD"]);
    let output = sandbox.sleip(&["add", "--git", &url, "--rev", &second[..10]]);
    assert!(output.status.success(), "{output:?}");
    let entry = &sandbox.locked().packages["acme/remote"];
    assert_eq!(entry.version, "0.2.0");
    assert_eq!(
        entry.source,
        Source::Git {
            url: url.clone(),
            commit: second
        }
    );
    // A tag that it lacks, and the clones gone from the cache.
    let output = sandbox.sleip(&["add", "--git", &url, "--tag", "v9"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("has no tag `v9`"));
    let clones = sandbox.home.join("Library/Caches/lokyr/sleip/git");
    assert!(std::fs::read_dir(&clones).map_or(true, |mut entries| entries.next().is_none()));
}

#[test]
fn a_conflict_tells_its_reason_and_a_failed_install_leaves_the_store_as_it_was() {
    let sandbox = Sandbox::new("add-conflict");
    // A plugin of another plugin API.
    let newer = made(&sandbox, "newer");
    edit(&newer, "plugin.toml", |text| {
        text.replace("api = \"^0.1\"", "api = \"^0.9\"")
    });
    let output = sandbox.sleip(&["add", "--path", &newer.display().to_string()]);
    assert_eq!(output.status.code(), Some(1));
    let told = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        told.contains("acme/newer depends on Lokyr's plugin API >=0.9.0, <0.10.0"),
        "{told}"
    );
    // A plugin that needs another of a version that none holds.
    let base = made(&sandbox, "base");
    assert!(
        sandbox
            .sleip(&["add", "--path", &base.display().to_string()])
            .status
            .success()
    );
    let top = made(&sandbox, "top");
    edit(&top, "plugin.toml", |text| {
        format!("{text}\n[dependencies]\n\"acme/base\" = \"^2\"\n")
    });
    let output = sandbox.sleip(&["add", "--path", &top.display().to_string()]);
    assert_eq!(output.status.code(), Some(1));
    let told = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        told.contains("acme/top depends on acme/base >=2.0.0, <3.0.0"),
        "{told}"
    );
    // A failure after a package stored: a plugin of the list that the
    // lockfile lacks, stored first, and one whose package does not pack.
    let before = sandbox.store().digests().unwrap();
    let lock_before = sandbox.locked();
    let early = made(&sandbox, "aaa");
    let list = sandbox.config().join("plugins.toml");
    edit(&sandbox.config(), "plugins.toml", |text| {
        format!(
            "{text}\"acme/aaa\" = {{ path = \"{}\" }}\n",
            early.display()
        )
    });
    let list_before = std::fs::read_to_string(&list).unwrap();
    let broken = made(&sandbox, "zzz");
    std::fs::remove_file(broken.join("src/init.luau")).unwrap();
    let output = sandbox.sleip(&["add", "--path", &broken.display().to_string()]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("`acme/zzz`'s package"));
    assert_eq!(sandbox.store().digests().unwrap(), before);
    assert_eq!(sandbox.locked(), lock_before);
    assert_eq!(std::fs::read_to_string(&list).unwrap(), list_before);
    let leftovers = std::fs::read_dir(sandbox.store().root())
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn a_failed_update_keeps_the_packages_that_the_lockfile_still_names() {
    let sandbox = Sandbox::new("update-kept");
    // Two plugins of folders, each of a README and tests beside their
    // package, and one of a link outside its package's layout, as a
    // folder of tools that links elsewhere, which installs.
    let kept = made(&sandbox, "aaa");
    #[cfg(unix)]
    std::os::unix::fs::symlink(sandbox.home.join("work"), kept.join("node_modules")).unwrap();
    let failing = made(&sandbox, "zzz");
    for folder in [&kept, &failing] {
        let output = sandbox.sleip(&["add", "--path", &folder.display().to_string()]);
        assert!(output.status.success(), "{output:?}");
    }
    let lock_before = sandbox.locked();
    let digests_before = sandbox.store().digests().unwrap();
    assert_eq!(digests_before.len(), 2);
    // The update of both, the second's package of no entry: the first's,
    // unchanged, found in the store, stays there.
    std::fs::remove_file(failing.join("src/init.luau")).unwrap();
    let output = sandbox.sleip(&["update"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("`acme/zzz`'s package"));
    assert_eq!(sandbox.locked(), lock_before);
    assert_eq!(sandbox.store().digests().unwrap(), digests_before);
    let doctor = sandbox.sleip(&["doctor"]);
    assert!(doctor.status.success(), "{}", stdout(&doctor));
    // A new version of the first, which the store lacks, beside the
    // second that fails: the new package goes, the old ones stay.
    versioned(&kept, "0.2.0");
    let output = sandbox.sleip(&["update"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(sandbox.locked(), lock_before);
    assert_eq!(sandbox.store().digests().unwrap(), digests_before);
    assert!(sandbox.sleip(&["doctor"]).status.success());
}
