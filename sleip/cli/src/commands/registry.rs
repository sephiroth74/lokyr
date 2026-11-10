//! `sleip registry`, hidden from the help: the registry's tools, which its
//! CI runs. `verify` checks a pull request of the registry's repository,
//! which publishes a plugin's versions, a file of each plugin,
//! `plugins/<owner>/<name>.toml`: the changes of the pull request alone,
//! since the commit that it shares with the main branch, each judged
//! against the main branch as it is now; a plugin's owner the author or
//! one of the author's organizations, its name no other owner's nor one
//! that Lokyr holds, the versions published left as they were, and each
//! new version cloned at its tag,
//! which must name its commit, its manifest the file's, checked as
//! `sleip check` checks it, tested as `sleip test` tests it, and packaged;
//! then whether it merges by itself, of a version of a plugin published
//! before that asks for nothing new, or the maintainer reviews it. `build`
//! packages a version's archive again, alike, which the CI of the
//! registry's main branch publishes; `versions` lists every version, of
//! its release's names.

use std::collections::BTreeSet;
use std::path::Path;

use lokyr_pkg_model::manifest::{Kind, Manifest};
use lokyr_pkg_model::store::Digest;
use serde_json::json;
use sleip_index::registry::{self, PluginFile, Release};

use super::{git, git_bytes};
use crate::args::Registry;
use crate::output::Out;

/// Runs the registry's tool `tool`, as `out` tells.
pub fn run(out: Out, tool: &Registry) -> Result<(), String> {
    match tool {
        Registry::Verify {
            head,
            base,
            author,
            orgs,
            out: into,
            report,
            local,
        } => verify(
            out,
            &PullRequest {
                head,
                base,
                author,
                orgs,
                local: *local,
            },
            into,
            report.as_deref(),
        ),
        Registry::Build {
            plugin,
            version,
            registry,
            out: into,
            local,
        } => build(out, registry, plugin, version, into, *local),
        Registry::Versions { registry } => versions(out, registry),
    }
}

/// A pull request of the registry.
struct PullRequest<'a> {
    /// Its git repository, at its head.
    head: &'a Path,
    /// The git repository of the branch that it merges into, at its head.
    base: &'a Path,
    /// Its author's login on GitHub.
    author: &'a str,
    /// The organizations that its author is a public member of.
    orgs: &'a [String],
    /// Whether repositories of `file://` are taken.
    local: bool,
}

/// What a pull request's check found.
#[derive(Default)]
struct Verdict {
    /// Why it is refused, if it is.
    refused: Vec<String>,
    /// Why the maintainer reviews it, if they do.
    review: Vec<String>,
    /// What else is worth telling.
    notes: Vec<String>,
    /// The archives of the versions that it publishes.
    built: Vec<Built>,
}

/// A version's archive, packaged.
struct Built {
    /// `owner/name`.
    plugin: String,
    /// Its version.
    version: String,
    /// The tag of its release in the registry.
    release: String,
    /// The archive's SHA-256.
    digest: Digest,
    /// Its manifest.
    manifest: Manifest,
}

