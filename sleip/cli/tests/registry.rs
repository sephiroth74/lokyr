//! `sleip registry`, the registry's tools, as its CI runs them: a pull
//! request's changes, since the commit that it shares with the branch that
//! it merges into, checked against that branch as it is now, each a git
//! repository, of plugins' repositories of the tests, of `file://`; each
//! version packaged alike again; every version listed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A folder of a test's own, of its home, its plugins' repositories and
/// its registries.
struct Lab {
    root: PathBuf,
}

impl Lab {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("sleip-registry-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        Self { root }
    }

    /// Sleip of `args`, in `folder`, of the lab's home.
    fn sleip(&self, args: &[&str], folder: &Path) -> Output {
        let home = self.root.join("home");
        Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args(args)
            .current_dir(folder)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap()
    }

    /// The repository of the plugin `acme/<name>` that `sleip new` makes,
    /// its first commit tagged `v0.1.0`: its folder and its address.
    fn plugin(&self, name: &str) -> (PathBuf, String) {
        let work = self.root.join("plugins");
        std::fs::create_dir_all(&work).unwrap();
        let made = self.sleip(&["new", &format!("acme/{name}")], &work);
        assert!(made.status.success(), "{made:?}");
        let folder = work.join(name);
        git(&folder, &["init", "--quiet"]);
        git(&folder, &["add", "."]);
        git(&folder, &["commit", "--quiet", "-m", "first"]);
        git(&folder, &["tag", "v0.1.0"]);
        let url = format!("file://{}", folder.display());
        (folder, url)
    }

    /// A registry of the folder `name`, a git repository of a commit of a
    /// README and of `files`, each its path and its text.
    fn registry(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let folder = self.root.join(name);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        git(&folder, &["init", "--quiet", "-b", "main"]);
        written(&folder, files);
        commit(&folder, "main");
        folder
    }

    /// The pull request of `files`, each its path and its text, beside the
    /// registry's README, against the registry `base`: a clone of it in the
    /// folder `head`, its tree replaced by them, of a commit of its own,
    /// empty if they change nothing.
    fn head(&self, base: &Path, files: &[(&str, &str)]) -> PathBuf {
        let folder = self.branch(base);
        git(&folder, &["rm", "-r", "--quiet", "."]);
        written(&folder, files);
        commit(&folder, "pull request");
        folder
    }

    /// A branch of the registry `base`, a clone of it in the folder `head`,
    /// as it is.
    fn branch(&self, base: &Path) -> PathBuf {
        let folder = self.root.join("head");
        let _ = std::fs::remove_dir_all(&folder);
        git(
            &self.root,
            &[
                "clone",
                "--quiet",
                &base.display().to_string(),
                &folder.display().to_string(),
            ],
        );
        folder
    }

