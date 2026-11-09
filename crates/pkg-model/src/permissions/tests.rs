use super::*;

#[test]
fn the_decisions_read_and_write_back_the_same() {
    let text = r#"
[plugins."acme/git-status"]
granted = ["process-read", "open", "open"]
refused = ["clipboard-read"]

[plugins."acme/fetch"]
granted = ["net:api.github.com"]
"#;
    let permissions = Permissions::parse(text).unwrap();
    let status = &permissions.plugins["acme/git-status"];
    assert_eq!(status.granted, [Capability::Open, Capability::ProcessRead]);
    assert_eq!(status.refused, [Capability::ClipboardRead]);
    assert_eq!(
        permissions.plugins["acme/fetch"].granted,
        [Capability::Net("api.github.com".to_owned())]
    );
    assert!(permissions.plugins["acme/fetch"].refused.is_empty());
    let written = permissions.text();
    assert!(written.starts_with("# The capabilities"), "{written}");
    assert_eq!(
        Permissions::parse(&written).unwrap(),
        permissions,
        "{written}"
    );
    assert_eq!(Permissions::parse("").unwrap(), Permissions::default());
}

#[test]
fn a_decision_moves_its_capabilities_to_its_list_alone() {
    let mut decisions = Decisions::default();
    decisions.decide(&[Capability::Open, Capability::ClipboardRead], true);
    decisions.decide(&[Capability::ClipboardRead, Capability::PtyWrite], false);
    assert_eq!(decisions.granted, [Capability::Open]);
    assert_eq!(
        decisions.refused,
        [Capability::PtyWrite, Capability::ClipboardRead]
    );
    decisions.decide(&[Capability::PtyWrite], true);
    assert_eq!(decisions.granted, [Capability::PtyWrite, Capability::Open]);
    assert_eq!(decisions.refused, [Capability::ClipboardRead]);
}

#[test]
fn wrong_decisions_are_refused_at_their_place() {
    let wrong = |text: &str| Permissions::parse(text).unwrap_err().to_string();
    for (text, told) in [
        (
            "[plugins.\"Acme/x\"]\n",
            "1:10: `Acme/x` is not a plugin's name",
        ),
        (
            "[plugins.\"acme/x\"]\ngranted = [\"opens\"]\n",
            "2:12: `opens` is no capability",
        ),
        (
            "[plugins.\"acme/x\"]\ngranted = [\"open\"]\nrefused = [\"open\"]\n",
            "3:12: `open` is granted and refused",
        ),
        (
            "[plugins.\"acme/x\"]\ngranted = [\"net:\"]\n",
            "2:12: `net:` is no capability",
        ),
        (
            "[plugins.\"acme/x\"]\nallowed = []\n",
            "unknown field `allowed`",
        ),
        ("[other]\n", "unknown field `other`"),
    ] {
        let told_back = wrong(text);
        assert!(told_back.contains(told), "{text}: {told_back}");
    }
}

#[test]
fn the_decisions_are_written_whole_and_read_back_of_their_file() {
    let folder = std::env::temp_dir().join(format!("lokyr-permissions-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(FILE);
    assert_eq!(Permissions::read(&path).unwrap(), Permissions::default());
    let mut permissions = Permissions::default();
    permissions
        .plugins
        .entry("acme/x".to_owned())
        .or_default()
        .decide(&[Capability::Open], true);
    permissions.write(&path).unwrap();
    assert_eq!(Permissions::read(&path).unwrap(), permissions);
    std::fs::write(&path, "[plugins.\"acme/x\"]\ngranted = [\"opens\"]\n").unwrap();
    let error = Permissions::read(&path).unwrap_err();
    assert_eq!(error.at, Some((2, 12)));
    let _ = std::fs::remove_dir_all(&folder);
}
