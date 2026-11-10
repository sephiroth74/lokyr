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
        self.verify_with(head, base, author, orgs, &[])
    }

    /// `verify`, as [`Lab::verify`] runs it, of the options `more`.
    fn verify_with(
        &self,
        head: &Path,
        base: &Path,
        author: &str,
        orgs: &str,
        more: &[&str],
    ) -> (Output, Value) {
        let out = self.root.join("dist");
        let _ = std::fs::remove_dir_all(&out);
        let (head, base, out, report) = (
            head.display().to_string(),
            base.display().to_string(),
            out.display().to_string(),
            self.root.join("report.md").display().to_string(),
        );
        let mut args = vec![
            "--json", "registry", "verify", &head, "--base", &base, "--author", author, "--orgs",
            orgs, "--out", &out, "--report", &report, "--local",
        ];
        args.extend_from_slice(more);
        let output = self.sleip(&args, &self.root);
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
    assert!(refused(&head, &empty, "acme", "").contains("has no tag 'v9'"));
    // A manifest of another name, and of another version.
    let head = lab.head(&empty, &[("plugins/acme/other.toml", &good)]);
    assert!(refused(&head, &empty, "acme", "").contains("its manifest names 'acme/hello'"));
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
    assert!(refused(&head, &empty, "acme", "").contains("`plugins/acme/hello.toml`: `line 1"));
    // A source that its check refuses, and a test that fails.
    edit(&folder, "src/init.luau", |text| {
        format!("{text}\nlocal wrong: number = \"text\"\n")
    });
    edit(&folder, "tests/broken.luau", |_| {
        "local test = {}\nfunction test.fails(host: TestHost)\n    assert(false, \"it fails, @everyone <b>`see`</b>\")\nend\nreturn test\n".to_owned()
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
    // The test's message in a span of code, of no mention nor HTML.
    assert!(
        why.contains("`test: tests/broken.luau:3 fails: it fails, everyone b>'see'/b>`"),
        "{why}"
    );
    let report = std::fs::read_to_string(lab.root.join("report.md")).unwrap();
    assert!(!report.contains('@') && !report.contains('<'), "{report}");
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

#[test]
fn the_files_alone_are_judged_as_the_job_that_merges_checks_again() {
    let lab = Lab::new("files-alone");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let published = entry(&url, &[("0.1.0", "v0.1.0", &first, false)]);
    let base = lab.registry("base", &[("plugins/acme/hello.toml", &published)]);
    let alone = |head: &Path, author: &str| {
        let (output, verdict) = lab.verify_with(head, &base, author, "", &["--files-alone"]);
        assert!(
            !events(&output)
                .iter()
                .any(|event| event["event"] == "built"),
            "{output:?}"
        );
        verdict
    };
    // A version of nothing new, of a tag that is not there: refused of
    // its clone, as the tag is the first check's; of its files alone, its
    // manifest of its commit weighed, it merges.
    let second = release(&folder, "0.1.1");
    let added = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.1.1", "v9.9.9", &second, false),
        ],
    );
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &added)]);
    let (output, verdict) = lab.verify(&head, &base, "acme", "");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(reasons(&verdict, "refused").contains("has no tag 'v9.9.9'"));
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "merge", "{verdict}");
    // A version that asks for a capability more, and runs a command: of
    // its files alone, of git's objects, the maintainer reviews it.
    edit(&folder, "plugin.toml", |text| {
        format!(
            "{text}\n[capabilities]\nclipboard-write = true\n\n[uses]\ncommands = [\"tabs.*\"]\n"
        )
    });
    git(&folder, &["commit", "--quiet", "-am", "more"]);
    let more = release(&folder, "0.2.0");
    let asking = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.2.0", "v0.2.0", &more, false),
        ],
    );
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &asking)]);
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(
        reasons(&verdict, "review").contains(
            "asks for more than 0.1.0 did: the capability `clipboard-write`, the command `tabs.*`"
        ),
        "{verdict}"
    );
    // A commit that is not there, or of another version's manifest, or of
    // a manifest that is a link, refused.
    let gone = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.1.1", "v0.1.1", &"d".repeat(40), false),
        ],
    );
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &gone)]);
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "refused", "{verdict}");
    assert!(
        reasons(&verdict, "refused").contains("holds the commit"),
        "{verdict}"
    );
    let other = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.3.0", "v0.1.1", &second, false),
        ],
    );
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &other)]);
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "refused", "{verdict}");
    assert!(
        reasons(&verdict, "refused").contains("its manifest's version is 0.1.1"),
        "{verdict}"
    );
    std::fs::rename(folder.join("plugin.toml"), folder.join("real.toml")).unwrap();
    std::os::unix::fs::symlink("real.toml", folder.join("plugin.toml")).unwrap();
    git(&folder, &["add", "-A"]);
    git(&folder, &["commit", "--quiet", "-m", "a link"]);
    let link = git(&folder, &["rev-parse", "HEAD"]);
    let linked = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.2.1", "v0.2.1", &link, false),
        ],
    );
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &linked)]);
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "refused", "{verdict}");
    assert!(
        reasons(&verdict, "refused").contains("its plugin.toml is a link"),
        "{verdict}"
    );
    // A new plugin, a repository changed and a yank taken back, the
    // maintainer reviews; a published version changed, refused.
    let (other_folder, other_url) = lab.plugin("other");
    let other_first = git(&other_folder, &["rev-parse", "HEAD"]);
    let head = lab.head(
        &base,
        &[
            ("plugins/acme/hello.toml", &published),
            (
                "plugins/acme/other.toml",
                &entry(&other_url, &[("0.1.0", "v0.1.0", &other_first, false)]),
            ),
        ],
    );
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(reasons(&verdict, "review").contains("is a new plugin"));
    let moved = format!("{url}-moved");
    let head = lab.head(
        &base,
        &[(
            "plugins/acme/hello.toml",
            &entry(&moved, &[("0.1.0", "v0.1.0", &first, false)]),
        )],
    );
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(reasons(&verdict, "review").contains("repository changes"));
    let yanked = entry(&url, &[("0.1.0", "v0.1.0", &first, true)]);
    let yanked_base = lab.registry("yanked", &[("plugins/acme/hello.toml", &yanked)]);
    let head = lab.head(&yanked_base, &[("plugins/acme/hello.toml", &published)]);
    let (_, verdict) = lab.verify_with(&head, &yanked_base, "acme", "", &["--files-alone"]);
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(reasons(&verdict, "review").contains("comes back from its yank"));
    let changed = entry(&url, &[("0.1.0", "v0.1.0", &second, false)]);
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &changed)]);
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "refused", "{verdict}");
    assert!(reasons(&verdict, "refused").contains("published, changes or goes"));
    // Beside another file, or of another owner, it merges not.
    let head = lab.head(
        &base,
        &[
            ("plugins/acme/hello.toml", &added),
            (".github/actions/sleip/action.yml", "runs: {}\n"),
        ],
    );
    let verdict = alone(&head, "acme");
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &added)]);
    let verdict = alone(&head, "bob");
    assert_eq!(verdict["verdict"], "refused", "{verdict}");
    // Of a push of another account than the author's, the maintainer
    // reviews it; of the author's own, of another case, it merges.
    let (_, verdict) = lab.verify_with(
        &head,
        &base,
        "acme",
        "",
        &["--files-alone", "--sender", "carol"],
    );
    assert_eq!(verdict["verdict"], "review", "{verdict}");
    assert!(
        reasons(&verdict, "review")
            .contains("its last push or event is `carol`'s, not `acme`'s, who opened it"),
        "{verdict}"
    );
    let (_, verdict) = lab.verify_with(
        &head,
        &base,
        "acme",
        "",
        &["--files-alone", "--sender", "Acme"],
    );
    assert_eq!(verdict["verdict"], "merge", "{verdict}");
}