/// Checks the pull request `request`, its archives packaged into `into`,
/// its report of Markdown written to `report`: a failure of a pull request
/// refused, as `out` tells.
fn verify(
    out: Out,
    request: &PullRequest,
    into: &Path,
    report: Option<&Path>,
) -> Result<(), String> {
    std::fs::create_dir_all(into)
        .map_err(|error| format!("{} cannot be made: {error}", into.display()))?;
    let changes = changes_of(request.head, request.base)?;
    let base = files_of(request.base)?;
    let mut verdict = Verdict::default();
    // The plugins' names, of the main branch's and of those that the pull
    // request adds or changes.
    let mut names: BTreeSet<(String, String)> = base
        .iter()
        .filter_map(|(path, _)| registry::plugin_of_path(path))
        .map(|(owner, name)| (owner.to_owned(), name.to_owned()))
        .collect();
    names.extend(
        changes
            .iter()
            .filter(|change| change.after != NO_ENTRY)
            .filter_map(|change| registry::plugin_of_path(&change.path))
            .map(|(owner, name)| (owner.to_owned(), name.to_owned())),
    );
    let mut changed = 0;
    for change in &changes {
        let path = &change.path;
        let Some((owner, name)) = registry::plugin_of_path(path) else {
            // Another file: the pull request's own change, whatever the
            // main branch holds now.
            changed += 1;
            verdict.review.push(format!(
                "it changes `{path}`{}, which is no plugin's file, `plugins/<owner>/<name>.toml`: the maintainer reviews it",
                told(change)
            ));
            continue;
        };
        let before = find(&base, path);
        if change.after == NO_ENTRY {
            if before.is_some() {
                changed += 1;
                verdict.review.push(format!(
                    "it takes `{owner}/{name}` out of the registry, which the maintainer reviews"
                ));
            }
            continue;
        }
        if change.after != PLAIN_FILE {
            changed += 1;
            verdict.refused.push(format!(
                "`{path}` is {}: a plugin's file is a plain file",
                kind_of(&change.after)
            ));
            continue;
        }
        let after = git_bytes(Some(request.head), &["cat-file", "blob", &change.object])
            .map_err(|error| format!("`{path}` of the pull request cannot be read: {error}"))?;
        // As the main branch holds it already, it changes nothing.
        if before == Some(after.as_slice()) {
            continue;
        }
        changed += 1;
        let before = before.map(|bytes| String::from_utf8_lossy(bytes).into_owned());
        let after = String::from_utf8_lossy(&after).into_owned();
        verify_plugin(
            out,
            request,
            &names,
            owner,
            name,
            before.as_deref(),
            &after,
            into,
            &mut verdict,
        );
    }
    if changed == 0 {
        verdict
            .refused
            .push("it changes no file of the registry".to_owned());
    }
    let (word, text) = if !verdict.refused.is_empty() {
        ("refused", "Refused")
    } else if !verdict.review.is_empty() {
        ("review", "For the maintainer's review")
    } else {
        ("merge", "Merges by itself")
    };
    let markdown = report_of(&verdict, text);
    if let Some(report) = report {
        std::fs::write(report, &markdown)
            .map_err(|error| format!("{} cannot be written: {error}", report.display()))?;
    }
    out.tell(
        "verdict",
        json!({
            "verdict": word,
            "refused": verdict.refused,
            "review": verdict.review,
            "notes": verdict.notes,
        }),
        &markdown,
    );
    if verdict.refused.is_empty() {
        Ok(())
    } else {
        // The verdict told why.
        Err(String::new())
    }
}

