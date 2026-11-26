//! `cargo xtask site [--out <folder>]`: the documentation's site, which
//! `lokyr.dev` serves: the public Markdown that git tracks, `docs/` and the
//! root's changelog, contributing and translating guides, each a page of
//! the site's style beside `site/public`'s home, at the same path, `.md`
//! made `.html` and a folder's `README.md` its `index.html`; the
//! documents' images copied beside them. A link to another document goes to
//! its page, of its anchor, which the page's headings give as GitHub gives
//! them; a link to another file of the repository goes to it on GitHub. The
//! site's settings, `site/site.toml`, name the project's accounts, which
//! every page links. Every link of the pages, the home's too, is checked
//! as the site is built, to a file and an anchor that it holds; and, where
//! Pagefind is there, on the `PATH` or as `just pagefind` fetches it into
//! `target/tools`, it indexes the documents' pages for the search's page.
//! For the search engines, each page names its canonical address, a
//! folder's `index.html` as its folder, its description, a document's of
//! its first paragraph, and its title, its heading's or the one of
//! `TITLES` that says more; `sitemap.xml` lists the home and the documents'
//! pages of their last commits' dates, but the reports' appendices, which
//! their engines do not list either; and `404.html` is the page that
//! GitHub Pages serves of an address that the site does not hold. Every
//! page has one heading of its title, the release notes' of their version:
//! the build fails of another document of no title, and of an image that
//! tells nothing to whom cannot see it. An image of an SVG of the docs
//! takes the SVG's size, so that the page does not move as it loads.
//! Into `target/site` by default.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};
use pulldown_cmark::{BlockQuoteKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::check_links::slug;

/// The repository, whose files the pages link on GitHub.
const BLOB: &str = "https://github.com/sephiroth74/lokyr/blob/main";

/// Where the site is served: the pages' canonical addresses and the
/// sitemap's are of it.
const ORIGIN: &str = "https://lokyr.dev";

/// The site's social image, which a link to any of its pages shows where
/// it is shared, and what it shows, for those who cannot see it.
const SOCIAL_IMAGE: &str = "https://lokyr.dev/og.png";
const SOCIAL_IMAGE_ALT: &str = "Lokyr's icon, a serpent round a prompt, beside its name and the words: a fast terminal for macOS, where every feature is a plugin.";

/// The most characters of a page's description, as the search engines show
/// about as many.
const DESCRIPTION: usize = 155;

/// The least characters of a paragraph that a page's description takes:
/// a shorter one, a note or a line of platforms, is passed over.
const DESCRIPTION_LEAST: usize = 50;

/// The root's documents that the site holds beside `docs/`.
const ROOT_DOCUMENTS: [&str; 3] = ["CHANGELOG.md", "CONTRIBUTING.md", "TRANSLATING.md"];

/// The documentation's pages in their groups and order, which the site
/// draws as its sidebar: every public page but itself is in it.
const SUMMARY: &str = "docs/SUMMARY.md";

/// The folders of dated reports, the measurements and the release notes,
/// whose pages the summary covers by any one of them that it lists, as a
/// list of each would drown the rest.
const COLLECTIONS: [&str; 2] = ["docs/bench", "docs/releases"];

/// The repository's editor of a file, which a page's footer links.
const EDIT: &str = "https://github.com/sephiroth74/lokyr/edit/main";

/// The pages made of their sources, each of the source that a change goes
/// to, which its footer links in place of the page's editor.
const GENERATED: [(&str, &str, &str); 2] = [
    (
        "docs/CONFIGURATION.md",
        "crates/kernel/config.schema.json",
        "the config's schema",
    ),
    (
        "docs/commands.md",
        "crates/app-core/src/app/tests/docs.rs",
        "the app's commands and default keys",
    ),
];

/// The pages' titles in the search engines' results and in a shared link's
/// preview, where their heading alone says too little: the words that people
/// search, of `TITLE_MOST` characters at most with the site's name. Their
/// heading, the sidebar and the breadcrumbs keep the document's own.
const TITLES: [(&str, &str); 16] = [
    (
        "CHANGELOG.md",
        "Changelog: every change, release by release",
    ),
    (
        "docs/CONFIGURATION.md",
        "Configuration: every key of config.toml",
    ),
    (
        "docs/README.md",
        "Documentation of Lokyr, a terminal emulator for macOS",
    ),
    (
        "docs/automation.md",
        "Scripting Lokyr with AppleScript and Shortcuts",
    ),
    (
        "docs/blocks.md",
        "Command blocks: select, copy and rerun commands",
    ),
    ("docs/commands.md", "Commands and keyboard shortcuts"),
    (
        "docs/escape-codes.md",
        "Escape codes: the control sequences that Lokyr supports",
    ),
    ("docs/links.md", "Clickable links and file paths (⌘-click)"),
    (
        "docs/plugins/README.md",
        "Terminal plugins: bundled and from the registry",
    ),
    (
        "docs/plugins/authoring.md",
        "Writing a terminal plugin in Luau",
    ),
    (
        "docs/plugins/quick-terminal.md",
        "Quick terminal: a drop-down terminal on a hotkey",
    ),
    (
        "docs/plugins/splits.md",
        "Split panes, side by side and stacked",
    ),
    (
        "docs/plugins/tabs.md",
        "Tabs: a tab bar on any side, or vertical tabs",
    ),
    (
        "docs/screenshots.md",
        "Screenshots of Lokyr, a terminal for macOS",
    ),
    (
        "docs/shell-integration.md",
        "Shell integration for zsh, bash and fish",
    ),
    (
        "docs/themes/authoring.md",
        "Making a terminal color theme in TOML",
    ),
];

/// The most characters of a page's title with the site's name, as the
/// search engines show about as many.
const TITLE_MOST: usize = 60;

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
    let mut since = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = Some(PathBuf::from(args.next().context("--out takes a folder")?)),
            "--changed-since" => {
                since = Some(args.next().context("--changed-since takes a commit")?);
            }
            _ => bail!("site takes `--out <folder>` or `--changed-since <commit>`, not `{arg}`"),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    if let Some(since) = since {
        for address in changed_pages(&root, &since)? {
            println!("{address}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let out = out.unwrap_or_else(|| root.join("target/site"));
    let pages = build(&root, &out)?;
    let broken = broken_links(&out)?;
    if !broken.is_empty() {
        for link in &broken {
            eprintln!("site: {link}");
        }
        bail!("{} links of the site do not resolve", broken.len());
    }
    let unlabelled = unlabelled_images(&out)?;
    if !unlabelled.is_empty() {
        for image in &unlabelled {
            eprintln!("site: {image}");
        }
        bail!(
            "{} images of the site tell nothing to whom cannot see them",
            unlabelled.len()
        );
    }
    match pagefind(&root) {
        Some(pagefind) => {
            let indexed = Command::new(&pagefind)
                .arg("--site")
                .arg(&out)
                .output()
                .with_context(|| format!("running {}", pagefind.display()))?;
            ensure!(
                indexed.status.success(),
                "Pagefind failed: {}",
                String::from_utf8_lossy(&indexed.stderr)
            );
            println!("site: the search's index of Pagefind");
        }
        None => println!("site: no search, as Pagefind is not there: `just pagefind` fetches it"),
    }
    println!("site: {pages} pages in {}", out.display());
    Ok(ExitCode::SUCCESS)
}

/// Pagefind: `target/tools/pagefind/pagefind`, the Mac's release that `just
/// pagefind` fetches, on a Mac alone, as a container sharing the folder runs
/// another system; or the `PATH`'s.
fn pagefind(root: &Path) -> Option<PathBuf> {
    let fetched = root.join("target/tools/pagefind/pagefind");
    if cfg!(target_os = "macos") && fetched.is_file() {
        return Some(fetched);
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|folder| folder.join("pagefind"))
            .find(|file| file.is_file())
    })
}

/// The links of the pages of the site in `out` that do not resolve: to a
/// file that is not there, or to an anchor that the page does not hold;
/// those to the web, by mail or of data left out, and Pagefind's own.
pub(crate) fn broken_links(out: &Path) -> Result<Vec<String>> {
    let link = regex::Regex::new(r#"(?:href|src)="([^"]*)""#)?;
    let mut pages = Vec::new();
    collect_pages(out, &mut pages)?;
    let mut held: HashMap<PathBuf, String> = HashMap::new();
    let mut broken = Vec::new();
    for page in &pages {
        let text =
            fs::read_to_string(page).with_context(|| format!("reading {}", page.display()))?;
        let name = page.strip_prefix(out).unwrap_or(page).display().to_string();
        for found in link.captures_iter(&text) {
            let target = found[1].replace("&amp;", "&");
            if target.is_empty()
                || target.contains("://")
                || target.starts_with("mailto:")
                || target.starts_with("data:")
                || target.starts_with("pagefind/")
                || target.contains("/pagefind/")
            {
                continue;
            }
            let (path, anchor) = target.split_once('#').unwrap_or((&target, ""));
            let path = path.split('?').next().unwrap_or_default();
            // A path from the root is the site's, as `lokyr.dev` serves it.
            let mut file = if path.is_empty() {
                page.clone()
            } else if let Some(rooted) = path.strip_prefix('/') {
                out.join(rooted)
            } else {
                page.parent().unwrap_or(out).join(path)
            };
            if file.is_dir() {
                file = file.join("index.html");
            }
            if !file.is_file() {
                broken.push(format!("{name}: `{target}`: no such file"));
                continue;
            }
            if anchor.is_empty() || file.extension().is_none_or(|extension| extension != "html") {
                continue;
            }
            let content = match held.get(&file) {
                Some(content) => content,
                None => {
                    let content = fs::read_to_string(&file).unwrap_or_default();
                    held.entry(file.clone()).or_insert(content)
                }
            };
            if !content.contains(&format!("id=\"{anchor}\""))
                && !content.contains(&format!("name=\"{anchor}\""))
            {
                broken.push(format!("{name}: `{target}`: no such anchor"));
            }
        }
    }
    Ok(broken)
}

/// The images of the pages of the site in `out` that tell nothing to whom
/// cannot see them: an `<img>` of no `alt`, or of an empty one, but in a
/// link of its own words, as the brand's icon beside the name, or marked as
/// a decoration; an `<svg>` of `role="img"`, as a chart, of no label.
pub(crate) fn unlabelled_images(out: &Path) -> Result<Vec<String>> {
    let image = regex::Regex::new(r"<(img|svg)\s[^>]*>")?;
    let tag = regex::Regex::new(r"<[^>]*>")?;
    let mut pages = Vec::new();
    collect_pages(out, &mut pages)?;
    let mut unlabelled = Vec::new();
    for page in &pages {
        let text =
            fs::read_to_string(page).with_context(|| format!("reading {}", page.display()))?;
        let name = page.strip_prefix(out).unwrap_or(page).display().to_string();
        for found in image.captures_iter(&text) {
            let whole = found.get(0).map_or("", |whole| whole.as_str());
            let at = found.get(0).map_or(0, |whole| whole.start());
            let own = attributes(&whole[4..]);
            let value = |wanted: &str| {
                own.iter()
                    .find(|(name, _)| *name == wanted)
                    .map(|(_, value)| value.trim())
            };
            let decorative = value("aria-hidden") == Some("true")
                || matches!(value("role"), Some("presentation" | "none"));
            if &found[1] == "svg" {
                if value("role") == Some("img")
                    && value("aria-label").is_none_or(str::is_empty)
                    && value("aria-labelledby").is_none_or(str::is_empty)
                {
                    unlabelled.push(format!("{name}: an <svg> of role=\"img\" of no label"));
                }
                continue;
            }
            match value("alt") {
                None => unlabelled.push(format!("{name}: an <img> of no alt: {whole}")),
                Some("") if !decorative && !in_worded_link(&text, at, &tag) => {
                    unlabelled.push(format!(
                        "{name}: an <img> of an empty alt outside a link of its own words: {whole}"
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(unlabelled)
}

/// Whether the element at `at` in `text` is in a link of words of its own,
/// beside it, `tag` matching a tag, which the link's words leave out.
fn in_worded_link(text: &str, at: usize, tag: &regex::Regex) -> bool {
    let before = &text[..at];
    let Some(opened) = ["<a ", "<a>"]
        .iter()
        .filter_map(|start| before.rfind(start))
        .max()
    else {
        return false;
    };
    if before.rfind("</a>").is_some_and(|closed| closed > opened) {
        return false;
    }
    let Some(words_start) = before[opened..].find('>').map(|end| opened + end + 1) else {
        return false;
    };
    let Some(words_end) = text[at..].find("</a>").map(|end| at + end) else {
        return false;
    };
    !tag.replace_all(&text[words_start..words_end], "")
        .trim()
        .is_empty()
}

/// The HTML pages under `folder`.
fn collect_pages(folder: &Path, pages: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "pagefind") {
                continue;
            }
            collect_pages(&path, pages)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "html")
        {
            pages.push(path);
        }
    }
    Ok(())
}

/// Builds the site of the documents of `root` into `out`, made anew: how
/// many pages it wrote.
pub(crate) fn build(root: &Path, out: &Path) -> Result<usize> {
    let settings = settings(root)?;
    let documents = documents(root)?;
    let summary = summary(root, &documents)?;
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("emptying {}", out.display()))?;
    }
    fs::create_dir_all(out).with_context(|| format!("making {}", out.display()))?;
    copy_folder(&root.join("site/public"), out, &|_| true)?;
    // The updates' feed: the latest published release's appcast, an asset
    // of the release, in place of the site's own, which a release of none
    // keeps.
    if let Some(appcast) = crate::feed::latest_release_appcast() {
        fs::write(out.join("appcast.xml"), appcast).context("writing the site's appcast")?;
    }
    for file in ["style.css", "search.js", "site.js"] {
        fs::copy(root.join("site").join(file), out.join(file))
            .with_context(|| format!("copying {file}"))?;
    }
    fs::write(out.join("search.html"), search_page(&settings))
        .context("writing the search's page")?;
    fs::write(out.join("404.html"), missing_page(&settings)).context("writing the 404 page")?;
    let dates = last_commits(root)?;
    let mut listed = vec![(
        canonical(Path::new("index.html")),
        dates.get(Path::new("site/public/index.html")).cloned(),
    )];
    for document in documents.iter().filter(|document| !appendix(document)) {
        listed.push((
            canonical(&page_path(document)),
            dates.get(document).cloned(),
        ));
    }
    fs::write(out.join("sitemap.xml"), sitemap(&listed)).context("writing the sitemap")?;
    let mut texts = Vec::with_capacity(documents.len());
    for document in &documents {
        let text = fs::read_to_string(root.join(document))
            .with_context(|| format!("reading {}", document.display()))?;
        texts.push(text);
    }
    // Each document's title, which the breadcrumbs of the pages below a
    // folder's index name it by; every document of a title of its own, its
    // page's heading, but a release's notes, whose page makes one.
    let mut titles = HashMap::with_capacity(documents.len());
    let mut untitled = Vec::new();
    for (document, text) in documents.iter().zip(&texts) {
        let Rendered { title, titled, .. } = html(document, text);
        if !titled && !release_notes(document) {
            untitled.push(document.display().to_string());
        }
        titles.insert(document.clone(), title);
    }
    ensure!(
        untitled.is_empty(),
        "documents of no title, a `# ` heading first: {}",
        untitled.join(", ")
    );
    let unknown: Vec<&str> = TITLES
        .iter()
        .map(|(page, _)| *page)
        .filter(|page| !documents.iter().any(|document| document == Path::new(page)))
        .collect();
    ensure!(
        unknown.is_empty(),
        "titles of no public document, in `TITLES`: {}",
        unknown.join(", ")
    );
    let long: Vec<String> = TITLES
        .iter()
        .map(|(_, title)| site_title(title))
        .filter(|title| title.chars().count() > TITLE_MOST)
        .collect();
    ensure!(
        long.is_empty(),
        "titles of more than {TITLE_MOST} characters with the site's name, in `TITLES`: {}",
        long.join(", ")
    );
    let changelog =
        fs::read_to_string(root.join("CHANGELOG.md")).context("reading the changelog")?;
    fs::write(
        out.join("releases.xml"),
        release_feed(&documents, &texts, &release_dates(&changelog)),
    )
    .context("writing the releases' feed")?;
    for (document, text) in documents.iter().zip(&texts) {
        let page = with_sizes(
            root,
            document,
            &with_charts(
                root,
                document,
                &page(document, text, &settings, &titles, &summary),
            ),
        );
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
    Ok(files
        .lines()
        .filter(|file| *file != SUMMARY)
        .map(PathBuf::from)
        .collect())
}

/// The documentation's pages in their groups, of the summary's sections,
/// and order: each group of its entries, a page each, of its title, which
/// may hold its own.
#[derive(Debug, Default, PartialEq, Eq)]
struct Summary {
    groups: Vec<Group>,
}

/// A group of the summary: its name and its pages.
#[derive(Debug, PartialEq, Eq)]
struct Group {
    name: String,
    entries: Vec<Entry>,
}

/// A page of the summary: its title, its document, from the root, and the
/// pages under it.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    title: String,
    document: PathBuf,
    entries: Vec<Entry>,
}

impl Summary {
    /// The summary of `text`, `docs/SUMMARY.md`'s: its title and what comes
    /// before its first group passed over, each group a `## ` heading and
    /// each page a list item of a link alone, `- [Title](page.md)`, two
    /// spaces deeper under the page that holds it; anything else after the
    /// first group refused, of its line.
    fn parse(text: &str) -> Result<Self> {
        let item = regex::Regex::new(r"^( *)- \[([^\]]+)\]\(([^)#\s]+)\)$").expect("the pattern");
        let mut summary = Self::default();
        for (at, line) in text.lines().enumerate() {
            let number = at + 1;
            if let Some(name) = line.strip_prefix("## ") {
                summary.groups.push(Group {
                    name: name.trim().to_owned(),
                    entries: Vec::new(),
                });
                continue;
            }
            let Some(group) = summary.groups.last_mut() else {
                continue;
            };
            if line.trim().is_empty() {
                continue;
            }
            let Some(found) = item.captures(line) else {
                bail!(
                    "{SUMMARY}:{number}: a line of no page: `- [Title](page.md)`, under a `## ` group"
                );
            };
            let depth = found[1].len();
            ensure!(
                depth % 2 == 0,
                "{SUMMARY}:{number}: a page's list item two spaces deeper than the page that holds it"
            );
            let document = normalized(&Path::new("docs").join(&found[3]))
                .with_context(|| format!("{SUMMARY}:{number}: a link past the repository"))?;
            let entry = Entry {
                title: found[2].to_owned(),
                document,
                entries: Vec::new(),
            };
            let mut entries = &mut group.entries;
            for _ in 0..depth / 2 {
                entries = &mut entries
                    .last_mut()
                    .with_context(|| {
                        format!("{SUMMARY}:{number}: a page deeper than the page above it")
                    })?
                    .entries;
            }
            entries.push(entry);
        }
        Ok(summary)
    }

    /// Every page that it lists, in their order.
    fn documents(&self) -> Vec<&Path> {
        fn walk<'a>(entries: &'a [Entry], found: &mut Vec<&'a Path>) {
            for entry in entries {
                found.push(&entry.document);
                walk(&entry.entries, found);
            }
        }
        let mut found = Vec::new();
        for group in &self.groups {
            walk(&group.entries, &mut found);
        }
        found
    }

    /// What does not hold of the summary against the public `documents`,
    /// each of the documents that it `links`: a page that it lists twice or
    /// that is no public document, and a public document that it leaves
    /// out, but a report of a collection of which it lists a page, which
    /// that page reaches through the collection's links.
    fn problems(
        &self,
        documents: &[PathBuf],
        links: &HashMap<PathBuf, Vec<PathBuf>>,
    ) -> Vec<String> {
        let listed = self.documents();
        let mut problems = Vec::new();
        for (at, document) in listed.iter().enumerate() {
            if listed[..at].contains(document) {
                problems.push(format!("{SUMMARY} lists {} twice", document.display()));
            }
            if !documents.iter().any(|public| public == document) {
                problems.push(format!(
                    "{SUMMARY} lists {}, which is no public page",
                    document.display()
                ));
            }
        }
        for document in documents {
            if listed.contains(&document.as_path()) {
                continue;
            }
            match collection_of(document, &listed) {
                None => problems.push(format!(
                    "{} is in no group of {SUMMARY}: list it where a reader would look for it",
                    document.display()
                )),
                Some(page) if !reaches(page, document, links) => problems.push(format!(
                    "{} is linked from no page of its collection: list it in {}",
                    document.display(),
                    page.display()
                )),
                Some(_) => {}
            }
        }
        problems
    }
}

/// Whether `page` reaches `document` through the links of `links`, within
/// the folder of `page`'s collection.
fn reaches(page: &Path, document: &Path, links: &HashMap<PathBuf, Vec<PathBuf>>) -> bool {
    let Some(folder) = COLLECTIONS.iter().find(|folder| page.starts_with(folder)) else {
        return false;
    };
    let mut seen: Vec<&Path> = vec![page];
    let mut next = 0;
    while let Some(&at) = seen.get(next) {
        next += 1;
        for linked in links.get(at).into_iter().flatten() {
            if linked == document {
                return true;
            }
            if linked.starts_with(folder) && !seen.contains(&linked.as_path()) {
                seen.push(linked);
            }
        }
    }
    false
}

/// The documents that the Markdown `text` of `document` links, from the
/// root, without their anchors: none of another site.
fn document_links(document: &Path, text: &str) -> Vec<PathBuf> {
    let folder = document.parent().unwrap_or(Path::new(""));
    let mut found = Vec::new();
    for event in Parser::new_ext(text, Options::all()) {
        let Event::Start(Tag::Link { dest_url, .. }) = event else {
            continue;
        };
        if dest_url.contains("://") || dest_url.starts_with("mailto:") {
            continue;
        }
        let path = dest_url.split('#').next().unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        if let Some(file) = normalized(&folder.join(path))
            && !found.contains(&file)
        {
            found.push(file);
        }
    }
    found
}

/// The page that the summary lists of the collection of `document`, a
/// report that it does not list, if it lists one.
fn collection_of<'a>(document: &Path, listed: &[&'a Path]) -> Option<&'a Path> {
    let folder = COLLECTIONS
        .iter()
        .find(|folder| document.starts_with(folder))?;
    listed.iter().find(|page| page.starts_with(folder)).copied()
}

