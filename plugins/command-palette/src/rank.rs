//! The ranking of the palette's commands for what the user types: a fuzzy
//! match of the query's characters in their order, by each command's title
//! first, then its name, its keys and its plugin, its words' starts and its
//! runs of characters scoring more, its gaps less; the recent commands
//! before the others, and first of all where nothing is typed.

/// A command of the palette's list, as the ranking reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Entry {
    /// Its name, `namespace.action`.
    pub(crate) name: String,
    /// Its title, as the menus show it.
    pub(crate) title: String,
    /// Its keys, as the platform writes them, if it has some.
    pub(crate) keys: Option<String>,
    /// The plugin that registered it, if one did.
    pub(crate) plugin: Option<String>,
}

/// What a character of the query matched scores.
const MATCH: i32 = 16;
/// What it scores more at the text's start, at a word's, and at a capital
/// after a small letter.
const START: i32 = 16;
const WORD: i32 = 12;
const CAPITAL: i32 = 8;
/// What it scores more right after the character before's match.
const RUN: i32 = 12;
/// What a gap between two matched characters costs, at its start and for
/// each character more, and the characters before the first match, each,
/// as far as the most.
const GAP: i32 = 3;
const GAP_MORE: i32 = 1;
const LEADING: i32 = 1;
const MOST_LEADING: i32 = 8;
/// What a match of a command's name, keys or plugin, rather than its
/// title, costs.
const NAME: i32 = 8;
const KEYS: i32 = 4;
const PLUGIN: i32 = 12;
/// How many recent commands score more, the latest the most, a point less
/// each.
const RECENT: usize = 10;

/// How well `query` matches `text`: every character of the query, but its
/// spaces, in their order, of either case; `None` where one is missing.
pub(crate) fn score(query: &str, text: &str) -> Option<i32> {
    let wanted: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if wanted.is_empty() {
        return Some(0);
    }
    let original: Vec<char> = text.chars().collect();
    let lower: Vec<char> = original
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    if wanted.len() > lower.len() {
        return None;
    }
    let bonus = |at: usize| -> i32 {
        if at == 0 {
            return START;
        }
        let (before, here) = (original[at - 1], original[at]);
        if !before.is_alphanumeric() {
            WORD
        } else if before.is_lowercase() && here.is_uppercase() {
            CAPITAL
        } else {
            0
        }
    };
    // The best score of the query's characters so far, the last matched
    // at each place of the text.
    let mut before: Vec<Option<i32>> = vec![None; lower.len()];
    for (index, &character) in wanted.iter().enumerate() {
        let mut here = vec![None; lower.len()];
        for at in 0..lower.len() {
            if lower[at] != character {
                continue;
            }
            let own = MATCH + bonus(at);
            here[at] = if index == 0 {
                let leading = i32::try_from(at)
                    .unwrap_or(i32::MAX)
                    .saturating_mul(LEADING);
                Some(own - leading.min(MOST_LEADING))
            } else {
                (0..at)
                    .filter_map(|last| {
                        let score = before[last]?;
                        let joined = if last + 1 == at {
                            RUN
                        } else {
                            let gap = i32::try_from(at - last - 2).unwrap_or(i32::MAX);
                            -GAP - gap.saturating_mul(GAP_MORE)
                        };
                        Some(score + own + joined)
                    })
                    .max()
            };
        }
        before = here;
    }
    before.into_iter().flatten().max()
}

/// How well `query` matches `entry`, by its title, its name, its keys or
/// its plugin, the best of them, the title's costing nothing.
fn entry_score(query: &str, entry: &Entry) -> Option<i32> {
    let of =
        |text: Option<&str>, cost: i32| text.and_then(|text| score(query, text)).map(|s| s - cost);
    [
        of(Some(&entry.title), 0),
        of(Some(&entry.name), NAME),
        of(entry.keys.as_deref(), KEYS),
        of(entry.plugin.as_deref(), PLUGIN),
    ]
    .into_iter()
    .flatten()
    .max()
}

