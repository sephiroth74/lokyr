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

#[test]
fn a_linked_file_stays_a_link_its_target_written_of_its_mode() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("lokyr-permissions-linked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dotfiles = root.join("dotfiles");
    let config = root.join("config");
    std::fs::create_dir_all(&dotfiles).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    // GNU Stow's link, of a file of its own mode.
    let target = dotfiles.join(FILE);
    std::fs::write(&target, "").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = config.join(FILE);
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut permissions = Permissions::default();
    permissions
        .plugins
        .entry("acme/x".to_owned())
        .or_default()
        .decide(&[Capability::Open], true);
    permissions.write(&link).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(Permissions::read(&target).unwrap(), permissions);
    let mode = std::fs::metadata(&target).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    // Nothing of the write left beside either.
    assert_eq!(std::fs::read_dir(&dotfiles).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(&config).unwrap().count(), 1);
    // A link to a file not there yet makes the file, the link kept.
    std::fs::remove_file(&target).unwrap();
    permissions.write(&link).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(Permissions::read(&target).unwrap(), permissions);
    let _ = std::fs::remove_dir_all(&root);
}
