//! Find in All Tabs: a search of every pane of the windows of the session
//! that shows, each match a row of the palette's list, of its line and of
//! its tab's title, in the windows', the tabs' and the panes' order, then
//! the rows'; [`MOST_FOUND`] at most.

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::host::{PaneId, SearchMatch, TabId, WindowId};
use lokyr_plugin_sdk::plugin::Cx;

use crate::rank::Entry;

/// The most matches that the list holds, past which the searches' others
/// are left out.
pub(crate) const MOST_FOUND: usize = 1000;

/// The most characters of a line that its row shows, past which it ends
/// in an ellipsis.
const MOST_LINE: usize = 200;

/// The rows of a line's logical line that its read takes before its row
/// and after it, as a long line wraps.
const REACH: usize = 4;

/// A pane that the search goes through: its window, its tab and its tab's
/// title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) window: WindowId,
    pub(crate) tab: TabId,
    pub(crate) pane: PaneId,
    pub(crate) title: String,
}

/// A match: the place of its pane among the places, its cells, and its
/// line's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) place: usize,
    pub(crate) found: SearchMatch,
    pub(crate) line: String,
}

impl Found {
    /// Where it goes in the list: its pane's place, then its first cell's
    /// row and column.
    fn order(&self) -> (usize, i64, u16) {
        (self.place, self.found.first.1, self.found.first.0)
    }
}

/// The panes of the windows of the session that shows, in the windows',
/// the tabs' and the panes' order, each of its tab's title.
pub(crate) fn places(cx: &mut Cx<'_>) -> Result<Vec<Place>, Error> {
    let shown: Option<Vec<WindowId>> = cx
        .sessions()?
        .into_iter()
        .find(|session| session.active)
        .map(|session| session.windows);
    let mut places = Vec::new();
    for window in cx.windows()? {
        if shown
            .as_ref()
            .is_some_and(|shown| !shown.contains(&window.id))
        {
            continue;
        }
        for tab in cx.tabs(window.id)? {
            for &pane in &tab.panes {
                places.push(Place {
                    window: window.id,
                    tab: tab.id,
                    pane,
                    title: tab.title.clone(),
                });
            }
        }
    }
    Ok(places)
}

/// Takes `matches`, of the pane at `place`, into `found`, in its order, of
/// each one's line as the pane holds it, up to [`MOST_FOUND`] in all.
pub(crate) fn take(
    cx: &mut Cx<'_>,
    found: &mut Vec<Found>,
    place: (usize, PaneId),
    matches: &[SearchMatch],
) -> Result<(), Error> {
    for &each in matches {
        if found.len() >= MOST_FOUND {
            break;
        }
        let line = cx
            .line_text(place.1, each.first.1, REACH)?
            .map(|line| shortened(line.text.trim()))
            .unwrap_or_default();
        let new = Found {
            place: place.0,
            found: each,
            line,
        };
        let at = found.partition_point(|kept| kept.order() < new.order());
        found.insert(at, new);
    }
    Ok(())
}

/// The palette's entries of `found`, each named of its place in the list,
/// of its line, and of its tab's title, of `places`, at its row's end.
pub(crate) fn entries(found: &[Found], places: &[Place]) -> Vec<Entry> {
    found
        .iter()
        .enumerate()
        .map(|(at, each)| Entry {
            name: at.to_string(),
            title: each.line.clone(),
            keys: places.get(each.place).map(|place| place.title.clone()),
            ..Entry::default()
        })
        .collect()
}

/// `text`, of [`MOST_LINE`] characters at most, past which it ends in an
/// ellipsis.
fn shortened(text: &str) -> String {
    let mut chars = text.chars();
    let mut short: String = chars.by_ref().take(MOST_LINE).collect();
    if chars.next().is_some() {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_line_ends_in_an_ellipsis() {
        assert_eq!(shortened("short"), "short");
        let long = "x".repeat(MOST_LINE + 5);
        let short = shortened(&long);
        assert_eq!(short.chars().count(), MOST_LINE + 1);
        assert!(short.ends_with('…'));
    }
}
