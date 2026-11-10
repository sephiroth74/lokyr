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
    let at = Timestamp::parse("2026-11-10T12:00:00Z").unwrap();
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
    let clock = entry(
        "acme/clock",
        "1.0.0",
        "A clock in the status bar",
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
    let sleip = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args(args)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("SLEIP_TEST_ROOT_KEYS", root_key.public().to_string())
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap()
    };
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
