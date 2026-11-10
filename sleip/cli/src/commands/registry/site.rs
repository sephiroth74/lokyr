//! `sleip registry site`: the registry's site, static, of its index: a
//! home page of every plugin, which Pagefind searches once the CI runs it
//! over the pages, and a page of each plugin, of its versions, their
//! capabilities and checksums, and of its README, of the archive of the
//! version shown. The pages tell the plugins' own texts, a README's
//! Markdown among them, as text alone: no HTML of theirs, no link but of
//! the web, of mail, or into their repository at the version's commit.

use std::path::Path;

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};
use serde_json::json;
use sleip_index::registry::{self, PluginFile};
use sleip_index::signing::{self, Entry};

use crate::output::Out;

/// A plugin as the home page lists it.
struct Listed {
    name: String,
    version: String,
    description: String,
    yanked: bool,
}

/// A plugin as the index publishes it, of the version shown, the latest
/// not yanked or the latest of all.
struct Published {
    owner: String,
    name: String,
    /// `owner/name`.
    plugin: String,
    /// Its versions, as its index file holds them.
    entries: Vec<Entry>,
    /// The version shown.
    shown: Entry,
    /// Its manifest's description, license and README, of the version
    /// shown.
    description: String,
    license: String,
    readme: Option<String>,
    /// Its repository, as the registry's file names it.
    repository: String,
    /// Where its README's relative links lead.
    links: Links,
}

/// The plugins that the index in `index` publishes, checked against the
/// root keys `roots`, of the registry in `registry`, in their paths'
/// order; of `archives`, each of its license and README, of the archive of
/// its version shown, checked against the index's checksum: one that
/// cannot be had or does not check told as failed, its page of the index
/// alone.
fn published(
    out: Out,
    registry: &Path,
    archives: Option<&super::Archives>,
    index: &Path,
    roots: &[signing::PublicKey],
) -> Result<Vec<Published>, String> {
    let keys_path = index.join("keys.json");
    let keys_text = std::fs::read_to_string(&keys_path)
        .map_err(|error| format!("{} cannot be read: {error}", keys_path.display()))?;
    let keys = signing::verify_keys(&keys_text, roots, None)?;
    let mut published = Vec::new();
    for (path, bytes) in &super::files_of(registry)? {
        let Some((owner, name)) = registry::plugin_of_path(path) else {
            continue;
        };
        let plugin = format!("{owner}/{name}");
        let file = PluginFile::parse(&String::from_utf8_lossy(bytes))
            .map_err(|error| format!("{path}: {error}"))?;
        let index_path = index.join("index").join(owner).join(name);
        let Ok(text) = std::fs::read_to_string(&index_path) else {
            continue;
        };
        let entries = signing::verify_index(&text, &plugin, &keys, None)?.entries;
        let Some(shown) = entries
            .iter()
            .rev()
            .find(|entry| !entry.yanked)
            .or_else(|| entries.last())
            .cloned()
        else {
            continue;
        };
        let (license, readme) = match archives {
            Some(archives) => match package_of_shown(archives, owner, name, &shown) {
                Ok((manifest, readme)) => (manifest.license, readme),
                Err(error) => {
                    out.tell(
                        "failed",
                        json!({ "name": plugin, "version": shown.vers, "message": error }),
                        &format!(
                            "{plugin} {}: its page has no README nor license, as {error}",
                            shown.vers
                        ),
                    );
                    (String::new(), None)
                }
            },
            None => (String::new(), None),
        };
        let commit = file
            .version(&shown.vers)
            .map(|release| release.commit.clone())
            .unwrap_or_default();
        published.push(Published {
            owner: owner.to_owned(),
            name: name.to_owned(),
            plugin,
            description: shown.description.clone().unwrap_or_default(),
            entries,
            shown,
            license,
            readme,
            links: Links::of(&file.repository, &commit),
            repository: file.repository,
        });
    }
    Ok(published)
}