    /// `verify` of the pull request `head` against `base`, of `author` and
    /// its organizations `orgs`: its output and its verdict's event.
    fn verify(&self, head: &Path, base: &Path, author: &str, orgs: &str) -> (Output, Value) {
        let out = self.root.join("dist");
        let _ = std::fs::remove_dir_all(&out);
        let output = self.sleip(
            &[
                "--json",
                "registry",
                "verify",
                &head.display().to_string(),
                "--base",
                &base.display().to_string(),
                "--author",
                author,
                "--orgs",
                orgs,
                "--out",
                &out.display().to_string(),
                "--report",
                &self.root.join("report.md").display().to_string(),
                "--local",
            ],
            &self.root,
        );
        let verdict = events(&output)
            .into_iter()
            .find(|event| event["event"] == "verdict")
            .unwrap_or_else(|| panic!("no verdict: {output:?}"));
        (output, verdict)
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Runs git of `args` in `folder`, of an author of the tests: its output.
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

/// Writes the registry's README and `files`, each its path and its text,
/// into `folder`.
fn written(folder: &Path, files: &[(&str, &str)]) {
    std::fs::write(folder.join("README.md"), "The registry of the tests.\n").unwrap();
    for (path, text) in files {
        let file = folder.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
}

/// Commits every change of the repository `folder`, of the message
/// `message`, empty if there is none.
fn commit(folder: &Path, message: &str) {
    git(folder, &["add", "-A"]);
    git(
        folder,
        &["commit", "--quiet", "--allow-empty", "-m", message],
    );
}

/// The events of NDJSON of `output`.
fn events(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// A plugin's file of the registry, of `url` and of `versions`, each its
/// version, tag, commit and whether it is yanked.
fn entry(url: &str, versions: &[(&str, &str, &str, bool)]) -> String {
    let mut text = format!("repository = \"{url}\"\n");
    for (version, tag, commit, yanked) in versions {
        text.push_str(&format!(
            "\n[[versions]]\nversion = \"{version}\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\n"
        ));
        if *yanked {
            text.push_str("yanked = true\n");
        }
    }
    text
}

/// The texts of the verdict's list `list`, joined.
fn reasons(verdict: &Value, list: &str) -> String {
    verdict[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|reason| reason.as_str().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Edits the file `path` of `folder`, its text of `edit`.
fn edit(folder: &Path, path: &str, edit: impl FnOnce(String) -> String) {
    let file = folder.join(path);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, edit(text)).unwrap();
}

/// Commits the plugin of `folder` at the version `version`, tagged
/// `v<version>`: the commit's hash.
fn release(folder: &Path, version: &str) -> String {
    edit(folder, "plugin.toml", |text| {
        let at = text.find("version = \"").unwrap() + "version = \"".len();
        let end = at + text[at..].find('"').unwrap();
        format!("{}{version}{}", &text[..at], &text[end..])
    });
    git(folder, &["commit", "--quiet", "-am", version]);
    git(folder, &["tag", &format!("v{version}")]);
    git(folder, &["rev-parse", "HEAD"])
}

#[test]
fn a_new_plugin_of_its_owner_passes_for_the_maintainer_s_review_its_archive_packaged_alike_again() {
    let lab = Lab::new("new");
    let (folder, url) = lab.plugin("hello");
    let commit = git(&folder, &["rev-parse", "HEAD"]);
    let file = entry(&url, &[("0.1.0", "v0.1.0", &commit, false)]);
    let base = lab.registry("base", &[]);
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &file)]);
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(reasons(&verdict, "review").contains("`acme/hello` is a new plugin"));
    let built = events(&output)
        .into_iter()
        .find(|event| event["event"] == "built")
        .unwrap();
    assert_eq!(built["release"], "acme.hello.0.1.0");
    let archive = PathBuf::from(built["archive"].as_str().unwrap());
    assert!(
        archive.ends_with("acme.hello.0.1.0/hello-0.1.0.tar.zst"),
        "{archive:?}"
    );
    let report = std::fs::read_to_string(lab.root.join("report.md")).unwrap();
    assert!(
        report.starts_with("### Sleip's check: For the maintainer's review"),
        "{report}"
    );
    assert!(
        report.contains("| `acme/hello` | 0.1.0 | `acme.hello.0.1.0` |"),
        "{report}"
    );
    // Its owner's organization opens it alike.
    let (output, verdict) = lab.verify(&head, &base, "Ann", "tools,ACME");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "review");
    // The main branch's CI packages it again of the same bytes.
    let first = std::fs::read(&archive).unwrap();
    let again = lab.root.join("again");
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "build",
            "acme/hello",
            "0.1.0",
            "--registry",
            &head.display().to_string(),
            "--out",
            &again.display().to_string(),
            "--local",
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        std::fs::read(again.join("acme.hello.0.1.0/hello-0.1.0.tar.zst")).unwrap(),
        first
    );
    // Every version, of its release's names.
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "versions",
            "--registry",
            &head.display().to_string(),
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    let listed = events(&output);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["name"], "acme/hello");
    assert_eq!(listed[0]["release"], "acme.hello.0.1.0");
    assert_eq!(listed[0]["archive"], "hello-0.1.0.tar.zst");
    assert_eq!(listed[0]["commit"], commit.as_str());
}