/// Checks the file of the plugin `owner/name` that the pull request
/// `request` changes, `before` of the branch that it merges into, if there,
/// and `after` of the pull request, of the plugins' names `names`, each
/// its owner and its name: each version that it publishes packaged into
/// `into`, into `verdict`.
#[allow(clippy::too_many_arguments)]
fn verify_plugin(
    out: Out,
    request: &PullRequest,
    names: &BTreeSet<(String, String)>,
    owner: &str,
    name: &str,
    before: Option<&str>,
    after: &str,
    into: &Path,
    verdict: &mut Verdict,
) {
    let plugin = format!("{owner}/{name}");
    let path = registry::path_of_plugin(owner, name);
    if !registry::is_part(owner) || !registry::is_part(name) {
        verdict.refused.push(format!(
            "`{path}` names no plugin: its owner and its name are lowercase letters, digits and `-`, from a letter, of {} characters at most",
            registry::LONGEST_PART
        ));
        return;
    }
    if owner == registry::RESERVED_OWNER {
        verdict.refused.push(format!(
            "`{}/*` belongs to Lokyr's first-party plugins",
            registry::RESERVED_OWNER
        ));
        return;
    }
    let owns = request.author.eq_ignore_ascii_case(owner)
        || request
            .orgs
            .iter()
            .any(|org| org.eq_ignore_ascii_case(owner));
    if !owns {
        verdict.refused.push(format!(
            "`{plugin}` is `{owner}`'s, and `{}`, who opened the pull request, is neither `{owner}` nor a public member of an organization `{owner}`",
            request.author
        ));
    }
    if registry::RESERVED_NAMES.contains(&name) {
        verdict.refused.push(format!(
            "the name `{name}` is Lokyr's own, of a first-party plugin or of the app's commands, as Lokyr knows a plugin by its name alone"
        ));
    }
    for (holder, held) in names {
        if held == name && holder != owner {
            verdict.refused.push(format!(
                "the name `{name}` is `{holder}/{name}`'s: a plugin's name is unique across owners, as Lokyr knows a plugin by its name alone"
            ));
        }
    }
    let file = match PluginFile::parse(after) {
        Ok(file) => file,
        Err(error) => {
            verdict.refused.push(format!("`{path}`: {error}"));
            return;
        }
    };
    if !request.local && !file.repository.starts_with("https://") {
        verdict.refused.push(format!(
            "`{path}`: `repository` is the address of a git repository over HTTPS, not `{}`",
            file.repository
        ));
        return;
    }
    let published = before.and_then(|text| PluginFile::parse(text).ok());
    let new: &[Release] = match &published {
        None => {
            verdict.review.push(format!(
                "`{plugin}` is a new plugin, which the maintainer reviews"
            ));
            &file.versions
        }
        Some(published) => {
            if published.repository != file.repository {
                verdict.review.push(format!(
                    "`{plugin}`'s repository changes, from {} to {}",
                    published.repository, file.repository
                ));
            }
            for (at, release) in published.versions.iter().enumerate() {
                match file.versions.get(at) {
                    Some(now) if now.version == release.version && now.tag == release.tag && now.commit == release.commit => {
                        if now.yanked && !release.yanked {
                            verdict
                                .notes
                                .push(format!("`{plugin}` {} is yanked", release.version));
                        } else if !now.yanked && release.yanked {
                            verdict.review.push(format!(
                                "`{plugin}` {} comes back from its yank",
                                release.version
                            ));
                        }
                    }
                    _ => verdict.refused.push(format!(
                        "`{plugin}` {}, published, changes or goes: a published version stays as it was, and a later one, or its yank, takes its place",
                        release.version
                    )),
                }
            }
            file.versions
                .get(published.versions.len()..)
                .unwrap_or_default()
        }
    };
    let mut asked = (BTreeSet::new(), BTreeSet::new());
    for release in new {
        match package(out, &plugin, &file.repository, release, into, true) {
            Ok(built) => {
                asked
                    .0
                    .extend(built.manifest.capabilities.iter().map(ToString::to_string));
                asked.1.extend(built.manifest.uses.iter().cloned());
                verdict.built.push(built);
            }
            Err(problems) => verdict.refused.extend(
                problems
                    .into_iter()
                    .map(|problem| format!("`{plugin}` {}: {problem}", release.version)),
            ),
        }
    }
    // What the new versions ask for, against the version published last.
    let Some(last) = published
        .as_ref()
        .and_then(|published| published.versions.last())
    else {
        return;
    };
    if new.is_empty() {
        return;
    }
    match manifest_at(&plugin, &file.repository, last, into) {
        Ok(had) => {
            let had_capabilities: BTreeSet<String> =
                had.capabilities.iter().map(ToString::to_string).collect();
            let had_uses: BTreeSet<String> = had.uses.iter().cloned().collect();
            let more: Vec<String> = asked
                .0
                .difference(&had_capabilities)
                .map(|capability| format!("the capability `{capability}`"))
                .chain(
                    asked
                        .1
                        .difference(&had_uses)
                        .map(|command| format!("the command `{command}`")),
                )
                .collect();
            if !more.is_empty() {
                verdict.review.push(format!(
                    "`{plugin}` asks for more than {} did: {}",
                    last.version,
                    more.join(", ")
                ));
            }
        }
        Err(error) => verdict.review.push(format!(
            "`{plugin}` {}, published last, cannot be read, against which the new versions' capabilities are weighed: {error}",
            last.version
        )),
    }
}

