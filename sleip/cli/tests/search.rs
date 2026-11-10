//! `sleip search` and `sleip info` of a registry served on the machine
//! itself, of an index signed by the test's keys, which the build for
//! debugging takes of `SLEIP_TEST_ROOT_KEYS`: each file checked, cached of
//! its ETag, a forged one and an older one refused, and the cache read
//! offline.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;
use sleip_index::signing::{self, Entry, Listed, SecretKey, Timestamp};

/// A registry served over HTTP on the machine, of the files of its
/// folder, each of an ETag of its bytes; how many times it answered "not
/// modified".
struct Served {
    folder: PathBuf,
    port: u16,
    not_modified: Arc<AtomicUsize>,
}

/// Serves `folder` on a port of the machine's own, until the test ends.
fn serve(folder: &Path) -> Served {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let not_modified = Arc::new(AtomicUsize::new(0));
    let (root, counted) = (folder.to_path_buf(), Arc::clone(&not_modified));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            if reader.read_line(&mut first).is_err() {
                continue;
            }
            let mut asked_etag = None;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("if-none-match")
                {
                    asked_etag = Some(value.trim().to_owned());
                }
            }
            let path = first
                .split(' ')
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/');
            let mut stream = stream;
            let file = root.join(path);
            let answer = match std::fs::read(&file) {
                Ok(bytes) if !path.contains("..") => {
                    use std::hash::{DefaultHasher, Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    bytes.hash(&mut hasher);
                    let etag = format!("\"{:x}\"", hasher.finish());
                    if asked_etag.as_deref() == Some(etag.as_str()) {
                        counted.fetch_add(1, Ordering::Relaxed);
                        format!(
                            "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nConnection: close\r\n\r\n"
                        )
                        .into_bytes()
                    } else {
                        let mut answer = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: {etag}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        )
                        .into_bytes();
                        answer.extend(bytes);
                        answer
                    }
                }
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            };
            let _ = stream.write_all(&answer);
        }
    });
    Served {
        folder: folder.to_path_buf(),
        port,
        not_modified,
    }
}

/// A version of `name`, of `vers`, its description and capabilities.
fn entry(name: &str, vers: &str, description: &str, caps: &[&str], yanked: bool) -> Entry {
    Entry {
        v: signing::VERSION,
        name: name.to_owned(),
        vers: vers.to_owned(),
        kind: "luau".to_owned(),
        api: Some("^0.1".to_owned()),
        deps: Vec::new(),
        caps: caps.iter().map(|cap| (*cap).to_owned()).collect(),
        uses: Vec::new(),
        cksum: format!("sha256:{}", "b".repeat(64)),
        yanked,
        description: Some(description.to_owned()),
    }
}

/// The plugin of `entry`, as the list holds it.
fn listed(entry: &Entry) -> Listed {
    Listed {
        v: signing::VERSION,
        name: entry.name.clone(),
        vers: entry.vers.clone(),
        description: entry.description.clone(),
        caps: entry.caps.clone(),
        yanked: entry.yanked,
    }
}

