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
//! packages a version's archive again, alike, of the commit that the file
//! pins whatever became of its tag, which the CI of the registry's main
//! branch publishes; `versions` lists every version, of
//! its release's names. `keygen` and `sign-keys` make the registry's keys
//! and its `keys.json`, offline; `index` writes its index, signed, of the
//! archives of its releases, which the CI of the main branch runs.

use std::collections::BTreeSet;
use std::path::Path;

use lokyr_pkg_model::manifest::{Kind, Manifest};
use lokyr_pkg_model::store::Digest;
use serde_json::json;
use sleip_index::registry::{self, PluginFile, Release};
use sleip_index::signing::{self, Dependency, Entry, PublicKey, SecretKey, Timestamp};

use super::{git, git_bytes};
use crate::args::Registry;

mod site;
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
            files_alone,
        } => verify(
            out,
            &PullRequest {
                head,
                base,
                author,
                orgs,
                local: *local,
                files_alone: *files_alone,
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
        Registry::Keygen { out: file } => keygen(out, file),
        Registry::SignKeys {
            root,
            keys,
            out: file,
        } => sign_keys(out, root, keys, file),
        Registry::Index {
            registry,
            archives,
            out: into,
            roots,
            dl,
        } => index(
            out,
            registry,
            archives,
            into,
            roots.as_deref(),
            dl.as_deref(),
        ),
        Registry::Site {
            registry,
            archives,
            out: into,
            roots,
        } => site::site(out, registry, archives, into, &roots_of(roots.as_deref())?),
    }
}

/// The root keys of the file `roots`, a key a line, or Sleip's own.
fn roots_of(roots: Option<&Path>) -> Result<Vec<PublicKey>, String> {
    match roots {
        Some(file) => std::fs::read_to_string(file)
            .map_err(|error| format!("{} cannot be read: {error}", file.display()))?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PublicKey::parse)
            .collect(),
        None => Ok(signing::roots()),
    }
}

/// Makes a key, its secret written to the new file `file`, which no one
/// else reads, as `out` tells its public part.
fn keygen(out: Out, file: &Path) -> Result<(), String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let key = SecretKey::generate()?;
    let mut written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(file)
        .map_err(|error| format!("{} cannot be made: {error}", file.display()))?;
    written
        .write_all(key.file_text().as_bytes())
        .and_then(|()| written.sync_all())
        .map_err(|error| format!("{} cannot be written: {error}", file.display()))?;
    let public = key.public().to_string();
    out.tell(
        "key",
        json!({ "public": public, "file": file.display().to_string() }),
        &format!(
            "The key's secret is in {}, which no one else reads: keep it as such.\nIts public part: {public}",
            file.display()
        ),
    );
    Ok(())
}

/// Writes `keys.json` to `file`, of the index keys `keys`, signed by the
/// root key whose secret the file `root` holds, as `out` tells.
fn sign_keys(out: Out, root: &Path, keys: &[String], file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(root)
        .map_err(|error| format!("{} cannot be read: {error}", root.display()))?;
    let root_key =
        SecretKey::parse(&text).map_err(|error| format!("{}: {error}", root.display()))?;
    let keys: Vec<PublicKey> = keys
        .iter()
        .map(|key| PublicKey::parse(key))
        .collect::<Result<_, _>>()?;
    if keys.is_empty() {
        return Err("keys.json lists an index key at least".to_owned());
    }
    let signed = signing::sign_keys(&root_key, &keys, Timestamp::now());
    lokyr_pkg_model::write_whole(file, &signed)
        .map_err(|error| format!("{} cannot be written: {error}", file.display()))?;
    let public = root_key.public();
    if !signing::roots().contains(&public) {
        out.tell(
            "note",
            json!({ "message": format!("{public} is no root key of this Sleip's, which reads none of its keys.json") }),
            &format!("{public} is no root key of this Sleip's, which reads none of its keys.json."),
        );
    }
    out.tell(
        "signed",
        json!({
            "root": public.to_string(),
            "keys": keys.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "file": file.display().to_string(),
        }),
        &format!(
            "{} lists {} index keys, signed by {public}.",
            file.display(),
            keys.len()
        ),
    );
    Ok(())
}

/// The kind of `kind`, as a manifest and the index write it.
fn kind_text(kind: Kind) -> &'static str {
    match kind {
        Kind::Luau => "luau",
        Kind::Themes => "themes",
        Kind::Builtin => "builtin",
    }
}