#[test]
fn a_version_that_asks_for_nothing_new_merges_by_itself_and_one_that_asks_more_is_reviewed() {
    let lab = Lab::new("versions");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let published = entry(&url, &[("0.1.0", "v0.1.0", &first, false)]);
    let base = lab.registry("base", &[("plugins/acme/hello.toml", &published)]);
    // A fix, of nothing new.
    let second = release(&folder, "0.1.1");
    let head = lab.head(
        &base,
        &[(
            "plugins/acme/hello.toml",
            &entry(
                &url,
                &[
                    ("0.1.0", "v0.1.0", &first, false),
                    ("0.1.1", "v0.1.1", &second, false),
                ],
            ),
        )],
    );
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "merge", "{verdict}");
    let report = std::fs::read_to_string(lab.root.join("report.md")).unwrap();
    assert!(report.contains("merges by itself"), "{report}");
    // A version that asks for a capability more, and runs a command.
    edit(&folder, "plugin.toml", |text| {
        format!(
            "{text}\n[capabilities]\nclipboard-write = true\n\n[uses]\ncommands = [\"tabs.*\"]\n"
        )
    });
    git(&folder, &["commit", "--quiet", "-am", "more"]);
    let third = release(&folder, "0.2.0");
    let head = lab.head(
        &base,
        &[(
            "plugins/acme/hello.toml",
            &entry(
                &url,
                &[
                    ("0.1.0", "v0.1.0", &first, false),
                    ("0.2.0", "v0.2.0", &third, false),
                ],
            ),
        )],
    );
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    let review = reasons(&verdict, "review");
    assert!(
        review.contains(
            "asks for more than 0.1.0 did: the capability `clipboard-write`, the command `tabs.*`"
        ),
        "{review}"
    );
    // A yank alone merges by itself; taking one back, the maintainer
    // reviews.
    let yanked = entry(&url, &[("0.1.0", "v0.1.0", &first, true)]);
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &yanked)]);
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "merge", "{verdict}");
    assert!(reasons(&verdict, "notes").contains("0.1.0 is yanked"));
    let yanked_base = lab.registry("yanked", &[("plugins/acme/hello.toml", &yanked)]);
    let head = lab.head(&yanked_base, &[("plugins/acme/hello.toml", &published)]);
    let (_, verdict) = lab.verify(&head, &yanked_base, "acme", "");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(reasons(&verdict, "review").contains("comes back from its yank"));
}

#[test]
fn each_fault_refuses_the_pull_request_with_its_reason() {
    let lab = Lab::new("faults");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let good = entry(&url, &[("0.1.0", "v0.1.0", &first, false)]);
    let empty = lab.registry("base", &[]);
    let refused = |head: &Path, base: &Path, author: &str, orgs: &str| {
        let (output, verdict) = lab.verify(head, base, author, orgs);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert_eq!(verdict["verdict"], "refused", "{verdict}");
        reasons(&verdict, "refused")
    };
    // Its author neither its owner nor of its organization.
    let head = lab.head(&empty, &[("plugins/acme/hello.toml", &good)]);
    let why = refused(&head, &empty, "bob", "tools");
    assert!(why.contains("`acme/hello` is `acme`'s, and `bob`"), "{why}");
    // The first-party plugins' owner.
    let head = lab.head(&empty, &[("plugins/lokyr/hello.toml", &good)]);
    assert!(refused(&head, &empty, "lokyr", "").contains("belongs to Lokyr's first-party plugins"));
    // A name of no plugin.
    let head = lab.head(&empty, &[("plugins/acme/Hello.toml", &good)]);
    assert!(refused(&head, &empty, "acme", "").contains("names no plugin"));
    // A name that another owner holds.
    let theirs = lab.registry("theirs", &[("plugins/bob/hello.toml", &good)]);
    let head = lab.head(
        &theirs,
        &[
            ("plugins/bob/hello.toml", &good),
            ("plugins/acme/hello.toml", &good),
        ],
    );
    let why = refused(&head, &theirs, "acme", "");
    assert!(why.contains("the name `hello` is `bob/hello`'s"), "{why}");
    // A tag that names another commit, and one that is not there.
    git(
        &folder,
        &["commit", "--quiet", "--allow-empty", "-m", "later"],
    );
    let later = git(&folder, &["rev-parse", "HEAD"]);
    let head = lab.head(
        &empty,
        &[(
            "plugins/acme/hello.toml",
            &entry(&url, &[("0.1.0", "v0.1.0", &later, false)]),
        )],
    );
    let why = refused(&head, &empty, "acme", "");
    assert!(
        why.contains(&format!("names the commit {first}, not {later}")),
        "{why}"
    );
    let head = lab.head(
        &empty,
        &[(
            "plugins/acme/hello.toml",
            &entry(&url, &[("0.1.0", "v9", &first, false)]),
        )],
    );
    assert!(refused(&head, &empty, "acme", "").contains("has no tag `v9`"));
    // A manifest of another name, and of another version.
    let head = lab.head(&empty, &[("plugins/acme/other.toml", &good)]);
    assert!(refused(&head, &empty, "acme", "").contains("its manifest names `acme/hello`"));
    let head = lab.head(
        &empty,
        &[(
            "plugins/acme/hello.toml",
            &entry(&url, &[("0.3.0", "v0.1.0", &first, false)]),
        )],
    );
    assert!(refused(&head, &empty, "acme", "").contains("its manifest's version is 0.1.0"));
    // A file that does not read, and a repository not of HTTPS outside the
    // tests.
    let head = lab.head(&empty, &[("plugins/acme/hello.toml", "repository = 1\n")]);
    assert!(refused(&head, &empty, "acme", "").contains("`plugins/acme/hello.toml`: line 1"));
    // A source that its check refuses, and a test that fails.
    edit(&folder, "src/init.luau", |text| {
        format!("{text}\nlocal wrong: number = \"text\"\n")
    });
    edit(&folder, "tests/broken.luau", |_| {
        "local test = {}\nfunction test.fails(host: TestHost)\n    assert(false, \"it fails\")\nend\nreturn test\n".to_owned()
    });
    git(&folder, &["add", "."]);
    git(&folder, &["commit", "--quiet", "-m", "broken"]);
    git(&folder, &["tag", "v0.1.0-broken"]);
    let broken = git(&folder, &["rev-parse", "HEAD"]);
    let head = lab.head(
        &empty,
        &[(
            "plugins/acme/hello.toml",
            &entry(&url, &[("0.1.0", "v0.1.0-broken", &broken, false)]),
        )],
    );
    let why = refused(&head, &empty, "acme", "");
    assert!(why.contains("check: src/init.luau:"), "{why}");
    assert!(
        why.contains("test: tests/broken.luau:3 fails: it fails"),
        "{why}"
    );
    // A published version that changes, and one that goes.
    let base = lab.registry("published", &[("plugins/acme/hello.toml", &good)]);
    let head = lab.head(
        &base,
        &[(
            "plugins/acme/hello.toml",
            &entry(&url, &[("0.1.0", "v0.1.0-broken", &broken, false)]),
        )],
    );
    assert!(refused(&head, &base, "acme", "").contains("0.1.0, published, changes or goes"));
    let head = lab.head(
        &base,
        &[(
            "plugins/acme/hello.toml",
            &entry(&url, &[("0.2.0", "v0.1.0", &first, false)]),
        )],
    );
    assert!(refused(&head, &base, "acme", "").contains("0.1.0, published, changes or goes"));
    // Nothing changed.
    let same = lab.head(&base, &[("plugins/acme/hello.toml", &good)]);
    assert!(refused(&same, &base, "acme", "").contains("changes no file"));
}

