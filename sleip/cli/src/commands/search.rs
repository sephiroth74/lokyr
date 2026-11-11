//! `sleip search` and `sleip info`: the registry's plugins, of its index,
//! fetched, checked against the root keys and cached, or of the cache
//! alone with `--offline`. `search` reads the list of the plugins, a
//! plugin that every word finds in its name or its description, every one
//! of no word, in a table; `info` reads a plugin's index file, its versions, their
//! plugin API, capabilities and checksums. Both mark a version that this
//! Lokyr does not take, of its plugin API and this Lokyr's. The registry is
//! `plugins.toml`'s `default`, or Lokyr's.

use lokyr_pkg_model::list::List;
use semver::Version;
use serde_json::json;
use sleip_index::client::Client;
use sleip_index::remote::Remote;

use crate::args::Options;
use crate::output::{Out, Span, one_line};
use crate::paths::Paths;

/// The site of Lokyr's registry, which searches its plugins too.
const SITE: &str = "https://pkg.lokyr.dev";

/// This Lokyr's plugin API, as Sleip finds a Lokyr; `None` of none.
pub(crate) fn this_api() -> Option<Version> {
    crate::paths::app_api().and_then(|(api, _)| Version::parse(&api).ok())
}

/// Whether this Lokyr, of the plugin API `this`, takes a version of the
/// plugin API `api`, a constraint: `None` where either is not known, as a
/// version that names none, which every Lokyr takes.
pub(crate) fn taken(api: Option<&str>, this: Option<&Version>) -> Option<bool> {
    let (api, this) = (api?, this?);
    sleip_resolver::ranges_of(api)
        .ok()
        .map(|ranges| ranges.contains(this))
}

/// Why this Lokyr, of the plugin API `this`, does not take a version of the
/// plugin API `api`: it needs a newer Lokyr, or it is made for an older
/// one.
pub(crate) fn untaken(api: &str, this: &Version) -> String {
    let newer = sleip_resolver::ranges_of(api)
        .ok()
        .and_then(|ranges| {
            ranges.bounding_range().map(|(start, _)| match start {
                std::ops::Bound::Included(least) | std::ops::Bound::Excluded(least) => least > this,
                std::ops::Bound::Unbounded => false,
            })
        })
        .unwrap_or(false);
    let api = one_line(api);
    if newer {
        format!("needs a newer Lokyr: the plugin API {api}, this one's {this}")
    } else {
        format!("made for an older Lokyr: the plugin API {api}, this one's {this}")
    }
}

/// The registry of `plugins.toml` of `paths`, its `default`.
fn registry_url(paths: &Paths) -> Result<String, String> {
    let list = List::read(&paths.list()).map_err(|error| error.to_string())?;
    list.registry("default")
        .map(str::to_owned)
        .ok_or_else(|| "plugins.toml names no default registry".to_owned())
}