/// `build` of the version `version` of `acme/<name>` of the registry
/// `registry` into the lab's folder `again`: its output.
fn build(lab: &Lab, registry: &Path, name: &str, version: &str) -> Output {
    let again = lab.root.join("again");
    let _ = std::fs::remove_dir_all(&again);
    lab.sleip(
        &[
            "--json",
            "registry",
            "build",
            &format!("acme/{name}"),
            version,
            "--registry",
            &registry.display().to_string(),
            "--out",
            &again.display().to_string(),
            "--local",
        ],
        &lab.root,
    )
}

#[test]
fn a_merged_version_is_built_of_its_commit_whatever_became_of_its_tag() {
    let lab = Lab::new("commit");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let second = release(&folder, "0.1.1");
    let published = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.1.1", "v0.1.1", &second, true),
        ],
    );
    let base = lab.registry("base", &[]);
    let head = lab.head(&base, &[("plugins/acme/hello.toml", &published)]);
    let (output, _) = lab.verify(&head, &base, "acme", "");
    assert!(output.status.success(), "{output:?}");
    let checked =
        std::fs::read(lab.root.join("dist/acme.hello.0.1.0/hello-0.1.0.tar.zst")).unwrap();
    // Its tag moved since it merged: built of its commit, alike.
    git(&folder, &["tag", "-f", "v0.1.0", &second]);
    let output = build(&lab, &head, "hello", "0.1.0");
    assert!(output.status.success(), "{output:?}");
    let again = lab.root.join("again/acme.hello.0.1.0/hello-0.1.0.tar.zst");
    assert_eq!(std::fs::read(&again).unwrap(), checked);
    // The versions tell a yank, which the main branch's CI skips of no
    // release.
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
    let listed = events(&output);
    assert_eq!(listed[0]["yanked"], false);
    assert_eq!(listed[1]["version"], "0.1.1");
    assert_eq!(listed[1]["yanked"], true);
    // Its commit that no branch nor tag holds any more: fetched by its
    // hash, as a host gives it still, and told once it is gone.
    git(&folder, &["tag", "-d", "v0.1.0"]);
    git(&folder, &["tag", "-d", "v0.1.1"]);
    let branch = git(&folder, &["rev-parse", "--abbrev-ref", "HEAD"]);
    git(&folder, &["checkout", "--quiet", "--orphan", "other"]);
    git(&folder, &["commit", "--quiet", "-m", "anew"]);
    git(&folder, &["branch", "-D", &branch]);
    git(&folder, &["branch", "-m", &branch]);
    let output = build(&lab, &head, "hello", "0.1.0");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(std::fs::read(&again).unwrap(), checked);
    git(&folder, &["reflog", "expire", "--expire=now", "--all"]);
    git(&folder, &["gc", "--quiet", "--prune=now"]);
    let output = build(&lab, &head, "hello", "0.1.0");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains(&format!("holds the commit {first} no longer")),
        "{output:?}"
    );
}