#[test]
fn a_file_outside_the_plugins_is_for_the_maintainer_and_https_is_needed_outside_the_tests() {
    let lab = Lab::new("outside");
    let base = lab.registry("base", &[]);
    let head = lab.head(&base, &[(".github/workflows/x.yml", "on: push\n")]);
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(
        reasons(&verdict, "review")
            .contains("`.github/workflows/x.yml`, which is no plugin's file")
    );
    // Of no `--local`, a repository of `file://` is refused.
    let head = lab.head(
        &base,
        &[(
            "plugins/acme/hello.toml",
            &entry(
                "file:///tmp/x",
                &[("0.1.0", "v0.1.0", &"a".repeat(40), false)],
            ),
        )],
    );
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "verify",
            &head.display().to_string(),
            "--base",
            &base.display().to_string(),
            "--author",
            "acme",
            "--out",
            &lab.root.join("dist").display().to_string(),
        ],
        &lab.root,
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("over HTTPS"),
        "{output:?}"
    );
}

#[test]
fn a_branch_made_before_the_main_branch_changed_is_judged_of_its_own_changes() {
    let lab = Lab::new("stale");
    let (folder, url) = lab.plugin("hello");
    let made = git(&folder, &["rev-parse", "HEAD"]);
    let hello = entry(&url, &[("0.1.0", "v0.1.0", &made, false)]);
    let other = |versions: &[(&str, &str, &str, bool)]| entry("https://example.com/x", versions);
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let first = other(&[("0.1.0", "v0.1.0", &a, false)]);
    let base = lab.registry("base", &[("plugins/carl/first.toml", &first)]);
    // A branch that adds a plugin of its own; then the main branch takes a
    // version of the other plugin and a third plugin.
    let head = lab.head(
        &base,
        &[
            ("plugins/carl/first.toml", &first),
            ("plugins/acme/hello.toml", &hello),
        ],
    );
    written(
        &base,
        &[
            (
                "plugins/carl/first.toml",
                &other(&[
                    ("0.1.0", "v0.1.0", &a, false),
                    ("0.1.1", "v0.1.1", &b, false),
                ]),
            ),
            (
                "plugins/dora/third.toml",
                &other(&[("1.0.0", "v1.0.0", &b, false)]),
            ),
        ],
    );
    commit(&base, "later");
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    let review = reasons(&verdict, "review");
    assert_eq!(
        review,
        "`acme/hello` is a new plugin, which the maintainer reviews"
    );
    // A plugin that the branch adds, which the main branch merged since of
    // the same file, changes nothing.
    let head = lab.branch(&base);
    written(&head, &[("plugins/acme/hello.toml", &hello)]);
    commit(&head, "pull request");
    written(&base, &[("plugins/acme/hello.toml", &hello)]);
    commit(&base, "merged");
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(reasons(&verdict, "refused").contains("changes no file"));
}