/// The summary of the repository at `root`, which every public page of
/// `documents` is in.
fn summary(root: &Path, documents: &[PathBuf]) -> Result<Summary> {
    let text =
        fs::read_to_string(root.join(SUMMARY)).with_context(|| format!("reading {SUMMARY}"))?;
    let summary = Summary::parse(&text)?;
    // The links of the collections' pages, which reach their reports.
    let mut links = HashMap::new();
    for document in documents {
        if COLLECTIONS
            .iter()
            .any(|folder| document.starts_with(folder))
        {
            let text = fs::read_to_string(root.join(document))
                .with_context(|| format!("reading {}", document.display()))?;
            links.insert(document.clone(), document_links(document, &text));
        }
    }
    let problems = summary.problems(documents, &links);
    if !problems.is_empty() {
        bail!("{}", problems.join("\n"));
    }
    Ok(summary)
}

/// The sidebar of the documentation's pages, of `summary`, on the page of
/// `document`, `up` from the root: beside the column on a wide screen,
/// each group of its name and its pages, the page shown marked, and the
/// pages under another a group that opens, open where it holds the page
/// shown; and the same as a menu that opens, above the page, on a narrow
/// one.
fn navigation(summary: &Summary, document: &Path, up: &str) -> (String, String) {
    let listed = summary.documents();
    // The page shown, or the collection's page of a report.
    let here = if listed.contains(&document) {
        Some(document)
    } else {
        collection_of(document, &listed)
    };
    fn holds(entry: &Entry, here: Option<&Path>) -> bool {
        here == Some(entry.document.as_path())
            || entry.entries.iter().any(|inner| holds(inner, here))
    }
    fn list(entries: &[Entry], document: &Path, here: Option<&Path>, up: &str, out: &mut String) {
        out.push_str("<ul>\n");
        for entry in entries {
            let current = if entry.document == document {
                r#" aria-current="page""#
            } else if here == Some(entry.document.as_path()) {
                r#" aria-current="true""#
            } else {
                ""
            };
            let link = format!(
                r#"<a href="{up}{}"{current}>{}</a>"#,
                folder_form(&page_path(&entry.document).to_string_lossy()),
                escaped(&entry.title)
            );
            if entry.entries.is_empty() {
                let _ = writeln!(out, "<li>{link}</li>");
                continue;
            }
            let open = if holds(entry, here) { " open" } else { "" };
            let _ = write!(out, "<li><details{open}><summary>{link}</summary>");
            list(&entry.entries, document, here, up, out);
            out.push_str("</details></li>\n");
        }
        out.push_str("</ul>\n");
    }
    let mut groups = String::new();
    for group in &summary.groups {
        let _ = writeln!(
            groups,
            "<p class=\"pages-group\">{}</p>",
            escaped(&group.name)
        );
        list(&group.entries, document, here, up, &mut groups);
    }
    (
        format!("<nav class=\"pages\" aria-label=\"Documentation\">\n{groups}</nav>"),
        format!(
            "<details class=\"pages-inline\">\n<summary>Documentation</summary>\n<nav aria-label=\"Documentation\">\n{groups}</nav>\n</details>"
        ),
    )
}

/// The date, `YYYY-MM-DD`, of the last commit of each of the site's
/// sources, the documents and the home, relative to the root, of one walk
/// of the history; a shallow clone gives its one commit's date to every
/// file.
fn last_commits(root: &Path) -> Result<HashMap<PathBuf, String>> {
    let logged = Command::new("git")
        .args([
            "log",
            "--format=%x00%cs",
            "--name-only",
            "--no-renames",
            "--",
        ])
        .arg("docs")
        .args(ROOT_DOCUMENTS)
        .arg("site/public/index.html")
        .current_dir(root)
        .output()
        .context("reading the files' last commits")?;
    ensure!(logged.status.success(), "git log failed");
    let text = String::from_utf8(logged.stdout).context("git's log")?;
    Ok(dated(&text))
}

/// Each file of `git log --format=%x00%cs --name-only`'s `text` and the
/// date of its first, latest, commit.
fn dated(text: &str) -> HashMap<PathBuf, String> {
    let mut dates = HashMap::new();
    let mut date = "";
    for line in text.lines() {
        if let Some(stamp) = line.strip_prefix('\0') {
            date = stamp;
        } else if !line.is_empty() && !date.is_empty() {
            dates
                .entry(PathBuf::from(line))
                .or_insert_with(|| date.to_owned());
        }
    }
    dates
}