#[test]
fn a_new_version_is_weighed_against_the_last_one_of_its_commit() {
    let lab = Lab::new("weighed");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let published = entry(&url, &[("0.1.0", "v0.1.0", &first, false)]);
    let base = lab.registry("base", &[("plugins/acme/hello.toml", &published)]);
    let second = release(&folder, "0.1.1");
    // The published version's tag moved since: its manifest read of its
    // commit, the new version of nothing more merging by itself.
    git(&folder, &["tag", "-f", "v0.1.0", &second]);
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
}

#[test]
fn the_index_is_signed_of_the_released_versions_by_a_key_that_keys_json_lists() {
    use sleip_index::signing::{self, PublicKey};

    let lab = Lab::new("index");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let second = release(&folder, "0.1.1");
    // The keys: the root's secret and the index key's, each a file of its
    // own that no one else reads, which a second keygen does not write
    // over.
    let keygen = |name: &str| {
        let file = lab.root.join(name);
        let output = lab.sleip(
            &[
                "--json",
                "registry",
                "keygen",
                "--out",
                &file.display().to_string(),
            ],
            &lab.root,
        );
        assert!(output.status.success(), "{output:?}");
        let public = events(&output)[0]["public"].as_str().unwrap().to_owned();
        (file, public)
    };
    let (root, root_public) = keygen("root.key");
    let (index_key, index_public) = keygen("index.key");
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&root).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let again = lab.sleip(
        &["registry", "keygen", "--out", &root.display().to_string()],
        &lab.root,
    );
    assert_eq!(again.status.code(), Some(1), "{again:?}");
    // The registry, of a version released and one not yet, and its
    // keys.json signed by the root.
    let base = lab.registry(
        "base",
        &[(
            "plugins/acme/hello.toml",
            &entry(
                &url,
                &[
                    ("0.1.0", "v0.1.0", &first, true),
                    ("0.1.1", "v0.1.1", &second, false),
                ],
            ),
        )],
    );
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "sign-keys",
            "--root",
            &root.display().to_string(),
            "--keys",
            &index_public,
            "--out",
            &base.join("keys.json").display().to_string(),
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    // The release of 0.1.0's archive, as the CI of main makes it.
    let archives = lab.root.join("archives");
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "build",
            "acme/hello",
            "0.1.0",
            "--registry",
            &base.display().to_string(),
            "--out",
            &archives.display().to_string(),
            "--local",
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    let cksum = events(&output)
        .into_iter()
        .find(|event| event["event"] == "built")
        .unwrap()["cksum"]
        .as_str()
        .unwrap()
        .to_owned();
    let roots = lab.root.join("roots");
    std::fs::write(&roots, format!("{root_public}\n")).unwrap();
    let site = lab.root.join("site");
    // The releases' address, of the folder of the archives, so that no
    // test reaches the network.
    let dl = format!(
        "file://{}/{{owner}}.{{name}}.{{version}}/{{name}}-{{version}}.tar.zst",
        archives.display()
    );
    let indexed = |key: &Path| {
        let home = lab.root.join("home");
        Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args([
                "--json",
                "registry",
                "index",
                "--registry",
                &base.display().to_string(),
                "--archives",
                &archives.display().to_string(),
                "--out",
                &site.display().to_string(),
                "--roots",
                &roots.display().to_string(),
                "--dl",
                &dl,
            ])
            .current_dir(&lab.root)
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .env(
                "SLEIP_INDEX_KEY",
                std::fs::read_to_string(key).unwrap_or_default(),
            )
            .output()
            .unwrap()
    };
    let output = indexed(&index_key);
    assert!(output.status.success(), "{output:?}");
    // Its files, as Sleip checks them.
    let root_key = PublicKey::parse(&root_public).unwrap();
    let keys = signing::verify_keys(
        &std::fs::read_to_string(site.join("keys.json")).unwrap(),
        &[root_key],
        None,
    )
    .unwrap();
    assert_eq!(keys.keys, [PublicKey::parse(&index_public).unwrap()]);
    let index = signing::verify_index(
        &std::fs::read_to_string(site.join("index/acme/hello")).unwrap(),
        "acme/hello",
        &keys,
        None,
    )
    .unwrap();
    let [only] = index.entries.as_slice() else {
        panic!("one version released: {:?}", index.entries);
    };
    assert_eq!(only.vers, "0.1.0");
    assert_eq!(only.cksum, cksum);
    assert_eq!(only.kind, "luau");
    assert_eq!(only.api.as_deref(), Some("^0.1"));
    assert!(only.yanked);
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(site.join("config.json")).unwrap()).unwrap();
    assert!(
        config["dl"]
            .as_str()
            .unwrap()
            .contains("{owner}.{name}.{version}")
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("acme/hello 0.1.1 has no release yet")
    );
    // The site beside it: the plugin's page, of the version released, its
    // README, and the home page that lists it.
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "site",
            "--registry",
            &base.display().to_string(),
            "--archives",
            &archives.display().to_string(),
            "--out",
            &site.display().to_string(),
            "--roots",
            &roots.display().to_string(),
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    let page = std::fs::read_to_string(site.join("plugins/acme/hello/index.html")).unwrap();
    assert!(page.contains("<h1>acme/hello</h1>"), "{page}");
    assert!(page.contains("data-pagefind-body"), "{page}");
    assert!(page.contains(&cksum), "{page}");
    assert!(page.contains("<h2>README</h2>"), "{page}");
    assert!(
        page.contains("<span class=\"badge\">yanked</span>"),
        "{page}"
    );
    let home = std::fs::read_to_string(site.join("index.html")).unwrap();
    assert!(home.contains("href=\"/plugins/acme/hello/\""), "{home}");
    assert!(site.join("style.css").is_file());
    // The README's list of the plugins, between its marks, once: a run
    // that changes nothing writes nothing; one of no marks is refused.
    let readme = lab.root.join("README.md");
    std::fs::write(
        &readme,
        "# Registry\n\n## Plugins\n\n<!-- plugins:start -->\nold\n<!-- plugins:end -->\n\nThe rest.\n",
    )
    .unwrap();
    let listed = |file: &Path| {
        lab.sleip(
            &[
                "--json",
                "registry",
                "readme",
                "--registry",
                &base.display().to_string(),
                "--index",
                &site.display().to_string(),
                "--readme",
                &file.display().to_string(),
                "--roots",
                &roots.display().to_string(),
            ],
            &lab.root,
        )
    };
    let output = listed(&readme);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(events(&output)[0]["changed"], true);
    let text = std::fs::read_to_string(&readme).unwrap();
    assert!(
        text.contains(
            "| [acme/hello](https://pkg.lokyr.dev/plugins/acme/hello/) | 0.1.0 (yanked) |"
        ),
        "{text}"
    );
    assert!(
        !text.contains("old") && text.ends_with("<!-- plugins:end -->\n\nThe rest.\n"),
        "{text}"
    );
    let output = listed(&readme);
    assert_eq!(events(&output)[0]["changed"], false);
    assert_eq!(std::fs::read_to_string(&readme).unwrap(), text);
    let unmarked = lab.root.join("UNMARKED.md");
    std::fs::write(&unmarked, "# Registry\n").unwrap();
    let output = listed(&unmarked);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("has no list of the plugins"));
    // Lokyr's icon, the tab's and the header's.
    let icon = std::fs::read_to_string(site.join("icon.svg")).unwrap();
    assert!(icon.starts_with("<svg"), "{icon}");
    assert!(
        home.contains("<link rel=\"icon\" href=\"/icon.svg\""),
        "{home}"
    );
    assert!(page.contains("<img src=\"/icon.svg\""), "{page}");
    // A key that keys.json does not list, and none at all.
    let (stranger, _) = keygen("stranger.key");
    let output = indexed(&stranger);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("is not in keys.json"));
    let output = indexed(&lab.root.join("none.key"));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("SLEIP_INDEX_KEY"));
    // Of keys.json of another root than Sleip's: refused.
    let output = Command::new(env!("CARGO_BIN_EXE_sleip"))
        .args([
            "--json",
            "registry",
            "index",
            "--registry",
            &base.display().to_string(),
            "--archives",
            &archives.display().to_string(),
            "--out",
            &site.display().to_string(),
        ])
        .env(
            "SLEIP_INDEX_KEY",
            std::fs::read_to_string(&index_key).unwrap(),
        )
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("signed by no root key"));
}

