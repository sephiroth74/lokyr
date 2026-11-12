use super::*;

/// A folder of the tests, of `name`, made anew.
fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("lokyr-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Writes the files of `files`, by their paths in `folder`, in their order.
fn write(folder: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = folder.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn a_digest_reads_and_writes_as_sha256_of_its_hexadecimal_digits() {
    let digest = Digest::of_bytes(b"lokyr");
    let text = digest.to_string();
    assert!(text.starts_with("sha256:") && text.len() == 71, "{text}");
    assert_eq!(text.parse::<Digest>(), Ok(digest));
    assert_eq!(Digest::of_hex(&digest.hex()), Some(digest));
    for wrong in [
        "sha256:",
        "sha1:00",
        &text[..70],
        &text.to_uppercase().replace("SHA256", "sha256"),
        &format!("{text}0"),
    ] {
        assert!(wrong.parse::<Digest>().is_err(), "{wrong}");
    }
}

#[test]
fn a_package_s_digest_is_of_its_files_alone() {
    let files = [
        ("plugin.toml", "[plugin]\n"),
        ("src/init.luau", "return {}\n"),
        ("src/ui/view.luau", "return 1\n"),
    ];
    let one = folder("one");
    write(&one, &files);
    // The same files written in another order, of an empty folder more and
    // of other permissions, have the same digest.
    let two = folder("two");
    let mut reversed = files;
    reversed.reverse();
    write(&two, &reversed);
    std::fs::create_dir_all(two.join("assets/empty")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            two.join("src/init.luau"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    let digest = digest_of(&one).unwrap();
    assert_eq!(digest_of(&two).unwrap(), digest);
    // A byte, a name or a file more changes it.
    write(&two, &[("src/init.luau", "return {} \n")]);
    assert_ne!(digest_of(&two).unwrap(), digest);
    let three = folder("three");
    write(
        &three,
        &[
            ("plugin.toml", "[plugin]\n"),
            ("src/init.luau", "return {}\n"),
            ("src/ui/views.luau", "return 1\n"),
        ],
    );
    assert_ne!(digest_of(&three).unwrap(), digest);
    write(&three, &[("src/ui/view.luau", "return 1\n")]);
    assert_ne!(digest_of(&three).unwrap(), digest);
    // A file's bytes and the next one's name do not run together.
    let four = folder("four");
    write(&four, &[("a", "bc"), ("d", "")]);
    let five = folder("five");
    write(&five, &[("a", "b"), ("cd", "")]);
    assert_ne!(digest_of(&four).unwrap(), digest_of(&five).unwrap());
    // A link is no package's.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", one.join("src/secret.luau")).unwrap();
        assert!(digest_of(&one).is_err());
    }
    for name in ["one", "two", "three", "four", "five"] {
        let _ = std::fs::remove_dir_all(folder(name));
    }
}

#[test]
fn the_store_holds_its_packages_by_their_digests_and_checks_them() {
    let root = folder("root");
    let store = Store::new(&root);
    assert_eq!(store.digests().unwrap(), []);
    assert_eq!(Store::new(root.join("none")).digests().unwrap(), []);
    // A package in its folder, of its digest.
    let package = folder("package");
    write(
        &package,
        &[
            ("plugin.toml", "[plugin]\n"),
            ("src/init.luau", "return {}\n"),
        ],
    );
    let digest = digest_of(&package).unwrap();
    std::fs::rename(&package, store.folder(&digest)).unwrap();
    assert!(store.holds(&digest));
    assert_eq!(store.folder(&digest), root.join(digest.hex()));
    // An unpack under way, and a file, are no packages.
    std::fs::create_dir_all(root.join(".unpack-1-0")).unwrap();
    std::fs::write(root.join(Digest::of_bytes(b"x").hex()), "").unwrap();
    assert_eq!(store.digests().unwrap(), [digest]);
    store.check(&digest).unwrap();
    // A package changed since it was stored, as by hand, is refused.
    write(
        &store.folder(&digest),
        &[("src/init.luau", "return { evil = true }\n")],
    );
    let error = store.check(&digest).unwrap_err();
    assert!(
        error.to_string().contains("changed since it was stored"),
        "{error}"
    );
    // One that the store does not hold.
    assert!(store.check(&Digest::of_bytes(b"none")).is_err());
    store.remove(&digest).unwrap();
    assert!(!store.holds(&digest));
    // The user's store.
    let home = Path::new("/Users/me");
    let mine = Store::of_user(home, Some(Path::new("/data")));
    if cfg!(target_os = "macos") {
        assert_eq!(
            mine.root(),
            Path::new("/Users/me/Library/Application Support/lokyr/store")
        );
    } else {
        assert_eq!(mine.root(), Path::new("/data/lokyr/store"));
        assert_eq!(
            Store::of_user(home, Some(Path::new("relative"))).root(),
            Path::new("/Users/me/.local/share/lokyr/store")
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