/// Writes the site of the registry in `registry` into `into`, which holds
/// its index, checked against the root keys `roots`, of the archives of
/// its releases in `archives` or downloaded of the index's address into
/// it, as `out` tells.
pub(super) fn site(
    out: Out,
    registry: &Path,
    archives: &Path,
    into: &Path,
    roots: &[signing::PublicKey],
) -> Result<(), String> {
    let config_path = into.join("config.json");
    let config: serde_json::Value = std::fs::read_to_string(&config_path)
        .map_err(|error| format!("{} cannot be read: {error}", config_path.display()))
        .and_then(|text| {
            serde_json::from_str(&text)
                .map_err(|error| format!("{} does not read: {error}", config_path.display()))
        })?;
    let dl = config["dl"]
        .as_str()
        .unwrap_or(registry::DOWNLOADS)
        .to_owned();
    let client = sleip_index::client::Client::new();
    let archives = super::Archives {
        folder: archives,
        dl: &dl,
        client: &client,
    };
    let mut listed = Vec::new();
    for plugin in published(out, registry, Some(&archives), into, roots)? {
        let page = plugin_page(
            &plugin.plugin,
            &plugin.description,
            &plugin.license,
            &plugin.repository,
            &plugin.entries,
            &plugin.shown,
            plugin
                .readme
                .as_deref()
                .map(|readme| markdown(readme, &plugin.links)),
        );
        let page_path = into
            .join("plugins")
            .join(&plugin.owner)
            .join(&plugin.name)
            .join("index.html");
        written(&page_path, &page)?;
        out.tell(
            "page",
            json!({ "name": plugin.plugin, "version": plugin.shown.vers }),
            &format!("{}: its page, of {}", plugin.plugin, plugin.shown.vers),
        );
        listed.push(Listed {
            name: plugin.plugin,
            version: plugin.shown.vers.clone(),
            description: plugin.description,
            yanked: plugin.shown.yanked,
        });
    }
    written(&into.join("style.css"), STYLE)?;
    written(&into.join("icon.svg"), ICON)?;
    written(&into.join("index.html"), &home_page(&listed))?;
    out.tell(
        "site",
        json!({ "plugins": listed.len() }),
        &format!("The site lists {} plugins.", listed.len()),
    );
    Ok(())
}

/// The mark of the README's list of the plugins, before it.
const LIST_START: &str = "<!-- plugins:start -->";

/// The mark of the README's list of the plugins, after it.
const LIST_END: &str = "<!-- plugins:end -->";

/// The most characters of a plugin's description that the README's list
/// shows.
const LONGEST_DESCRIPTION: usize = 200;

/// Rewrites the list of the plugins of the README `readme`, between its
/// marks, of the index in `index`, checked against the root keys `roots`,
/// and of the registry in `registry`, as `out` tells whether it changed.
pub(super) fn readme(
    out: Out,
    registry: &Path,
    index: &Path,
    readme: &Path,
    roots: &[signing::PublicKey],
) -> Result<(), String> {
    let text = std::fs::read_to_string(readme)
        .map_err(|error| format!("{} cannot be read: {error}", readme.display()))?;
    let (Some(start), Some(end)) = (text.find(LIST_START), text.find(LIST_END)) else {
        return Err(format!(
            "{} has no list of the plugins between `{LIST_START}` and `{LIST_END}`",
            readme.display()
        ));
    };
    if end < start {
        return Err(format!(
            "{}: `{LIST_END}` comes before `{LIST_START}`",
            readme.display()
        ));
    }
    let list = plugin_list(&published(out, registry, None, index, roots)?);
    let rewritten = format!("{}{LIST_START}\n{list}{}", &text[..start], &text[end..]);
    let changed = rewritten != text;
    if changed {
        lokyr_pkg_model::write_whole(readme, &rewritten)
            .map_err(|error| format!("{} cannot be written: {error}", readme.display()))?;
    }
    out.tell(
        "readme",
        json!({ "changed": changed }),
        if changed {
            "The README's list of the plugins changed."
        } else {
            "The README's list of the plugins is as it was."
        },
    );
    Ok(())
}

