//! The registry's repository: a file of each plugin, `plugins/<owner>/<name>.toml`,
//! of the repository that holds the plugin's sources and of its published
//! versions, each of its tag and the tag's commit, which the registry's CI
//! packages; and the names of each version's release and archive.
//!
//! ```toml
//! repository = "https://github.com/acme/hello"
//!
//! [[versions]]
//! version = "0.1.0"
//! tag = "v0.1.0"
//! commit = "0123456789abcdef0123456789abcdef01234567"
//! ```

use serde::Deserialize;

/// The owner that the first-party plugins hold, which no one publishes as.
pub const RESERVED_OWNER: &str = "lokyr";

/// The address of a version's archive, of its owner, name and version, as
/// the registry's releases hold them, which its `config.json` tells.
pub const DOWNLOADS: &str = "https://github.com/sephiroth74/sleip-registry/releases/download/{owner}.{name}.{version}/{name}-{version}.tar.zst";

/// The names that no plugin of the registry takes, whatever its owner, as
/// Lokyr knows a plugin by its name alone: the first-party plugins', which
/// the app bundles; the namespaces of the app's own commands and contexts,
/// which the app refuses a plugin of; and the app's and Sleip's own.
pub const RESERVED_NAMES: &[&str] = &[
    "alternate-screen",
    "app",
    "command-header",
    "command-palette",
    "copy-mode",
    "core-actions",
    "keymap-macos",
    "links",
    "lokyr",
    "pane",
    "plugin-manager",
    "progress",
    "quick-terminal",
    "quick-tips",
    "search",
    "selection",
    "sleip",
    "splits",
    "tabs",
    "window",
];

/// The most characters of a name's part, its owner's or its own.
pub const LONGEST_PART: usize = 64;

/// A plugin's file of the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginFile {
    /// The git repository of the plugin's sources, its manifest at its root.
    pub repository: String,
    /// The versions, in the order of their publishing, each greater than
    /// the one before.
    pub versions: Vec<Release>,
}

/// A version of a plugin, as published.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// Its version, the manifest's at its commit.
    pub version: String,
    /// The tag of the repository that names its commit.
    pub tag: String,
    /// The commit's whole hash, of 40 hexadecimal digits.
    pub commit: String,
    /// Whether its owner took it back: no new install takes it, a lockfile
    /// that holds it keeps it.
    #[serde(default)]
    pub yanked: bool,
}

/// The file as TOML writes it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    repository: String,
    #[serde(default)]
    versions: Vec<Release>,
}

impl PluginFile {
    /// The plugin's file of `text`, or why it does not read.
    pub fn parse(text: &str) -> Result<Self, String> {
        let written: Written = toml::from_str(text).map_err(|error| {
            let message = error.message().to_owned();
            match error.span().map(|span| line_of(text, span.start)) {
                Some(line) => format!("line {line}: {message}"),
                None => message,
            }
        })?;
        if !written.repository.starts_with("https://") && !written.repository.starts_with("file://")
        {
            return Err(format!(
                "`repository` is the address of a git repository over HTTPS, not `{}`",
                written.repository
            ));
        }
        if written.versions.is_empty() {
            return Err("it publishes no version, as `[[versions]]` lists none".to_owned());
        }
        let mut before: Option<semver::Version> = None;
        for release in &written.versions {
            let version = semver::Version::parse(&release.version).map_err(|error| {
                format!(
                    "`{}` is not a version of Semantic Versioning: {error}",
                    release.version
                )
            })?;
            if let Some(before) = &before
                && version <= *before
            {
                return Err(format!(
                    "version {} comes after {before}, which is not lower",
                    release.version
                ));
            }
            before = Some(version);
            if !is_tag(&release.tag) {
                return Err(format!(
                    "the tag of version {} is not a tag's name: `{}`",
                    release.version, release.tag
                ));
            }
            if release.commit.len() != 40
                || !release
                    .commit
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(format!(
                    "the commit of version {} is not a whole hash of 40 lowercase hexadecimal digits: `{}`",
                    release.version, release.commit
                ));
            }
        }
        Ok(Self {
            repository: written.repository,
            versions: written.versions,
        })
    }

    /// The version `version`, if published.
    pub fn version(&self, version: &str) -> Option<&Release> {
        self.versions
            .iter()
            .find(|release| release.version == version)
    }
}

/// The owner and the name of the path `path` of a plugin's file in the
/// registry, `plugins/<owner>/<name>.toml`, of `/` between its folders; or
/// `None` of a path of another place.
pub fn plugin_of_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("plugins/")?;
    let (owner, file) = rest.split_once('/')?;
    let name = file.strip_suffix(".toml")?;
    (!name.contains('/')).then_some((owner, name))
}

/// The path of the file of the plugin `owner/name` in the registry.
pub fn path_of_plugin(owner: &str, name: &str) -> String {
    format!("plugins/{owner}/{name}.toml")
}

/// Whether `part` is a name's part, its owner's or its own: lowercase
/// letters, digits and `-`, from a letter, of 64 characters at most.
pub fn is_part(part: &str) -> bool {
    part.len() <= LONGEST_PART
        && lokyr_pkg_model::manifest::is_plugin_name(&format!("{part}/{part}"))
}

