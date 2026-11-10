//! `sleip publish`: the version of the plugin of the current folder
//! proposed to the registry. The plugin is checked and tested as the
//! registry's CI checks and tests it, its tag found, of the version that
//! its `plugin.toml` names at that commit, and the plugin's repository
//! holding it; then the registry's file of the plugin, of the version added
//! at its end, or made of its first version, goes to a pull request of the
//! registry's: through `gh`, of the author's fork, its branch and file made
//! through GitHub's API; or of a link to propose it on GitHub, without `gh`
//! or with `--link`; or, of a registry of git that is not GitHub's, as the
//! tests have, a branch pushed to it. What the CI checks is told.

use std::path::PathBuf;

use lokyr_pkg_model::manifest::{Kind, Manifest};
use semver::Version;
use serde_json::json;
use sleip_index::registry::{self, PluginFile};

use super::git;
use crate::output::Out;
use crate::paths::Paths;

/// The registry that Sleip publishes to unless `--registry` names
/// another: its repository on GitHub.
pub const REGISTRY: &str = "sephiroth74/sleip-registry";

/// What the registry's CI checks of a pull request, as it is told.
const CHECKED: &str = "The registry's CI checks that you own the plugin, as its owner or a public member of its organization, that its name is no other owner's, that its tag names its commit, that its manifest is of its name and version, and that `sleip check` and `sleip test` pass; then it packages it. A version that asks for nothing new merges by itself and is released; a new plugin, or one that asks for more, the maintainer reviews.";

/// A clone of the registry, taken away as it drops.
struct Clone(PathBuf);