/// Packages the version `release` of the plugin `plugin`, of its
/// repository `repository`, into `into`: cloned at its tag, which must name
/// its commit, its manifest of its name and version, and, of `checked`,
/// checked and tested; or each problem found.
fn package(
    out: Out,
    plugin: &str,
    repository: &str,
    release: &Release,
    into: &Path,
    checked: bool,
) -> Result<Built, Vec<String>> {
    let (owner, name) = plugin.split_once('/').unwrap_or_default();
    let folder = into
        .join("sources")
        .join(registry::release_tag(owner, name, &release.version));
    clone_at(repository, release, &folder).map_err(|error| vec![error])?;
    let manifest = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| format!("its commit holds no plugin.toml at its root: {error}"))
        .and_then(|text| {
            Manifest::parse(&text).map_err(|error| format!("its plugin.toml: {error}"))
        })
        .map_err(|error| vec![error])?;
    let mut problems = Vec::new();
    if manifest.name != plugin {
        problems.push(format!("its manifest names `{}`", manifest.name));
    }
    if manifest.version.as_deref() != Some(release.version.as_str()) {
        problems.push(format!(
            "its manifest's version is {}",
            manifest.version.as_deref().unwrap_or("none")
        ));
    }
    if manifest.kind != Kind::Luau {
        problems.push("it is no plugin of Luau, which the registry publishes alone".to_owned());
    }
    if checked && problems.is_empty() {
        problems.extend(
            super::check::problems_of(&folder, out)
                .into_iter()
                .map(|problem| format!("check: {problem}")),
        );
        match super::test::failures_of(&folder, out) {
            Ok(failures) => problems.extend(
                failures
                    .into_iter()
                    .map(|failure| format!("test: {failure}")),
            ),
            Err(error) => problems.push(format!("test: {error}")),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let bytes = lokyr_pkg_model::archive::pack(&folder).map_err(|error| vec![error.to_string()])?;
    let release_tag = registry::release_tag(owner, name, &release.version);
    let archive = into
        .join(&release_tag)
        .join(registry::archive_name(name, &release.version));
    if let Some(parent) = archive.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| vec![format!("{} cannot be made: {error}", parent.display())])?;
    }
    std::fs::write(&archive, &bytes)
        .map_err(|error| vec![format!("{} cannot be written: {error}", archive.display())])?;
    let digest = Digest::of_bytes(&bytes);
    let _ = std::fs::remove_dir_all(&folder);
    out.tell(
        "built",
        json!({
            "name": plugin,
            "version": release.version,
            "release": release_tag,
            "archive": archive.display().to_string(),
            "cksum": digest.to_string(),
        }),
        &format!(
            "{plugin} {}: {} ({digest})",
            release.version,
            archive.display()
        ),
    );
    Ok(Built {
        plugin: plugin.to_owned(),
        version: release.version.clone(),
        release: release_tag,
        digest,
        manifest,
    })
}

/// The manifest of the version `release` of `plugin`, of `repository`,
/// published before, its clone in `into` for the time it takes.
fn manifest_at(
    plugin: &str,
    repository: &str,
    release: &Release,
    into: &Path,
) -> Result<Manifest, String> {
    let folder =
        into.join("published")
            .join(format!("{}-{}", plugin.replace('/', "."), release.version));
    clone_at(repository, release, &folder)?;
    let manifest = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| error.to_string())
        .and_then(|text| Manifest::parse(&text).map_err(|error| error.to_string()));
    let _ = std::fs::remove_dir_all(&folder);
    manifest
}

/// Clones `repository` into `folder` at the commit of `release`, which its
/// tag must name.
fn clone_at(repository: &str, release: &Release, folder: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(folder);
    if let Some(parent) = folder.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} cannot be made: {error}", parent.display()))?;
    }
    git(
        None,
        &[
            "clone",
            "--quiet",
            "--no-checkout",
            "--",
            repository,
            &folder.display().to_string(),
        ],
    )
    .map_err(|error| format!("{repository} cannot be cloned: {error}"))?;
    let tag = &release.tag;
    let named = git(
        Some(folder),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag}^{{commit}}"),
        ],
    )
    .map_err(|_| format!("{repository} has no tag `{tag}`"))?;
    if named != release.commit {
        return Err(format!(
            "the tag `{tag}` of {repository} names the commit {named}, not {}",
            release.commit
        ));
    }
    git(
        Some(folder),
        &["checkout", "--quiet", "--detach", &release.commit],
    )
    .map_err(|error| {
        format!(
            "{repository} at {} cannot be checked out: {error}",
            release.commit
        )
    })?;
    Ok(())
}