/// The most bytes of a version's tag.
pub const LONGEST_TAG: usize = 128;

/// Whether `tag` names a tag as git takes its name, `refs/tags/<tag>`, as
/// `git check-ref-format` checks it, and as no option nor revision reads
/// it: no part of `/` empty, from `.` or of `.lock`; no `..`, `@{`, `\`,
/// control character, space, `~`, `^`, `:`, `?`, `*` nor `[`; not `@`, not
/// from `-`, not of a `.` at its end; of [`LONGEST_TAG`] bytes at most.
pub fn is_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= LONGEST_TAG
        && tag != "@"
        && !tag.starts_with('-')
        && !tag.ends_with('.')
        && !tag.contains("..")
        && !tag.contains("@{")
        && tag
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
        && !tag.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
}

/// The tag of the registry's release of `version` of `owner/name`, which
/// holds its archive, as the index's `dl` reads it.
pub fn release_tag(owner: &str, name: &str, version: &str) -> String {
    format!("{owner}.{name}.{version}")
}

/// The file name of the archive of `version` of the plugin `name`, in its
/// release.
pub fn archive_name(name: &str, version: &str) -> String {
    format!("{name}-{version}.tar.zst")
}

/// The line, from 1, of the byte `at` of `text`.
fn line_of(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn file(versions: &str) -> String {
        format!("repository = \"https://github.com/acme/hello\"\n{versions}")
    }

    fn release(version: &str) -> String {
        format!(
            "[[versions]]\nversion = \"{version}\"\ntag = \"v{version}\"\ncommit = \"{COMMIT}\"\n"
        )
    }

    #[test]
    fn a_file_reads_its_repository_and_its_versions_in_their_order() {
        let text = file(&format!(
            "{}{}yanked = true\n",
            release("0.1.0"),
            release("0.2.0")
        ));
        let parsed = PluginFile::parse(&text).unwrap();
        assert_eq!(parsed.repository, "https://github.com/acme/hello");
        assert_eq!(parsed.versions.len(), 2);
        assert_eq!(parsed.versions[0].tag, "v0.1.0");
        assert!(!parsed.versions[0].yanked);
        assert!(parsed.version("0.2.0").unwrap().yanked);
        assert!(parsed.version("0.3.0").is_none());
    }

    #[test]
    fn a_wrong_file_tells_why() {
        let wrong = |text: &str| PluginFile::parse(text).unwrap_err();
        assert!(wrong(&file("")).contains("publishes no version"));
        assert!(
            wrong("repository = \"git@github.com:acme/hello\"\n").contains("over HTTPS"),
            "{}",
            wrong("repository = \"git@github.com:acme/hello\"\n")
        );
        assert!(wrong(&file(&release("1.0"))).contains("Semantic Versioning"));
        assert!(
            wrong(&file(&format!("{}{}", release("0.2.0"), release("0.1.0"))))
                .contains("not lower")
        );
        assert!(
            wrong(&file(&format!("{}{}", release("0.1.0"), release("0.1.0"))))
                .contains("not lower")
        );
        assert!(wrong(&file(&release("0.1.0").replace(COMMIT, "abc123"))).contains("40 lowercase"));
        for tag in [
            "-x",
            "v1 0",
            "HEAD~1",
            "v1^{tree}",
            "v1@{0}",
            "a..b",
            "v1.lock",
            ".v1",
            "v1/",
            "/v1",
            "a//b",
            "v1.",
            "@",
            "v*",
            "v?",
            "v[1]",
            "a:b",
            "a\\b",
        ] {
            assert!(
                wrong(&file(
                    &release("0.1.0").replace("\"v0.1.0\"", &format!("{tag:?}"))
                ))
                .contains("not a tag's name"),
                "{tag}"
            );
        }
        assert!(!is_tag(&"v".repeat(LONGEST_TAG + 1)));
        assert!(!is_tag("v\u{7f}") && !is_tag("v\u{1b}[0m"));
        for tag in [
            "v0.1.0",
            "release/0.1.0",
            "0.1.0-rc.1",
            "v1+build",
            "hello_v1",
        ] {
            assert!(is_tag(tag), "{tag}");
        }
        let unknown = wrong(&file(&format!("{}sha = \"x\"\n", release("0.1.0"))));
        assert!(unknown.starts_with("line 6:"), "{unknown}");
    }

    #[test]
    fn a_path_tells_its_plugin_and_the_names_of_its_releases() {
        assert_eq!(
            plugin_of_path("plugins/acme/hello.toml"),
            Some(("acme", "hello"))
        );
        assert_eq!(plugin_of_path("plugins/acme/x/hello.toml"), None);
        assert_eq!(plugin_of_path("README.md"), None);
        assert_eq!(path_of_plugin("acme", "hello"), "plugins/acme/hello.toml");
        assert_eq!(release_tag("acme", "hello", "0.1.0"), "acme.hello.0.1.0");
        assert_eq!(archive_name("hello", "0.1.0"), "hello-0.1.0.tar.zst");
        assert!(is_part("hello-2"));
        assert!(!is_part("Hello"));
        assert!(!is_part("2hello"));
        assert!(is_part(&"a".repeat(64)));
        assert!(!is_part(&"a".repeat(65)));
    }
}