impl Drop for Clone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Proposes the version of the plugin of the current folder, of `tag`, or
/// `v<version>`, to the registry `registry`, `owner/repository` of GitHub's
/// or a git repository's address, or Lokyr's; of a link alone if `link`, as
/// `out` tells.
pub fn publish(
    paths: &Paths,
    out: Out,
    tag: Option<&str>,
    registry_at: Option<&str>,
    link: bool,
) -> Result<(), String> {
    let folder = std::env::current_dir()
        .map_err(|error| format!("the current folder cannot be read: {error}"))?;
    let text = std::fs::read_to_string(folder.join("plugin.toml")).map_err(|error| {
        format!("plugin.toml cannot be read, as no plugin is in this folder: {error}")
    })?;
    let manifest = Manifest::parse(&text).map_err(|error| format!("plugin.toml:{error}"))?;
    if manifest.kind != Kind::Luau {
        return Err("the registry publishes plugins of Luau alone".to_owned());
    }
    let version = manifest
        .version
        .clone()
        .ok_or_else(|| "plugin.toml names no version".to_owned())?;
    let repository = manifest
        .repository
        .clone()
        .filter(|url| url.starts_with("https://") || url.starts_with("file://"))
        .ok_or_else(|| {
            "plugin.toml names no repository of `https://`, which the registry clones".to_owned()
        })?;
    // As the registry's CI checks and tests it.
    let problems = super::check::problems_of(&folder, out);
    if !problems.is_empty() {
        return Err(format!(
            "`sleip check` finds {} problems, which the registry's CI would refuse",
            problems.len()
        ));
    }
    let failures = super::test::failures_of(&folder, out)?;
    if !failures.is_empty() {
        return Err(format!(
            "{} tests fail, which the registry's CI would refuse",
            failures.len()
        ));
    }
    // Its tag, of the version at its commit, which its repository holds.
    let tag = tag.map_or_else(|| format!("v{version}"), str::to_owned);
    let commit = git(
        Some(&folder),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag}^{{commit}}"),
        ],
    )
    .map_err(|_| {
        format!("no tag `{tag}` here: `git tag {tag}` and `git push origin {tag}` make it")
    })?;
    let tagged = super::git_bytes(Some(&folder), &["show", &format!("{commit}:plugin.toml")])
        .map_err(|_| format!("the tag `{tag}` holds no plugin.toml"))?;
    let at_tag = Manifest::parse(&String::from_utf8_lossy(&tagged))
        .map_err(|error| format!("plugin.toml at `{tag}`:{error}"))?;
    if at_tag.name != manifest.name || at_tag.version.as_deref() != Some(version.as_str()) {
        return Err(format!(
            "the tag `{tag}` holds {} {}, not {} {version}",
            at_tag.name,
            at_tag.version.as_deref().unwrap_or("of no version"),
            manifest.name
        ));
    }
    let remote = super::git_bytes(
        None,
        &[
            "ls-remote",
            "--tags",
            &repository,
            &format!("refs/tags/{tag}*"),
        ],
    )
    .map_err(|error| format!("{repository} cannot be read: {error}"))?;
    let remote = String::from_utf8_lossy(&remote);
    let held = remote.lines().any(|line| {
        line.split_once('\t').is_some_and(|(hash, name)| {
            hash == commit
                && (name == format!("refs/tags/{tag}") || name == format!("refs/tags/{tag}^{{}}"))
        })
    });
    if !held {
        return Err(format!(
            "{repository} holds no tag `{tag}` of {commit}: `git push origin {tag}` pushes it"
        ));
    }
    // The registry's file of the plugin, of the version added.
    let (owner, name) = manifest
        .name
        .split_once('/')
        .ok_or_else(|| format!("`{}` is not owner/name", manifest.name))?;
    let registry_at = registry_at.unwrap_or(REGISTRY);
    let github = !registry_at.contains("://");
    let registry_url = if github {
        format!("https://github.com/{registry_at}.git")
    } else {
        registry_at.to_owned()
    };
    let clone = Clone(
        paths
            .cache
            .join("publish")
            .join(format!("{owner}.{name}.{version}-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&clone.0);
    std::fs::create_dir_all(clone.0.parent().unwrap_or(&clone.0))
        .map_err(|error| format!("{} cannot be made: {error}", clone.0.display()))?;
    git(
        None,
        &[
            "clone",
            "--quiet",
            "--depth",
            "1",
            "--",
            &registry_url,
            &clone.0.display().to_string(),
        ],
    )
    .map_err(|error| format!("{registry_url} cannot be cloned: {error}"))?;
    let path = registry::path_of_plugin(owner, name);
    let file = clone.0.join(&path);
    let before = std::fs::read_to_string(&file).ok();
    let after = with_version(before.as_deref(), &repository, &version, &tag, &commit)?;
    let branch = format!("publish/{owner}-{name}-{version}");
    let title = format!("{} {version}", manifest.name);
    out.tell(
        "checked",
        json!({ "name": manifest.name, "version": version, "tag": tag, "commit": commit }),
        &format!("{title} of the tag `{tag}`, {commit}: checked and tested."),
    );
    if !github {
        // A registry of git that is not GitHub's: a branch pushed to it.
        std::fs::create_dir_all(file.parent().unwrap_or(&clone.0))
            .and_then(|()| std::fs::write(&file, &after))
            .map_err(|error| format!("{} cannot be written: {error}", file.display()))?;
        let at = Some(clone.0.as_path());
        git(at, &["switch", "--quiet", "-c", &branch])?;
        git(at, &["add", "--", &path])?;
        git(
            at,
            &[
                "-c",
                "user.name=Sleip",
                "-c",
                "user.email=sleip@lokyr.dev",
                "commit",
                "--quiet",
                "-m",
                &title,
            ],
        )?;
        git(at, &["push", "--quiet", "origin", &branch])
            .map_err(|error| format!("{registry_url} takes no branch: {error}"))?;
        out.tell(
            "pushed",
            json!({ "branch": branch, "registry": registry_url }),
            &format!("The branch {branch} of {registry_url} holds {path}: its pull request asks to merge it.\n{CHECKED}"),
        );
        return Ok(());
    }
    if link || !gh_ready() {
        let added = after
            .strip_prefix(before.as_deref().unwrap_or_default().trim_end())
            .unwrap_or(&after)
            .trim_start();
        return tell_link(out, registry_at, &path, before.is_some(), &after, added);
    }
    let url = pull_request(registry_at, &path, &branch, &title, &after)?;
    out.tell(
        "opened",
        json!({ "url": url, "name": manifest.name, "version": version }),
        &format!("The pull request of {title}: {url}\n{CHECKED}"),
    );
    Ok(())
}

/// The registry's file of a plugin, `before` if it has one, of the version
/// `version` of `tag` at `commit` added at its end, or made of it and of
/// the plugin's `repository`: refused of a version that it holds, or that
/// is not above its last one.
fn with_version(
    before: Option<&str>,
    repository: &str,
    version: &str,
    tag: &str,
    commit: &str,
) -> Result<String, String> {
    let block = format!(
        "\n[[versions]]\nversion = \"{version}\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\n"
    );
    let text = match before {
        Some(before) => {
            let file = PluginFile::parse(before)?;
            if file.version(version).is_some() {
                return Err(format!("the registry holds {version} already"));
            }
            if let Some(last) = file.versions.last() {
                let (new, old) = (
                    Version::parse(version).map_err(|error| error.to_string())?,
                    Version::parse(&last.version).map_err(|error| error.to_string())?,
                );
                if new <= old {
                    return Err(format!(
                        "{version} is not above {}, the registry's last version of the plugin",
                        last.version
                    ));
                }
            }
            format!("{}\n{block}", before.trim_end())
        }
        None => format!("repository = \"{repository}\"\n{block}"),
    };
    PluginFile::parse(&text)
        .map_err(|error| format!("the registry's file would not read: {error}"))?;
    Ok(text)
}

/// Whether `gh` runs and holds an account of GitHub.
fn gh_ready() -> bool {
    std::process::Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Runs `gh` of `args`, `input` on its standard input if given: its
/// output, trimmed, or its error.
fn gh(args: &[&str], input: Option<&str>) -> Result<String, String> {
    use std::io::Write as _;

    let mut command = std::process::Command::new("gh");
    command
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("gh cannot run: {error}"))?;
    // Its input, or none, its standard input closed either way.
    if let Some(mut stdin) = child.stdin.take()
        && let Some(input) = input
    {
        stdin
            .write_all(input.as_bytes())
            .map_err(|error| format!("gh takes no input: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("gh cannot run: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Opens the pull request of `text`, the file `path`, to the registry of
/// GitHub `registry`, of the branch `branch` and the title `title`: of the
/// author's fork, or of the registry itself for its owner, the branch and
/// the file made through GitHub's API; its address.
fn pull_request(
    registry: &str,
    path: &str,
    branch: &str,
    title: &str,
    text: &str,
) -> Result<String, String> {
    use base64::Engine as _;

    let login = gh(&["api", "user", "--jq", ".login"], None)?;
    let (registry_owner, registry_name) = registry
        .split_once('/')
        .ok_or_else(|| format!("`{registry}` is not owner/repository"))?;
    let target = if login.eq_ignore_ascii_case(registry_owner) {
        registry.to_owned()
    } else {
        gh(
            &[
                "repo",
                "fork",
                registry,
                "--clone=false",
                "--default-branch-only",
            ],
            None,
        )?;
        format!("{login}/{registry_name}")
    };
    let main = gh(
        &[
            "api",
            &format!("repos/{registry}/git/ref/heads/main"),
            "--jq",
            ".object.sha",
        ],
        None,
    )?;
    let reference = json!({ "ref": format!("refs/heads/{branch}"), "sha": main }).to_string();
    gh(
        &[
            "api",
            "-X",
            "POST",
            &format!("repos/{target}/git/refs"),
            "--input",
            "-",
        ],
        Some(&reference),
    )
    .map_err(|error| format!("the branch {branch} of {target} cannot be made: {error}"))?;
    // The file's blob of main, if the plugin has one, which the change
    // replaces.
    let sha = gh(
        &[
            "api",
            &format!("repos/{target}/contents/{path}?ref={branch}"),
            "--jq",
            ".sha",
        ],
        None,
    )
    .ok();
    let mut change = json!({
        "message": title,
        "content": base64::engine::general_purpose::STANDARD.encode(text),
        "branch": branch,
    });
    if let Some(sha) = sha {
        change["sha"] = sha.into();
    }
    gh(
        &[
            "api",
            "-X",
            "PUT",
            &format!("repos/{target}/contents/{path}"),
            "--input",
            "-",
        ],
        Some(&change.to_string()),
    )
    .map_err(|error| format!("{path} of {target} cannot be written: {error}"))?;
    let head = if target == registry {
        branch.to_owned()
    } else {
        format!("{login}:{branch}")
    };
    gh(
        &[
            "pr", "create", "--repo", registry, "--base", "main", "--head", &head, "--title",
            title, "--body", CHECKED,
        ],
        None,
    )
}

/// Tells the link that proposes `text`, the file `path` of the registry of
/// GitHub `registry`: a new file's, of its text, or an existing one's
/// edit, and `added`, the text to put at its end, as `existing` says.
fn tell_link(
    out: Out,
    registry: &str,
    path: &str,
    existing: bool,
    text: &str,
    added: &str,
) -> Result<(), String> {
    let (folder, file) = path.rsplit_once('/').unwrap_or(("", path));
    let url = if existing {
        format!("https://github.com/{registry}/edit/main/{path}")
    } else {
        format!(
            "https://github.com/{registry}/new/main/{folder}?filename={file}&value={}",
            url_encoded(text)
        )
    };
    let how = if existing {
        format!(
            "Open it, put the version at the end of the file, as follows, then \"Propose changes\":\n\n{added}"
        )
    } else {
        "Open it, then \"Propose new file\": GitHub forks the registry and opens the pull request."
            .to_owned()
    };
    out.tell(
        "link",
        json!({ "url": url, "path": path, "text": text }),
        &format!("{url}\n{how}\n{CHECKED}"),
    );
    Ok(())
}

/// `text` as a URL's query takes it: every byte but the unreserved ones of
/// RFC 3986 percent-encoded.
fn url_encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_goes_at_the_file_s_end_above_its_last() {
        let commit = "a".repeat(40);
        let first = with_version(
            None,
            "https://github.com/acme/hello",
            "0.1.0",
            "v0.1.0",
            &commit,
        )
        .unwrap();
        assert!(
            first.starts_with("repository = \"https://github.com/acme/hello\"\n"),
            "{first}"
        );
        let second = with_version(Some(&first), "x", "0.1.1", "v0.1.1", &"b".repeat(40)).unwrap();
        let file = PluginFile::parse(&second).unwrap();
        assert_eq!(file.versions.len(), 2);
        assert_eq!(file.versions[1].version, "0.1.1");
        assert!(with_version(Some(&second), "x", "0.1.1", "v0.1.1", &commit).is_err());
        assert!(with_version(Some(&second), "x", "0.1.0-rc.1", "v0", &commit).is_err());
        assert_eq!(url_encoded("a b/\n"), "a%20b%2F%0A");
    }
}