/// The sitemap of the `pages`' addresses, each of the date of its last
/// change where it is known.
fn sitemap(pages: &[(String, Option<String>)]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for (address, date) in pages {
        let _ = write!(xml, "  <url><loc>{}</loc>", escaped(address));
        if let Some(date) = date {
            let _ = write!(xml, "<lastmod>{date}</lastmod>");
        }
        xml.push_str("</url>\n");
    }
    xml.push_str("</urlset>\n");
    xml
}

/// The canonical addresses of the pages that changed since the commit
/// `since`, for the search engines that take word of them (IndexNow):
/// those of the documents added, changed or removed, the home's of its
/// file; every page where the site's template changed, or where `since`
/// is no commit of this history, as a push's first or a forced one is.
fn changed_pages(root: &Path, since: &str) -> Result<Vec<String>> {
    let listed = Command::new("git")
        .args(["diff", "--name-only", "--no-renames"])
        .arg(format!("{since}..HEAD"))
        .current_dir(root)
        .output()
        .context("listing the files changed")?;
    let files = String::from_utf8_lossy(&listed.stdout).into_owned();
    let changed: Vec<PathBuf> = files.lines().map(PathBuf::from).collect();
    let everything = !listed.status.success()
        || changed
            .iter()
            .any(|file| file == Path::new("tools/xtask/src/site.rs"));
    let documents = documents(root)?;
    Ok(pages_of(&changed, &documents, everything))
}

/// The canonical addresses of the pages of the `changed` files among the
/// site's `documents` and its home, or of every page, `everything` or of
/// the summary changed, which every page's sidebar shows, and which is no
/// page itself; the reports' appendices, which the engines do not list,
/// none.
fn pages_of(changed: &[PathBuf], documents: &[PathBuf], everything: bool) -> Vec<String> {
    let everything = everything || changed.iter().any(|file| file == Path::new(SUMMARY));
    let home = Path::new("site/public/index.html");
    let mut pages = Vec::new();
    if everything || changed.iter().any(|file| file == home) {
        pages.push(canonical(Path::new("index.html")));
    }
    for file in changed {
        let is_document = file.extension().is_some_and(|extension| extension == "md")
            && file != Path::new(SUMMARY)
            && (file.starts_with("docs")
                || ROOT_DOCUMENTS.iter().any(|root| file == Path::new(root)));
        if !everything && is_document && !appendix(file) {
            pages.push(canonical(&page_path(file)));
        }
    }
    if everything {
        pages.extend(
            documents
                .iter()
                .filter(|document| !appendix(document))
                .map(|document| canonical(&page_path(document))),
        );
    }
    pages
}

/// The date of each release that `changelog` tells, by its version, of its
/// headings `## [X.Y.Z] - YYYY-MM-DD`.
fn release_dates(changelog: &str) -> HashMap<String, String> {
    changelog
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("## [")?;
            let (version, date) = rest.split_once("] - ")?;
            Some((version.to_owned(), date.trim().to_owned()))
        })
        .collect()
}

/// The Atom feed of the releases' notes among `documents`, of their
/// `texts`, each of its release's date in `dates`, the newest first: each
/// entry its notes whole, their links of the notes' folder.
fn release_feed(
    documents: &[PathBuf],
    texts: &[String],
    dates: &HashMap<String, String>,
) -> String {
    let folder = Path::new("docs/releases");
    let mut releases: Vec<(Vec<u64>, String, String, Rendered)> = documents
        .iter()
        .zip(texts)
        .filter(|(document, _)| document.parent() == Some(folder))
        .filter_map(|(document, text)| {
            let version = document.file_stem()?.to_str()?.to_owned();
            let date = dates.get(&version)?.clone();
            let order = version
                .split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect();
            Some((order, version, date, html(document, text)))
        })
        .collect();
    releases.sort_by(|a, b| b.0.cmp(&a.0));
    let updated = releases
        .first()
        .map_or("2026-01-01".to_owned(), |release| release.2.clone());
    let mut feed = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <feed xmlns=\"http://www.w3.org/2005/Atom\">\n\
         <title>Lokyr releases</title>\n\
         <subtitle>The notes of each release of Lokyr, a fast, open-source terminal emulator for macOS.</subtitle>\n\
         <id>{ORIGIN}/releases.xml</id>\n\
         <link rel=\"self\" type=\"application/atom+xml\" href=\"{ORIGIN}/releases.xml\"/>\n\
         <link rel=\"alternate\" type=\"text/html\" href=\"{ORIGIN}/docs/releases/\"/>\n\
         <icon>{ORIGIN}/apple-touch-icon.png</icon>\n\
         <author><name>Alessandro Crugnola</name></author>\n\
         <updated>{updated}T00:00:00Z</updated>\n"
    );
    for (_, version, date, rendered) in &releases {
        let address = canonical(&folder.join(format!("{version}.html")));
        let summary = rendered
            .description
            .as_deref()
            .map(shortened)
            .unwrap_or_default();
        let _ = write!(
            feed,
            "<entry>\n\
             <title>Lokyr {version}</title>\n\
             <id>{address}</id>\n\
             <link rel=\"alternate\" type=\"text/html\" href=\"{address}\"/>\n\
             <updated>{date}T00:00:00Z</updated>\n\
             <summary>{}</summary>\n\
             <content type=\"html\" xml:base=\"{ORIGIN}/docs/releases/\">{}</content>\n\
             </entry>\n",
            escaped(&summary),
            escaped(&rendered.body)
        );
    }
    feed.push_str("</feed>\n");
    feed
}

/// The canonical address of the site's `page`, from the root: a folder's
/// `index.html` as its folder.
fn canonical(page: &Path) -> String {
    format!("{ORIGIN}/{}", folder_form(&page.to_string_lossy()))
}

/// A link to a page as the site prefers it: `index.html` dropped, so that
/// a folder's page is its folder; `./` of the folder itself.
fn folder_form(path: &str) -> String {
    match path.strip_suffix("index.html") {
        Some(folder) if folder.is_empty() || folder.ends_with('/') => folder.to_owned(),
        _ => path.to_owned(),
    }
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

/// The page of `document`, of its Markdown `text`, in the site's template,
/// beside the sidebar of `summary`'s pages: the search indexes its body.
fn page(
    document: &Path,
    text: &str,
    settings: &Settings,
    titles: &HashMap<PathBuf, String>,
    summary: &Summary,
) -> String {
    let Rendered {
        body,
        title,
        titled,
        description,
        headings,
    } = html(document, text);
    // A release's notes, of no title of their own, take the one made of
    // their version as their heading, as the other pages take their first.
    let body = if titled {
        body
    } else {
        format!(
            "<h1 id=\"{}\" data-pagefind-weight=\"10\">{}</h1>\n{body}",
            escaped(&slug(&title)),
            escaped(&title)
        )
    };
    let depth = document.components().count().saturating_sub(1);
    let up = "../".repeat(depth);
    // A page made of a source links the source, which a change goes to.
    let source = match GENERATED
        .iter()
        .find(|(page, ..)| document == Path::new(page))
    {
        Some((_, made_of, name)) => {
            format!(r#"Made of <a href="{BLOB}/{made_of}">{name}</a>"#)
        }
        None => format!(
            r#"<a href="{EDIT}/{}">Edit this page on GitHub</a>"#,
            document.display()
        ),
    };
    let description = description.unwrap_or_else(|| {
        format!(
            "{title}, of the documentation of Lokyr, a fast, open-source terminal emulator for macOS."
        )
    });
    // Its title in the results and the previews, which may say more than
    // its heading.
    let shown = TITLES
        .iter()
        .find(|(page, _)| document == Path::new(page))
        .map_or(title.as_str(), |(_, shown)| shown);
    template(
        &Head {
            title: shown,
            description: Some(&shortened(&description)),
            canonical: Some(&canonical(&page_path(document))),
            indexed: !appendix(document),
            section: Section::of(document),
            structured: Some(&breadcrumbs(&crumbs(document, &title, titles))),
        },
        &up,
        settings,
        &contents(&body, &headings, &navigation(summary, document, &up)),
        &source,
        "",
    )
}

/// `page`, of `document`, of its charts' images in it as their SVG: a
/// paragraph of an image alone of a chart of the benches, an SVG of the
/// docs whose root tells its kind, `data-chart`, which the site's script
/// makes answer the pointer and the keyboard, in the site's dark colors
/// whatever the reader's appearance, of the width that the page gives it,
/// its image's text its label; a page of a chart is wide, as its charts and
/// its tables take more room than its text's column on a wide screen.
fn with_charts(root: &Path, document: &Path, page: &str) -> String {
    const START: &str = "<p><img src=\"";
    let mut shown = String::with_capacity(page.len());
    let mut rest = page;
    let mut charts = 0;
    while let Some(at) = rest.find(START) {
        let tail = &rest[at..];
        let Some(end) = tail.find("</p>") else {
            break;
        };
        let paragraph = &tail[..end + 4];
        shown.push_str(&rest[..at]);
        match chart_of(root, document, paragraph) {
            Some(figure) => {
                shown.push_str(&figure);
                charts += 1;
            }
            None => shown.push_str(paragraph),
        }
        rest = &tail[end + 4..];
    }
    shown.push_str(rest);
    if charts == 0 {
        return shown;
    }
    shown.replacen(
        "<div class=\"wrap page has-nav",
        "<div class=\"wrap page wide has-nav",
        1,
    )
}

/// The figure of a chart of `paragraph`, an image alone of `document`'s, as
/// [`with_charts`] tells; none of another image.
fn chart_of(root: &Path, document: &Path, paragraph: &str) -> Option<String> {
    let (source, rest) = paragraph
        .strip_prefix("<p><img src=\"")?
        .split_once("\" alt=\"")?;
    let label = rest.strip_suffix("\" /></p>")?;
    if source.contains(':') || source.starts_with('/') || label.contains('"') {
        return None;
    }
    let file = normalized(&document.parent().unwrap_or(Path::new("")).join(source))?;
    if file.extension().is_none_or(|extension| extension != "svg") || !file.starts_with("docs") {
        return None;
    }
    let svg = fs::read_to_string(root.join(&file)).ok()?;
    let svg = svg.trim();
    let open = svg.find('>')?;
    let tag = &svg[..open];
    if !tag.starts_with("<svg ") || !tag.contains(" data-chart=\"") {
        return None;
    }
    // Its root of no size of its own, the page giving it its width, of the
    // dark colors, and of the image's text.
    let mut root_tag = String::new();
    for (name, value) in attributes(&tag[5..]) {
        let _ = match name {
            "width" | "height" => Ok(()),
            "class" => write!(root_tag, " class=\"{value} dark\""),
            "aria-label" => write!(root_tag, " aria-label=\"{label}\""),
            _ => write!(root_tag, " {name}=\"{value}\""),
        };
    }
    Some(format!(
        "<figure class=\"chart-figure\" data-pagefind-ignore><svg{root_tag}{}</figure>\n",
        &svg[open..]
    ))
}

/// `page`, of `document`, of the size of each of its images of an SVG of
/// the docs that tells none: the SVG's own, so that the browser keeps the
/// image's room before it loads and the page does not move as it does. The
/// site's style keeps an image within its column, of its proportions.
fn with_sizes(root: &Path, document: &Path, page: &str) -> String {
    const START: &str = "<img ";
    let mut shown = String::with_capacity(page.len());
    let mut rest = page;
    while let Some(at) = rest.find(START) {
        let tail = &rest[at..];
        let Some(end) = tail.find('>') else {
            break;
        };
        let tag = &tail[..=end];
        shown.push_str(&rest[..at]);
        match svg_size(root, document, tag) {
            Some((width, height)) => {
                let close = [" />", "/>", ">"]
                    .into_iter()
                    .find(|close| tag.ends_with(close))
                    .unwrap_or(">");
                let open = &tag[..tag.len() - close.len()];
                let _ = write!(shown, "{open} width=\"{width}\" height=\"{height}\"{close}");
            }
            None => shown.push_str(tag),
        }
        rest = &tail[end + 1..];
    }
    shown.push_str(rest);
    shown
}

/// The size of the image of `tag`, an `<img>` of `document`'s of no size of
/// its own, of an SVG of the docs: its root's `width` and `height`, in
/// pixels, or else its `viewBox`'s; none of another image.
fn svg_size(root: &Path, document: &Path, tag: &str) -> Option<(String, String)> {
    let found = attributes(tag.strip_prefix("<img")?);
    if found
        .iter()
        .any(|(name, _)| *name == "width" || *name == "height")
    {
        return None;
    }
    let source = found.iter().find(|(name, _)| *name == "src")?.1;
    if source.contains(':') || source.starts_with('/') {
        return None;
    }
    let file = normalized(&document.parent().unwrap_or(Path::new("")).join(source))?;
    if file.extension().is_none_or(|extension| extension != "svg") || !file.starts_with("docs") {
        return None;
    }
    let svg = fs::read_to_string(root.join(&file)).ok()?;
    let start = svg.find("<svg")?;
    let open = start + svg[start..].find('>')?;
    let own = attributes(&svg[start + 4..open]);
    let value = |wanted: &str| {
        own.iter()
            .find(|(name, _)| *name == wanted)
            .map(|(_, value)| *value)
    };
    let pixels = |value: &str| {
        value
            .strip_suffix("px")
            .unwrap_or(value)
            .parse::<f64>()
            .ok()
            .filter(|pixels| *pixels > 0.0)
    };
    let (width, height) = match (
        value("width").and_then(pixels),
        value("height").and_then(pixels),
    ) {
        (Some(width), Some(height)) => (width, height),
        _ => {
            let view: Vec<f64> = value("viewBox")?
                .split([' ', ','])
                .filter(|part| !part.is_empty())
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            match view[..] {
                [_, _, width, height] if width > 0.0 && height > 0.0 => (width, height),
                _ => return None,
            }
        }
    };
    Some((format!("{width:.0}"), format!("{height:.0}")))
}

/// The attributes of an element's opening tag, past its name: each name
/// and its value, of double quotes.
fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    let mut rest = tag.trim_start();
    while let Some((name, after)) = rest.split_once("=\"") {
        let Some((value, next)) = after.split_once('"') else {
            break;
        };
        found.push((name.trim(), value));
        rest = next.trim_start();
    }
    found
}

/// The least sections of a page that has a table of contents.
const TOC_LEAST: usize = 4;

/// A document's page's contents: the documentation's sidebar, `pages`, its
/// own and its menu's; its `main`, of its `body`; and, of `TOC_LEAST`
/// sections or more, its table of contents of its `headings`: beside the
/// column on a wide screen, each section a group that opens on its parts,
/// and a short one of the sections alone, which opens, under the title on
/// a narrower one. The search indexes the `main` alone.
fn contents(body: &str, headings: &[Heading], (pages, menu): &(String, String)) -> String {
    let sections = headings.iter().filter(|heading| heading.level == 2).count();
    if sections < TOC_LEAST {
        return format!(
            "<div class=\"wrap page has-nav\">\n{pages}\n{menu}\n<main class=\"doc\" id=\"main\" data-pagefind-body>\n{body}</main>\n</div>"
        );
    }
    let mut side = String::from(
        "<nav class=\"toc\" aria-label=\"On this page\">\n<p class=\"toc-title\">On this page</p>\n<ul>\n",
    );
    let mut inline =
        String::from("<details class=\"toc-inline\">\n<summary>On this page</summary>\n<ul>\n");
    let mut index = 0;
    // Parts before the first section have none to belong to: they go alone.
    while index < headings.len() {
        let heading = &headings[index];
        index += 1;
        let link = format!(
            "<a href=\"#{}\">{}</a>",
            escaped(&heading.id),
            heading_words(&heading.words)
        );
        if heading.level == 3 {
            let _ = writeln!(side, "<li>{link}</li>");
            continue;
        }
        let _ = writeln!(inline, "<li>{link}</li>");
        let parts: Vec<&Heading> = headings[index..]
            .iter()
            .take_while(|part| part.level == 3)
            .collect();
        index += parts.len();
        if parts.is_empty() {
            let _ = writeln!(side, "<li>{link}</li>");
            continue;
        }
        let _ = write!(side, "<li><details><summary>{link}</summary><ul>");
        for part in parts {
            let _ = write!(
                side,
                "<li><a href=\"#{}\">{}</a></li>",
                escaped(&part.id),
                heading_words(&part.words)
            );
        }
        side.push_str("</ul></details></li>\n");
    }
    side.push_str("</ul>\n</nav>");
    inline.push_str("</ul>\n</details>\n");
    // The short one right after the page's title, or first where it has none.
    let body = match body.find("</h1>") {
        Some(end) => format!("{}\n{inline}{}", &body[..end + 5], &body[end + 5..]),
        None => format!("{inline}{body}"),
    };
    format!(
        "<div class=\"wrap page has-nav has-toc\">\n{pages}\n{menu}\n{side}\n<main class=\"doc\" id=\"main\" data-pagefind-body>\n{body}</main>\n</div>"
    )
}

/// A heading's words as HTML, its code as code.
fn heading_words(words: &[(bool, String)]) -> String {
    words
        .iter()
        .map(|(code, text)| {
            if *code {
                format!("<code>{}</code>", escaped(text))
            } else {
                escaped(text)
            }
        })
        .collect()
}

/// The search's page, of Pagefind's interface, which a query of `?q=` fills;
/// kept out of the search engines' results, as it shows nothing of its own.
fn search_page(settings: &Settings) -> String {
    template(
        &Head {
            title: "Search",
            description: None,
            canonical: None,
            indexed: false,
            section: Some(Section::Search),
            structured: None,
        },
        "",
        settings,
        "<div class=\"wrap page\">\n<main class=\"doc\" id=\"main\">\n<h1>Search</h1>\n<div id=\"search\"></div>\n</main>\n</div>",
        "",
        r#"<link rel="stylesheet" href="pagefind/pagefind-ui.css">
<script src="pagefind/pagefind-ui.js" defer></script>
<script src="search.js" defer></script>
"#,
    )
}

/// The page of an address that the site does not hold, which GitHub Pages
/// serves at whatever depth: its links are from the root.
fn missing_page(settings: &Settings) -> String {
    template(
        &Head {
            title: "Page not found",
            description: None,
            canonical: None,
            indexed: false,
            section: None,
            structured: None,
        },
        "/",
        settings,
        r#"<div class="wrap page">
<main class="doc" id="main">
<h1>Page not found</h1>
<p>There is no page at this address; it may have moved. The <a href="/docs/">documentation</a> lists every guide, and the <a href="/search.html">search</a> finds a word in all of them.</p>
<p><a href="/">Lokyr's home</a></p>
</main>
</div>"#,
        "",
        "",
    )
}

/// What a page's head tells of it.
struct Head<'a> {
    /// The page's own title, of no site's name.
    title: &'a str,
    /// What the page holds, in a sentence or two.
    description: Option<&'a str>,
    /// The page's canonical address.
    canonical: Option<&'a str>,
    /// Whether the search engines may list the page.
    indexed: bool,
    /// The top bar's section that the page belongs to, which the bar marks.
    section: Option<Section>,
    /// Its structured data for the search engines, as JSON-LD.
    structured: Option<&'a str>,
}

