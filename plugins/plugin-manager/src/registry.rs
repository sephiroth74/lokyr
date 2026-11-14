//! The registry's plugins as Sleip's lines of `--json` tell them: each
//! plugin found of a search, a plugin's versions of `info`, and the order
//! of two versions, of Semantic Versioning's.

use std::cmp::Ordering;

use lokyr_plugin_sdk::message::Data;

/// A plugin that the registry has, as `search` finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Its name, `owner/name`.
    pub name: String,
    /// The version that the registry shows of it, its newest.
    pub version: String,
    /// What it does, if it tells.
    pub description: String,
    /// The capabilities that it asks for, as its manifest writes them.
    pub capabilities: Vec<String>,
    /// Whether this Lokyr takes the version shown; `None` as Sleip found
    /// no Lokyr.
    pub taken: Option<bool>,
    /// Whether the version shown was yanked, which no install takes.
    pub yanked: bool,
}

/// A plugin's versions and what each asks for, as `info` tells them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// Its name.
    pub name: String,
    /// What it does.
    pub description: String,
    /// The commands of other plugins that it runs.
    pub uses: Vec<String>,
    /// Its versions, newest first.
    pub versions: Vec<Version>,
}

/// A version of a plugin of the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    /// Its number.
    pub version: String,
    /// Whether it was yanked, which no new install takes.
    pub yanked: bool,
    /// The capabilities that it asks for.
    pub capabilities: Vec<String>,
    /// The plugin API that it takes, as a constraint.
    pub api: String,
    /// Whether this Lokyr takes it; `None` as Sleip found no Lokyr.
    pub taken: Option<bool>,
}

/// The event of Sleip's `line`, if it is one of an `event`.
pub fn event(line: &Data) -> Option<&str> {
    text(line, "event")
}

/// The plugin found of a `found` line.
pub fn found(line: &Data) -> Option<Found> {
    (event(line)? == "found").then(|| Found {
        name: owned(line, "name"),
        version: owned(line, "version"),
        description: owned(line, "description"),
        capabilities: texts(line, "capabilities"),
        taken: boolean(line, "taken"),
        yanked: boolean(line, "yanked").unwrap_or(false),
    })
}

/// The installed plugin and where it came from, of a `plugin` line of
/// `list`, as `registry+<address>` or `path+<path>`.
pub fn listed(line: &Data) -> Option<(String, String)> {
    (event(line)? == "plugin").then(|| (owned(line, "name"), owned(line, "source")))
}

/// What a line of a change tells of the lockfile: a plugin installed, of
/// its version; one removed, of none.
pub fn changed(line: &Data) -> Option<(String, Option<String>)> {
    match event(line)? {
        "installed" => Some((owned(line, "name"), Some(owned(line, "version")))),
        "removed" => Some((owned(line, "name"), None)),
        _ => None,
    }
}

/// Whether the lockfile changed, of the `done` line of `add` or `update`;
/// `None` of another line.
pub fn done_changed(line: &Data) -> Option<bool> {
    (event(line)? == "done").then(|| boolean(line, "changed").unwrap_or(true))
}

/// The plugin's versions of an `info` line.
pub fn info(line: &Data) -> Option<Info> {
    if event(line)? != "info" {
        return None;
    }
    let versions = match field(line, "versions") {
        Some(Data::List(versions)) => versions
            .iter()
            .map(|version| Version {
                version: owned(version, "version"),
                yanked: boolean(version, "yanked").unwrap_or(false),
                capabilities: texts(version, "capabilities"),
                api: owned(version, "api"),
                taken: boolean(version, "taken"),
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(Info {
        name: owned(line, "name"),
        description: owned(line, "description"),
        uses: texts(line, "uses"),
        versions,
    })
}

/// The message of a line of `event`, as an `error` or a `note`.
pub fn message_of(line: &Data, of: &str) -> Option<String> {
    (event(line)? == of).then(|| owned(line, "message"))
}

/// The order of the versions `a` and `b`, of Semantic Versioning's: by
/// their numbers, a pre-release before its release, a build ignored; a
/// number that does not read as one orders as 0.
pub fn compare(a: &str, b: &str) -> Ordering {
    let parts = |version: &str| {
        let version = version.split('+').next().unwrap_or_default();
        let (numbers, pre) = match version.split_once('-') {
            Some((numbers, pre)) => (numbers.to_owned(), Some(pre.to_owned())),
            None => (version.to_owned(), None),
        };
        let numbers: Vec<u64> = numbers
            .split('.')
            .map(|number| number.parse().unwrap_or(0))
            .collect();
        (numbers, pre)
    };
    let ((a_numbers, a_pre), (b_numbers, b_pre)) = (parts(a), parts(b));
    let width = a_numbers.len().max(b_numbers.len());
    let at = |numbers: &[u64], index: usize| numbers.get(index).copied().unwrap_or(0);
    (0..width)
        .map(|index| at(&a_numbers, index).cmp(&at(&b_numbers, index)))
        .find(|order| order.is_ne())
        .unwrap_or_else(|| match (a_pre, b_pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => compare_pre(&a, &b),
        })
}

/// The order of two pre-releases, of their identifiers: numbers by their
/// values and before words, words as texts, a shorter one first.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut a = a.split('.');
    let mut b = b.split('.');
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order.is_ne() {
                    return order;
                }
            }
        }
    }
}