/// The events of NDJSON of `output`.
fn events(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn search_and_info_read_the_registry_s_signed_index_cached_and_offline() {
    let root = std::env::temp_dir().join(format!("sleip-search-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (home, site) = (root.join("home"), root.join("site"));
    std::fs::create_dir_all(home.join(".config/lokyr")).unwrap();
    std::fs::create_dir_all(site.join("index/acme")).unwrap();
    // The registry: its keys, its list and an index file, signed.
    let (root_key, index_key) = (
        SecretKey::generate().unwrap(),
        SecretKey::generate().unwrap(),
    );
    // A few hours ago, as the registry signs its index every few hours.
    let at = Timestamp(Timestamp::now().0 - 4 * 3600);
    std::fs::write(
        site.join("keys.json"),
        signing::sign_keys(&root_key, &[index_key.public()], at),
    )
    .unwrap();
    std::fs::write(
        site.join("config.json"),
        "{\"v\":1,\"dl\":\"http://x/{name}\"}\n",
    )
    .unwrap();
    let hello = [
        entry("acme/hello", "0.1.0", "Says hello in a notice", &[], true),
        entry(
            "acme/hello",
            "0.2.0",
            "Says hello in a notice",
            &["clipboard-write"],
            false,
        ),
    ];
    // Its description of an escape that would clear the line before it,
    // as an index that no registry of ours made could hold.
    let clock = entry(
        "acme/clock",
        "1.0.0",
        "A clock in the status bar\u{1b}[2K\u{202E}",
        &["pane-view"],
        false,
    );
    let list = [listed(&clock), listed(&hello[1])];
    std::fs::write(site.join("list"), signing::sign_list(&list, &index_key, at)).unwrap();
    std::fs::write(
        site.join("index/acme/hello"),
        signing::sign_index("acme/hello", &hello, &index_key, at),
    )
    .unwrap();
    let served = serve(&site);
    std::fs::write(
        home.join(".config/lokyr/plugins.toml"),
        format!(
            "[registries]\ndefault = \"http://127.0.0.1:{}\"\n",
            served.port
        ),
    )
    .unwrap();
    // Sleip of `args`, of the clock of `now` if given, as a build for
    // debugging takes it.
    let sleip_at = |now: Option<Timestamp>, args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sleip"));
        command
            .args(args)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("SLEIP_TEST_ROOT_KEYS", root_key.public().to_string())
            .env("PATH", "/usr/bin:/bin");
        if let Some(now) = now {
            command.env("SLEIP_TEST_NOW", now.to_string());
        }
        command.output().unwrap()
    };
    let sleip = |args: &[&str]| sleip_at(None, args);
    let found = |output: &Output| -> Vec<String> {
        events(output)
            .iter()
            .filter(|event| event["event"] == "found")
            .map(|event| event["name"].as_str().unwrap().to_owned())
            .collect()
    };
    // Search, of the names and descriptions, every word.
    let output = sleip(&["--json", "search", "hello"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(found(&output), ["acme/hello"]);
    let output = sleip(&["--json", "search", "STATUS", "clock"]);
    assert_eq!(found(&output), ["acme/clock"]);
    // At a terminal, as text.
    let text = String::from_utf8_lossy(&sleip(&["search", "clock"]).stdout).into_owned();
    assert!(
        text.contains("A clock in the status bar\u{FFFD}[2K\u{FFFD}"),
        "{text:?}"
    );
    assert!(
        !text.contains('\u{1b}') && !text.contains('\u{202E}'),
        "{text:?}"
    );
    assert_eq!(
        found(&sleip(&["--json", "search", "nothing"])),
        Vec::<String>::new()
    );
    // Its second request of a file asks of it as not modified.
    assert!(
        served.not_modified.load(Ordering::Relaxed) >= 2,
        "{}",
        served.not_modified.load(Ordering::Relaxed)
    );
    // A plugin told, its versions newest first.
    let output = sleip(&["--json", "info", "acme/hello"]);
    assert!(output.status.success(), "{output:?}");
    let info = &events(&output)[0];
    assert_eq!(info["version"], "0.2.0");
    assert_eq!(info["capabilities"][0], "clipboard-write");
    assert_eq!(info["versions"][0]["version"], "0.2.0");
    assert_eq!(info["versions"][1]["yanked"], true);
    let text = String::from_utf8_lossy(&sleip(&["info", "acme/hello"]).stdout).into_owned();
    assert!(
        text.starts_with("acme/hello 0.2.0\nSays hello in a notice\n"),
        "{text}"
    );
    let output = sleip(&["info", "acme/none"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    // A forged list, its description changed after its signature: refused,
    // the cache holding the one checked.
    let original = std::fs::read_to_string(site.join("list")).unwrap();
    std::fs::write(site.join("list"), original.replace("A clock", "A trojan")).unwrap();
    let output = sleip(&["--json", "search", "clock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("not as its signature says"));
    // One older than the one held, of a valid signature: refused.
    let older = Timestamp(at.0 - 3600);
    std::fs::write(
        served.folder.join("list"),
        signing::sign_list(&list, &index_key, older),
    )
    .unwrap();
    let output = sleip(&["--json", "search", "clock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("before the one that this Sleip holds"),
        "{output:?}"
    );
    // Offline, of the cache alone, as it was checked; a plugin never
    // fetched it cannot tell.
    let output = sleip(&["--json", "--offline", "search", "clock"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(found(&output), ["acme/clock"]);
    assert!(sleip(&["--offline", "info", "acme/hello"]).status.success());
    let output = sleip(&["--offline", "info", "acme/clock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("the cache holds no"),
        "{output:?}"
    );
    // A week and a day later, the list that the registry serves still the
    // one held: expired online, refused; offline, taken with a note.
    std::fs::write(site.join("list"), &original).unwrap();
    let later = Some(Timestamp(at.0 + 8 * 86_400));
    let output = sleip_at(later, &["search", "clock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("`list` of http://127.0.0.1"),
        "{output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("expired at"),
        "{output:?}"
    );
    let output = sleip_at(later, &["--offline", "search", "clock"]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        text.contains("Note: `list` of") && text.contains("expired at"),
        "{text}"
    );
    // A file of a time to come, as of a clock two days behind: refused.
    let output = sleip_at(Some(Timestamp(at.0 - 2 * 86_400)), &["search", "clock"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("a time to come"),
        "{output:?}"
    );
    // A keys.json of a valid signature made before the one that this Sleip
    // was built with, of a cache of none yet: refused.
    let keys = std::fs::read_to_string(site.join("keys.json")).unwrap();
    std::fs::write(
        site.join("keys.json"),
        signing::sign_keys(
            &root_key,
            &[index_key.public()],
            Timestamp::parse("2026-01-01T00:00:00Z").unwrap(),
        ),
    )
    .unwrap();
    let fresh_home = root.join("fresh-home");
    std::fs::create_dir_all(fresh_home.join(".config/lokyr")).unwrap();
    std::fs::copy(
        home.join(".config/lokyr/plugins.toml"),
        fresh_home.join(".config/lokyr/plugins.toml"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sleip"))
        .args(["search", "clock"])
        .env("HOME", &fresh_home)
        .env("XDG_CONFIG_HOME", fresh_home.join(".config"))
        .env("XDG_CACHE_HOME", fresh_home.join(".cache"))
        .env("XDG_DATA_HOME", fresh_home.join(".local/share"))
        .env("SLEIP_TEST_ROOT_KEYS", root_key.public().to_string())
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("the time of the one that this Sleip was built with"),
        "{output:?}"
    );
    std::fs::write(site.join("keys.json"), keys).unwrap();
    // A registry's address of a user, its host the one after its `@`.
    let other_home = root.join("other-home");
    std::fs::create_dir_all(other_home.join(".config/lokyr")).unwrap();
    std::fs::write(
        other_home.join(".config/lokyr/plugins.toml"),
        format!(
            "[registries]\ndefault = \"http://127.0.0.1:{}@evil.example\"\n",
            served.port
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sleip"))
        .args(["search", "clock"])
        .env("HOME", &other_home)
        .env("XDG_CONFIG_HOME", other_home.join(".config"))
        .env("XDG_CACHE_HOME", other_home.join(".cache"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("evil.example"),
        "{output:?}"
    );
    // Of another root than the one that signed keys.json: refused.
    let output = Command::new(env!("CARGO_BIN_EXE_sleip"))
        .args(["search", "clock"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", root.join("other-cache"))
        .env(
            "SLEIP_TEST_ROOT_KEYS",
            SecretKey::generate().unwrap().public().to_string(),
        )
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("signed by no root key"),
        "{output:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The package of a plugin `acme/hello` of `version`, as `sleip new` makes
/// it, packed: its archive's bytes.
fn packed(root: &Path, version: &str) -> Vec<u8> {
    let work = root.join(format!("work-{version}"));
    let folder = work.join("hello");
    if !folder.is_dir() {
        std::fs::create_dir_all(&work).unwrap();
        let made = Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args(["new", "acme/hello"])
            .current_dir(&work)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
    }
    let manifest = std::fs::read_to_string(folder.join("plugin.toml")).unwrap();
    std::fs::write(
        folder.join("plugin.toml"),
        manifest.replace("version = \"0.1.0\"", &format!("version = \"{version}\"")),
    )
    .unwrap();
    lokyr_pkg_model::archive::pack(&folder).unwrap()
}

#[test]
fn add_and_update_install_the_registry_s_versions_checked_against_its_index() {
    use lokyr_pkg_model::store::Digest;

    let root = std::env::temp_dir().join(format!("sleip-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (home, site) = (root.join("home"), root.join("site"));
    std::fs::create_dir_all(home.join(".config/lokyr")).unwrap();
    std::fs::create_dir_all(site.join("index/acme")).unwrap();
    std::fs::create_dir_all(site.join("archives")).unwrap();
    std::fs::write(home.join("api-version"), "0.1.0\n").unwrap();
    let served = serve(&site);
    let (root_key, index_key) = (
        SecretKey::generate().unwrap(),
        SecretKey::generate().unwrap(),
    );
    // A few hours ago, as the registry signs its index every few hours.
    let at = Timestamp(Timestamp::now().0 - 4 * 3600);
    std::fs::write(
        site.join("keys.json"),
        signing::sign_keys(&root_key, &[index_key.public()], at),
    )
    .unwrap();
    std::fs::write(
        site.join("config.json"),
        format!(
            "{{\"v\":1,\"dl\":\"http://127.0.0.1:{}/archives/{{owner}}.{{name}}.{{version}}.tar.zst\"}}\n",
            served.port
        ),
    )
    .unwrap();
    // The registry of the versions `versions`, each of its archive, its
    // index and its list signed at `at`.
    let publish = |versions: &[(&str, bool)], at: Timestamp| {
        let mut entries = Vec::new();
        for (version, yanked) in versions {
            let archive = packed(&root, version);
            std::fs::write(
                site.join(format!("archives/acme.hello.{version}.tar.zst")),
                &archive,
            )
            .unwrap();
            let mut entry = entry("acme/hello", version, "Says hello", &[], *yanked);
            entry.cksum = Digest::of_bytes(&archive).to_string();
            entries.push(entry);
        }
        std::fs::write(
            site.join("index/acme/hello"),
            signing::sign_index("acme/hello", &entries, &index_key, at),
        )
        .unwrap();
        let shown = entries.iter().rev().find(|entry| !entry.yanked).unwrap();
        std::fs::write(
            site.join("list"),
            signing::sign_list(&[listed(shown)], &index_key, at),
        )
        .unwrap();
    };
    publish(&[("0.1.0", false), ("0.2.0", false)], at);
    std::fs::write(
        home.join(".config/lokyr/plugins.toml"),
        format!(
            "[registries]\ndefault = \"http://127.0.0.1:{}\"\n",
            served.port
        ),
    )
    .unwrap();
    let sleip = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args(args)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("SLEIP_TEST_ROOT_KEYS", root_key.public().to_string())
            .env("SLEIP_LOKYR", home.join("api-version"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap()
    };
    let locked = || {
        lokyr_pkg_model::lock::Lock::read(&home.join(".config/lokyr/plugins.lock"))
            .unwrap()
            .unwrap()
    };
    let list = || std::fs::read_to_string(home.join(".config/lokyr/plugins.toml")).unwrap();
    // Of a constraint given: the newest version that it takes, written as
    // given, the capabilities told.
    let output = sleip(&["--json", "add", "acme/hello@^0.1"]);
    assert!(output.status.success(), "{output:?}");
    let installed = events(&output)
        .into_iter()
        .find(|event| event["event"] == "installed")
        .unwrap();
    assert_eq!(installed["version"], "0.1.0");
    assert_eq!(
        installed["source"],
        format!("registry+http://127.0.0.1:{}", served.port)
    );
    assert!(list().contains("\"acme/hello\" = \"^0.1\""), "{}", list());
    let entry = locked().packages["acme/hello"].clone();
    assert_eq!(entry.version, "0.1.0");
    let store = lokyr_pkg_model::store::Store::of_user(&home, Some(&home.join(".local/share")));
    store.check(&entry.checksum.unwrap()).unwrap();
    assert!(sleip(&["doctor"]).status.success());
    // A version within the constraint, published since: update takes it.
    publish(
        &[("0.1.0", false), ("0.1.1", false), ("0.2.0", false)],
        Timestamp(at.0 + 60),
    );
    let output = sleip(&["update"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(locked().packages["acme/hello"].version, "0.1.1");
    // An archive of another hash than its index's: refused, the lockfile
    // as it was.
    publish(
        &[
            ("0.1.0", false),
            ("0.1.1", false),
            ("0.1.2", false),
            ("0.2.0", false),
        ],
        Timestamp(at.0 + 120),
    );
    let archive = site.join("archives/acme.hello.0.1.2.tar.zst");
    let mut bytes = std::fs::read(&archive).unwrap();
    bytes.extend(b"tampered");
    std::fs::write(&archive, bytes).unwrap();
    let output = sleip(&["update"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("which the registry's signed index gives"),
        "{output:?}"
    );
    assert_eq!(locked().packages["acme/hello"].version, "0.1.1");
    // A plugin that it does not publish.
    let output = sleip(&["add", "acme/none"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("publishes no plugin `acme/none`"));
    // A config.json, which no key signs, that names archives of another
    // scheme than the registry's, as a device of the machine's: refused,
    // nothing installed.
    let config = std::fs::read_to_string(site.join("config.json")).unwrap();
    std::fs::write(
        site.join("config.json"),
        "{\"v\":1,\"dl\":\"file:///dev/zero\"}\n",
    )
    .unwrap();
    let output = sleip(&["update"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not of the registry's own scheme"),
        "{output:?}"
    );
    assert_eq!(locked().packages["acme/hello"].version, "0.1.1");
    std::fs::write(site.join("config.json"), config).unwrap();
    // Of no constraint: the newest that the plugin API takes, written as
    // `^<version>`.
    publish(
        &[("0.1.0", false), ("0.1.1", false), ("0.2.0", false)],
        Timestamp(at.0 + 180),
    );
    std::fs::remove_file(home.join(".config/lokyr/plugins.lock")).unwrap();
    std::fs::write(
        home.join(".config/lokyr/plugins.toml"),
        format!(
            "[registries]\ndefault = \"http://127.0.0.1:{}\"\n",
            served.port
        ),
    )
    .unwrap();
    let output = sleip(&["add", "acme/hello"]);
    assert!(output.status.success(), "{output:?}");
    assert!(list().contains("\"acme/hello\" = \"^0.2.0\""), "{}", list());
    assert_eq!(locked().packages["acme/hello"].version, "0.2.0");
    // Offline, of the cache of the index and of the archives: as it was.
    let output = sleip(&["--offline", "update"]);
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Nothing changed."),
        "{output:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The archive of `name` `version`, a plugin that `sleip new` makes, of the
/// dependencies `deps`, each its name and constraint.
fn packed_of(root: &Path, name: &str, version: &str, deps: &[(&str, &str)]) -> Vec<u8> {
    let short = name.split_once('/').unwrap().1;
    let work = root.join(format!("work-{short}-{version}"));
    let folder = work.join(short);
    // Made anew, as alike as the first time.
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let made = Command::new(env!("CARGO_BIN_EXE_sleip"))
        .args(["new", name])
        .current_dir(&work)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
    let mut manifest = std::fs::read_to_string(folder.join("plugin.toml"))
        .unwrap()
        .replace("version = \"0.1.0\"", &format!("version = \"{version}\""));
    if !deps.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for (needed, req) in deps {
            manifest.push_str(&format!("\"{needed}\" = \"{req}\"\n"));
        }
    }
    std::fs::write(folder.join("plugin.toml"), manifest).unwrap();
    lokyr_pkg_model::archive::pack(&folder).unwrap()
}

/// A plugin's versions, each its version, its dependencies, each its name
/// and constraint, and whether it is yanked.
type Versions<'a> = &'a [(&'a str, &'a [(&'a str, &'a str)], bool)];

/// The folder of the archives that Sleip of `home` keeps.
fn archives_of(home: &Path) -> Option<PathBuf> {
    let mut left = vec![home.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.ends_with("sleip/archives") {
                    return Some(path);
                }
                left.push(path);
            }
        }
    }
    None
}

#[test]
fn installs_keep_the_locked_versions_and_reproduce_a_lockfile() {
    use lokyr_pkg_model::lock::Lock;
    use lokyr_pkg_model::store::{Digest, Store};
    use sleip_index::signing::Dependency;

    let root = std::env::temp_dir().join(format!("sleip-locked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let site = root.join("site");
    std::fs::create_dir_all(site.join("index/acme")).unwrap();
    std::fs::create_dir_all(site.join("archives")).unwrap();
    let served = serve(&site);
    let (root_key, index_key) = (
        SecretKey::generate().unwrap(),
        SecretKey::generate().unwrap(),
    );
    let at = Timestamp(Timestamp::now().0 - 4 * 3600);
    std::fs::write(
        site.join("keys.json"),
        signing::sign_keys(&root_key, &[index_key.public()], at),
    )
    .unwrap();
    std::fs::write(
        site.join("config.json"),
        format!(
            "{{\"v\":1,\"dl\":\"http://127.0.0.1:{}/archives/{{owner}}.{{name}}.{{version}}.tar.zst\"}}\n",
            served.port
        ),
    )
    .unwrap();
    // The registry of `plugins`, each its name and its versions, of their
    // dependencies and yanks; a plugin's index and the list signed at a
    // later time at each publishing.
    let times = AtomicUsize::new(0);
    let publish = |plugins: &[(&str, Versions)]| {
        let at = Timestamp(at.0 + 60 * times.fetch_add(1, Ordering::Relaxed) as u64);
        let mut listed_all = Vec::new();
        for (name, versions) in plugins {
            let mut entries = Vec::new();
            for (version, deps, yanked) in *versions {
                let archive = packed_of(&root, name, version, deps);
                let short = name.split_once('/').unwrap().1;
                std::fs::write(
                    site.join(format!("archives/acme.{short}.{version}.tar.zst")),
                    &archive,
                )
                .unwrap();
                let mut entry = entry(name, version, "A plugin", &[], *yanked);
                entry.cksum = Digest::of_bytes(&archive).to_string();
                entry.deps = deps
                    .iter()
                    .map(|(needed, req)| Dependency {
                        name: (*needed).to_owned(),
                        req: (*req).to_owned(),
                    })
                    .collect();
                entries.push(entry);
            }
            std::fs::write(
                site.join(format!("index/{name}")),
                signing::sign_index(name, &entries, &index_key, at),
            )
            .unwrap();
            listed_all.push(listed(entries.last().unwrap()));
        }
        std::fs::write(
            site.join("list"),
            signing::sign_list(&listed_all, &index_key, at),
        )
        .unwrap();
    };
    let base: Versions = &[("0.1.0", &[], false)];
    let needs_base: &[(&str, &str)] = &[("acme/base", "^0.1")];
    let app: Versions = &[("1.0.0", needs_base, false)];
    let other: Versions = &[("1.0.0", needs_base, false)];
    // Its yanked version of a dependency that the registry does not
    // publish.
    let hello: Versions = &[
        ("0.1.0", &[("acme/gone", "^1")], true),
        ("0.2.0", &[], false),
    ];
    publish(&[
        ("acme/base", base),
        ("acme/app", app),
        ("acme/other", other),
        ("acme/hello", hello),
    ]);
    // A home of the registry, of Lokyr's plugin API.
    let home_of = |name: &str| {
        let home = root.join(name);
        std::fs::create_dir_all(home.join(".config/lokyr")).unwrap();
        std::fs::write(home.join("api-version"), "0.1.0\n").unwrap();
        std::fs::write(
            home.join(".config/lokyr/plugins.toml"),
            format!(
                "[registries]\ndefault = \"http://127.0.0.1:{}\"\n",
                served.port
            ),
        )
        .unwrap();
        home
    };
    let sleip = |home: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args(args)
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("SLEIP_TEST_ROOT_KEYS", root_key.public().to_string())
            .env("SLEIP_LOKYR", home.join("api-version"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap()
    };
    let locked = |home: &Path| {
        Lock::read(&home.join(".config/lokyr/plugins.lock"))
            .unwrap()
            .unwrap()
    };
    let home = home_of("home");
    let output = sleip(&home, &["add", "acme/app"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(locked(&home).packages["acme/base"].version, "0.1.0");
    // base 0.1.1 published: a plugin added that depends on it too keeps
    // the version locked.
    let base: Versions = &[("0.1.0", &[], false), ("0.1.1", &[], false)];
    publish(&[
        ("acme/base", base),
        ("acme/app", app),
        ("acme/other", other),
        ("acme/hello", hello),
    ]);
    let output = sleip(&home, &["add", "acme/other"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(locked(&home).packages["acme/base"].version, "0.1.0");
    assert_eq!(locked(&home).packages["acme/other"].version, "1.0.0");
    // A plugin whose yanked version needs one that the registry does not
    // publish: its version not yanked installs.
    let output = sleip(&home, &["add", "acme/hello"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(locked(&home).packages["acme/hello"].version, "0.2.0");
    // Offline, of no archive cached, a plugin updated that changes nothing
    // downloads nothing.
    let archives = archives_of(&home).expect("Sleip's archives");
    std::fs::remove_dir_all(&archives).unwrap();
    let output = sleip(&home, &["--offline", "update", "acme/app"]);
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Nothing changed."),
        "{output:?}"
    );
    // The lockfile and the list copied to a machine of an empty store:
    // `update --locked` fetches each package again, of the lockfile's
    // digests, the lockfile as it was.
    let copy = home_of("copy");
    for file in ["plugins.toml", "plugins.lock"] {
        std::fs::copy(
            home.join(".config/lokyr").join(file),
            copy.join(".config/lokyr").join(file),
        )
        .unwrap();
    }
    let output = sleip(&copy, &["--json", "update", "--locked"]);
    assert!(output.status.success(), "{output:?}");
    let restored: Vec<String> = events(&output)
        .iter()
        .filter(|event| event["event"] == "restored")
        .map(|event| event["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        restored,
        ["acme/app", "acme/base", "acme/hello", "acme/other"],
        "{output:?}"
    );
    assert_eq!(locked(&copy), locked(&home));
    let store = Store::of_user(&copy, Some(&copy.join(".local/share")));
    for package in locked(&copy).packages.values() {
        assert!(store.folder(package.checksum.as_ref().unwrap()).is_dir());
    }
    // A lockfile of another digest than the registry's archive: refused,
    // nothing written.
    let forged = home_of("forged");
    std::fs::copy(
        home.join(".config/lokyr/plugins.toml"),
        forged.join(".config/lokyr/plugins.toml"),
    )
    .unwrap();
    let mut lock = locked(&home);
    let base_digest = lock.packages["acme/base"].checksum.unwrap();
    let other_digest = lock.packages["acme/app"].checksum.unwrap();
    lock.packages.get_mut("acme/base").unwrap().checksum = Some(other_digest);
    lock.write(&forged.join(".config/lokyr/plugins.lock"))
        .unwrap();
    let output = sleip(&forged, &["update", "--locked"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("is not the one locked"),
        "{output:?}"
    );
    assert_eq!(locked(&forged), lock);
    assert_ne!(base_digest, other_digest);
    // A lockfile that cannot be written, of a link into a folder of no
    // writing: `plugins.toml` put back as it was, nothing installed.
    let stuck = home_of("stuck");
    let output = sleip(&stuck, &["add", "acme/app"]);
    assert!(output.status.success(), "{output:?}");
    let config = stuck.join(".config/lokyr");
    let shut = root.join("shut");
    std::fs::create_dir_all(&shut).unwrap();
    std::fs::rename(config.join("plugins.lock"), shut.join("plugins.lock")).unwrap();
    std::os::unix::fs::symlink(shut.join("plugins.lock"), config.join("plugins.lock")).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o555)).unwrap();
    }
    // Of an account that writes there all the same, as root: not told.
    if std::fs::write(shut.join("probe"), "").is_err() {
        let before = std::fs::read_to_string(config.join("plugins.toml")).unwrap();
        let output = sleip(&stuck, &["add", "acme/other"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("plugins.lock"),
            "{output:?}"
        );
        assert_eq!(
            std::fs::read_to_string(config.join("plugins.toml")).unwrap(),
            before
        );
        assert!(!locked(&stuck).packages.contains_key("acme/other"));
    }
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // `update` of every plugin moves the dependencies too.
    let output = sleip(&home, &["update"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(locked(&home).packages["acme/base"].version, "0.1.1");
    let _ = std::fs::remove_dir_all(&root);
}