/// The trail of `document`'s page, of its `title`, from the site's home:
/// each folder above it of an index of its own, the documentation's of
/// its name, the others of their index's title in `titles`, then the page,
/// each of its name and its canonical address.
fn crumbs(
    document: &Path,
    title: &str,
    titles: &HashMap<PathBuf, String>,
) -> Vec<(String, String)> {
    let mut trail = vec![("Lokyr".to_owned(), canonical(Path::new("index.html")))];
    let mut folder = PathBuf::new();
    let folders: Vec<_> = document
        .parent()
        .map(|parent| parent.components().collect())
        .unwrap_or_default();
    for part in folders {
        folder.push(part);
        let index = folder.join("README.md");
        if index == document {
            break;
        }
        if let Some(name) = titles.get(&index) {
            let name = if folder == Path::new("docs") {
                "Documentation".to_owned()
            } else {
                name.clone()
            };
            trail.push((name, canonical(&page_path(&index))));
        }
    }
    let name = if document == Path::new("docs/README.md") {
        "Documentation".to_owned()
    } else {
        title.to_owned()
    };
    trail.push((name, canonical(&page_path(document))));
    trail
}

/// The JSON-LD of a `BreadcrumbList` of `trail`.
fn breadcrumbs(trail: &[(String, String)]) -> String {
    let items: Vec<serde_json::Value> = trail
        .iter()
        .enumerate()
        .map(|(at, (name, address))| {
            serde_json::json!({
                "@type": "ListItem",
                "position": at + 1,
                "name": name,
                "item": address,
            })
        })
        .collect();
    let list = serde_json::json!({
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        "itemListElement": items,
    });
    // As a script's text: no `</` that would end it.
    list.to_string().replace("</", "<\\/")
}

/// The top bar's sections, each of its label and its page from the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Docs,
    Configuration,
    Plugins,
    Themes,
    Changelog,
    Search,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::Docs,
        Section::Configuration,
        Section::Plugins,
        Section::Themes,
        Section::Changelog,
        Section::Search,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::Docs => "Docs",
            Section::Configuration => "Configuration",
            Section::Plugins => "Plugins",
            Section::Themes => "Themes",
            Section::Changelog => "Changelog",
            Section::Search => "Search",
        }
    }

    fn page(self) -> &'static str {
        match self {
            Section::Docs => "docs/",
            Section::Configuration => "docs/CONFIGURATION.html",
            Section::Plugins => "docs/plugins/",
            Section::Themes => "docs/themes/authoring.html",
            Section::Changelog => "changelog.html",
            Section::Search => "search.html",
        }
    }

    /// The section of `document`: the configuration's, a plugin's or a
    /// theme's guide, the changelog and the release notes; the other
    /// documents of `docs/` the documentation's; the root's guides none.
    fn of(document: &Path) -> Option<Section> {
        if document == Path::new("docs/CONFIGURATION.md") {
            Some(Section::Configuration)
        } else if document.starts_with("docs/plugins") {
            Some(Section::Plugins)
        } else if document.starts_with("docs/themes") {
            Some(Section::Themes)
        } else if document == Path::new("CHANGELOG.md") || document.starts_with("docs/releases") {
            Some(Section::Changelog)
        } else if document.starts_with("docs") {
            Some(Section::Docs)
        } else {
            None
        }
    }
}

/// `text` of `DESCRIPTION` characters at most: cut at the last space within
/// them, of an ellipsis.
fn shortened(text: &str) -> String {
    if text.chars().count() <= DESCRIPTION {
        return text.to_owned();
    }
    let kept: String = text.chars().take(DESCRIPTION - 1).collect();
    let cut = kept
        .rfind(' ')
        .map_or(kept.as_str(), |space| &kept[..space]);
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ']))
}

/// A page's `title` as its head tells it: the site's name after it, unless
/// it holds the name.
fn site_title(title: &str) -> String {
    if title.contains("Lokyr") {
        title.to_owned()
    } else {
        format!("{title} — Lokyr")
    }
}

/// A page of the site, of its `head`, the way `up` to the site's root, its
/// `main` and its footer's `extra` link, and `more` in its head.
fn template(
    head: &Head,
    up: &str,
    settings: &Settings,
    main: &str,
    extra: &str,
    more: &str,
) -> String {
    let mut social = String::new();
    for (name, address) in [
        ("Bluesky", &settings.social.bluesky),
        ("X", &settings.social.x),
    ] {
        if !address.is_empty() {
            let _ = write!(social, r#"<a href="{}">{name}</a>"#, escaped(address));
        }
    }
    let title = site_title(head.title);
    let mut tags = String::new();
    if let Some(description) = head.description {
        let _ = writeln!(
            tags,
            r#"<meta name="description" content="{}">"#,
            escaped(description)
        );
    }
    if !head.indexed {
        tags.push_str("<meta name=\"robots\" content=\"noindex, follow\">\n");
    }
    if let Some(data) = head.structured {
        let _ = writeln!(
            tags,
            r#"<script type="application/ld+json">{data}</script>"#
        );
    }
    if let Some(address) = head.canonical {
        let _ = writeln!(
            tags,
            r#"<link rel="canonical" href="{}">"#,
            escaped(address)
        );
        let _ = writeln!(
            tags,
            "<meta property=\"og:type\" content=\"website\">\n\
             <meta property=\"og:site_name\" content=\"Lokyr\">\n\
             <meta property=\"og:url\" content=\"{}\">\n\
             <meta property=\"og:title\" content=\"{}\">",
            escaped(address),
            escaped(head.title)
        );
        if let Some(description) = head.description {
            let _ = writeln!(
                tags,
                r#"<meta property="og:description" content="{}">"#,
                escaped(description)
            );
        }
        let _ = writeln!(
            tags,
            "<meta property=\"og:image\" content=\"{SOCIAL_IMAGE}\">\n\
             <meta property=\"og:image:width\" content=\"1200\">\n\
             <meta property=\"og:image:height\" content=\"630\">\n\
             <meta property=\"og:image:alt\" content=\"{}\">\n\
             <meta name=\"twitter:card\" content=\"summary_large_image\">",
            escaped(SOCIAL_IMAGE_ALT)
        );
    }
    // The root as a link: the folder itself where the page is at the root.
    let root = if up.is_empty() { "./" } else { up };
    // The bar's sections, the page's own marked as the current one.
    let mut sections = String::new();
    for section in Section::ALL {
        let current = if head.section == Some(section) {
            r#" aria-current="page""#
        } else {
            ""
        };
        let _ = write!(
            sections,
            "\n    <a href=\"{up}{}\"{current}>{}</a>",
            section.page(),
            section.label()
        );
    }
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
{tags}<meta name="color-scheme" content="dark">
<meta name="theme-color" content="#0b0911">
<link rel="icon" href="{up}icon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="{up}apple-touch-icon.png">
<link rel="alternate" type="application/atom+xml" title="Lokyr releases" href="{up}releases.xml">
<link rel="stylesheet" href="{up}style.css">
<script src="{up}site.js" defer></script>
{more}</head>
<body>
<a class="skip" href="#main">Skip to content</a>
<div class="backdrop" aria-hidden="true"></div>
<header class="bar">
<div class="wrap top">
  <a class="brand" href="{root}"><img src="{up}icon.svg" alt="" width="28" height="28">Lokyr</a>
  <nav aria-label="Sections">{sections}
    <a href="https://github.com/sephiroth74/lokyr">GitHub</a>{social}
  </nav>
</div>
</header>
{main}
<footer>
  <div class="wrap">
    <nav class="footer-links" aria-label="More">
      <a href="https://github.com/sephiroth74/lokyr">GitHub</a>
      <a href="https://pkg.lokyr.dev">Plugin registry</a>
      <a href="{up}docs/releases/">Release notes</a>
      <a href="{up}releases.xml">Releases feed</a>
    </nav>
    <p>© 2026 Alessandro Crugnola · Lokyr and Sleip are under the MIT License.</p>
    <p>{extra}</p>
  </div>
</footer>
</body>
</html>
"##,
        title = escaped(&title),
    )
}