/// Searches the registry's plugins of `words`, as `options` and `out`
/// say.
pub fn search(paths: &Paths, options: Options, out: Out, words: &[String]) -> Result<(), String> {
    let url = registry_url(paths)?;
    let client = (!options.offline).then(Client::new);
    let remote = Remote::new(&url, &paths.cache, client.as_ref(), super::roots());
    let keys = remote.keys()?;
    let list = remote.list(&keys)?;
    tell_notes(out, &remote);
    let words: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();
    let this = this_api();
    let mut rows = Vec::new();
    for plugin in &list.plugins {
        let text = format!(
            "{} {}",
            plugin.name,
            plugin.description.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        if !words.iter().all(|word| text.contains(word.as_str())) {
            continue;
        }
        let description = plugin.description.as_deref().unwrap_or_default();
        let taken = taken(plugin.api.as_deref(), this.as_ref());
        if out.json() {
            out.tell(
                "found",
                json!({
                    "name": plugin.name,
                    "version": plugin.vers,
                    "description": description,
                    "capabilities": plugin.caps,
                    "yanked": plugin.yanked,
                    "api": plugin.api,
                    "taken": taken,
                }),
                "",
            );
        }
        let yanked = if plugin.yanked { " (yanked)" } else { "" };
        rows.push(Row {
            name: one_line(&plugin.name),
            untaken: taken == Some(false),
            version: format!("{}{yanked}", one_line(&plugin.vers)),
            description: one_line(description),
        });
    }
    if !out.json() && !rows.is_empty() {
        let width = terminal_size::terminal_size_of(std::io::stdout())
            .map(|(terminal_size::Width(width), _)| usize::from(width));
        table(out, &rows, width);
        if let (true, Some(this)) = (rows.iter().any(|row| row.untaken), &this) {
            out.tell(
                "",
                json!({}),
                &format!(
                    "\n* This Lokyr, of the plugin API {this}, does not take its version shown: \
                     `sleip info <plugin>` tells which it needs."
                ),
            );
        }
    }
    let found = rows.len();
    let more = if url.trim_end_matches('/') == SITE {
        format!("; {SITE} searches their pages too")
    } else {
        String::new()
    };
    out.tell(
        "done",
        json!({ "found": found }),
        &format!("{found} plugins found{more}."),
    );
    Ok(())
}

/// A plugin of a search's table: its name, whether this Lokyr does not take
/// its version shown, its version and its description, each of one line.
struct Row {
    name: String,
    untaken: bool,
    version: String,
    description: String,
}

/// The least columns of a description, below which it goes as it is,
/// rather than wrapped in a column too narrow to read.
const LEAST_DESCRIPTION: usize = 24;

/// Tells `rows` as a table, of a header: each plugin's name bold, an
/// asterisk beside it where this Lokyr does not take its version shown,
/// its version and its description in columns, a description wrapped
/// within the terminal's `width`, where it is known, under its column.
fn table(out: Out, rows: &[Row], width: Option<usize>) {
    use unicode_width::UnicodeWidthStr as _;
    const HEADER: [&str; 3] = ["Plugin", "Version", "Description"];
    let name_width = rows
        .iter()
        .map(|row| row.name.width() + usize::from(row.untaken))
        .chain([HEADER[0].len()])
        .max()
        .unwrap_or(0);
    let version_width = rows
        .iter()
        .map(|row| row.version.width())
        .chain([HEADER[1].len()])
        .max()
        .unwrap_or(0);
    let left = name_width + 2 + version_width + 2;
    let room = width
        .and_then(|width| width.checked_sub(left))
        .filter(|room| *room >= LEAST_DESCRIPTION);
    let header = format!(
        "{:<name_width$}  {:<version_width$}  {}",
        HEADER[0], HEADER[1], HEADER[2]
    );
    out.tell_spans("", json!({}), &[Span::Bold(header.trim_end())]);
    for row in rows {
        let marked = row.name.width() + usize::from(row.untaken);
        let lines = match room {
            Some(room) => wrapped(&row.description, room),
            None => vec![row.description.clone()],
        };
        let mut rest = format!(
            "{}{}  {}{}  {}",
            if row.untaken { "*" } else { "" },
            " ".repeat(name_width - marked),
            row.version,
            " ".repeat(version_width - row.version.width()),
            lines.first().map_or("", String::as_str)
        );
        for line in lines.iter().skip(1) {
            rest.push('\n');
            rest.push_str(&" ".repeat(left));
            rest.push_str(line);
        }
        let rest = rest.trim_end().to_owned();
        out.tell_spans("", json!({}), &[Span::Bold(&row.name), Span::Plain(&rest)]);
    }
}

/// `text` in lines of `width` columns at most, broken between its words,
/// a word longer than a line on a line of its own.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthStr as _;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let wanted = if line.is_empty() {
            word.width()
        } else {
            line.width() + 1 + word.width()
        };
        if wanted > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Tells the registry's plugin `plugin`, `owner/name`: its versions, newest