/// The README's list of `published`, a table of Markdown, each plugin's
/// texts as text alone.
fn plugin_list(published: &[Published]) -> String {
    if published.is_empty() {
        return "No plugin is published yet.\n".to_owned();
    }
    let mut list =
        "| Plugin | Version | Description | Capabilities |\n|---|---|---|---|\n".to_owned();
    for plugin in published {
        let yanked = if plugin.shown.yanked { " (yanked)" } else { "" };
        let capabilities = if plugin.shown.caps.is_empty() {
            "none".to_owned()
        } else {
            plugin
                .shown
                .caps
                .iter()
                .map(|capability| markdown_text(capability))
                .collect::<Vec<_>>()
                .join(", ")
        };
        list.push_str(&format!(
            "| [{name}](https://pkg.lokyr.dev/plugins/{name}/) | {}{yanked} | {} | {capabilities} |\n",
            markdown_text(&plugin.shown.vers),
            markdown_text(&plugin.description),
            name = plugin.plugin,
        ));
    }
    list
}

/// `text`, a plugin's, as Markdown's text alone, in a table's cell: no
/// HTML, link, image, code nor emphasis of its own, its punctuation
/// escaped, `<`, `>` and `&` as entities, an address of the web or of mail
/// broken by a space of no width, which GitHub links no more; a control
/// character a space; of [`LONGEST_DESCRIPTION`] characters at most.
fn markdown_text(text: &str) -> String {
    let mut out = String::new();
    for (count, character) in text.chars().enumerate() {
        if count == LONGEST_DESCRIPTION {
            out.push('…');
            break;
        }
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '!' | '|'
            | '~' => {
                out.push('\\');
                out.push(character);
            }
            '@' => out.push_str("@\u{200B}"),
            character if character.is_control() => out.push(' '),
            character => out.push(character),
        }
    }
    out.replace("://", ":\u{200B}//")
        .replace("www.", "www\u{200B}.")
        .replace("WWW.", "WWW\u{200B}.")
}

/// Writes `text` to `path`, its folders made.
fn written(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} cannot be made: {error}", parent.display()))?;
    }
    std::fs::write(path, text)
        .map_err(|error| format!("{} cannot be written: {error}", path.display()))
}

/// The manifest and README of the archive of the version `shown` of
/// `owner/name`, of `archives`, which must be of the index's checksum.
fn package_of_shown(
    archives: &super::Archives,
    owner: &str,
    name: &str,
    shown: &Entry,
) -> Result<(lokyr_pkg_model::manifest::Manifest, Option<String>), String> {
    let archive = archives
        .get(owner, name, &shown.vers)?
        .ok_or_else(|| "its archive has no release".to_owned())?;
    let digest = lokyr_pkg_model::store::Digest::of_bytes(&archive).to_string();
    if digest != shown.cksum {
        return Err(format!(
            "its archive is of {digest}, not of {}, which the index signed",
            shown.cksum
        ));
    }
    package_of(&archive)
}

/// The manifest and the README of the package of `archive`, unpacked into
/// a folder of its own for the time it takes.
fn package_of(
    archive: &[u8],
) -> Result<(lokyr_pkg_model::manifest::Manifest, Option<String>), String> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static UNPACKS: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "sleip-site-{}-{}",
        std::process::id(),
        UNPACKS.fetch_add(1, Ordering::Relaxed)
    ));
    let unpacked = lokyr_pkg_model::archive::unpack(archive, &folder);
    let readme = ["README.md", "README.markdown", "README"]
        .iter()
        .find_map(|name| std::fs::read_to_string(folder.join(name)).ok());
    let _ = std::fs::remove_dir_all(&folder);
    unpacked
        .map(|unpacked| (unpacked.manifest, readme))
        .map_err(|error| error.to_string())
}

/// `text` as HTML's text and attributes hold it.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            character => out.push(character),
        }
    }
    out
}