/// A document as the site renders it.
struct Rendered {
    /// Its HTML: its headings of GitHub's anchors, its links to the
    /// documents made their pages', and to the repository's other files
    /// made GitHub's.
    body: String,
    /// Its first heading's text.
    title: String,
    /// Whether its title is its own, of a first-level heading, rather than
    /// made of its file's name.
    titled: bool,
    /// Its first paragraph outside a list, a quote or a table of
    /// `DESCRIPTION_LEAST` characters or more, as plain text.
    description: Option<String>,
    /// Its sections and their parts, its second and third levels of
    /// headings, for its table of contents.
    headings: Vec<Heading>,
}

/// A heading's words, each run of them told as code or not.
type Words = Vec<(bool, String)>;

/// A heading of the table of contents: its level, 2 or 3, its anchor, and
/// its words.
#[derive(Debug, PartialEq)]
struct Heading {
    level: u8,
    id: String,
    words: Words,
}

/// The rendering of `document`'s Markdown `text`.
fn html(document: &Path, text: &str) -> Rendered {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let events: Vec<Event> = Parser::new_ext(text, options).collect();
    // Each heading's text, of its anchor, a repeated one numbered as GitHub
    // numbers it.
    let mut ids = Vec::new();
    // Each heading's weight in the search, where it is not Pagefind's own.
    let mut weights = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut title = None;
    let mut heading: Option<(HeadingLevel, String, Words)> = None;
    let mut headings = Vec::new();
    // The description: the paragraph read, and how deep the events are in
    // lists, quotes, tables and footnotes, whose paragraphs it passes over.
    let mut description = None;
    let mut paragraph: Option<String> = None;
    // Whether the paragraph read is a marker line, of a `<sub>` alone,
    // which the description passes over.
    let mut marker = false;
    let mut nested = 0usize;
    // The first list item's text, the description of a document of no
    // paragraph of its own, as the release notes are.
    let mut item: Option<String> = None;
    let mut first_item: Option<String> = None;
    for event in &events {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((*level, String::new(), Vec::new()));
            }
            Event::Start(
                Tag::List(_) | Tag::BlockQuote(_) | Tag::Table(_) | Tag::FootnoteDefinition(_),
            ) => nested += 1,
            Event::End(
                TagEnd::List(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::Table
                | TagEnd::FootnoteDefinition,
            ) => nested = nested.saturating_sub(1),
            Event::Start(Tag::Paragraph) if nested == 0 && description.is_none() => {
                paragraph = Some(String::new());
                marker = false;
            }
            Event::InlineHtml(html)
                if paragraph.as_deref() == Some("")
                    && html.trim().to_ascii_lowercase().starts_with("<sub") =>
            {
                marker = true;
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(words) = paragraph.take() {
                    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !marker && words.chars().count() >= DESCRIPTION_LEAST {
                        description = Some(words);
                    }
                }
            }
            Event::Start(Tag::Item) if item.is_none() && first_item.is_none() => {
                item = Some(String::new());
            }
            Event::End(TagEnd::Item) if nested == 1 => {
                if let Some(words) = item.take() {
                    first_item = Some(words.split_whitespace().collect::<Vec<_>>().join(" "));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                for words in [&mut paragraph, &mut item].into_iter().flatten() {
                    words.push(' ');
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, words, runs)) = &mut heading {
                    words.push_str(text);
                    runs.push((matches!(event, Event::Code(_)), text.to_string()));
                }
                for words in [&mut paragraph, &mut item].into_iter().flatten() {
                    words.push_str(text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, words, runs)) = heading.take() {
                    if title.is_none() && level == HeadingLevel::H1 {
                        title = Some(words.clone());
                    }
                    weights.push(weight(level, &runs));
                    let base = slug(&words);
                    let count = seen.entry(base.clone()).or_insert(0);
                    let id = if *count == 0 {
                        base
                    } else {
                        format!("{base}-{count}")
                    };
                    *count += 1;
                    let level = match level {
                        HeadingLevel::H2 => Some(2),
                        HeadingLevel::H3 => Some(3),
                        _ => None,
                    };
                    if let Some(level) = level {
                        headings.push(Heading {
                            level,
                            id: id.clone(),
                            words: runs,
                        });
                    }
                    ids.push(id);
                }
            }
            _ => {}
        }
    }
    let mut ids = ids.into_iter();
    let mut weights = weights.into_iter();
    // The headings of their anchors, a section's and a part's of a `#` after
    // their words that links to them, for the pointer alone (the table of
    // contents is the keyboard's way), which the search leaves out.
    let mut rendered = Vec::with_capacity(events.len());
    let mut anchor = None;
    for event in events {
        match event {
            Event::Start(Tag::Heading {
                level,
                id: None,
                classes,
                mut attrs,
            }) => {
                let id = ids.next();
                if matches!(level, HeadingLevel::H2 | HeadingLevel::H3) {
                    anchor.clone_from(&id);
                }
                if let Some(weight) = weights.next().flatten() {
                    attrs.push((
                        CowStr::from("data-pagefind-weight"),
                        Some(CowStr::from(weight)),
                    ));
                }
                rendered.push(Event::Start(Tag::Heading {
                    level,
                    id: id.map(CowStr::from),
                    classes,
                    attrs,
                }));
            }
            // GitHub's alerts, `> [!NOTE]` and the others, as callouts of
            // their kind's title, as GitHub shows them.
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let (class, title) = callout(kind);
                rendered.push(Event::Html(CowStr::from(format!(
                    "<div class=\"callout callout-{class}\">\n<p class=\"callout-title\">{title}</p>\n"
                ))));
            }
            Event::End(TagEnd::BlockQuote(Some(_))) => {
                rendered.push(Event::Html(CowStr::from("</div>\n")));
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some(id) = anchor.take() {
                    rendered.push(Event::InlineHtml(CowStr::from(format!(
                        r##"<a class="anchor" href="#{}" aria-hidden="true" tabindex="-1" data-pagefind-ignore>#</a>"##,
                        escaped(&id)
                    ))));
                }
                rendered.push(Event::End(TagEnd::Heading(level)));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => rendered.push(Event::Start(Tag::Link {
                link_type,
                dest_url: CowStr::from(linked(document, &dest_url)),
                title,
                id,
            })),
            event => rendered.push(event),
        }
    }
    let events = rendered.into_iter();
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, events);
    // A document of no title of its own: a release's notes named of their
    // version, another of its file's name.
    let stem = document
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fallback = if release_notes(document) {
        format!("Lokyr {stem} release notes")
    } else {
        stem
    };
    let description = description.or(first_item.filter(|words| !words.is_empty()));
    Rendered {
        body,
        titled: title.is_some(),
        title: title.unwrap_or(fallback),
        description,
        headings,
    }
}

/// Whether `document` is a release's notes, which have no title of their
/// own, as GitHub's release shows them under its version: their page takes
/// one of their version. The folder's index is not.
fn release_notes(document: &Path) -> bool {
    document.parent() == Some(Path::new("docs/releases"))
        && document.file_name().is_some_and(|name| name != "README.md")
}

/// Whether `document` is a report's appendix: a file of the report's own
/// folder in a collection, as its tables or its machine's setup, which the
/// report links. It stays out of the search engines' results and of the
/// sitemap, as the report stands for it.
fn appendix(document: &Path) -> bool {
    COLLECTIONS.iter().any(|collection| {
        let collection = Path::new(collection);
        document.starts_with(collection) && document.parent() != Some(collection)
    })
}

/// The weight in the search of a heading of `level` and of the words of
/// `runs`, where Pagefind's own, of its level, is not enough: the page's
/// title above every other heading, and a heading of code alone, as a
/// key of the config's or a command, as high as a title.
fn weight(level: HeadingLevel, runs: &Words) -> Option<&'static str> {
    if level == HeadingLevel::H1 {
        Some("10")
    } else if !runs.is_empty() && runs.iter().all(|(code, _)| *code) {
        Some("8")
    } else {
        None
    }
}

/// The class and the title of a callout of `kind`.
fn callout(kind: BlockQuoteKind) -> (&'static str, &'static str) {
    match kind {
        BlockQuoteKind::Note => ("note", "Note"),
        BlockQuoteKind::Tip => ("tip", "Tip"),
        BlockQuoteKind::Important => ("important", "Important"),
        BlockQuoteKind::Warning => ("warning", "Warning"),
        BlockQuoteKind::Caution => ("caution", "Caution"),
    }
}

