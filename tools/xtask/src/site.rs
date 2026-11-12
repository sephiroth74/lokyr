//! `cargo xtask site [--out <folder>]`: the documentation's site, which
//! `lokyr.dev` serves: the public Markdown that git tracks, `docs/` and the
//! root's changelog, contributing and translating guides, each a page of
//! the site's style beside `site/public`'s home, at the same path, `.md`
//! made `.html` and a folder's `README.md` its `index.html`; the
//! documents' images copied beside them. A link to another document goes to
//! its page, of its anchor, which the page's headings give as GitHub gives
//! them; a link to another file of the repository goes to it on GitHub. The
//! site's settings, `site/site.toml`, name the project's accounts, which
//! every page links. Into `target/site` by default.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};
use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::check_links::slug;

/// The repository, whose files the pages link on GitHub.
const BLOB: &str = "https://github.com/sephiroth74/lokyr/blob/main";

/// The root's documents that the site holds beside `docs/`.
const ROOT_DOCUMENTS: [&str; 3] = ["CHANGELOG.md", "CONTRIBUTING.md", "TRANSLATING.md"];

/// The files of the documents' folders that the site copies: their images.
const IMAGES: [&str; 6] = ["svg", "png", "jpg", "jpeg", "gif", "webp"];

/// The site's settings: the project's accounts, each linked from every page
/// where it is set.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    #[serde(default)]
    pub(crate) social: Social,
}

/// The project's accounts, by their addresses; none shown of none.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Social {
    #[serde(default)]
    pub(crate) bluesky: String,
    #[serde(default)]
    pub(crate) x: String,
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut out = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = Some(PathBuf::from(args.next().context("--out takes a folder")?)),
            _ => bail!("site takes `--out <folder>`, not `{arg}`"),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    let out = out.unwrap_or_else(|| root.join("target/site"));
    let pages = build(&root, &out)?;
    println!("site: {pages} pages in {}", out.display());
    Ok(ExitCode::SUCCESS)
}

/// Builds the site of the documents of `root` into `out`, made anew: how
/// many pages it wrote.
pub(crate) fn build(root: &Path, out: &Path) -> Result<usize> {
    let settings = settings(root)?;
    let documents = documents(root)?;
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("emptying {}", out.display()))?;
    }
    fs::create_dir_all(out).with_context(|| format!("making {}", out.display()))?;
    copy_folder(&root.join("site/public"), out, &|name| name != ".htaccess")?;
    fs::copy(root.join("site/style.css"), out.join("style.css")).context("copying the style")?;
    for document in &documents {
        let text = fs::read_to_string(root.join(document))
            .with_context(|| format!("reading {}", document.display()))?;
        let page = page(document, &text, &settings);
        let target = out.join(page_path(document));
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
        }
        fs::write(&target, page).with_context(|| format!("writing {}", target.display()))?;
    }
    copy_folder(&root.join("docs"), &out.join("docs"), &|name| {
        Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| IMAGES.contains(&extension.to_ascii_lowercase().as_str()))
    })?;
    Ok(documents.len())
}

/// The site's settings, of `site/site.toml`.
fn settings(root: &Path) -> Result<Settings> {
    let file = root.join("site/site.toml");
    let text = fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    toml::from_str(&text).with_context(|| format!("reading {}", file.display()))
}

/// The public documents that git tracks: `docs/`'s Markdown, and the
/// root's that the site holds, relative to the root.
fn documents(root: &Path) -> Result<Vec<PathBuf>> {
    let listed = Command::new("git")
        .args(["ls-files", "--", "docs/*.md"])
        .args(ROOT_DOCUMENTS)
        .current_dir(root)
        .output()
        .context("listing the documents that git tracks")?;
    ensure!(listed.status.success(), "git ls-files failed");
    let files = String::from_utf8(listed.stdout).context("git's list of files")?;
    Ok(files.lines().map(PathBuf::from).collect())
}

/// Copies the files of `from` that `taken` takes, by their names, into
/// `to`, of their folders.
fn copy_folder(from: &Path, to: &Path, taken: &dyn Fn(&str) -> bool) -> Result<()> {
    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            copy_folder(&path, &to.join(&name), taken)?;
        } else if taken(&name) {
            fs::create_dir_all(to).with_context(|| format!("making {}", to.display()))?;
            fs::copy(&path, to.join(&name))
                .with_context(|| format!("copying {}", path.display()))?;
        }
    }
    Ok(())
}