/// Where a README's relative links lead: into its repository at the
/// version's commit, of GitHub's; nowhere of another host.
struct Links {
    /// The address of a file's page, before its path.
    pages: Option<String>,
    /// The address of a file's bytes, as an image's, before its path.
    files: Option<String>,
}

impl Links {
    /// The links of the repository `repository` at `commit`.
    fn of(repository: &str, commit: &str) -> Self {
        let repository = repository.trim_end_matches('/').trim_end_matches(".git");
        let github = repository.starts_with("https://github.com/") && !commit.is_empty();
        Self {
            pages: github.then(|| format!("{repository}/blob/{commit}/")),
            files: github.then(|| format!("{repository}/raw/{commit}/")),
        }
    }

    /// The address that `url` of a README leads to, of an image's if
    /// `image`: of the web or of mail as it is; a place of the page as it
    /// is; a relative one into the repository; `#` of any other.
    fn resolve(&self, url: &str, image: bool) -> String {
        let trimmed = url.trim();
        if trimmed.starts_with('#') {
            return trimmed.to_owned();
        }
        if trimmed.starts_with("//") {
            return "#".to_owned();
        }
        let before = trimmed
            .find(['/', '?', '#'])
            .map_or(trimmed, |at| &trimmed[..at]);
        if let Some((scheme, _)) = before.split_once(':') {
            let scheme = scheme.to_ascii_lowercase();
            return if ["http", "https", "mailto"].contains(&scheme.as_str()) {
                trimmed.to_owned()
            } else {
                "#".to_owned()
            };
        }
        let base = if image { &self.files } else { &self.pages };
        match base {
            Some(base) => {
                let path = trimmed.trim_start_matches("./").trim_start_matches('/');
                format!("{base}{path}")
            }
            None => "#".to_owned(),
        }
    }
}

/// The HTML of `readme`, of Markdown: its raw HTML shown as text, its
/// links as [`Links::resolve`] takes them.
fn markdown(readme: &str, links: &Links) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let events = Parser::new_ext(readme, options).map(|event| match event {
        Event::Html(html) | Event::InlineHtml(html) => Event::Text(html),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(links.resolve(&dest_url, false)),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from(links.resolve(&dest_url, true)),
            title,
            id,
        }),
        other => other,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

/// A page of `title`, of `main`, its body, and of what follows it, `end`.
fn page(title: &str, main: &str, end: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<link rel=\"icon\" href=\"/icon.svg\" type=\"image/svg+xml\">\n<link rel=\"stylesheet\" href=\"/style.css\">\n</head>\n<body>\n<header><a class=\"home\" href=\"/\"><img src=\"/icon.svg\" alt=\"\" width=\"28\" height=\"28\">Lokyr plugins</a></header>\n{main}\n<footer>The registry of <a href=\"https://lokyr.dev\">Lokyr</a>'s plugins; its index, signed, which Sleip reads, at <a href=\"/config.json\">config.json</a>.</footer>\n{end}</body>\n</html>\n",
        escaped(title)
    )
}