/// Packages the archive of the version `version` of the plugin `plugin`
/// of the registry in `registry` into `into`, as `out` tells.
fn build(
    out: Out,
    registry: &Path,
    plugin: &str,
    version: &str,
    into: &Path,
    local: bool,
) -> Result<(), String> {
    let (owner, name) = plugin
        .split_once('/')
        .ok_or_else(|| format!("`{plugin}` is not `owner/name`"))?;
    let path = registry::path_of_plugin(owner, name);
    let text = std::fs::read_to_string(registry.join(&path))
        .map_err(|error| format!("{path} cannot be read: {error}"))?;
    let file = PluginFile::parse(&text).map_err(|error| format!("{path}: {error}"))?;
    if !local && !file.repository.starts_with("https://") {
        return Err(format!(
            "{path}: `repository` is the address of a git repository over HTTPS, not `{}`",
            file.repository
        ));
    }
    let release = file
        .version(version)
        .ok_or_else(|| format!("{path} publishes no version {version}"))?;
    std::fs::create_dir_all(into)
        .map_err(|error| format!("{} cannot be made: {error}", into.display()))?;
    package(out, plugin, &file.repository, release, into, false)
        .map(|_| ())
        .map_err(|problems| problems.join("; "))
}

/// Lists every version of the registry in `registry`, as `out` tells.
fn versions(out: Out, registry: &Path) -> Result<(), String> {
    let files = files_of(registry)?;
    for (path, bytes) in &files {
        let Some((owner, name)) = registry::plugin_of_path(path) else {
            continue;
        };
        let file = PluginFile::parse(&String::from_utf8_lossy(bytes))
            .map_err(|error| format!("{path}: {error}"))?;
        for release in &file.versions {
            let tag = registry::release_tag(owner, name, &release.version);
            out.tell(
                "version",
                json!({
                    "name": format!("{owner}/{name}"),
                    "version": release.version,
                    "tag": release.tag,
                    "commit": release.commit,
                    "repository": file.repository,
                    "release": tag,
                    "archive": registry::archive_name(name, &release.version),
                    "yanked": release.yanked,
                }),
                &format!("{owner}/{name} {} {tag}", release.version),
            );
        }
    }
    Ok(())
}

/// The mode of no entry, of a file added or removed.
const NO_ENTRY: &str = "000000";

/// The mode of a plain file, as git keeps it.
const PLAIN_FILE: &str = "100644";

/// A change of a pull request, since the commit that it shares with the
/// branch that it merges into, as git tells it.
struct Change {
    /// The path of its entry.
    path: String,
    /// The entry's mode before, `000000` of none.
    before: String,
    /// The entry's mode after, `000000` of none.
    after: String,
    /// The id of the entry's object after.
    object: String,
}

/// The changes of the pull request of the git repository `head` since the
/// commit that it shares with the branch of the git repository `base`,
/// which it fetches into it, as `refs/sleip/base`: each entry of the
/// trees, of its modes, so that a link, a submodule or a mode's change is
/// told, a rename the removal of a path and the addition of another.
fn changes_of(head: &Path, base: &Path) -> Result<Vec<Change>, String> {
    let base = std::fs::canonicalize(base)
        .map_err(|error| format!("{} cannot be read: {error}", base.display()))?;
    git(
        Some(head),
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--",
            &base.display().to_string(),
            "+HEAD:refs/sleip/base",
        ],
    )
    .map_err(|error| {
        format!(
            "the branch of {} cannot be fetched into the pull request's repository: {error}",
            base.display()
        )
    })?;
    let since = git(Some(head), &["merge-base", "refs/sleip/base", "HEAD"]).map_err(|error| {
        format!(
            "the pull request shares no commit with the branch that it merges into, whose whole history is needed: {error}"
        )
    })?;
    let raw = git_bytes(
        Some(head),
        &["diff-tree", "-r", "-z", "--no-renames", &since, "HEAD"],
    )
    .map_err(|error| format!("the pull request's changes cannot be read: {error}"))?;
    // Each change, `:<mode> <mode> <object> <object> <status>` and its
    // path, each of a NUL after it.
    let mut changes = Vec::new();
    let mut parts = raw.split(|byte| *byte == 0);
    while let Some(meta) = parts.next() {
        if meta.is_empty() {
            continue;
        }
        let meta = String::from_utf8_lossy(meta);
        let fields: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
        let (Some(path), [before, after, _, object, _]) = (parts.next(), fields.as_slice()) else {
            return Err(format!("git told a change that does not read: `{meta}`"));
        };
        changes.push(Change {
            path: String::from_utf8_lossy(path).into_owned(),
            before: (*before).to_owned(),
            after: (*after).to_owned(),
            object: (*object).to_owned(),
        });
    }
    Ok(changes)
}