/// The field `name` of `line`, if it is a map of one.
fn field<'d>(line: &'d Data, name: &str) -> Option<&'d Data> {
    match line {
        Data::Map(map) => map.get(name),
        _ => None,
    }
}

/// The text of the field `name` of `line`, if it is one.
fn text<'d>(line: &'d Data, name: &str) -> Option<&'d str> {
    match field(line, name)? {
        Data::Text(text) => Some(text),
        _ => None,
    }
}

/// The text of the field `name` of `line`, empty of none.
fn owned(line: &Data, name: &str) -> String {
    text(line, name).unwrap_or_default().to_owned()
}

/// The boolean of the field `name` of `line`, if it is one.
fn boolean(line: &Data, name: &str) -> Option<bool> {
    match field(line, name)? {
        Data::Boolean(value) => Some(*value),
        _ => None,
    }
}

/// The texts of the list of the field `name` of `line`, none of another.
fn texts(line: &Data, name: &str) -> Vec<String> {
    match field(line, name) {
        Some(Data::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                Data::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn map(fields: &[(&str, Data)]) -> Data {
        Data::Map(
            fields
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn text(value: &str) -> Data {
        Data::Text(value.to_owned())
    }

    #[test]
    fn versions_order_as_semantic_versioning_s() {
        let less = [
            ("0.1.0", "0.2.0"),
            ("0.9.0", "0.10.0"),
            ("1.0.0-rc.1", "1.0.0"),
            ("1.0.0-alpha", "1.0.0-alpha.1"),
            ("1.0.0-alpha.1", "1.0.0-beta"),
            ("1.0.0-2", "1.0.0-11"),
            ("1.0.0-11", "1.0.0-rc"),
            ("1.0", "1.0.1"),
        ];
        for (a, b) in less {
            assert_eq!(compare(a, b), Ordering::Less, "{a} {b}");
            assert_eq!(compare(b, a), Ordering::Greater, "{b} {a}");
        }
        assert_eq!(compare("1.2.3+build", "1.2.3"), Ordering::Equal);
        assert_eq!(compare("1.0", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn sleip_s_lines_read_as_found_plugins_and_versions() {
        let line = map(&[
            ("event", text("found")),
            ("name", text("acme/notes")),
            ("version", text("1.2.0")),
            ("description", text("Notes")),
            (
                "capabilities",
                Data::List(vec![text("clipboard-write"), text("title-text")]),
            ),
            ("yanked", Data::Boolean(false)),
            ("api", text("^0.1")),
            ("taken", Data::Nil),
        ]);
        assert_eq!(
            found(&line),
            Some(Found {
                name: "acme/notes".to_owned(),
                version: "1.2.0".to_owned(),
                description: "Notes".to_owned(),
                capabilities: vec!["clipboard-write".to_owned(), "title-text".to_owned()],
                taken: None,
                yanked: false,
            })
        );
        assert_eq!(info(&line), None);
        let version = |number: &str, yanked: bool| {
            map(&[
                ("version", text(number)),
                ("yanked", Data::Boolean(yanked)),
                ("capabilities", Data::List(Vec::new())),
                ("cksum", text("sha256:00")),
                ("api", text("^0.1")),
                ("taken", Data::Boolean(true)),
            ])
        };
        let line = map(&[
            ("event", text("info")),
            ("name", text("acme/notes")),
            ("description", text("Notes")),
            ("uses", Data::List(vec![text("tabs.*")])),
            (
                "versions",
                Data::List(vec![version("1.2.0", false), version("1.1.0", true)]),
            ),
        ]);
        let read = info(&line).unwrap();
        assert_eq!(read.uses, ["tabs.*"]);
        let versions: Vec<(&str, bool, Option<bool>)> = read
            .versions
            .iter()
            .map(|version| (version.version.as_str(), version.yanked, version.taken))
            .collect();
        assert_eq!(
            versions,
            [("1.2.0", false, Some(true)), ("1.1.0", true, Some(true))]
        );
        let error = map(&[("event", text("error")), ("message", text("offline"))]);
        assert_eq!(message_of(&error, "error").as_deref(), Some("offline"));
        assert_eq!(message_of(&error, "note"), None);
        assert_eq!(found(&Data::Nil), None);
        // The installed plugins' sources, and what a change did.
        let plugin = map(&[
            ("event", text("plugin")),
            ("name", text("acme/notes")),
            ("source", text("path+/Users/me/notes")),
        ]);
        assert_eq!(
            listed(&plugin),
            Some(("acme/notes".to_owned(), "path+/Users/me/notes".to_owned()))
        );
        let installed = map(&[
            ("event", text("installed")),
            ("name", text("acme/notes")),
            ("version", text("1.3.0")),
        ]);
        assert_eq!(
            changed(&installed),
            Some(("acme/notes".to_owned(), Some("1.3.0".to_owned())))
        );
        let removed = map(&[("event", text("removed")), ("name", text("acme/notes"))]);
        assert_eq!(changed(&removed), Some(("acme/notes".to_owned(), None)));
        let done = map(&[("event", text("done")), ("changed", Data::Boolean(false))]);
        assert_eq!(done_changed(&done), Some(false));
        assert_eq!(done_changed(&removed), None);
    }
}