/// Writes the index of the registry in `registry` into `into`, of the
/// archives of its releases in `archives`, `<release>/<archive>`, each
/// version of an archive a line of its plugin's file, signed by the index
/// key that `SLEIP_INDEX_KEY` holds, which the registry's `keys.json`
/// lists, checked against the root keys of the file `roots`, a key a line,
/// or Sleip's own; and `config.json`, of the archives' address `dl` or the
/// registry's; as `out` tells.
fn index(
    out: Out,
    registry: &Path,
    archives: &Path,
    into: &Path,
    roots: Option<&Path>,
    dl: Option<&str>,
) -> Result<(), String> {
    let key = std::env::var("SLEIP_INDEX_KEY")
        .map_err(|_| "SLEIP_INDEX_KEY holds no index key".to_owned())
        .and_then(|text| {
            SecretKey::parse(&text).map_err(|error| format!("SLEIP_INDEX_KEY: {error}"))
        })?;
    let roots = roots_of(roots)?;
    let keys_path = registry.join("keys.json");
    let keys_text = std::fs::read_to_string(&keys_path)
        .map_err(|error| format!("{} cannot be read: {error}", keys_path.display()))?;
    let keys = signing::verify_keys(&keys_text, &roots, None)?;
    if !keys.keys.contains(&key.public()) {
        return Err(format!(
            "the index key {} is not in keys.json, whose index keys are {}",
            key.public(),
            keys.keys
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let made = |path: &Path, text: &str| {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{} cannot be made: {error}", parent.display()))?;
        }
        std::fs::write(path, text)
            .map_err(|error| format!("{} cannot be written: {error}", path.display()))
    };
    made(&into.join("keys.json"), &keys_text)?;
    let config = json!({ "v": signing::VERSION, "dl": dl.unwrap_or(registry::DOWNLOADS) });
    made(&into.join("config.json"), &format!("{config}\n"))?;
    let generated = Timestamp::now();
    for (path, bytes) in &files_of(registry)? {
        let Some((owner, name)) = registry::plugin_of_path(path) else {
            continue;
        };
        let plugin = format!("{owner}/{name}");
        let file = PluginFile::parse(&String::from_utf8_lossy(bytes))
            .map_err(|error| format!("{path}: {error}"))?;
        let mut entries = Vec::new();
        for release in &file.versions {
            let tag = registry::release_tag(owner, name, &release.version);
            let archive = archives
                .join(&tag)
                .join(registry::archive_name(name, &release.version));
            // A version of no release yet, which a later run takes.
            let Ok(archive_bytes) = std::fs::read(&archive) else {
                out.tell(
                    "note",
                    json!({ "message": format!("{plugin} {} has no release yet", release.version) }),
                    &format!("{plugin} {} has no release yet.", release.version),
                );
                continue;
            };
            let manifest = manifest_of_archive(&archive_bytes)
                .map_err(|error| format!("{}: {error}", archive.display()))?;
            if manifest.name != plugin
                || manifest.version.as_deref() != Some(release.version.as_str())
            {
                return Err(format!(
                    "{} holds {} {}, not {plugin} {}",
                    archive.display(),
                    manifest.name,
                    manifest.version.as_deref().unwrap_or("of no version"),
                    release.version
                ));
            }
            entries.push(Entry {
                v: signing::VERSION,
                name: plugin.clone(),
                vers: release.version.clone(),
                kind: kind_text(manifest.kind).to_owned(),
                api: manifest.api.clone(),
                deps: manifest
                    .dependencies
                    .iter()
                    .map(|(name, req)| Dependency {
                        name: name.clone(),
                        req: req.clone(),
                    })
                    .collect(),
                caps: manifest
                    .capabilities
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                uses: manifest.uses.clone(),
                cksum: Digest::of_bytes(&archive_bytes).to_string(),
                yanked: release.yanked,
            });
        }
        let text = signing::sign_index(&plugin, &entries, &key, generated);
        // Read back as Sleip reads it, of the keys that it checked.
        signing::verify_index(&text, &plugin, &keys, None)
            .map_err(|error| format!("the index that this run signed does not check: {error}"))?;
        made(&into.join("index").join(owner).join(name), &text)?;
        out.tell(
            "indexed",
            json!({ "name": plugin, "versions": entries.len() }),
            &format!("{plugin}: {} versions", entries.len()),
        );
    }
    Ok(())
}

/// The manifest of the package of `archive`, unpacked into a folder of its
/// own for the time it takes, as the store unpacks it.
fn manifest_of_archive(archive: &[u8]) -> Result<Manifest, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static UNPACKS: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "sleip-index-{}-{}",
        std::process::id(),
        UNPACKS.fetch_add(1, Ordering::Relaxed)
    ));
    let unpacked = lokyr_pkg_model::archive::unpack(archive, &folder);
    let _ = std::fs::remove_dir_all(&folder);
    unpacked
        .map(|unpacked| unpacked.manifest)
        .map_err(|error| error.to_string())
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
    /// Whether its files are judged alone, no version cloned, checked,
    /// tested nor packaged.
    files_alone: bool,
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
                "it changes {}{}, which is no plugin's file, `plugins/<owner>/<name>.toml`: the maintainer reviews it",
                literal(path),
                told(change)
            ));
            continue;
        };
        let before = find(&base, path);
        if change.after == NO_ENTRY {
            if before.is_some() {
                changed += 1;
                verdict.review.push(format!(
                    "it takes {} out of the registry, which the maintainer reviews",
                    literal(&format!("{owner}/{name}"))
                ));
            }
            continue;
        }
        if change.after != PLAIN_FILE {
            changed += 1;
            verdict.refused.push(format!(
                "{} is {}: a plugin's file is a plain file",
                literal(path),
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
            "{} names no plugin: its owner and its name are lowercase letters, digits and `-`, from a letter, of {} characters at most",
            literal(&path),
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
            "`{plugin}` is `{owner}`'s, and {}, who opened the pull request, is neither `{owner}` nor a public member of an organization `{owner}`",
            literal(request.author)
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
                "the name `{name}` is {}'s: a plugin's name is unique across owners, as Lokyr knows a plugin by its name alone",
                literal(&format!("{holder}/{name}"))
            ));
        }
    }
    let file = match PluginFile::parse(after) {
        Ok(file) => file,
        Err(error) => {
            verdict
                .refused
                .push(format!("`{path}`: {}", literal(&error)));
            return;
        }
    };
    if !request.local && !file.repository.starts_with("https://") {
        verdict.refused.push(format!(
            "`{path}`: `repository` is the address of a git repository over HTTPS, not {}",
            literal(&file.repository)
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
                    literal(&published.repository),
                    literal(&file.repository)
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
    if request.files_alone {
        return;
    }
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
            Err(problems) => {
                verdict.refused.extend(problems.into_iter().map(|problem| {
                    format!("`{plugin}` {}: {}", release.version, literal(&problem))
                }));
            }
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
                .map(|capability| format!("the capability {}", literal(capability)))
                .chain(
                    asked
                        .1
                        .difference(&had_uses)
                        .map(|command| format!("the command {}", literal(command))),
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
            "`{plugin}` {}, published last, cannot be read, against which the new versions' capabilities are weighed: {}",
            last.version,
            literal(&error)
        )),
    }
}