/// What an entry of `mode` is, in words.
fn kind_of(mode: &str) -> String {
    match mode {
        PLAIN_FILE => "a file".to_owned(),
        "100755" => "an executable file".to_owned(),
        "120000" => "a link".to_owned(),
        "160000" => "a submodule".to_owned(),
        _ => format!("an entry of mode {mode}"),
    }
}

/// What `change` makes of its entry beside its content, in words after its
/// path: its removal, or what it adds or turns it into, of a mode other
/// than a plain file's.
fn told(change: &Change) -> String {
    match (change.before.as_str(), change.after.as_str()) {
        (_, NO_ENTRY) => ", which it removes".to_owned(),
        (NO_ENTRY, PLAIN_FILE) => String::new(),
        (NO_ENTRY, after) => format!(", {}", kind_of(after)),
        (before, after) if before == after => String::new(),
        (before, after) => format!(", from {} to {}", kind_of(before), kind_of(after)),
    }
}

/// The files of the registry in `folder`, its `.git` left out, by their
/// paths in it, of `/` between their folders, in their order, each of its
/// bytes.
fn files_of(folder: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    let mut left = vec![folder.to_path_buf()];
    while let Some(at) = left.pop() {
        let entries = std::fs::read_dir(&at)
            .map_err(|error| format!("{} cannot be read: {error}", at.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name() == ".git" {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                left.push(path);
            } else {
                let name = path
                    .strip_prefix(folder)
                    .map(|inner| {
                        inner
                            .iter()
                            .map(|part| part.to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/")
                    })
                    .unwrap_or_default();
                // A link counts by its target's path, as git keeps it.
                let bytes = if kind.is_symlink() {
                    std::fs::read_link(&path)
                        .map(|target| target.to_string_lossy().into_owned().into_bytes())
                } else {
                    std::fs::read(&path)
                }
                .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
                files.push((name, bytes));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// The bytes of the file `path` of `files`, if there.
fn find<'a>(files: &'a [(String, Vec<u8>)], path: &str) -> Option<&'a [u8]> {
    files
        .binary_search_by(|(at, _)| at.as_str().cmp(path))
        .ok()
        .map(|at| files[at].1.as_slice())
}

/// The report of `verdict`, headed `heading`, in Markdown, as the pull
/// request's comment shows it.
fn report_of(verdict: &Verdict, heading: &str) -> String {
    let mut text = format!("### Sleip's check: {heading}\n\n");
    for (title, lines) in [
        ("Refused", &verdict.refused),
        ("For the maintainer's review", &verdict.review),
        ("Notes", &verdict.notes),
    ] {
        if lines.is_empty() {
            continue;
        }
        text.push_str(&format!("**{title}**\n\n"));
        for line in lines {
            text.push_str(&format!("- {line}\n"));
        }
        text.push('\n');
    }
    if !verdict.built.is_empty() {
        text.push_str("| Plugin | Version | Release | Archive's SHA-256 |\n|---|---|---|---|\n");
        for built in &verdict.built {
            text.push_str(&format!(
                "| `{}` | {} | `{}` | `{}` |\n",
                built.plugin,
                built.version,
                built.release,
                built.digest.hex()
            ));
        }
        text.push('\n');
    }
    if verdict.refused.is_empty() && verdict.review.is_empty() {
        text.push_str("It asks for nothing that the versions published before did not, and merges by itself.\n");
    }
    text
}