/// first, as `options` and `out` say.
pub fn info(paths: &Paths, options: Options, out: Out, plugin: &str) -> Result<(), String> {
    let url = registry_url(paths)?;
    let client = (!options.offline).then(Client::new);
    let remote = Remote::new(&url, &paths.cache, client.as_ref(), super::roots());
    let keys = remote.keys()?;
    let index = remote
        .index(plugin, &keys)?
        .ok_or_else(|| format!("{url} publishes no plugin `{plugin}`"))?;
    tell_notes(out, &remote);
    let Some(shown) = index
        .entries
        .iter()
        .rev()
        .find(|entry| !entry.yanked)
        .or_else(|| index.entries.last())
    else {
        return Err(format!("{url} publishes no version of `{plugin}` yet"));
    };
    let this = this_api();
    let versions: Vec<serde_json::Value> = index
        .entries
        .iter()
        .rev()
        .map(|entry| {
            json!({
                "version": entry.vers,
                "yanked": entry.yanked,
                "capabilities": entry.caps,
                "cksum": entry.cksum,
                "api": entry.api,
                "taken": taken(entry.api.as_deref(), this.as_ref()),
            })
        })
        .collect();
    // Each field of the index on a line of its own, as an index of another
    // registry's could hold a line's end in it.
    let mut text = format!("{plugin} {}", one_line(&shown.vers));
    if let Some(description) = &shown.description {
        text.push_str(&format!("\n{}", one_line(description)));
    }
    let list = |items: &[String]| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items
                .iter()
                .map(|item| one_line(item))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    text.push_str(&format!("\nCapabilities: {}", list(&shown.caps)));
    if !shown.uses.is_empty() {
        text.push_str(&format!("\nCommands it runs: {}", list(&shown.uses)));
    }
    if !shown.deps.is_empty() {
        let deps: Vec<String> = shown
            .deps
            .iter()
            .map(|dependency| {
                format!(
                    "{} {}",
                    one_line(&dependency.name),
                    one_line(&dependency.req)
                )
            })
            .collect();
        text.push_str(&format!("\nDepends on: {}", deps.join(", ")));
    }
    // Each version of its plugin API, marked where this Lokyr does not take
    // it; of none that it takes, a line that tells it.
    let takes_none = this.as_ref().is_some_and(|this| {
        index
            .entries
            .iter()
            .all(|entry| taken(entry.api.as_deref(), Some(this)) == Some(false))
    });
    if let (true, Some(this)) = (takes_none, &this) {
        text.push_str(&format!(
            "\nThis Lokyr, of the plugin API {this}, takes none of its versions."
        ));
    }
    text.push_str("\nVersions:");
    for entry in index.entries.iter().rev() {
        let yanked = if entry.yanked { " (yanked)" } else { "" };
        let api = entry
            .api
            .as_deref()
            .map_or_else(String::new, |api| format!("  plugin API {}", one_line(api)));
        let mark = match (
            taken(entry.api.as_deref(), this.as_ref()),
            &entry.api,
            &this,
        ) {
            (Some(false), Some(api), Some(this)) => format!(" ({})", untaken(api, this)),
            _ => String::new(),
        };
        text.push_str(&format!(
            "\n  {}{yanked}{api}  {}{mark}",
            one_line(&entry.vers),
            one_line(&entry.cksum)
        ));
    }
    if url.trim_end_matches('/') == SITE {
        text.push_str(&format!("\n{SITE}/plugins/{plugin}/"));
    }
    out.tell(
        "info",
        json!({
            "name": plugin,
            "version": shown.vers,
            "description": shown.description,
            "kind": shown.kind,
            "api": shown.api,
            "capabilities": shown.caps,
            "uses": shown.uses,
            "dependencies": shown.deps.iter().map(|dependency| json!({ "name": dependency.name, "req": dependency.req })).collect::<Vec<_>>(),
            "versions": versions,
        }),
        &text,
    );
    Ok(())
}

/// Tells what `remote` took that is worth telling, as an expired file of
/// the cache offline.
fn tell_notes(out: Out, remote: &Remote) {
    for note in remote.notes() {
        out.tell(
            "note",
            json!({ "message": note }),
            &format!("Note: {note}."),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_wraps_between_its_words_within_its_column() {
        assert_eq!(
            wrapped("An image of a terminal's text, to copy or to save", 20),
            ["An image of a", "terminal's text, to", "copy or to save"]
        );
        assert_eq!(
            wrapped("averyveryverylongword short", 8),
            ["averyveryverylongword", "short"]
        );
        assert_eq!(wrapped("", 10), [""]);
        // Of the columns that a wide character takes.
        assert_eq!(wrapped("日本語 のテキスト", 8), ["日本語", "のテキスト"]);
    }
}