/// The page's path of `document`: `.md` made `.html`, a `README.md` its
/// folder's `index.html`, the root's documents in lower case.
pub(crate) fn page_path(document: &Path) -> PathBuf {
    let name = document
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = document.parent().unwrap_or(Path::new(""));
    if name == "README.md" {
        return folder.join("index.html");
    }
    let stem = name.strip_suffix(".md").unwrap_or(&name);
    if folder.as_os_str().is_empty() {
        folder.join(format!("{}.html", stem.to_ascii_lowercase()))
    } else {
        folder.join(format!("{stem}.html"))
    }
}

/// The page of `document`, of its Markdown `text`, in the site's template.
fn page(document: &Path, text: &str, settings: &Settings) -> String {
    let (body, title) = html(document, text);
    let depth = document.components().count().saturating_sub(1);
    let up = "../".repeat(depth);
    let mut social = String::new();
    for (name, address) in [
        ("Bluesky", &settings.social.bluesky),
        ("X", &settings.social.x),
    ] {
        if !address.is_empty() {
            let _ = write!(social, r#"<a href="{}">{name}</a>"#, escaped(address));
        }
    }
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — Lokyr</title>
<meta name="color-scheme" content="dark">
<meta name="theme-color" content="#0b0911">
<link rel="icon" href="{up}icon.svg" type="image/svg+xml">
<link rel="stylesheet" href="{up}style.css">
</head>
<body>
<div class="backdrop" aria-hidden="true"></div>
<header class="wrap top">
  <a class="brand" href="{up}index.html"><img src="{up}icon.svg" alt="" width="28" height="28">Lokyr</a>
  <nav>
    <a href="{up}docs/index.html">Docs</a>
    <a href="{up}docs/CONFIGURATION.html">Configuration</a>
    <a href="{up}docs/plugins/index.html">Plugins</a>
    <a href="{up}docs/themes/authoring.html">Themes</a>
    <a href="{up}changelog.html">Changelog</a>
    <a href="https://github.com/sephiroth74/lokyr">GitHub</a>{social}
  </nav>
</header>
<main class="wrap doc">
{body}</main>
<footer>
  <div class="wrap">
    <p>© 2026 Alessandro Crugnola · Lokyr and Sleip are under the MIT License.</p>
    <p><a href="{BLOB}/{source}">This page's source</a></p>
  </div>
</footer>
</body>
</html>
"##,
        title = escaped(&title),
        source = document.display(),
    )
}

/// The HTML of `document`'s Markdown `text`, and its title, its first
/// heading's text: its headings of GitHub's anchors, its links to the
/// documents made their pages', and to the repository's other files made
/// GitHub's.
fn html(document: &Path, text: &str) -> (String, String) {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS;
    let events: Vec<Event> = Parser::new_ext(text, options).collect();
    // Each heading's text, of its anchor, a repeated one numbered as GitHub
    // numbers it.
    let mut ids = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut title = None;
    let mut heading: Option<(HeadingLevel, String)> = None;
    for event in &events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => heading = Some((*level, String::new())),
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, words)) = &mut heading {
                    words.push_str(text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, words)) = heading.take() {
                    if title.is_none() && level == HeadingLevel::H1 {
                        title = Some(words.clone());
                    }
                    let base = slug(&words);
                    let count = seen.entry(base.clone()).or_insert(0);
                    ids.push(if *count == 0 {
                        base
                    } else {
                        format!("{base}-{count}")
                    });
                    *count += 1;
                }
            }
            _ => {}
        }
    }
    let mut ids = ids.into_iter();
    let events = events.into_iter().map(|event| match event {
        Event::Start(Tag::Heading {
            level,
            id: None,
            classes,
            attrs,
        }) => Event::Start(Tag::Heading {
            level,
            id: ids.next().map(CowStr::from),
            classes,
            attrs,
        }),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(linked(document, &dest_url)),
            title,
            id,
        }),
        event => event,
    });
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, events);
    let fallback = document
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    (body, title.unwrap_or(fallback))
}

/// Where a link of `document` to `target` goes in the site: another
/// document's page, of its anchor; a file of the repository on GitHub; the
/// web, an anchor of the page or a mail as it is.
pub(crate) fn linked(document: &Path, target: &str) -> String {
    if target.is_empty()
        || target.starts_with('#')
        || target.contains("://")
        || target.starts_with("mailto:")
    {
        return target.to_owned();
    }
    let (path, anchor) = match target.split_once('#') {
        Some((path, anchor)) => (path, format!("#{anchor}")),
        None => (target, String::new()),
    };
    let folder = document.parent().unwrap_or(Path::new(""));
    let Some(file) = normalized(&folder.join(path)) else {
        return target.to_owned();
    };
    let is_document = file.extension().is_some_and(|extension| extension == "md")
        && (file.starts_with("docs")
            || ROOT_DOCUMENTS
                .iter()
                .any(|document| file == Path::new(document)));
    let is_image = file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| IMAGES.contains(&extension.to_ascii_lowercase().as_str()))
        && file.starts_with("docs");
    if is_document {
        let page = page_path(&file);
        return format!("{}{anchor}", relative(folder, &page));
    }
    if is_image {
        return target.to_owned();
    }
    format!("{BLOB}/{}{anchor}", file.display())
}

