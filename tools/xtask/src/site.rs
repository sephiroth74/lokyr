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
//! folder's `index.html` as its folder, and its description, a document's
//! of its first paragraph; `sitemap.xml` lists the home and the documents'
//! pages of their last commits' dates, and `404.html` is the page that
//! GitHub Pages serves of an address that the site does not hold.
//! Into `target/site` by default.

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
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("emptying {}", out.display()))?;
    }
    fs::create_dir_all(out).with_context(|| format!("making {}", out.display()))?;
    copy_folder(&root.join("site/public"), out, &|_| true)?;
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
    for document in &documents {
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
    // folder's index name it by.
    let titles: HashMap<PathBuf, String> = documents
        .iter()
        .zip(&texts)
        .map(|(document, text)| (document.clone(), html(document, text).title))
        .collect();
    let changelog =
        fs::read_to_string(root.join("CHANGELOG.md")).context("reading the changelog")?;
    fs::write(
        out.join("releases.xml"),
        release_feed(&documents, &texts, &release_dates(&changelog)),
    )
    .context("writing the releases' feed")?;
    for (document, text) in documents.iter().zip(&texts) {
        let page = page(document, text, &settings, &titles);
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
/// site's `documents` and its home, or of every page.
fn pages_of(changed: &[PathBuf], documents: &[PathBuf], everything: bool) -> Vec<String> {
    let home = Path::new("site/public/index.html");
    let mut pages = Vec::new();
    if everything || changed.iter().any(|file| file == home) {
        pages.push(canonical(Path::new("index.html")));
    }
    for file in changed {
        let is_document = file.extension().is_some_and(|extension| extension == "md")
            && (file.starts_with("docs")
                || ROOT_DOCUMENTS.iter().any(|root| file == Path::new(root)));
        if !everything && is_document {
            pages.push(canonical(&page_path(file)));
        }
    }
    if everything {
        pages.extend(
            documents
                .iter()
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

/// The page of `document`, of its Markdown `text`, in the site's template:
/// the search indexes its body.
fn page(
    document: &Path,
    text: &str,
    settings: &Settings,
    titles: &HashMap<PathBuf, String>,
) -> String {
    let Rendered {
        body,
        title,
        description,
        headings,
    } = html(document, text);
    let depth = document.components().count().saturating_sub(1);
    let source = format!(
        r#"<a href="{BLOB}/{}">This page's source</a>"#,
        document.display()
    );
    let description = description.unwrap_or_else(|| {
        format!(
            "{title}, of the documentation of Lokyr, a fast, open-source terminal emulator for macOS."
        )
    });
    template(
        &Head {
            title: &title,
            description: Some(&shortened(&description)),
            canonical: Some(&canonical(&page_path(document))),
            indexed: true,
            section: Section::of(document),
            structured: Some(&breadcrumbs(&crumbs(document, &title, titles))),
        },
        &"../".repeat(depth),
        settings,
        &contents(&body, &headings),
        &source,
        "",
    )
}

/// The least sections of a page that has a table of contents.
const TOC_LEAST: usize = 4;

/// A document's page's contents: its `main`, of its `body`, and, of
/// `TOC_LEAST` sections or more, its table of contents of its `headings`:
/// beside the column on a wide screen, each section a group that opens on
/// its parts, and a short one of the sections alone, which opens, under
/// the title on a narrow one. The search indexes the `main` alone.
fn contents(body: &str, headings: &[Heading]) -> String {
    let sections = headings.iter().filter(|heading| heading.level == 2).count();
    if sections < TOC_LEAST {
        return format!(
            "<div class=\"wrap page\">\n<main class=\"doc\" id=\"main\" data-pagefind-body>\n{body}</main>\n</div>"
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
        "<div class=\"wrap page has-toc\">\n{side}\n<main class=\"doc\" id=\"main\" data-pagefind-body>\n{body}</main>\n</div>"
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
    // The site's name after the page's title, unless the title holds it.
    let title = if head.title.contains("Lokyr") {
        head.title.to_owned()
    } else {
        format!("{} — Lokyr", head.title)
    };
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
        | Options::ENABLE_TASKLISTS;
    let events: Vec<Event> = Parser::new_ext(text, options).collect();
    // Each heading's text, of its anchor, a repeated one numbered as GitHub
    // numbers it.
    let mut ids = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut title = None;
    let mut heading: Option<(HeadingLevel, String, Words)> = None;
    let mut headings = Vec::new();
    // The description: the paragraph read, and how deep the events are in
    // lists, quotes, tables and footnotes, whose paragraphs it passes over.
    let mut description = None;
    let mut paragraph: Option<String> = None;
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
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(words) = paragraph.take() {
                    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
                    if words.chars().count() >= DESCRIPTION_LEAST {
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
                attrs,
            }) => {
                let id = ids.next();
                if matches!(level, HeadingLevel::H2 | HeadingLevel::H3) {
                    anchor.clone_from(&id);
                }
                rendered.push(Event::Start(Tag::Heading {
                    level,
                    id: id.map(CowStr::from),
                    classes,
                    attrs,
                }));
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
    let fallback = if document.parent() == Some(Path::new("docs/releases")) {
        format!("Lokyr {stem} release notes")
    } else {
        stem
    };
    let description = description.or(first_item.filter(|words| !words.is_empty()));
    Rendered {
        body,
        title: title.unwrap_or(fallback),
        description,
        headings,
    }
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
            "# X\n\n<sub>Platforms: macOS</sub>\n\n- a list's item that is long enough to be taken, yet passed over\n\n> A quote that is long enough to be taken, yet passed over too.\n\nThe guide to `x`, of [every key](y.md)\nthat the file holds, and of its values.\n\nA later paragraph that the description does not take.\n",
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
            Path::new("docs/plugins/README.md"),
            "# The bundled plugins\n\nEvery feature that you see comes from a plugin, and these come with Lokyr.\n",
            &settings,
            &HashMap::new(),
        );
        assert!(shown.contains("<title>The bundled plugins — Lokyr</title>"));
        assert!(shown.contains(r#"<meta name="description" content="Every feature that you see comes from a plugin, and these come with Lokyr.">"#));
        assert!(shown.contains(r#"<link rel="canonical" href="https://lokyr.dev/docs/plugins/">"#));
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
            Path::new("docs/README.md"),
            "# Lokyr — the documentation\n",
            &settings,
            &HashMap::new(),
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
        let shown = contents(&rendered.body, &rendered.headings);
        assert!(shown.starts_with("<div class=\"wrap page has-toc\">\n<nav class=\"toc\""));
        assert!(shown.contains(r##"<li><details><summary><a href="#font"><code>[font]</code></a></summary><ul><li><a href="#family"><code>family</code></a></li><li><a href="#bold-style-italic-style"><code>bold-style</code>, <code>italic-style</code></a></li></ul></details></li>"##), "{shown}");
        assert!(shown.contains(r##"<li><a href="#text"><code>[text]</code></a></li>"##));
        // The short one, of the sections alone, right after the title.
        assert!(shown.contains("</h1>\n<details class=\"toc-inline\">\n<summary>On this page</summary>\n<ul>\n<li><a href=\"#the-file\">The file</a></li>"));
        let inline = &shown[shown.find("toc-inline").unwrap()..];
        assert!(!inline[..inline.find("</details>").unwrap()].contains("#family\""));
        // Three sections have none.
        let short = html(Path::new("docs/y.md"), "# Y\n\n## A\n\n## B\n\n## C\n");
        assert!(!contents(&short.body, &short.headings).contains("toc"));
    }

    #[test]
    fn a_page_s_headings_take_github_s_anchors_and_its_first_its_title() {
        let Rendered { body, title, .. } = html(
            Path::new("docs/x.md"),
            "# The `plugin.toml` guide\n\n## Limits\n\n## Limits\n\nSee [the guide](y.md#a).\n",
        );
        assert_eq!(title, "The plugin.toml guide");
        assert!(body.contains(r#"<h1 id="the-plugintoml-guide">"#), "{body}");
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
        // The sitemap lists the home and every document's page, each of its
        // last commit's date; the robots name it.
        let sitemap = fs::read_to_string(out.join("sitemap.xml")).unwrap();
        assert_eq!(
            sitemap.matches("<url>").count(),
            documents.len() + 1,
            "{sitemap}"
        );
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
        let shown = page(Path::new("docs/x.md"), "# X\n", &settings, &HashMap::new());
        assert!(shown.contains(r#"<a href="https://bsky.app/profile/lokyr.dev">Bluesky</a>"#));
        assert!(!shown.contains(">X</a>"));
        let _ = fs::remove_dir_all(out);
    }
}