/// The capabilities of `entry`, as a list of codes, or none.
fn capabilities(entry: &Entry) -> String {
    if entry.caps.is_empty() {
        return "none".to_owned();
    }
    entry
        .caps
        .iter()
        .map(|capability| format!("<code>{}</code>", escaped(capability)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The page of the plugin `name`, of its `description`, `license` and
/// `repository`, of its versions `entries`, of `shown`, and of its
/// `readme` in HTML, if it has one.
fn plugin_page(
    name: &str,
    description: &str,
    license: &str,
    repository: &str,
    entries: &[Entry],
    shown: &Entry,
    readme: Option<String>,
) -> String {
    let yanked = if shown.yanked {
        " <span class=\"badge\">yanked</span>"
    } else {
        ""
    };
    let uses = if shown.uses.is_empty() {
        String::new()
    } else {
        format!(
            "<h2>Commands it runs</h2>\n<p>{}</p>\n",
            shown
                .uses
                .iter()
                .map(|command| format!("<code>{}</code>", escaped(command)))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let mut versions = String::new();
    for entry in entries.iter().rev() {
        let (class, badge) = if entry.yanked {
            (" class=\"yanked\"", " <span class=\"badge\">yanked</span>")
        } else {
            ("", "")
        };
        let short: String = entry.cksum.chars().take("sha256:".len() + 12).collect();
        versions.push_str(&format!(
            "<tr{class}><td>{}{badge}</td><td>{}</td><td><code title=\"{}\">{}…</code></td></tr>\n",
            escaped(&entry.vers),
            capabilities(entry),
            escaped(&entry.cksum),
            escaped(&short)
        ));
    }
    let readme = readme.map_or_else(String::new, |readme| {
        format!("<h2>README</h2>\n<article class=\"readme\">\n{readme}</article>\n")
    });
    let main = format!(
        "<main data-pagefind-body>\n<h1>{name_text}</h1>\n<p class=\"description\">{description}</p>\n<dl class=\"facts\">\n<dt>Version</dt><dd>{version}{yanked}</dd>\n<dt>License</dt><dd>{license}</dd>\n<dt>Repository</dt><dd><a href=\"{repository_attribute}\">{repository_text}</a></dd>\n<dt>Install</dt><dd><code>sleip add {name_text}</code></dd>\n</dl>\n<h2>Capabilities</h2>\n<p>{capabilities}</p>\n{uses}<h2>Versions</h2>\n<table class=\"versions\">\n<thead><tr><th>Version</th><th>Capabilities</th><th>Checksum</th></tr></thead>\n<tbody>\n{versions}</tbody>\n</table>\n{readme}</main>",
        name_text = escaped(name),
        description = escaped(description),
        version = escaped(&shown.vers),
        license = escaped(license),
        repository_attribute = escaped(repository),
        repository_text = escaped(repository),
        capabilities = capabilities(shown),
    );
    page(&format!("{name} — Lokyr plugins"), &main, "")
}

/// The home page of the plugins `listed`, which Pagefind searches.
fn home_page(listed: &[Listed]) -> String {
    let mut items = String::new();
    for plugin in listed {
        let badge = if plugin.yanked {
            " <span class=\"badge\">yanked</span>"
        } else {
            ""
        };
        items.push_str(&format!(
            "<li><a href=\"/plugins/{name}/\">{name}</a> <span class=\"version\">{}</span>{badge}<p>{}</p></li>\n",
            escaped(&plugin.version),
            escaped(&plugin.description),
            name = escaped(&plugin.name),
        ));
    }
    let main = format!(
        "<main>\n<h1>Lokyr plugins</h1>\n<p class=\"description\">The plugins of Lokyr's registry, each built from its tagged sources, checked and tested; <code>sleip add owner/name</code> installs one.</p>\n<div id=\"search\"></div>\n<ul class=\"plugins\">\n{items}</ul>\n</main>"
    );
    let end = "<link rel=\"stylesheet\" href=\"/pagefind/pagefind-ui.css\">\n<script src=\"/pagefind/pagefind-ui.js\"></script>\n<script>window.addEventListener(\"DOMContentLoaded\", () => { if (window.PagefindUI) { new PagefindUI({ element: \"#search\", showSubResults: false }); } });</script>\n";
    page("Lokyr plugins", &main, end)
}

/// Lokyr's icon, the app's own, of the site's tab and its header.
const ICON: &str = include_str!("../../../../../crates/app/resources/icons/lokyr-jormungandr.svg");

/// The site's style sheet: the system's font, of a light and a dark
/// appearance.
const STYLE: &str = r#":root { color-scheme: light dark; --bg: #ffffff; --fg: #1d1d1f; --muted: #6e6e73; --line: #d2d2d7; --accent: #b8860b; --code: #f5f5f7; }
@media (prefers-color-scheme: dark) { :root { --bg: #161618; --fg: #f5f5f7; --muted: #a1a1a6; --line: #3a3a3c; --accent: #e0b84a; --code: #2c2c2e; } }
body { font: 16px/1.55 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; background: var(--bg); color: var(--fg); max-width: 52rem; margin: 0 auto; padding: 1.5rem; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
header { margin-bottom: 2rem; }
header .home { display: inline-flex; align-items: center; gap: 10px; font-weight: 600; color: var(--fg); }
header .home img { width: 28px; height: 28px; }
footer { margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--line); color: var(--muted); font-size: 0.875rem; }
.description { color: var(--muted); font-size: 1.125rem; }
code { font: 0.875em ui-monospace, "SF Mono", Menlo, monospace; background: var(--code); padding: 0.1em 0.35em; border-radius: 4px; }
pre code { display: block; padding: 0.75rem 1rem; overflow-x: auto; }
.facts { display: grid; grid-template-columns: max-content 1fr; gap: 0.35rem 1.5rem; }
.facts dt { color: var(--muted); }
.facts dd { margin: 0; }
table { border-collapse: collapse; width: 100%; }
th, td { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid var(--line); vertical-align: top; }
tr.yanked td { color: var(--muted); }
.badge { font-size: 0.75rem; border: 1px solid var(--line); border-radius: 999px; padding: 0 0.5em; color: var(--muted); }
.plugins { list-style: none; padding: 0; }
.plugins li { padding: 0.75rem 0; border-bottom: 1px solid var(--line); }
.plugins li p { margin: 0.25rem 0 0; color: var(--muted); }
.version { color: var(--muted); font-size: 0.875rem; }
.readme img { max-width: 100%; }
#search { margin: 1.5rem 0; --pagefind-ui-scale: 0.9; --pagefind-ui-primary: var(--accent); --pagefind-ui-text: var(--fg); --pagefind-ui-background: var(--bg); --pagefind-ui-border: var(--line); --pagefind-ui-tag: var(--code); --pagefind-ui-border-width: 1px; --pagefind-ui-border-radius: 8px; --pagefind-ui-font: inherit; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_readme_s_html_shows_as_text_and_its_links_stay_safe() {
        let links = Links::of("https://github.com/acme/hello.git", "abc123");
        let html = markdown(
            "# Hello\n\n<script>alert(1)</script>\n\nText <b>bold</b> [site](https://lokyr.dev) [bad](javascript:alert(1)) [doc](docs/a.md) [top](#hello) [net](//evil.example)\n\n![shot](./img/shot.png) ![remote](https://x.example/a.png)\n",
            &links,
        );
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
        assert!(html.contains("href=\"https://lokyr.dev\""), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(
            html.contains("href=\"https://github.com/acme/hello/blob/abc123/docs/a.md\""),
            "{html}"
        );
        assert!(html.contains("href=\"#hello\""), "{html}");
        assert!(!html.contains("evil.example"), "{html}");
        assert!(
            html.contains("src=\"https://github.com/acme/hello/raw/abc123/img/shot.png\""),
            "{html}"
        );
        assert!(html.contains("src=\"https://x.example/a.png\""), "{html}");
        // Of another host than GitHub, a relative link leads nowhere.
        let other = Links::of("https://git.example/acme/hello", "abc123");
        assert_eq!(other.resolve("docs/a.md", false), "#");
        assert_eq!(other.resolve("MAILTO:a@b.c", false), "MAILTO:a@b.c");
        assert_eq!(other.resolve("data:text/html,x", true), "#");
    }

    #[test]
    fn a_plugin_s_texts_are_escaped_on_its_pages() {
        let entry = Entry {
            v: signing::VERSION,
            name: "acme/hello".to_owned(),
            vers: "0.1.0".to_owned(),
            kind: "luau".to_owned(),
            api: None,
            deps: Vec::new(),
            caps: vec!["fs-read:~/<x>".to_owned()],
            uses: Vec::new(),
            cksum: format!("sha256:{}", "a".repeat(64)),
            yanked: false,
            description: None,
        };
        let page = plugin_page(
            "acme/hello",
            "It <em>greets</em> & \"quotes\"",
            "MIT",
            "https://github.com/acme/hello\" onclick=\"x",
            std::slice::from_ref(&entry),
            &entry,
            None,
        );
        assert!(!page.contains("<em>"), "{page}");
        assert!(page.contains("It &lt;em&gt;greets&lt;/em&gt; &amp; &quot;quotes&quot;"));
        assert!(!page.contains("\" onclick=\""), "{page}");
        assert!(page.contains("<code>fs-read:~/&lt;x&gt;</code>"), "{page}");
        let home = home_page(&[Listed {
            name: "acme/hello".to_owned(),
            version: "0.1.0".to_owned(),
            description: "<img src=x onerror=alert(1)>".to_owned(),
            yanked: false,
        }]);
        // Its description's image as text, beside the header's icon.
        assert!(!home.contains("<img src=x"), "{home}");
        assert!(
            home.contains("&lt;img src=x onerror=alert(1)&gt;"),
            "{home}"
        );
    }

    #[test]
    fn the_readme_s_list_shows_a_plugin_s_texts_as_text() {
        let shown = Entry {
            v: signing::VERSION,
            name: "acme/hello".to_owned(),
            vers: "0.1.0".to_owned(),
            kind: "luau".to_owned(),
            api: None,
            deps: Vec::new(),
            caps: vec!["clipboard-write".to_owned(), "fs-read:~/a|b".to_owned()],
            uses: Vec::new(),
            cksum: format!("sha256:{}", "a".repeat(64)),
            yanked: false,
            description: None,
        };
        let published = Published {
            owner: "acme".to_owned(),
            name: "hello".to_owned(),
            plugin: "acme/hello".to_owned(),
            entries: vec![shown.clone()],
            shown,
            description: "A | B <script>x</script> [x](y) `code` *em* see https://evil.example or me@evil.example\nnext".to_owned(),
            license: "MIT".to_owned(),
            readme: None,
            repository: "https://github.com/acme/hello".to_owned(),
            links: Links::of("https://github.com/acme/hello", "abc"),
        };
        let list = plugin_list(std::slice::from_ref(&published));
        let html = {
            let mut html = String::new();
            pulldown_cmark::html::push_html(
                &mut html,
                Parser::new_ext(&list, Options::ENABLE_TABLES),
            );
            html
        };
        // One row of four cells, the plugin's own link alone.
        let row = html.split("<tbody>").nth(1).unwrap_or_default();
        assert_eq!(row.matches("<td>").count(), 4, "{html}");
        assert_eq!(html.matches("<a href=").count(), 1, "{html}");
        assert!(
            html.contains("<a href=\"https://pkg.lokyr.dev/plugins/acme/hello/\">"),
            "{html}"
        );
        assert!(
            !html.contains("<script>") && !html.contains("<code>") && !html.contains("<em>"),
            "{html}"
        );
        assert!(
            html.contains("A | B &lt;script&gt;x&lt;/script&gt; [x](y) `code` *em*"),
            "{html}"
        );
        // An address of the web or of mail broken, which GitHub links not.
        assert!(
            !list.contains("https://evil") && !list.contains("me@evil"),
            "{list}"
        );
        assert!(!list.contains("\nnext"), "{list}");
        // Bounded.
        let long = markdown_text(&"x".repeat(LONGEST_DESCRIPTION + 5));
        assert_eq!(long.chars().count(), LONGEST_DESCRIPTION + 1);
        assert_eq!(plugin_list(&[]), "No plugin is published yet.\n");
    }

    #[test]
    fn the_search_takes_the_site_s_colors_of_each_appearance() {
        // Pagefind's own, of a white background whatever the appearance,
        // given the site's, which its dark appearance changes.
        for variable in [
            "--pagefind-ui-background: var(--bg)",
            "--pagefind-ui-text: var(--fg)",
            "--pagefind-ui-border: var(--line)",
        ] {
            assert!(STYLE.contains(variable), "{variable}");
        }
        let dark = STYLE
            .split("prefers-color-scheme: dark")
            .nth(1)
            .unwrap_or_default();
        assert!(dark.contains("--bg:"), "{dark}");
    }
}