/// `path` without its `.` and `..`, within the root; none past it.
fn normalized(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.iter().collect())
}

/// The path of `to`, from the root, relative to the folder `from`.
fn relative(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut path = "../".repeat(from.len() - common);
    let rest: PathBuf = to_parts[common..].iter().collect();
    path.push_str(&rest.to_string_lossy());
    path
}

/// `text` for HTML's text and attributes.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_s_page_keeps_its_path_and_its_links_go_to_pages_or_github() {
        assert_eq!(
            page_path(Path::new("docs/CONFIGURATION.md")),
            Path::new("docs/CONFIGURATION.html")
        );
        assert_eq!(
            page_path(Path::new("docs/README.md")),
            Path::new("docs/index.html")
        );
        assert_eq!(
            page_path(Path::new("docs/plugins/README.md")),
            Path::new("docs/plugins/index.html")
        );
        assert_eq!(
            page_path(Path::new("CHANGELOG.md")),
            Path::new("changelog.html")
        );
        let plugin = Path::new("docs/plugins/authoring.md");
        assert_eq!(
            linked(plugin, "reference.md#capabilities"),
            "reference.html#capabilities"
        );
        assert_eq!(
            linked(plugin, "../PACKAGE-MANAGER.md#plugintoml"),
            "../PACKAGE-MANAGER.html#plugintoml"
        );
        assert_eq!(linked(plugin, "README.md"), "index.html");
        assert_eq!(
            linked(plugin, "../../TRANSLATING.md"),
            "../../translating.html"
        );
        assert_eq!(linked(plugin, "#the-manifest"), "#the-manifest");
        assert_eq!(
            linked(plugin, "https://pkg.lokyr.dev"),
            "https://pkg.lokyr.dev"
        );
        assert_eq!(
            linked(
                Path::new("TRANSLATING.md"),
                "plugins/tabs/locales/en/tabs.ftl"
            ),
            format!("{BLOB}/plugins/tabs/locales/en/tabs.ftl")
        );
        assert_eq!(
            linked(Path::new("docs/README.md"), "images/lokyr-htop.png"),
            "images/lokyr-htop.png"
        );
        assert_eq!(linked(plugin, "../../../outside.md"), "../../../outside.md");
    }

    #[test]
    fn a_page_s_headings_take_github_s_anchors_and_its_first_its_title() {
        let (body, title) = html(
            Path::new("docs/x.md"),
            "# The `plugin.toml` guide\n\n## Limits\n\n## Limits\n\nSee [the guide](y.md#a).\n",
        );
        assert_eq!(title, "The plugin.toml guide");
        assert!(body.contains(r#"<h1 id="the-plugintoml-guide">"#), "{body}");
        assert!(
            body.contains(r#"<h2 id="limits">"#) && body.contains(r#"<h2 id="limits-1">"#),
            "{body}"
        );
        assert!(body.contains(r#"<a href="y.html#a">"#), "{body}");
    }

    #[test]
    fn the_site_holds_every_public_document_and_the_accounts_set() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let out = std::env::temp_dir().join(format!("lokyr-site-{}", std::process::id()));
        let pages = build(&root, &out).unwrap();
        let documents = documents(&root).unwrap();
        assert_eq!(pages, documents.len());
        for document in &documents {
            assert!(
                out.join(page_path(document)).is_file(),
                "{}",
                document.display()
            );
        }
        for file in [
            "index.html",
            "style.css",
            "icon.svg",
            "docs/images/lokyr-htop.png",
        ] {
            assert!(out.join(file).is_file(), "{file}");
        }
        assert!(!out.join(".htaccess").exists());
        let configuration = fs::read_to_string(out.join("docs/CONFIGURATION.html")).unwrap();
        assert!(configuration.contains(r#"id="font""#));
        // The accounts, none of none, each of its address.
        let settings = Settings {
            social: Social {
                bluesky: "https://bsky.app/profile/lokyr.dev".to_owned(),
                x: String::new(),
            },
        };
        let shown = page(Path::new("docs/x.md"), "# X\n", &settings);
        assert!(shown.contains(r#"<a href="https://bsky.app/profile/lokyr.dev">Bluesky</a>"#));
        assert!(!shown.contains(">X</a>"));
        let _ = fs::remove_dir_all(out);
    }
}