#[test]
fn a_name_that_the_main_branch_gained_since_or_that_lokyr_holds_is_refused() {
    let lab = Lab::new("names");
    let (folder, url) = lab.plugin("hello");
    let commit_id = git(&folder, &["rev-parse", "HEAD"]);
    let hello = entry(&url, &[("0.1.0", "v0.1.0", &commit_id, false)]);
    let base = lab.registry("base", &[]);
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &hello)]);
    // Another owner's plugin of the name, merged since the branch.
    written(&base, &[("plugins/bob/hello.toml", &hello)]);
    commit(&base, "bob's");
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let why = reasons(&verdict, "refused");
    assert!(why.contains("the name `hello` is `bob/hello`'s"), "{why}");
    // A first-party plugin's name, and one of the app's commands.
    let base = lab.registry("fresh", &[]);
    for name in ["tabs", "pane"] {
        let path = format!("plugins/acme/{name}.toml");
        let head = lab.head(&base, &[(&path, &hello)]);
        let (output, verdict) = lab.verify(&head, &base, "acme", "");
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let why = reasons(&verdict, "refused");
        assert!(
            why.contains(&format!("the name `{name}` is Lokyr's own")),
            "{why}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_a_submodule_and_a_mode_s_change_are_told() {
    let lab = Lab::new("modes");
    let base = lab.registry("base", &[]);
    // A plugin's file that is a link.
    let head = lab.head(&base, &[]);
    std::fs::create_dir_all(head.join("plugins/acme")).unwrap();
    std::os::unix::fs::symlink("../../README.md", head.join("plugins/acme/hello.toml")).unwrap();
    commit(&head, "link");
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let why = reasons(&verdict, "refused");
    assert!(
        why.contains("`plugins/acme/hello.toml` is a link: a plugin's file is a plain file"),
        "{why}"
    );
    // A submodule, and the README made executable.
    let head = lab.head(&base, &[]);
    git(
        &head,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},vendor/tool", "c".repeat(40)),
        ],
    );
    git(&head, &["update-index", "--chmod=+x", "README.md"]);
    git(&head, &["commit", "--quiet", "-m", "modes"]);
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    let review = reasons(&verdict, "review");
    assert!(
        review.contains("it changes `vendor/tool`, a submodule, which is no plugin's file"),
        "{review}"
    );
    assert!(
        review.contains("it changes `README.md`, from a file to an executable file"),
        "{review}"
    );
}

#[test]
fn the_reserved_names_hold_the_first_party_plugins_and_the_app_s_own() {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let mut found = 0;
    for entry in std::fs::read_dir(&plugins).unwrap().flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path().join("plugin.toml")) else {
            continue;
        };
        let manifest = lokyr_pkg_model::manifest::Manifest::parse(&text).unwrap();
        assert!(
            sleip_index::registry::RESERVED_NAMES.contains(&manifest.namespace()),
            "`{}` is not reserved",
            manifest.namespace()
        );
        found += 1;
    }
    assert!(found >= 11, "{found} first-party plugins");
    for context in lokyr_plugin_sdk::keys::CONTEXTS {
        let namespace = context.split('.').next().unwrap();
        assert!(
            sleip_index::registry::RESERVED_NAMES.contains(&namespace),
            "`{namespace}` is not reserved"
        );
    }
    for namespace in ["app", "window", "pane"] {
        assert!(sleip_index::registry::RESERVED_NAMES.contains(&namespace));
    }
}