/// Where a link of `document` to `target` goes in the site: another
/// document's page, of its anchor; a file of the repository on GitHub, the
/// summary's too; the web, an anchor of the page or a mail as it is.
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
    // The summary, which the sidebar shows, is no page: its file, on
    // GitHub.
    let is_document = file.extension().is_some_and(|extension| extension == "md")
        && file != Path::new(SUMMARY)
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
        let page = folder_form(&relative(folder, &page_path(&file)));
        let page = if page.is_empty() {
            "./".to_owned()
        } else {
            page
        };
        return format!("{page}{anchor}");
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
        assert_eq!(linked(plugin, "README.md"), "./");
        assert_eq!(linked(plugin, "README.md#tabs"), "./#tabs");
        assert_eq!(linked(plugin, "../README.md"), "../");
        assert_eq!(
            linked(Path::new("CONTRIBUTING.md"), "docs/SUMMARY.md"),
            "https://github.com/sephiroth74/lokyr/blob/main/docs/SUMMARY.md"
        );
        assert_eq!(
            linked(Path::new("docs/README.md"), "plugins/README.md"),
            "plugins/"
        );
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
    fn a_link_to_no_file_or_no_anchor_is_told() {
        let out = std::env::temp_dir().join(format!("lokyr-site-links-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        fs::create_dir_all(out.join("docs")).unwrap();
        fs::write(out.join("index.html"), r#"<a href="docs/b.html#y">b</a>"#).unwrap();
        fs::write(
            out.join("docs/a.html"),
            r##"<a href="b.html#x">a</a> <a href="c.html">c</a> <a href="/">home</a> <a href="https://x.dev">web</a> <a href="#here">here</a> <h2 id="here">Here</h2>"##,
        )
        .unwrap();
        fs::write(out.join("docs/b.html"), r#"<h2 id="y">Y</h2>"#).unwrap();
        let mut broken = broken_links(&out).unwrap();
        broken.sort();
        assert_eq!(
            broken,
            [
                "docs/a.html: `b.html#x`: no such anchor",
                "docs/a.html: `c.html`: no such file"
            ]
        );
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn a_page_s_description_is_its_first_paragraph_of_some_length() {
        let Rendered { description, .. } = html(
            Path::new("docs/x.md"),
            "# X\n\n<sub>macOS · zsh, bash, fish · on by default · [`[terminal] shell-integration`](CONFIGURATION.md#shell-integration)</sub>\n\n- a list's item that is long enough to be taken, yet passed over\n\n> A quote that is long enough to be taken, yet passed over too.\n\nThe guide to `x`, of [every key](y.md)\nthat the file holds, and of its values.\n\nA later paragraph that the description does not take.\n",
        );
        assert_eq!(
            description.as_deref(),
            Some("The guide to x, of every key that the file holds, and of its values.")
        );
        let Rendered {
            title,
            description: listed,
            ..
        } = html(
            Path::new("docs/releases/9.9.9.md"),
            "## What's new\n\n- **Plugins**: `sleip add`\n  installs one.\n- Another.\n",
        );
        assert_eq!(title, "Lokyr 9.9.9 release notes");
        assert_eq!(listed.as_deref(), Some("Plugins: sleip add installs one."));
        let Rendered {
            description: none, ..
        } = html(Path::new("docs/x.md"), "# X\n\nShort.\n");
        assert_eq!(none, None);
        assert_eq!(shortened("Short."), "Short.");
        let long = "word ".repeat(60);
        let cut = shortened(long.trim());
        assert!(cut.chars().count() <= DESCRIPTION, "{cut}");
        assert!(cut.ends_with("word…"), "{cut}");
    }

    #[test]
    fn a_page_s_address_is_canonical_of_its_folder_and_the_sitemap_lists_it() {
        assert_eq!(canonical(Path::new("index.html")), "https://lokyr.dev/");
        assert_eq!(
            canonical(Path::new("docs/plugins/index.html")),
            "https://lokyr.dev/docs/plugins/"
        );
        assert_eq!(
            canonical(Path::new("docs/CONFIGURATION.html")),
            "https://lokyr.dev/docs/CONFIGURATION.html"
        );
        assert_eq!(folder_form("reindex.html"), "reindex.html");
        let dates =
            dated("\x002026-11-13\n\ndocs/a.md\n\n\x002026-11-01\n\ndocs/a.md\ndocs/b.md\n");
        assert_eq!(dates[Path::new("docs/a.md")], "2026-11-13");
        assert_eq!(dates[Path::new("docs/b.md")], "2026-11-01");
        let xml = sitemap(&[
            (
                "https://lokyr.dev/".to_owned(),
                Some("2026-11-13".to_owned()),
            ),
            ("https://lokyr.dev/a.html?x&y".to_owned(), None),
        ]);
        assert!(
            xml.contains("<url><loc>https://lokyr.dev/</loc><lastmod>2026-11-13</lastmod></url>")
        );
        assert!(xml.contains("<url><loc>https://lokyr.dev/a.html?x&amp;y</loc></url>"));
    }

    #[test]
    fn the_pages_changed_are_those_of_their_documents_or_all() {
        let documents = [
            PathBuf::from("docs/README.md"),
            PathBuf::from("CHANGELOG.md"),
        ];
        let changed = [
            PathBuf::from("docs/plugins/tabs.md"),
            PathBuf::from("CHANGELOG.md"),
            PathBuf::from("crates/kernel/src/lib.rs"),
            PathBuf::from("README.md"),
            PathBuf::from("site/public/index.html"),
        ];
        assert_eq!(
            pages_of(&changed, &documents, false),
            [
                "https://lokyr.dev/",
                "https://lokyr.dev/docs/plugins/tabs.html",
                "https://lokyr.dev/changelog.html"
            ]
        );
        assert!(pages_of(&[PathBuf::from("Cargo.toml")], &documents, false).is_empty());
        // A report's appendix, which the engines do not list, told of none.
        let appendix = PathBuf::from("docs/bench/ab-F3-49/motion/tables.md");
        assert!(pages_of(std::slice::from_ref(&appendix), &documents, false).is_empty());
        assert_eq!(pages_of(&[], &[appendix], true), ["https://lokyr.dev/"]);
        // The summary, which every page's sidebar shows, and no page.
        let pages = pages_of(&[PathBuf::from(SUMMARY)], &documents, false);
        assert_eq!(
            pages,
            [
                "https://lokyr.dev/",
                "https://lokyr.dev/docs/",
                "https://lokyr.dev/changelog.html"
            ]
        );
        assert_eq!(
            pages_of(&[], &documents, true),
            [
                "https://lokyr.dev/",
                "https://lokyr.dev/docs/",
                "https://lokyr.dev/changelog.html"
            ]
        );
    }

    #[test]
    fn the_releases_feed_holds_each_release_s_notes_the_newest_first() {
        let documents = [
            PathBuf::from("docs/releases/0.9.0.md"),
            PathBuf::from("docs/releases/1.0.0.md"),
            PathBuf::from("docs/releases/README.md"),
            PathBuf::from("docs/releases/2.0.0.md"),
            PathBuf::from("docs/x.md"),
        ];
        let texts = [
            "## What's new\n\n- **Plugins** from the registry & more.\n".to_owned(),
            "The first public release, of [the guide](../CONFIGURATION.md).\n".to_owned(),
            "# Release notes\n".to_owned(),
            "Not released yet.\n".to_owned(),
            "# X\n".to_owned(),
        ];
        let dates = release_dates(
            "## [Unreleased]\n\n## [1.0.0] - 2026-11-13\n\n## [0.9.0] - 2026-11-12\n",
        );
        let feed = release_feed(&documents, &texts, &dates);
        assert!(feed.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
        assert!(feed.contains("<updated>2026-11-13T00:00:00Z</updated>\n<entry>"));
        let first = feed.find("<title>Lokyr 1.0.0</title>").unwrap();
        let second = feed.find("<title>Lokyr 0.9.0</title>").unwrap();
        assert!(first < second);
        // A version of no date in the changelog, and the folder's index, are
        // no release.
        assert!(!feed.contains("2.0.0") && !feed.contains("Release notes</title>"));
        assert!(feed.contains("<id>https://lokyr.dev/docs/releases/1.0.0.html</id>"));
        assert!(
            feed.contains("href=&quot;../CONFIGURATION.html&quot;"),
            "{feed}"
        );
        assert!(feed.contains("registry &amp;amp; more"), "{feed}");
        assert_eq!(feed.matches("<entry>").count(), 2);
    }

    #[test]
    fn a_page_s_breadcrumbs_lead_from_the_home_through_its_folders_indexes() {
        let titles = HashMap::from([
            (
                PathBuf::from("docs/README.md"),
                "Lokyr — project documentation".to_owned(),
            ),
            (
                PathBuf::from("docs/plugins/README.md"),
                "Plugins".to_owned(),
            ),
        ]);
        let trail = |document: &str, title: &str| -> Vec<String> {
            crumbs(Path::new(document), title, &titles)
                .into_iter()
                .map(|(name, address)| format!("{name} {address}"))
                .collect()
        };
        assert_eq!(
            trail("docs/plugins/tabs.md", "tabs: the tab bar"),
            [
                "Lokyr https://lokyr.dev/",
                "Documentation https://lokyr.dev/docs/",
                "Plugins https://lokyr.dev/docs/plugins/",
                "tabs: the tab bar https://lokyr.dev/docs/plugins/tabs.html"
            ]
        );
        assert_eq!(
            trail("docs/plugins/README.md", "Plugins"),
            [
                "Lokyr https://lokyr.dev/",
                "Documentation https://lokyr.dev/docs/",
                "Plugins https://lokyr.dev/docs/plugins/"
            ]
        );
        // A folder of no index of its own takes no step.
        assert_eq!(trail("docs/bench/x.md", "X").len(), 3);
        assert_eq!(
            trail("CHANGELOG.md", "Changelog"),
            [
                "Lokyr https://lokyr.dev/",
                "Changelog https://lokyr.dev/changelog.html"
            ]
        );
        let data = breadcrumbs(&crumbs(
            Path::new("docs/plugins/tabs.md"),
            "tabs</script>",
            &titles,
        ));
        let value: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(value["@type"], "BreadcrumbList");
        assert_eq!(value["itemListElement"][3]["position"], 4);
        assert_eq!(value["itemListElement"][3]["name"], "tabs</script>");
        assert!(!data.contains("</"));
    }

    #[test]
    fn a_page_s_head_tells_its_title_description_and_address() {
        let settings = Settings::default();
        let shown = page(
            Path::new("docs/plugins/reference.md"),
            "# The bundled plugins\n\nEvery feature that you see comes from a plugin, and these come with Lokyr.\n",
            &settings,
            &HashMap::new(),
            &Summary::default(),
        );
        assert!(shown.contains("<title>The bundled plugins — Lokyr</title>"));
        assert!(shown.contains(r#"<meta name="description" content="Every feature that you see comes from a plugin, and these come with Lokyr.">"#));
        assert!(shown.contains(
            r#"<link rel="canonical" href="https://lokyr.dev/docs/plugins/reference.html">"#
        ));
        assert!(shown.contains(r#"<meta property="og:title" content="The bundled plugins">"#));
        assert!(!shown.contains("noindex"));
        assert!(shown.contains(r#"<a href="../../docs/">Docs</a>"#));
        // The skip link first, to the page's main.
        let body = &shown[shown.find("<body>").unwrap()..];
        assert!(body.contains("<body>\n<a class=\"skip\" href=\"#main\">Skip to content</a>"));
        assert!(body.contains(r#"<main class="doc" id="main" data-pagefind-body>"#));
        assert!(shown.contains(r#"<a href="../../docs/plugins/" aria-current="page">Plugins</a>"#));
        assert_eq!(shown.matches("aria-current").count(), 1);
        assert!(shown.contains(r#"<meta property="og:image" content="https://lokyr.dev/og.png">"#));
        assert!(shown.contains(r#"<meta name="twitter:card" content="summary_large_image">"#));
        // A title of the name keeps it once; a page of no paragraph is
        // described by its title.
        let named = page(
            Path::new("docs/guide.md"),
            "# Lokyr — the documentation\n",
            &settings,
            &HashMap::new(),
            &Summary::default(),
        );
        assert!(named.contains("<title>Lokyr — the documentation</title>"));
        assert!(named.contains(r#"<meta name="description" content="Lokyr — the documentation, of the documentation of Lokyr"#));
        // The search and the missing page are kept out of the results, the
        // missing page's links from the root.
        let search = search_page(&settings);
        assert!(search.contains(r#"<meta name="robots" content="noindex, follow">"#));
        assert!(!search.contains("canonical"));
        assert!(search.contains(r#"<a class="brand" href="./">"#));
        assert!(search.contains(r#"<a href="search.html" aria-current="page">Search</a>"#));
        assert_eq!(
            Section::of(Path::new("docs/releases/1.0.0.md")),
            Some(Section::Changelog)
        );
        assert_eq!(
            Section::of(Path::new("docs/bench/RELEASES.md")),
            Some(Section::Docs)
        );
        assert_eq!(Section::of(Path::new("CONTRIBUTING.md")), None);
        let missing = missing_page(&settings);
        assert!(missing.contains("noindex"));
        assert!(missing.contains(r#"<link rel="stylesheet" href="/style.css">"#));
    }

    #[test]
    fn the_home_s_road_tells_its_steps_and_the_one_under_way() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let home = fs::read_to_string(root.join("site/public/index.html")).unwrap();
        let start = "<ol class=\"road\" style=\"--steps: ";
        let road = &home[home.find(start).expect("the home's road") + start.len()..];
        let (steps, rest) = road.split_once("; --now: ").unwrap();
        let (now, rest) = rest.split_once("\">").unwrap();
        let items = &rest[..rest.find("</ol>").unwrap()];
        let classes: Vec<&str> = items
            .split("<li class=\"")
            .skip(1)
            .map(|item| &item[..item.find('"').unwrap()])
            .collect();
        // A column of each step, and the line's colors to the one under
        // way, which a release moves: its count and its place, from 1.
        assert_eq!(
            steps.parse::<usize>().unwrap(),
            classes.len(),
            "{classes:?}"
        );
        let under_way = classes.iter().position(|class| *class == "now").unwrap() + 1;
        assert_eq!(now.parse::<usize>().unwrap(), under_way, "{classes:?}");
        assert_eq!(classes.iter().filter(|class| **class == "now").count(), 1);
    }

    #[test]
    fn the_home_s_structured_data_is_of_the_latest_release() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let home = fs::read_to_string(root.join("site/public/index.html")).unwrap();
        let start = r#"<script type="application/ld+json">"#;
        let data = &home[home.find(start).expect("the home's JSON-LD") + start.len()..];
        let data: serde_json::Value =
            serde_json::from_str(&data[..data.find("</script>").unwrap()]).unwrap();
        assert_eq!(data["@context"], "https://schema.org");
        let app = data["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["@type"] == "SoftwareApplication")
            .expect("a SoftwareApplication");
        // The feed's latest release, which `just release` writes as the
        // release is published.
        let feed = fs::read_to_string(root.join("site/public/appcast.xml")).unwrap();
        let version = regex::Regex::new(r"<sparkle:shortVersionString>([^<]+)<")
            .unwrap()
            .captures_iter(&feed)
            .map(|found| found[1].to_owned())
            .max_by_key(|version| {
                version
                    .split('.')
                    .map(|part| part.parse::<u64>().unwrap_or(0))
                    .collect::<Vec<_>>()
            })
            .expect("a release in the feed");
        assert_eq!(
            app["softwareVersion"], version,
            "the home's JSON-LD names another version than the feed's latest"
        );
        assert_eq!(
            app["releaseNotes"],
            format!("{ORIGIN}/docs/releases/{version}.html")
        );
        assert!(root.join(format!("docs/releases/{version}.md")).is_file());
        assert_eq!(app["image"], SOCIAL_IMAGE);
        // Its screenshots, the home's, each a file of the docs at the width
        // that the home's image offers, of the image's text as its caption.
        let screenshots = app["screenshot"].as_array().expect("its screenshots");
        assert!(!screenshots.is_empty());
        for shot in screenshots {
            let address = shot["url"].as_str().unwrap();
            let file = address.strip_prefix(&format!("{ORIGIN}/")).unwrap();
            assert!(root.join(file).is_file(), "{file}");
            assert!(
                home.contains(&format!("{file} {}w", shot["width"])),
                "the home shows no {file} of its width"
            );
            assert!(
                home.contains(&format!("alt=\"{}\"", shot["caption"].as_str().unwrap())),
                "{file}: its caption is not its image's text"
            );
        }
        assert!(home.contains(&format!(
            r#"<meta property="og:image:alt" content="{}">"#,
            escaped(SOCIAL_IMAGE_ALT)
        )));
        assert!(root.join("site/public/og.png").is_file());
        // The top bar's pill of the same release, to its notes.
        assert!(home.contains(&format!(
            r#"<a class="pill" href="docs/releases/{version}.html">"#
        )));
        assert!(home.contains(&format!("{version} is out</a>")));
    }

    #[test]
    fn a_page_of_four_sections_has_a_table_of_contents_of_their_parts() {
        let text = "# Guide\n\nIntro.\n\n## The file\n\n### A complete example\n\n## `[font]`\n\n### `family`\n\n### `bold-style`, `italic-style`\n\n## `[text]`\n\n## Last\n";
        let rendered = html(Path::new("docs/x.md"), text);
        assert_eq!(rendered.headings.len(), 7);
        assert_eq!(
            rendered.headings[4],
            Heading {
                level: 3,
                id: "bold-style-italic-style".to_owned(),
                words: vec![
                    (true, "bold-style".to_owned()),
                    (false, ", ".to_owned()),
                    (true, "italic-style".to_owned())
                ],
            }
        );
        let shown = contents(
            &rendered.body,
            &rendered.headings,
            &(String::new(), String::new()),
        );
        assert!(shown.starts_with("<div class=\"wrap page has-nav has-toc\">\n"));
        assert!(shown.contains("\n<nav class=\"toc\""));
        assert!(shown.contains(r##"<li><details><summary><a href="#font"><code>[font]</code></a></summary><ul><li><a href="#family"><code>family</code></a></li><li><a href="#bold-style-italic-style"><code>bold-style</code>, <code>italic-style</code></a></li></ul></details></li>"##), "{shown}");
        assert!(shown.contains(r##"<li><a href="#text"><code>[text]</code></a></li>"##));
        // The short one, of the sections alone, right after the title.
        assert!(shown.contains("</h1>\n<details class=\"toc-inline\">\n<summary>On this page</summary>\n<ul>\n<li><a href=\"#the-file\">The file</a></li>"));
        let inline = &shown[shown.find("toc-inline").unwrap()..];
        assert!(!inline[..inline.find("</details>").unwrap()].contains("#family\""));
        // Three sections have none.
        let short = html(Path::new("docs/y.md"), "# Y\n\n## A\n\n## B\n\n## C\n");
        assert!(
            !contents(
                &short.body,
                &short.headings,
                &(String::new(), String::new())
            )
            .contains("toc")
        );
    }

    #[test]
    fn a_page_s_headings_take_github_s_anchors_and_its_first_its_title() {
        let Rendered { body, title, .. } = html(
            Path::new("docs/x.md"),
            "# The `plugin.toml` guide\n\n## Limits\n\n## Limits\n\nSee [the guide](y.md#a).\n",
        );
        assert_eq!(title, "The plugin.toml guide");
        // The title weighs most in the search.
        assert!(
            body.contains(r#"<h1 id="the-plugintoml-guide" data-pagefind-weight="10">"#),
            "{body}"
        );
        assert!(
            body.contains(r##"<h2 id="limits">Limits<a class="anchor" href="#limits" aria-hidden="true" tabindex="-1" data-pagefind-ignore>#</a></h2>"##),
            "{body}"
        );
        assert!(!body.contains(r##"href="#the-plugintoml-guide""##));
        assert!(
            body.contains(r#"<h2 id="limits">"#) && body.contains(r#"<h2 id="limits-1">"#),
            "{body}"
        );
        assert!(body.contains(r#"<a href="y.html#a">"#), "{body}");
    }

    /// The summary of the tests: a home, a guide of two pages under it,
    /// and the release notes' first page.
    const SUMMARY_TEXT: &str = "# Summary\n\nWhat it holds.\n\n## Start\n\n- [Home](README.md)\n\n## Use\n\n- [Plugins](plugins/README.md)\n  - [Tabs](plugins/tabs.md)\n    - [Groups](plugins/groups.md)\n- [Changelog](../CHANGELOG.md)\n\n## Reference\n\n- [Release notes](releases/README.md)\n";

    #[test]
    fn the_summary_reads_its_groups_and_their_pages_under_one_another() {
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let names: Vec<&str> = summary
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect();
        assert_eq!(names, ["Start", "Use", "Reference"]);
        let plugins = &summary.groups[1].entries[0];
        assert_eq!(
            (plugins.title.as_str(), plugins.document.as_path()),
            ("Plugins", Path::new("docs/plugins/README.md"))
        );
        assert_eq!(
            plugins.entries[0].entries[0].document,
            Path::new("docs/plugins/groups.md")
        );
        // A root's document, of its path from the root.
        assert_eq!(
            summary.groups[1].entries[1].document,
            Path::new("CHANGELOG.md")
        );
        assert_eq!(summary.documents().len(), 6);
        // A line that is no page, a page an odd number of spaces deep, or
        // deeper than the one above it, refused of its line.
        for (text, told) in [
            ("## A\n\nSome words.\n", "SUMMARY.md:3: a line of no page"),
            (
                "## A\n\n- [X](x.md)\n - [Y](y.md)\n",
                "SUMMARY.md:4: a page's list item two spaces",
            ),
            ("## A\n\n    - [Y](y.md)\n", "SUMMARY.md:3: a page deeper"),
            ("## A\n\n- [X](../../x.md)\n", "SUMMARY.md:3: a link past"),
        ] {
            let error = Summary::parse(text).unwrap_err().to_string();
            assert!(error.contains(told), "{error}");
        }
    }

    #[test]
    fn every_public_page_is_in_the_summary_once_but_the_reports_of_a_listed_collection() {
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let mut documents: Vec<PathBuf> = summary
            .documents()
            .into_iter()
            .map(Path::to_path_buf)
            .collect();
        documents.push(PathBuf::from("docs/releases/1.2.0.md"));
        // The release notes' page lists the notes.
        let links = HashMap::from([(
            PathBuf::from("docs/releases/README.md"),
            vec![PathBuf::from("docs/releases/1.2.0.md")],
        )]);
        assert!(summary.problems(&documents, &links).is_empty());
        // A page that it leaves out, and a measurement of a collection that
        // it does not list.
        documents.push(PathBuf::from("docs/keyboard.md"));
        documents.push(PathBuf::from("docs/bench/rest.md"));
        let problems = summary.problems(&documents, &links);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("docs/keyboard.md is in no group of docs/SUMMARY.md"));
        assert!(problems[1].starts_with("docs/bench/rest.md is in no group"));
        // A page that it lists twice, or that is no public page.
        let twice = Summary::parse(&format!("{SUMMARY_TEXT}- [Again](README.md)\n")).unwrap();
        let problems = twice.problems(&documents[..7], &links);
        assert_eq!(problems, ["docs/SUMMARY.md lists docs/README.md twice"]);
        let problems = summary.problems(&documents[1..], &links);
        assert!(
            problems.contains(
                &"docs/SUMMARY.md lists docs/README.md, which is no public page".to_owned()
            ),
            "{problems:?}"
        );
        // The repository's own summary holds every public page.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let public = super::documents(&root).unwrap();
        assert!(!public.iter().any(|document| document == Path::new(SUMMARY)));
        summary_of(&root, &public);
    }

    #[test]
    fn a_report_of_a_listed_collection_is_reached_from_its_listed_page() {
        let summary =
            Summary::parse("# Summary\n\n## Reference\n\n- [Release notes](releases/README.md)\n")
                .unwrap();
        let documents = [
            PathBuf::from("docs/releases/README.md"),
            PathBuf::from("docs/releases/1.2.0.md"),
            PathBuf::from("docs/releases/1.1.0.md"),
            PathBuf::from("docs/releases/0.9.0.md"),
        ];
        // 1.2.0 listed, 1.1.0 of a link of 1.2.0's, 0.9.0 of none.
        let readme = "# Release notes\n\n- [1.2.0](1.2.0.md#added), of [the changelog](../../CHANGELOG.md)\n- [GitHub](https://github.com/x/releases)\n";
        let links = HashMap::from([
            (documents[0].clone(), document_links(&documents[0], readme)),
            (
                documents[1].clone(),
                document_links(&documents[1], "See [1.1.0](1.1.0.md)."),
            ),
        ]);
        assert_eq!(
            links[&documents[0]],
            [
                PathBuf::from("docs/releases/1.2.0.md"),
                PathBuf::from("CHANGELOG.md")
            ]
        );
        assert_eq!(
            summary.problems(&documents, &links),
            [
                "docs/releases/0.9.0.md is linked from no page of its collection: list it in docs/releases/README.md"
            ]
        );
    }

    #[test]
    fn a_chart_of_the_benches_is_its_svg_in_a_wide_page_and_another_image_stays() {
        let root = std::env::temp_dir().join(format!("lokyr-site-charts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs/bench/releases")).unwrap();
        fs::write(
            root.join("docs/bench/releases/start.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"chart\" data-chart=\"trends\" width=\"680\" height=\"418\" viewBox=\"0 0 680 418\" role=\"img\" aria-label=\"Starting\">\n<style>.chart .surface { fill: #fcfcfb; }</style>\n<rect class=\"surface\"/>\n</svg>\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/bench/plain.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"></svg>\n",
        )
        .unwrap();
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let text = "# The releases\n\n![Starting and typing: the gap & the echo](releases/start.svg)\n\n![A plain one](plain.svg)\n\nText ![inline](releases/start.svg) beside.\n";
        let document = Path::new("docs/bench/RELEASES.md");
        let made = with_charts(
            &root,
            document,
            &page(
                document,
                text,
                &Settings::default(),
                &HashMap::new(),
                &summary,
            ),
        );
        // The chart's SVG, of no size of its own, of the dark colors, of the
        // image's text, its own text out of the search.
        assert!(
            made.contains("<figure class=\"chart-figure\" data-pagefind-ignore><svg xmlns=\"http://www.w3.org/2000/svg\" class=\"chart dark\" data-chart=\"trends\" viewBox=\"0 0 680 418\" role=\"img\" aria-label=\"Starting and typing: the gap &amp; the echo\">\n<style>"),
            "{made}"
        );
        assert!(made.contains("<rect class=\"surface\"/>\n</svg></figure>"));
        // Another image, and a chart's beside text, stay images.
        assert!(made.contains("<p><img src=\"plain.svg\" alt=\"A plain one\" /></p>"));
        assert!(made.contains("Text <img src=\"releases/start.svg\" alt=\"inline\" /> beside."));
        // Its page wide; a page of none not.
        assert!(
            made.contains("<div class=\"wrap page wide has-nav\">"),
            "{made}"
        );
        let plain = with_charts(
            &root,
            document,
            &page(
                document,
                "# Plain\n\n![A plain one](plain.svg)\n",
                &Settings::default(),
                &HashMap::new(),
                &summary,
            ),
        );
        assert!(!plain.contains("wide"));
        assert!(!plain.contains("<figure"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_release_s_notes_are_headed_of_their_version_and_an_appendix_is_not_listed() {
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let settings = Settings::default();
        let titles = HashMap::new();
        let notes = page(
            Path::new("docs/releases/9.9.9.md"),
            "Blocks, and scrolling that follows your fingers.\n\n## What's new\n\n- One.\n\n## Changed\n\n- Two.\n\n## Fixed\n\n- Three.\n\n## Before you update\n\n- Four.\n",
            &settings,
            &titles,
            &summary,
        );
        // The heading of their version, first, its table of contents under it.
        assert!(
            notes.contains("data-pagefind-body>\n<h1 id=\"lokyr-999-release-notes\" data-pagefind-weight=\"10\">Lokyr 9.9.9 release notes</h1>\n<details class=\"toc-inline\">"),
            "{notes}"
        );
        assert!(notes.contains("<title>Lokyr 9.9.9 release notes</title>"));
        assert_eq!(notes.matches("<h1").count(), 1);
        // A document of a title of its own keeps its heading alone.
        let titled = page(
            Path::new("docs/keyboard.md"),
            "# Keyboard\n\nThe keys that Lokyr takes, and how to change them in the config.\n",
            &settings,
            &titles,
            &summary,
        );
        assert_eq!(titled.matches("<h1").count(), 1, "{titled}");
        assert!(release_notes(Path::new("docs/releases/1.4.0.md")));
        assert!(!release_notes(Path::new("docs/releases/README.md")));
        assert!(!html(Path::new("docs/bench/x/tables.md"), "| a |\n|---|\n| 1 |\n").titled);
        // A report's appendix, in its folder, out of the engines' results;
        // the report and the other pages in them.
        assert!(appendix(Path::new("docs/bench/ab-F3-49/motion/tables.md")));
        assert!(appendix(Path::new(
            "docs/bench/glyphs-F2-146/throughput-new.md"
        )));
        assert!(!appendix(Path::new("docs/bench/ab-F3-49.md")));
        assert!(!appendix(Path::new("docs/releases/1.4.0.md")));
        assert!(!appendix(Path::new("docs/plugins/tabs.md")));
        let tables = page(
            Path::new("docs/bench/ab-F3-49/motion/tables.md"),
            "# The motion's means\n\n| a |\n|---|\n| 1 |\n",
            &settings,
            &titles,
            &summary,
        );
        assert!(tables.contains(r#"<meta name="robots" content="noindex, follow">"#));
        assert!(tables.contains(
            r#"<link rel="canonical" href="https://lokyr.dev/docs/bench/ab-F3-49/motion/tables.html">"#
        ));
        assert!(!notes.contains("noindex"));
    }

    #[test]
    fn a_page_s_title_in_the_results_may_say_more_than_its_heading() {
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let titles = HashMap::from([(
            PathBuf::from("docs/plugins/README.md"),
            "Plugins".to_owned(),
        )]);
        let tabs = page(
            Path::new("docs/plugins/tabs.md"),
            "# `tabs`: the tab bar\n\nEach window's tabs, in a bar at the top, at the bottom, or in a column.\n",
            &Settings::default(),
            &titles,
            &summary,
        );
        // The head of the title that says more, the preview's too.
        assert!(
            tabs.contains("<title>Tabs: a tab bar on any side, or vertical tabs — Lokyr</title>"),
            "{tabs}"
        );
        assert!(tabs.contains(
            r#"<meta property="og:title" content="Tabs: a tab bar on any side, or vertical tabs">"#
        ));
        // The heading and the breadcrumbs of the document's own.
        assert!(tabs.contains(">tabs</code>: the tab bar</h1>"), "{tabs}");
        assert!(tabs.contains(r#""name":"tabs: the tab bar""#), "{tabs}");
        // A page of none keeps its heading's.
        let keyboard = page(
            Path::new("docs/keyboard.md"),
            "# Keyboard\n\nThe keys that Lokyr takes, and how to change them in the config.\n",
            &Settings::default(),
            &titles,
            &summary,
        );
        assert!(keyboard.contains("<title>Keyboard — Lokyr</title>"));
        // Each of a length that the results show whole.
        for (document, title) in TITLES {
            let shown = site_title(title);
            assert!(
                shown.chars().count() <= TITLE_MOST,
                "{document}: {shown} is longer than {TITLE_MOST} characters"
            );
        }
        assert_eq!(
            site_title("Screenshots of Lokyr, a terminal for macOS"),
            "Screenshots of Lokyr, a terminal for macOS"
        );
    }

    #[test]
    fn an_svg_image_of_no_size_takes_its_own() {
        let root = std::env::temp_dir().join(format!("lokyr-site-sizes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs/bench/charts")).unwrap();
        fs::write(
            root.join("docs/bench/charts/sized.svg"),
            "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"680\" height=\"284\" viewBox=\"0 0 680 284\"></svg>\n",
        )
        .unwrap();
        fs::write(
            root.join("docs/bench/charts/viewed.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"\n  width=\"100%\" viewBox=\"0,0,640.4,360\"></svg>\n",
        )
        .unwrap();
        let document = Path::new("docs/bench/report.md");
        let page = "<p><img src=\"charts/sized.svg\" alt=\"Startup\" /></p>\n\
                    Text <img src=\"charts/viewed.svg\" alt=\"Memory\"> beside.\n\
                    <img src=\"charts/sized.svg\" alt=\"Kept\" width=\"10\" height=\"5\">\n\
                    <img src=\"../images/shot.webp\" alt=\"A shot\">\n\
                    <img src=\"https://x.dev/a.svg\" alt=\"Far\" />\n\
                    <img src=\"charts/none.svg\" alt=\"None\" />";
        assert_eq!(
            with_sizes(&root, document, page),
            "<p><img src=\"charts/sized.svg\" alt=\"Startup\" width=\"680\" height=\"284\" /></p>\n\
             Text <img src=\"charts/viewed.svg\" alt=\"Memory\" width=\"640\" height=\"360\"> beside.\n\
             <img src=\"charts/sized.svg\" alt=\"Kept\" width=\"10\" height=\"5\">\n\
             <img src=\"../images/shot.webp\" alt=\"A shot\">\n\
             <img src=\"https://x.dev/a.svg\" alt=\"Far\" />\n\
             <img src=\"charts/none.svg\" alt=\"None\" />"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_image_that_tells_nothing_is_told_but_an_icon_beside_its_link_s_words() {
        let out = std::env::temp_dir().join(format!("lokyr-site-alts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        fs::create_dir_all(out.join("docs")).unwrap();
        fs::write(
            out.join("index.html"),
            r#"<a class="brand" href="./"><img src="icon.svg" alt="" width="28" height="28">Lokyr</a> <img src="shot.webp" alt="A window of two panes"> <svg class="mark" aria-hidden="true"></svg> <svg role="img" aria-label="Startup"></svg>"#,
        )
        .unwrap();
        fs::write(
            out.join("docs/a.html"),
            r#"<img src="a.svg"> <a href="b.html"><img src="b.svg" alt=""></a> <p><img src="c.svg" alt=""></p> <img src="d.svg" alt="" role="presentation"> <svg role="img" aria-label=""></svg>"#,
        )
        .unwrap();
        let mut unlabelled = unlabelled_images(&out).unwrap();
        unlabelled.sort();
        assert_eq!(
            unlabelled,
            [
                r#"docs/a.html: an <img> of an empty alt outside a link of its own words: <img src="b.svg" alt="">"#,
                r#"docs/a.html: an <img> of an empty alt outside a link of its own words: <img src="c.svg" alt="">"#,
                r#"docs/a.html: an <img> of no alt: <img src="a.svg">"#,
                r#"docs/a.html: an <svg> of role="img" of no label"#,
            ]
        );
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn a_page_made_of_a_source_links_its_source_and_no_editor() {
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let settings = Settings::default();
        let titles = HashMap::new();
        let made = page(
            Path::new("docs/CONFIGURATION.md"),
            "# The configuration\n\nEvery key of the config, of its values and its default.\n",
            &settings,
            &titles,
            &summary,
        );
        assert!(
            made.contains(r#"Made of <a href="https://github.com/sephiroth74/lokyr/blob/main/crates/kernel/config.schema.json">the config's schema</a>"#),
            "{made}"
        );
        assert!(!made.contains("Edit this page on GitHub"));
        let written = page(
            Path::new("docs/keyboard.md"),
            "# Keyboard\n\nThe keys that Lokyr takes, and how to change them in the config.\n",
            &settings,
            &titles,
            &summary,
        );
        assert!(written.contains(r#"<a href="https://github.com/sephiroth74/lokyr/edit/main/docs/keyboard.md">Edit this page on GitHub</a>"#));
    }

    /// The summary of the repository at `root`, which holds `public`.
    fn summary_of(root: &Path, public: &[PathBuf]) -> Summary {
        super::summary(root, public).unwrap()
    }

    #[test]
    fn the_sidebar_marks_the_page_shown_and_opens_the_pages_that_hold_it() {
        let summary = Summary::parse(SUMMARY_TEXT).unwrap();
        let (side, menu) = navigation(&summary, Path::new("docs/plugins/tabs.md"), "../../");
        assert!(side.starts_with("<nav class=\"pages\" aria-label=\"Documentation\">\n<p class=\"pages-group\">Start</p>\n<ul>\n<li><a href=\"../../docs/\">Home</a></li>"), "{side}");
        // The page shown, marked, open in the page that holds it, its own
        // pages too, the root's documents of their pages.
        assert!(side.contains(r#"<li><details open><summary><a href="../../docs/plugins/">Plugins</a></summary><ul>"#), "{side}");
        assert!(side.contains(r#"<li><details open><summary><a href="../../docs/plugins/tabs.html" aria-current="page">Tabs</a></summary>"#), "{side}");
        assert!(side.contains(r#"<li><a href="../../changelog.html">Changelog</a></li>"#));
        // The same, as a menu that opens.
        assert!(menu.starts_with("<details class=\"pages-inline\">\n<summary>Documentation</summary>\n<nav aria-label=\"Documentation\">\n<p class=\"pages-group\">Start</p>"));
        // A release's notes mark the collection's page.
        let (side, _) = navigation(&summary, Path::new("docs/releases/1.2.0.md"), "../../");
        assert!(
            side.contains(
                r#"<a href="../../docs/releases/" aria-current="true">Release notes</a>"#
            ),
            "{side}"
        );
        assert!(!side.contains("<details open>"));
    }

    #[test]
    fn github_s_alerts_are_callouts_and_code_headings_weigh_more_in_the_search() {
        let Rendered { body, .. } = html(
            Path::new("docs/x.md"),
            "# X\n\n> [!TIP]\n> Press ⌘U.\n\n> [!WARNING]\n> It costs.\n\n> A quote.\n\n## `background-opacity`\n\n## The `font`\n",
        );
        assert!(
            body.contains("<div class=\"callout callout-tip\">\n<p class=\"callout-title\">Tip</p>\n<p>Press ⌘U.</p>\n</div>"),
            "{body}"
        );
        assert!(body.contains(
            "<div class=\"callout callout-warning\">\n<p class=\"callout-title\">Warning</p>"
        ));
        assert!(
            body.contains("<blockquote>\n<p>A quote.</p>\n</blockquote>"),
            "{body}"
        );
        assert!(
            body.contains(r#"<h2 id="background-opacity" data-pagefind-weight="8">"#),
            "{body}"
        );
        assert!(body.contains(r#"<h2 id="the-font">"#), "{body}");
        // A page's footer links its source in GitHub's editor.
        let shown = page(
            Path::new("docs/plugins/tabs.md"),
            "# Tabs\n",
            &Settings::default(),
            &HashMap::new(),
            &Summary::parse(SUMMARY_TEXT).unwrap(),
        );
        assert!(shown.contains(r#"<a href="https://github.com/sephiroth74/lokyr/edit/main/docs/plugins/tabs.md">Edit this page on GitHub</a>"#));
        assert!(shown.contains(r#"<div class="wrap page has-nav">"#));
        assert!(shown.contains(r#"aria-current="page">Tabs</a>"#));
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
            "site.js",
            "icon.svg",
            "robots.txt",
            "llms.txt",
            "releases.xml",
            "404.html",
            "sitemap.xml",
            "docs/images/lokyr-htop.png",
        ] {
            assert!(out.join(file).is_file(), "{file}");
        }
        // The sitemap lists the home and every document's page but the
        // reports' appendices, each of its last commit's date; the robots
        // name it.
        let sitemap = fs::read_to_string(out.join("sitemap.xml")).unwrap();
        let listed = documents.iter().filter(|document| !appendix(document));
        assert_eq!(
            sitemap.matches("<url>").count(),
            listed.count() + 1,
            "{sitemap}"
        );
        assert!(!sitemap.contains("/tables.html"));
        assert!(sitemap.contains("<loc>https://lokyr.dev/</loc><lastmod>"));
        assert!(sitemap.contains("<loc>https://lokyr.dev/docs/</loc><lastmod>"));
        assert!(!sitemap.contains("lokyr.dev/search.html") && !sitemap.contains("index.html"));
        let robots = fs::read_to_string(out.join("robots.txt")).unwrap();
        assert!(robots.contains("Sitemap: https://lokyr.dev/sitemap.xml"));
        // `llms.txt`'s links to the site are to its pages.
        let llms = fs::read_to_string(out.join("llms.txt")).unwrap();
        let ours = regex::Regex::new(r"\(https://lokyr\.dev/([^)#]*)\)").unwrap();
        let mut linked_pages = 0;
        for found in ours.captures_iter(&llms) {
            let mut file = out.join(&found[1]);
            if file.is_dir() {
                file = file.join("index.html");
            }
            assert!(file.is_file(), "llms.txt: {}", &found[1]);
            linked_pages += 1;
        }
        assert!(linked_pages > 10, "{linked_pages}");
        // Every page of a canonical address and a description, the home too.
        for document in &documents {
            let shown = fs::read_to_string(out.join(page_path(document))).unwrap();
            assert!(
                shown.contains(r#"<link rel="canonical" href="https://lokyr.dev/"#)
                    && shown.contains(r#"<meta name="description" content=""#),
                "{}",
                document.display()
            );
        }
        let home = fs::read_to_string(out.join("index.html")).unwrap();
        assert!(home.contains(r#"<link rel="canonical" href="https://lokyr.dev/">"#));
        assert!(home.contains(r#"<meta name="description" content=""#));
        let configuration = fs::read_to_string(out.join("docs/CONFIGURATION.html")).unwrap();
        assert!(configuration.contains(r#"id="font""#));
        // Every link of the pages resolves, the home's too.
        let broken = broken_links(&out).unwrap();
        assert!(broken.is_empty(), "{broken:?}");
        // Every image tells what it shows, or is a decoration; every image of
        // an SVG takes its size; every page has one heading of its title, a
        // release's notes too.
        let unlabelled = unlabelled_images(&out).unwrap();
        assert!(unlabelled.is_empty(), "{unlabelled:?}");
        let svg_image = regex::Regex::new(r#"<img src="[^"]*\.svg"[^>]*>"#).unwrap();
        for document in &documents {
            let shown = fs::read_to_string(out.join(page_path(document))).unwrap();
            for image in svg_image.find_iter(&shown) {
                assert!(
                    image.as_str().contains(" width=\""),
                    "{}: {}",
                    document.display(),
                    image.as_str()
                );
            }
            assert_eq!(shown.matches("<h1").count(), 1, "{}", document.display());
        }
        // The search, where Pagefind is there: the configuration's page among
        // those that hold a key of the config.
        if let Some(pagefind) = pagefind(&root) {
            let indexed = Command::new(pagefind)
                .arg("--site")
                .arg(&out)
                .output()
                .unwrap();
            assert!(indexed.status.success());
            let holding: Vec<String> = fs::read_dir(out.join("pagefind/fragment"))
                .unwrap()
                .filter_map(|entry| {
                    let unpacked = Command::new("gzip")
                        .arg("-dc")
                        .arg(entry.ok()?.path())
                        .output()
                        .ok()?;
                    let text = String::from_utf8_lossy(&unpacked.stdout).into_owned();
                    text.contains("scrollback").then_some(text)
                })
                .collect();
            assert!(
                holding
                    .iter()
                    .any(|text| text.contains("/docs/CONFIGURATION.html"))
            );
        }
        // The accounts, none of none, each of its address.
        let settings = Settings {
            social: Social {
                bluesky: "https://bsky.app/profile/lokyr.dev".to_owned(),
                x: String::new(),
            },
        };
        let shown = page(
            Path::new("docs/x.md"),
            "# X\n",
            &settings,
            &HashMap::new(),
            &Summary::default(),
        );
        assert!(shown.contains(r#"<a href="https://bsky.app/profile/lokyr.dev">Bluesky</a>"#));
        assert!(!shown.contains(">X</a>"));
        let _ = fs::remove_dir_all(out);
    }
}
