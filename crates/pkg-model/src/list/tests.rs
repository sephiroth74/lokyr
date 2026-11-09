use super::*;

/// The docs' example, of every kind of plugin.
const EXAMPLE: &str = r#"
[plugins]
"sephiroth74/search-plus" = "^0.1"
"acme/nord-themes" = "1"
"me/my-plugin" = { path = "~/dev/my-plugin" }
"acme/git-status" = { git = "https://github.com/acme/git-status", tag = "v0.2.0" }
"acme/pinned" = { git = "git@github.com:acme/pinned.git", rev = "4f2a9c1" }

[registries]
default = "https://pkg.lokyr.dev"
mirror = "file:///tmp/registry"
"#;

#[test]
fn the_list_reads_each_kind_of_plugin_and_writes_back_the_same() {
    let list = List::parse(EXAMPLE).unwrap();
    assert_eq!(
        list.plugins["sephiroth74/search-plus"],
        Wanted::Registry("^0.1".to_owned())
    );
    assert_eq!(
        list.plugins["acme/nord-themes"],
        Wanted::Registry("1".to_owned())
    );
    assert_eq!(
        list.plugins["me/my-plugin"],
        Wanted::Path("~/dev/my-plugin".to_owned())
    );
    assert_eq!(
        list.plugins["acme/git-status"],
        Wanted::Git {
            url: "https://github.com/acme/git-status".to_owned(),
            at: GitAt::Tag("v0.2.0".to_owned()),
        }
    );
    assert_eq!(
        list.plugins["acme/pinned"],
        Wanted::Git {
            url: "git@github.com:acme/pinned.git".to_owned(),
            at: GitAt::Rev("4f2a9c1".to_owned()),
        }
    );
    assert_eq!(list.registry("mirror"), Some("file:///tmp/registry"));
    let text = list.text();
    assert_eq!(List::parse(&text).unwrap(), list, "{text}");
    assert!(
        text.starts_with("[plugins]\n\"acme/git-status\" = { git = "),
        "{text}"
    );
    // An empty file, and the default registry of none named.
    let empty = List::parse("").unwrap();
    assert_eq!(empty, List::default());
    assert_eq!(empty.registry("default"), Some(REGISTRY));
    assert_eq!(empty.registry("other"), None);
    assert_eq!(List::parse(&empty.text()).unwrap(), empty);
}

#[test]
fn a_wrong_list_is_refused_at_its_place() {
    let wrong = |text: &str| List::parse(text).unwrap_err().to_string();
    let plugin = |value: &str| format!("[plugins]\n\"acme/x\" = {value}\n");
    assert!(
        wrong("[plugins]\n\"Acme/x\" = \"1\"\n")
            .starts_with("2:1: `Acme/x` is not a plugin's name"),
        "{}",
        wrong("[plugins]\n\"Acme/x\" = \"1\"\n")
    );
    for (value, told) in [
        ("\"newest\"", "2:12: `newest` is not a version's constraint"),
        ("3", "2:12: a plugin is a version's constraint"),
        (
            "{ path = \"dev/x\" }",
            "2:12: `dev/x` is not a folder's path, absolute or from `~`",
        ),
        ("{ path = \"\" }", "2:12: `path` is a text, not empty"),
        (
            "{ git = \"https://x/y\" }",
            "2:12: a git repository's plugin is at a `tag` or a `rev`, one",
        ),
        (
            "{ git = \"https://x/y\", tag = \"v1\", rev = \"abcdef1\" }",
            "2:12: a git repository's plugin is at a `tag` or a `rev`, one",
        ),
        (
            "{ git = \"https://x/y\", rev = \"main\" }",
            "2:12: `main` is not a commit",
        ),
        (
            "{ git = \"http://x/y\", tag = \"v1\" }",
            "2:12: `http://x/y` is not a git repository's address",
        ),
        (
            "{ git = \"git://x/y\", tag = \"v1\" }",
            "2:12: `git://x/y` is not a git repository's address",
        ),
        (
            "{ path = \"/x\", git = \"https://x/y\" }",
            "2:12: a plugin is of a `path`, or of a `git` repository, one",
        ),
        (
            "{ path = \"/x\", branch = \"main\" }",
            "2:12: `branch` is not a key of a plugin",
        ),
    ] {
        let told_back = wrong(&plugin(value));
        assert!(told_back.starts_with(told), "{value}: {told_back}");
    }
    assert!(
        wrong("[registries]\nDefault = \"https://x\"\n")
            .starts_with("2:1: `Default` is not a registry's name")
    );
    assert!(
        wrong("[registries]\ndefault = \"http://x\"\n")
            .starts_with("2:11: `http://x` is not a registry's address")
    );
    assert!(
        wrong("[other]\n").contains("unknown field `other`"),
        "{}",
        wrong("[other]\n")
    );
    assert!(
        wrong("[plugins\n").starts_with("1:"),
        "{}",
        wrong("[plugins\n")
    );
}

#[test]
fn the_list_is_written_whole_and_read_back_of_its_file() {
    let folder = std::env::temp_dir().join(format!("lokyr-list-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE);
    // Of no file, an empty list.
    assert_eq!(List::read(&path).unwrap(), List::default());
    let list = List::parse(EXAMPLE).unwrap();
    list.write(&path).unwrap();
    assert_eq!(List::read(&path).unwrap(), list);
    // Written again, in place, of nothing left beside it.
    let mut fewer = list.clone();
    fewer.plugins.remove("acme/pinned");
    fewer.write(&path).unwrap();
    assert_eq!(List::read(&path).unwrap(), fewer);
    let left: Vec<_> = std::fs::read_dir(&folder).unwrap().collect();
    assert_eq!(left.len(), 1);
    // A file that does not read tells its path and its place.
    std::fs::write(&path, "[plugins]\n\"acme/x\" = \"newest\"\n").unwrap();
    let error = List::read(&path).unwrap_err();
    assert_eq!(error.at, Some((2, 12)));
    assert!(
        error.message.starts_with(&path.display().to_string()),
        "{error}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