#[test]
fn the_index_keeps_its_deployed_lines_and_a_version_that_fails_leaves_the_others() {
    use sleip_index::signing::{self, Entry, Timestamp};

    let lab = Lab::new("deployed");
    let (folder, url) = lab.plugin("hello");
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let second = release(&folder, "0.1.1");
    let third = release(&folder, "0.1.2");
    let keygen = |name: &str| {
        let file = lab.root.join(name);
        let output = lab.sleip(
            &[
                "--json",
                "registry",
                "keygen",
                "--out",
                &file.display().to_string(),
            ],
            &lab.root,
        );
        assert!(output.status.success(), "{output:?}");
        let public = events(&output)[0]["public"].as_str().unwrap().to_owned();
        (file, public)
    };
    let (root, root_public) = keygen("root.key");
    let (index_key, index_public) = keygen("index.key");
    let roots = lab.root.join("roots");
    std::fs::write(&roots, format!("{root_public}\n")).unwrap();
    // The registry of `files`, of its keys.json.
    let registry_of_files = |files: &[(&str, &str)]| {
        let base = lab.registry("base", files);
        let output = lab.sleip(
            &[
                "registry",
                "sign-keys",
                "--root",
                &root.display().to_string(),
                "--keys",
                &index_public,
                "--out",
                &base.join("keys.json").display().to_string(),
            ],
            &lab.root,
        );
        assert!(output.status.success(), "{output:?}");
        base
    };
    // The registry of `text` as the plugin's file.
    let registry_of = |text: &str| registry_of_files(&[("plugins/acme/hello.toml", text)]);
    // The releases, a folder that stands for them, each version's archive
    // built of its commit, as the CI of main makes it.
    let releases = lab.root.join("releases");
    let built = |base: &Path, version: &str| {
        let output = lab.sleip(
            &[
                "--json",
                "registry",
                "build",
                "acme/hello",
                version,
                "--registry",
                &base.display().to_string(),
                "--out",
                &releases.display().to_string(),
                "--local",
            ],
            &lab.root,
        );
        assert!(output.status.success(), "{output:?}");
        events(&output)
            .into_iter()
            .find(|event| event["event"] == "built")
            .unwrap()["cksum"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let asset =
        |version: &str| releases.join(format!("acme.hello.{version}/hello-{version}.tar.zst"));
    let dl = format!(
        "file://{}/{{owner}}.{{name}}.{{version}}/{{name}}-{{version}}.tar.zst",
        releases.display()
    );
    // `index` into the folder `site`, of the archives at hand in `cache`,
    // of the index deployed in `deployed`.
    // `index` as above, of the index key in the file `key` and of the clock
    // of `now`, if given, as a build for debugging takes it.
    let indexed_with = |base: &Path,
                        cache: &str,
                        site: &str,
                        deployed: Option<&str>,
                        key: &Path,
                        now: Option<Timestamp>| {
        let mut args = vec![
            "--json".to_owned(),
            "registry".to_owned(),
            "index".to_owned(),
            "--registry".to_owned(),
            base.display().to_string(),
            "--archives".to_owned(),
            lab.root.join(cache).display().to_string(),
            "--out".to_owned(),
            lab.root.join(site).display().to_string(),
            "--roots".to_owned(),
            roots.display().to_string(),
            "--dl".to_owned(),
            dl.clone(),
        ];
        if let Some(deployed) = deployed {
            args.push("--deployed".to_owned());
            args.push(format!("file://{}", lab.root.join(deployed).display()));
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_sleip"));
        command
            .args(&args)
            .current_dir(&lab.root)
            .env("HOME", lab.root.join("home"))
            .env("PATH", "/usr/bin:/bin")
            .env("SLEIP_INDEX_KEY", std::fs::read_to_string(key).unwrap());
        if let Some(now) = now {
            command.env("SLEIP_TEST_NOW", now.to_string());
        }
        command.output().unwrap()
    };
    let indexed = |base: &Path, cache: &str, site: &str, deployed: Option<&str>| {
        indexed_with(base, cache, site, deployed, &index_key, None)
    };
    let lines = |site: &str| -> Vec<Entry> {
        let keys = signing::verify_keys(
            &std::fs::read_to_string(lab.root.join(site).join("keys.json")).unwrap(),
            &[signing::PublicKey::parse(&root_public).unwrap()],
            None,
        )
        .unwrap();
        signing::verify_index(
            &std::fs::read_to_string(lab.root.join(site).join("index/acme/hello")).unwrap(),
            "acme/hello",
            &keys,
            None,
        )
        .unwrap()
        .entries
    };
    let failed = |output: &Output| -> Vec<Value> {
        events(output)
            .into_iter()
            .filter(|event| event["event"] == "failed")
            .collect()
    };
    // The first index, of 0.1.0, its archive downloaded of its release.
    let base = registry_of(&entry(&url, &[("0.1.0", "v0.1.0", &first, false)]));
    let cksum0 = built(&base, "0.1.0");
    let output = indexed(&base, "cache1", "site1", None);
    assert!(output.status.success(), "{output:?}");
    assert!(
        lab.root
            .join("cache1/acme.hello.0.1.0/hello-0.1.0.tar.zst")
            .is_file()
    );
    assert_eq!(lines("site1")[0].cksum, cksum0);
    // 0.1.0's asset replaced, 0.1.1 released and 0.1.2 of an archive that
    // does not read: of the index deployed, 0.1.0's line as it was signed,
    // its archive not downloaded; 0.1.1 added; 0.1.2 told as failed, the
    // others written.
    std::fs::write(asset("0.1.0"), "replaced").unwrap();
    let base = registry_of(&entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, false),
            ("0.1.1", "v0.1.1", &second, false),
            ("0.1.2", "v0.1.2", &third, false),
        ],
    ));
    let cksum1 = built(&base, "0.1.1");
    std::fs::create_dir_all(asset("0.1.2").parent().unwrap()).unwrap();
    std::fs::write(asset("0.1.2"), "no archive").unwrap();
    let output = indexed(&base, "cache2", "site2", Some("site1"));
    assert!(output.status.success(), "{output:?}");
    let told = failed(&output);
    assert_eq!(told.len(), 1, "{output:?}");
    assert_eq!(told[0]["version"], "0.1.2");
    let held = lines("site2");
    assert_eq!(
        held.iter()
            .map(|line| (line.vers.as_str(), line.cksum.as_str()))
            .collect::<Vec<_>>(),
        [("0.1.0", cksum0.as_str()), ("0.1.1", cksum1.as_str())]
    );
    assert!(!lab.root.join("cache2/acme.hello.0.1.0").exists());
    // Of the index without the one deployed, the replaced asset would be
    // signed: so it is not.
    let output = indexed(&base, "cache2b", "site2b", None);
    assert!(output.status.success(), "{output:?}");
    assert!(
        failed(&output)
            .iter()
            .any(|event| event["version"] == "0.1.0"),
        "{output:?}"
    );
    // 0.1.0 yanked and 0.1.2 released anew: its yank the registry's, its
    // checksum as signed; 0.1.2 added.
    let base = registry_of(&entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, true),
            ("0.1.1", "v0.1.1", &second, false),
            ("0.1.2", "v0.1.2", &third, false),
        ],
    ));
    let cksum2 = built(&base, "0.1.2");
    let output = indexed(&base, "cache3", "site3", Some("site2"));
    assert!(output.status.success(), "{output:?}");
    assert!(failed(&output).is_empty(), "{output:?}");
    let held = lines("site3");
    assert_eq!(
        held.iter()
            .map(|line| (line.vers.as_str(), line.cksum.as_str(), line.yanked))
            .collect::<Vec<_>>(),
        [
            ("0.1.0", cksum0.as_str(), true),
            ("0.1.1", cksum1.as_str(), false),
            ("0.1.2", cksum2.as_str(), false)
        ]
    );
    // The site, of the shown version's archive: one of another checksum
    // told, its page of the index alone; one downloaded of its release.
    let sited = |cache: &str, site: &str| {
        lab.sleip(
            &[
                "--json",
                "registry",
                "site",
                "--registry",
                &base.display().to_string(),
                "--archives",
                &lab.root.join(cache).display().to_string(),
                "--out",
                &lab.root.join(site).display().to_string(),
                "--roots",
                &roots.display().to_string(),
            ],
            &lab.root,
        )
    };
    let cached = lab.root.join("cache3/acme.hello.0.1.2/hello-0.1.2.tar.zst");
    std::fs::copy(asset("0.1.1"), &cached).unwrap();
    let output = sited("cache3", "site3");
    assert!(output.status.success(), "{output:?}");
    let told = failed(&output);
    assert_eq!(told.len(), 1, "{output:?}");
    assert!(
        told[0]["message"]
            .as_str()
            .unwrap()
            .contains("which the index signed"),
        "{output:?}"
    );
    let page =
        std::fs::read_to_string(lab.root.join("site3/plugins/acme/hello/index.html")).unwrap();
    assert!(!page.contains("<h2>README</h2>"), "{page}");
    std::fs::remove_file(&cached).unwrap();
    let output = sited("cache3", "site3");
    assert!(failed(&output).is_empty(), "{output:?}");
    let page =
        std::fs::read_to_string(lab.root.join("site3/plugins/acme/hello/index.html")).unwrap();
    assert!(page.contains("<h2>README</h2>"), "{page}");
    // A plugin's file that does not read keeps its lines as they were
    // signed.
    let base = registry_of("repository = [\n");
    let output = indexed(&base, "cache4", "site4", Some("site3"));
    assert!(output.status.success(), "{output:?}");
    let told = failed(&output);
    assert_eq!(told.len(), 1, "{output:?}");
    assert_eq!(told[0]["version"], Value::Null);
    assert_eq!(lines("site4"), lines("site3"));
    // The site and the README's list of it, as the index tolerates the
    // file: each told, its page and its line of the index alone.
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "site",
            "--registry",
            &base.display().to_string(),
            "--archives",
            &lab.root.join("cache4").display().to_string(),
            "--out",
            &lab.root.join("site4").display().to_string(),
            "--roots",
            &roots.display().to_string(),
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(failed(&output).len(), 1, "{output:?}");
    assert!(
        lab.root
            .join("site4/plugins/acme/hello/index.html")
            .is_file()
    );
    let readme = lab.root.join("README-unread.md");
    std::fs::write(
        &readme,
        "# R\n<!-- plugins:start -->\n<!-- plugins:end -->\n",
    )
    .unwrap();
    let output = lab.sleip(
        &[
            "--json",
            "registry",
            "readme",
            "--registry",
            &base.display().to_string(),
            "--index",
            &lab.root.join("site4").display().to_string(),
            "--readme",
            &readme.display().to_string(),
            "--roots",
            &roots.display().to_string(),
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    assert!(
        std::fs::read_to_string(&readme)
            .unwrap()
            .contains("acme/hello"),
        "{output:?}"
    );
    // A name that two owners hold, as no check of a pull request would
    // let it be: neither's versions listed for their releases; of the
    // index, the one deployed keeps its lines, the other not indexed.
    let published = entry(
        &url,
        &[
            ("0.1.0", "v0.1.0", &first, true),
            ("0.1.1", "v0.1.1", &second, false),
            ("0.1.2", "v0.1.2", &third, false),
        ],
    );
    let base = registry_of_files(&[
        ("plugins/acme/hello.toml", &published),
        ("plugins/bob/hello.toml", &published),
    ]);
    let listed = lab.sleip(
        &[
            "--json",
            "registry",
            "versions",
            "--registry",
            &base.display().to_string(),
        ],
        &lab.root,
    );
    assert!(listed.status.success(), "{listed:?}");
    assert!(
        !events(&listed)
            .iter()
            .any(|event| event["event"] == "version"),
        "{listed:?}"
    );
    let told = failed(&listed);
    assert_eq!(told.len(), 2, "{listed:?}");
    assert!(
        told[0]["message"]
            .as_str()
            .unwrap()
            .contains("held by more than one owner"),
        "{listed:?}"
    );
    let output = indexed(&base, "cache5", "site5", Some("site3"));
    assert!(output.status.success(), "{output:?}");
    assert_eq!(failed(&output).len(), 2, "{output:?}");
    assert_eq!(lines("site5"), lines("site3"));
    assert!(!lab.root.join("site5/index/bob/hello").exists());
    // An index deployed of a time to come, as of the registry's clock
    // ahead: its lines kept all the same, as the registry's own tools take
    // its index whatever its time.
    let base = registry_of(&published);
    let ahead = Timestamp(Timestamp::now().0 + 3 * 86_400);
    let output = indexed_with(
        &base,
        "cache6",
        "site6",
        Some("site3"),
        &index_key,
        Some(ahead),
    );
    assert!(output.status.success(), "{output:?}");
    let output = indexed(&base, "cache7", "site7", Some("site6"));
    assert!(output.status.success(), "{output:?}");
    assert!(failed(&output).is_empty(), "{output:?}");
    assert_eq!(lines("site7"), lines("site3"));
    // No index deployed, as of a host that lost it: each plugin made anew
    // of its releases, of nothing failed. 0.1.0's asset made anew first, as
    // above it was replaced.
    built(&base, "0.1.0");
    std::fs::create_dir_all(lab.root.join("nothing")).unwrap();
    let output = indexed(&base, "cache8", "site8", Some("nothing"));
    assert!(output.status.success(), "{output:?}");
    assert!(failed(&output).is_empty(), "{output:?}");
    assert_eq!(
        lines("site8")
            .iter()
            .map(|line| (line.vers.clone(), line.cksum.clone()))
            .collect::<Vec<_>>(),
        lines("site3")
            .iter()
            .map(|line| (line.vers.clone(), line.cksum.clone()))
            .collect::<Vec<_>>()
    );
    // The index key replaced: the deployed lines, of the key taken off,
    // checked against the registry's own keys.json, made anew of the
    // releases and signed of the new key, the plugin told.
    let (new_key, new_public) = keygen("new.key");
    let rotated = lab.registry("rotated", &[("plugins/acme/hello.toml", &published)]);
    let output = lab.sleip(
        &[
            "registry",
            "sign-keys",
            "--root",
            &root.display().to_string(),
            "--keys",
            &new_public,
            "--out",
            &rotated.join("keys.json").display().to_string(),
        ],
        &lab.root,
    );
    assert!(output.status.success(), "{output:?}");
    let output = indexed_with(&rotated, "cache9", "site9", Some("site3"), &new_key, None);
    assert!(output.status.success(), "{output:?}");
    let told = failed(&output);
    assert_eq!(told.len(), 1, "{output:?}");
    assert!(
        told[0]["message"]
            .as_str()
            .unwrap()
            .contains("made anew of its releases"),
        "{output:?}"
    );
    assert_eq!(lines("site9").len(), 3);
    // The registry's keys.json expired, refused; within its last 30 days,
    // told.
    let late = Timestamp(Timestamp::now().0 + 400 * 86_400);
    let output = indexed_with(&base, "cache10", "site10", None, &index_key, Some(late));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("keys.json expired at"),
        "{output:?}"
    );
    let near = Timestamp(Timestamp::now().0 + 350 * 86_400);
    let output = indexed_with(
        &base,
        "cache11",
        "site11",
        Some("site3"),
        &index_key,
        Some(near),
    );
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("keys.json expires at"),
        "{output:?}"
    );
    // `sign-keys` of a clock behind the file's, or behind the keys.json
    // that the Sleips take, refused.
    let signed_at = |now: Timestamp, out: &Path| {
        Command::new(env!("CARGO_BIN_EXE_sleip"))
            .args([
                "registry",
                "sign-keys",
                "--root",
                &root.display().to_string(),
                "--keys",
                &index_public,
                "--out",
                &out.display().to_string(),
            ])
            .env("HOME", lab.root.join("home"))
            .env("SLEIP_TEST_NOW", now.to_string())
            .output()
            .unwrap()
    };
    let keys_file = lab.root.join("keys-clock.json");
    let output = signed_at(Timestamp(Timestamp::now().0 + 7200), &keys_file);
    assert!(output.status.success(), "{output:?}");
    let output = signed_at(Timestamp::now(), &keys_file);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("after this machine's clock"),
        "{output:?}"
    );
    let output = signed_at(
        Timestamp::parse("2026-01-01T00:00:00Z").unwrap(),
        &lab.root.join("keys-behind.json"),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("set the clock right"),
        "{output:?}"
    );
}