/// Packages the version `release` of the plugin `plugin`, of its
/// repository `repository`, into `into`: of `checked`, cloned at its tag,
/// which must name its commit, checked and tested, as a pull request's
/// version is; else at its commit alone, which pins its files, whatever
/// became of its tag since it merged; its manifest of its name and
/// version; or each problem found.
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
    let at = if checked { At::Tag } else { At::Commit };
    clone_at(repository, release, at, &folder).map_err(|error| vec![error])?;
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
/// published before, of its commit, its clone in `into` for the time it
/// takes.
fn manifest_at(
    plugin: &str,
    repository: &str,
    release: &Release,
    into: &Path,
) -> Result<Manifest, String> {
    let folder =
        into.join("published")
            .join(format!("{}-{}", plugin.replace('/', "."), release.version));
    clone_at(repository, release, At::Commit, &folder)?;
    let manifest = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| error.to_string())
        .and_then(|text| Manifest::parse(&text).map_err(|error| error.to_string()));
    let _ = std::fs::remove_dir_all(&folder);
    manifest
}

/// What a clone of a version's repository is checked out at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum At {
    /// Its commit, which its tag must name, as a pull request's version is
    /// checked.
    Tag,
    /// Its commit alone, which the registry's file pins, whatever became of
    /// its tag: fetched by its hash if no branch nor tag of the clone holds
    /// it.
    Commit,
}