/// The places of `entries` that match `query`, the best first, of the
/// recent commands' names `recent`, the latest first, scoring more; of no
/// query, the recent ones first, then the others in their order.
pub(crate) fn rank(entries: &[Entry], query: &str, recent: &[String]) -> Vec<usize> {
    let recency = |entry: &Entry| recent.iter().position(|name| *name == entry.name);
    if query.trim().is_empty() {
        let mut ranked: Vec<usize> = recent
            .iter()
            .filter_map(|name| entries.iter().position(|entry| entry.name == *name))
            .collect();
        ranked.extend((0..entries.len()).filter(|at| recency(&entries[*at]).is_none()));
        return ranked;
    }
    let mut scored: Vec<(i32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| {
            let score = entry_score(query, entry)?;
            let bonus = recency(entry).map_or(0, |place| RECENT.saturating_sub(place));
            Some((score + i32::try_from(bonus).unwrap_or(0), at))
        })
        .collect();
    scored.sort_by(|(one, at), (other, other_at)| other.cmp(one).then(at.cmp(other_at)));
    scored.into_iter().map(|(_, at)| at).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, title: &str, keys: Option<&str>, plugin: Option<&str>) -> Entry {
        Entry {
            name: name.to_owned(),
            title: title.to_owned(),
            keys: keys.map(str::to_owned),
            plugin: plugin.map(str::to_owned),
        }
    }

    fn commands() -> Vec<Entry> {
        vec![
            entry("window.new", "New Window", Some("⌘N"), None),
            entry("window.set-title", "Set Window Title…", None, None),
            entry("pane.copy", "Copy", Some("⌘C"), None),
            entry("pane.paste", "Paste", Some("⌘V"), None),
            entry("tabs.new", "New Tab", Some("⌘T"), Some("lokyr/tabs")),
            entry("tabs.close", "Close Tab", Some("⌘W"), Some("lokyr/tabs")),
            entry(
                "splits.split",
                "Split Right",
                Some("⌘D"),
                Some("lokyr/splits"),
            ),
            entry("search.open", "Find…", Some("⌘F"), Some("lokyr/search")),
        ]
    }

    /// The names of the commands that `query` ranks, of `recent`.
    fn ranked(query: &str, recent: &[&str]) -> Vec<String> {
        let entries = commands();
        let recent: Vec<String> = recent.iter().map(|name| (*name).to_owned()).collect();
        rank(&entries, query, &recent)
            .into_iter()
            .map(|at| entries[at].name.clone())
            .collect()
    }

    #[test]
    fn a_query_matches_its_characters_in_their_order_of_either_case() {
        assert!(score("nt", "New Tab").is_some());
        assert!(score("NT", "new tab").is_some());
        assert_eq!(score("tn", "New Tab"), None);
        assert_eq!(score("x", "New Tab"), None);
        assert_eq!(score("new tabs", "New Tab"), None);
        assert_eq!(score("", "New Tab"), Some(0));
        // Words' starts and runs score more than letters within words.
        assert!(score("nt", "New Tab") > score("nt", "Window Title"));
        assert!(score("tab", "New Tab") > score("tab", "Set Window Title…"));
        assert!(score("cop", "Copy") > score("cop", "Close Tab Copy"));
    }

    #[test]
    fn the_commands_rank_by_their_titles_first_then_their_names_keys_and_plugins() {
        assert_eq!(ranked("tab", &[])[..2], ["tabs.new", "tabs.close"]);
        assert_eq!(ranked("copy", &[]), ["pane.copy"]);
        assert_eq!(ranked("nw", &[])[0], "window.new");
        assert_eq!(ranked("split", &[])[0], "splits.split");
        // By its name, its keys or its plugin, where its title has none.
        assert_eq!(ranked("set-title", &[]), ["window.set-title"]);
        assert_eq!(ranked("⌘F", &[]), ["search.open"]);
        assert_eq!(ranked("search", &[]), ["search.open"]);
        assert!(ranked("zzz", &[]).is_empty());
    }

    #[test]
    fn the_recent_commands_come_first_and_score_more() {
        // Of no query, the recent ones first, the latest first, then the
        // others in their order.
        assert_eq!(
            ranked("", &["pane.paste", "tabs.close"]),
            [
                "pane.paste",
                "tabs.close",
                "window.new",
                "window.set-title",
                "pane.copy",
                "tabs.new",
                "splits.split",
                "search.open"
            ]
        );
        // A recent one of the query before another of the same score.
        assert_eq!(ranked("new", &[])[0], "window.new");
        assert_eq!(ranked("new", &["tabs.new"])[0], "tabs.new");
        // A recent command that the registry no longer has, left out.
        assert_eq!(ranked("", &["gone.away"])[0], "window.new");
    }
}
