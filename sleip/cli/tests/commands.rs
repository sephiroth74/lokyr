//! Sleip's commands, run as a user runs them, on an installation of the
//! tests' own, in a home folder of its own: each of text and of JSON, and
//! two at once, which wait on the store's lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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
fn two_sleips_at_once_wait_on_the_store_s_lock() {
    let sandbox = Sandbox::new("lock");
    sandbox.lock(BTreeMap::new());
    // Another Sleip holds the lock, as this test does.
    let path = sandbox.store().root().with_file_name("store.lock");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let held = std::fs::File::create(&path).unwrap();
    held.lock().unwrap();
    let start = Instant::now();
    let mut waiting = sandbox
        .command(&["--json", "gc"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert!(waiting.try_wait().unwrap().is_none(), "it did not wait");
    drop(held);
    let output = waiting.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(start.elapsed() >= Duration::from_millis(400));
    let json = events(&output);
    assert_eq!(json[0]["event"], "waiting");
    assert_eq!(json.last().unwrap()["event"], "done");
    // Free, it takes the lock at once, of no wait.
    let free = sandbox.sleip(&["--json", "gc"]);
    assert_eq!(events(&free)[0]["event"], "done");
    let _: &Path = &path;
}