/// Clones `repository` into `folder` at the commit of `release`, as `at`
/// says.
fn clone_at(repository: &str, release: &Release, at: At, folder: &Path) -> Result<(), String> {
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
    let commit = &release.commit;
    match at {
        At::Tag => {
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
            if named != *commit {
                return Err(format!(
                    "the tag `{tag}` of {repository} names the commit {named}, not {commit}"
                ));
            }
        }
        At::Commit => {
            let held = |folder: &Path| {
                git(
                    Some(folder),
                    &["cat-file", "-e", &format!("{commit}^{{commit}}")],
                )
                .is_ok()
            };
            if !held(folder) {
                // A commit that its branches and tags left, which a host
                // may give still by its hash.
                let _ = git(
                    Some(folder),
                    &["fetch", "--quiet", "--no-tags", "origin", commit],
                );
                if !held(folder) {
                    return Err(format!("{repository} holds the commit {commit} no longer"));
                }
            }
        }
    }
    git(Some(folder), &["checkout", "--quiet", "--detach", commit])
        .map_err(|error| format!("{repository} at {commit} cannot be checked out: {error}"))?;
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

/// The most characters of a text of a pull request or of a plugin that the
/// report shows.
const LONGEST_LITERAL: usize = 500;

/// The most bytes of the report, below the 65 536 characters of a comment
/// of GitHub's.
const LONGEST_REPORT: usize = 60_000;

/// `text`, of a pull request or of a plugin, as a span of code in the
/// report, which shows it as it is: of no backtick, which would end the
/// span, a quote in its place; of no `@` nor `<`, which would mention
/// someone or open HTML; a control character a space; and of
/// [`LONGEST_LITERAL`] characters at most.
fn literal(text: &str) -> String {
    let mut kept = String::new();
    for (count, character) in text
        .chars()
        .filter(|character| !matches!(character, '@' | '<'))
        .enumerate()
    {
        if count == LONGEST_LITERAL {
            kept.push('…');
            break;
        }
        kept.push(match character {
            '`' => '\'',
            character if character.is_control() => ' ',
            character => character,
        });
    }
    if kept.trim().is_empty() {
        return "an empty text".to_owned();
    }
    format!("`{kept}`")
}

/// The report of `verdict`, headed `heading`, in Markdown, as the pull
/// request's comment shows it, of [`LONGEST_REPORT`] bytes at most: the
/// lines past them left to the job's log, whose verdict holds them all.
fn report_of(verdict: &Verdict, heading: &str) -> String {
    let whole = whole_report_of(verdict, heading);
    if whole.len() <= LONGEST_REPORT {
        return whole;
    }
    let ending = "\n… The report is longer than a comment holds: the job's log has it whole.\n";
    let mut cut = LONGEST_REPORT - ending.len();
    while !whole.is_char_boundary(cut) {
        cut -= 1;
    }
    let cut = whole[..cut].rfind('\n').map_or(cut, |at| at + 1);
    format!("{}{ending}", &whole[..cut])
}

/// The report of `verdict`, headed `heading`, whole.
fn whole_report_of(verdict: &Verdict, heading: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_of_a_pull_request_is_a_span_of_code_of_no_mention_nor_html_and_bounded() {
        assert_eq!(
            literal("a `b` @all <img src=x>\nnext"),
            "`a 'b' all img src=x> next`"
        );
        let shown = literal(&"x".repeat(LONGEST_LITERAL + 10));
        assert_eq!(shown.chars().count(), LONGEST_LITERAL + 3);
        assert!(shown.ends_with("x…`"), "{shown}");
        assert_eq!(literal("\n"), "an empty text");
    }

    #[test]
    fn a_report_longer_than_a_comment_holds_is_cut_at_a_line() {
        let tail = "é".repeat(40);
        let verdict = Verdict {
            refused: (0..2000).map(|at| format!("reason {at} {tail}")).collect(),
            ..Verdict::default()
        };
        let report = report_of(&verdict, "Refused");
        assert!(report.len() <= LONGEST_REPORT, "{}", report.len());
        assert!(report.starts_with("### Sleip's check: Refused\n"));
        assert!(
            report.ends_with("the job's log has it whole.\n"),
            "{report}"
        );
        let reasons: Vec<&str> = report
            .lines()
            .filter(|line| line.starts_with("- reason"))
            .collect();
        assert!(reasons.len() > 100 && reasons.len() < 2000);
        assert!(reasons.iter().all(|line| line.ends_with(&tail)));
        // A short one, whole.
        let short = Verdict {
            refused: vec!["one".to_owned()],
            ..Verdict::default()
        };
        assert_eq!(
            report_of(&short, "Refused"),
            whole_report_of(&short, "Refused")
        );
    }
}