#[test]
fn publish_proposes_a_checked_version_of_its_tag_to_the_registry() {
    let lab = Lab::new("publish");
    // The registry, a bare repository of a commit of its README.
    let bare = lab.root.join("registry.git");
    git(
        &lab.root,
        &[
            "init",
            "--quiet",
            "--bare",
            "-b",
            "main",
            &bare.display().to_string(),
        ],
    );
    let seed = lab.registry("seed", &[]);
    git(
        &seed,
        &["push", "--quiet", &bare.display().to_string(), "main"],
    );
    let registry_url = format!("file://{}", bare.display());
    // The plugin, its repository its own folder, its tag of its version.
    let (folder, url) = lab.plugin("hello");
    edit(&folder, "plugin.toml", |text| {
        text.replacen(
            "[plugin]\n",
            &format!("[plugin]\nrepository = \"{url}\"\n"),
            1,
        )
    });
    git(&folder, &["commit", "--quiet", "-am", "repository"]);
    git(&folder, &["tag", "-f", "v0.1.0"]);
    let first = git(&folder, &["rev-parse", "HEAD"]);
    let published =
        |folder: &Path| lab.sleip(&["--json", "publish", "--registry", &registry_url], folder);
    let output = published(&folder);
    assert!(output.status.success(), "{output:?}");
    let pushed = events(&output)
        .into_iter()
        .find(|event| event["event"] == "pushed")
        .unwrap();
    assert_eq!(pushed["branch"], "publish/acme-hello-0.1.0");
    let file = git(
        &bare,
        &["show", "publish/acme-hello-0.1.0:plugins/acme/hello.toml"],
    );
    let read = sleip_index::registry::PluginFile::parse(&format!("{file}\n")).unwrap();
    assert_eq!(read.repository, url);
    assert_eq!(read.versions.len(), 1);
    assert_eq!(read.versions[0].tag, "v0.1.0");
    assert_eq!(read.versions[0].commit, first);
    // Merged, a later version goes at the file's end.
    let merger = lab.root.join("merger");
    git(
        &lab.root,
        &[
            "clone",
            "--quiet",
            &registry_url,
            &merger.display().to_string(),
        ],
    );
    git(
        &merger,
        &[
            "merge",
            "--quiet",
            "--ff-only",
            "origin/publish/acme-hello-0.1.0",
        ],
    );
    git(&merger, &["push", "--quiet", "origin", "main"]);
    let second = release(&folder, "0.1.1");
    let output = published(&folder);
    assert!(output.status.success(), "{output:?}");
    let file = git(
        &bare,
        &["show", "publish/acme-hello-0.1.1:plugins/acme/hello.toml"],
    );
    let read = sleip_index::registry::PluginFile::parse(&format!("{file}\n")).unwrap();
    assert_eq!(read.versions.len(), 2);
    assert_eq!(read.versions[1].commit, second);
    // No tag of the version.
    edit(&folder, "plugin.toml", |text| {
        text.replace("version = \"0.1.1\"", "version = \"0.1.2\"")
    });
    git(&folder, &["commit", "--quiet", "-am", "0.1.2"]);
    let output = published(&folder);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("no tag `v0.1.2`"),
        "{output:?}"
    );
    // A tag of another version's plugin.toml.
    git(&folder, &["tag", "v0.1.2", &second]);
    let output = published(&folder);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("holds acme/hello 0.1.1, not acme/hello 0.1.2"),
        "{output:?}"
    );
    // A plugin that `sleip check` refuses.
    edit(&folder, "src/init.luau", |text| {
        format!("{text}\nlocal wrong: number = \"text\"\n")
    });
    let output = published(&folder);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("`sleip check` finds"),
        "{output:?}"
    );
}
