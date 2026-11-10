use std::path::PathBuf;

use super::*;

/// The manifest of a plugin of Luau of the tests, of the config's schema
/// `schema` if given.
fn manifest(schema: Option<&str>) -> String {
    let mut text = "[plugin]\nname = \"acme/hello\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n".to_owned();
    if let Some(schema) = schema {
        text.push_str(&format!("[contributes]\nconfig-schema = \"{schema}\"\n"));
    }
    text
}

/// A folder of the tests, of `name`, made anew.
fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("lokyr-archive-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Writes the files of `files`, by their paths in `folder`.
fn write(folder: &Path, files: &[(&str, &[u8])]) {
    for (path, bytes) in files {
        let path = folder.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

/// An archive of the entries of `entries`, each its path, its kind and
/// its bytes, as a tar of any tool's might hold them, compressed.
fn archive(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, kind, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(*kind);
        header.set_mode(0o777);
        match kind {
            tar::EntryType::Symlink | tar::EntryType::Link => {
                header.set_size(0);
                builder
                    .append_link(&mut header, path, std::str::from_utf8(bytes).unwrap())
                    .unwrap();
            }
            _ => {
                header.set_size(bytes.len() as u64);
                // A path as given, `..` too, which `append_data` refuses.
                header.as_mut_bytes()[..100].fill(0);
                header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
                header.set_cksum();
                builder.append(&header, *bytes).unwrap();
            }
        }
    }
    let tar = builder.into_inner().unwrap();
    ruzstd::encoding::compress_to_vec(&tar[..], ruzstd::encoding::CompressionLevel::Fastest)
}

/// The entries of a plugin of Luau, its manifest and its entry, and those
/// of `more`.
fn plugin<'a>(
    more: &[(&'a str, tar::EntryType, &'a [u8])],
) -> Vec<(&'a str, tar::EntryType, &'a [u8])> {
    let manifest: &'static str = Box::leak(manifest(None).into_boxed_str());
    let mut entries = vec![
        ("plugin.toml", tar::EntryType::Regular, manifest.as_bytes()),
        (
            "src/init.luau",
            tar::EntryType::Regular,
            b"return {}\n".as_slice(),
        ),
    ];
    entries.extend_from_slice(more);
    entries
}

/// The refusal of unpacking `archive`, within `bounds`, into a folder of
/// the tests, which is gone after it.
fn refusal(archive: &[u8], bounds: Bounds) -> String {
    static REFUSALS: AtomicUsize = AtomicUsize::new(0);
    let name = format!("refused-{}", REFUSALS.fetch_add(1, Ordering::Relaxed));
    let parent = folder(&name);
    let into = parent.join("package");
    let error = unpack_within(archive, &into, bounds).unwrap_err();
    assert!(!into.exists(), "the folder of a refused archive stays");
    let _ = std::fs::remove_dir_all(parent);
    match error {
        ArchiveError::Refused(message) => message,
        ArchiveError::Io(error) => panic!("{error}"),
    }
}

#[test]
fn a_package_packs_alike_and_unpacks_into_the_store_of_its_digest() {
    let package = folder("package");
    write(
        &package,
        &[
            (
                "plugin.toml",
                manifest(Some("config/schema.json")).as_bytes(),
            ),
            ("src/init.luau", b"return require(\"./ui/view\")\n"),
            ("src/ui/view.luau", b"return {}\n"),
            ("config/schema.json", b"{}"),
            ("locales/en/hello.ftl", b"command-hello-count = Count\n"),
            ("locales/it/hello.ftl", b"command-hello-count = Conta\n"),
            ("assets/icon.png", &[0x89, b'P', b'N', b'G']),
            ("README.md", b"# Hello\n"),
            ("LICENSE", b"MIT\n"),
            // The tools' files and the tests are none of the package's.
            (".git/HEAD", b"ref: refs/heads/main\n"),
            (".vscode/settings.json", b"{}"),
            (".luaurc", b"{}"),
            ("tests/hello.luau", b"return {}\n"),
            ("notes.txt", b"todo\n"),
        ],
    );
    let packed = pack(&package).unwrap();
    assert_eq!(pack(&package).unwrap(), packed, "a package packs alike");
    let root = folder("store");
    let store = Store::new(&root);
    let stored = store.add(packed.as_slice()).unwrap();
    assert!(stored.new, "the store held none of it");
    let unpacked = stored.unpacked;
    assert_eq!(unpacked.manifest.name, "acme/hello");
    assert!(store.holds(&unpacked.digest));
    store.check(&unpacked.digest).unwrap();
    let stored = store.folder(&unpacked.digest);
    assert!(stored.join("locales/it/hello.ftl").is_file());
    assert!(!stored.join(".git").exists() && !stored.join("tests").exists());
    assert!(!stored.join("notes.txt").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(stored.join("src/init.luau"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }
    // Added again, it stays as it was, of nothing left beside it, and it
    // is not new.
    let again = store.add(packed.as_slice()).unwrap();
    assert!(!again.new);
    assert_eq!(again.unpacked, unpacked);
    assert_eq!(store.digests().unwrap(), [unpacked.digest]);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    // A package of themes.
    let themes = folder("themes");
    write(
        &themes,
        &[
            (
                "plugin.toml",
                b"[plugin]\nname = \"acme/nord\"\nversion = \"1.0.0\"\nkind = \"themes\"\nlicense = \"MIT\"\n",
            ),
            ("themes/nord.toml", b"[theme]\n"),
            ("themes/nord-light.itermcolors", b"<plist/>"),
        ],
    );
    let unpacked = store
        .add(pack(&themes).unwrap().as_slice())
        .unwrap()
        .unpacked;
    assert!(
        store
            .folder(&unpacked.digest)
            .join("themes/nord.toml")
            .is_file()
    );
    for name in ["package", "store", "themes"] {
        let _ = std::fs::remove_dir_all(folder(name));
    }
}

#[test]
fn an_archive_of_a_link_or_of_a_path_out_of_the_package_is_refused() {
    use tar::EntryType::{Directory, Link, Regular, Symlink};
    let bounds = Bounds::default();
    for (entries, told) in [
        (
            plugin(&[("src/evil.luau", Symlink, b"/etc/passwd")]),
            "`src/evil.luau` is a link",
        ),
        (
            plugin(&[("src/evil.luau", Link, b"src/init.luau")]),
            "`src/evil.luau` is a link",
        ),
        (
            plugin(&[("src/../../evil.luau", Regular, b"x")]),
            "is not a path within the package",
        ),
        (
            plugin(&[("/etc/evil.luau", Regular, b"x")]),
            "is not a path within the package",
        ),
        (
            plugin(&[("src//evil.luau", Regular, b"x")]),
            "is not a path within the package",
        ),
        (
            plugin(&[("src\\evil.luau", Regular, b"x")]),
            "is not a path within the package",
        ),
        (
            plugin(&[("src/Init.luau", Regular, b"x")]),
            "`src/Init.luau` is twice in the archive",
        ),
        (
            plugin(&[("src/init.luau", Regular, b"x")]),
            "`src/init.luau` is twice in the archive",
        ),
        (
            plugin(&[("src/init.luau/x.luau", Regular, b"x")]),
            "is within a file of the archive",
        ),
        (
            plugin(&[("SRC/INIT.LUAU", Directory, b"")]),
            "is within a file of the archive",
        ),
        (
            plugin(&[("fifo", tar::EntryType::Fifo, b"")]),
            "`fifo` is a link, or another entry",
        ),
    ] {
        let told_back = refusal(&archive(&entries), bounds);
        assert!(told_back.contains(told), "{told}: {told_back}");
    }
    // A folder's entries, and `./` before the paths, as tools write them.
    let dotted = archive(&[
        ("./", Directory, b""),
        ("./plugin.toml", Regular, manifest(None).as_bytes()),
        ("./src/", Directory, b""),
        ("./src/init.luau", Regular, b"return {}\n"),
    ]);
    let into = folder("dotted").join("package");
    unpack(dotted.as_slice(), &into).unwrap();
    assert!(into.join("src/init.luau").is_file());
    let _ = std::fs::remove_dir_all(folder("dotted"));
}

#[test]
fn an_archive_past_its_sizes_or_of_a_bomb_is_refused() {
    use tar::EntryType::Regular;
    // Bytes that compress little, of a generator of the tests.
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let noise: Vec<u8> = (0..256 * 1024)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect();
    let noisy = archive(&plugin(&[("assets/noise.bin", Regular, &noise)]));
    let small = |change: fn(&mut Bounds)| {
        let mut bounds = Bounds::default();
        change(&mut bounds);
        bounds
    };
    assert!(
        refusal(&noisy, small(|bounds| bounds.compressed = 64 * 1024))
            .contains("the archive is more than 65536 bytes")
    );
    assert!(
        refusal(&noisy, small(|bounds| bounds.extracted = 128 * 1024))
            .contains("the archive holds more than 131072 bytes")
    );
    // A bomb: megabytes of zeros of a few hundred bytes compressed.
    let zeros = vec![0; 8 * 1024 * 1024];
    let bomb = archive(&plugin(&[("assets/zeros.bin", Regular, &zeros)]));
    assert!(bomb.len() < 64 * 1024, "{}", bomb.len());
    assert!(
        refusal(&bomb, Bounds::default()).contains("as a bomb would"),
        "{}",
        refusal(&bomb, Bounds::default())
    );
    // Too many files.
    let many = archive(&plugin(&[
        ("src/a.luau", Regular, b""),
        ("src/b.luau", Regular, b""),
        ("src/c.luau", Regular, b""),
    ]));
    assert!(
        refusal(&many, small(|bounds| bounds.files = 3))
            .contains("the archive holds more than 3 files")
    );
    // Not zstd.
    assert!(refusal(&noise, Bounds::default()).contains("the archive is not of zstd"));
    // The default bounds, as the rules say.
    assert_eq!(MOST_COMPRESSED, 20 * 1024 * 1024);
    assert_eq!(MOST_EXTRACTED, 100 * 1024 * 1024);
}

#[test]
fn a_package_out_of_its_layout_is_refused() {
    use tar::EntryType::Regular;
    let bounds = Bounds::default();
    let themes = b"[plugin]\nname = \"acme/nord\"\nversion = \"1.0.0\"\nkind = \"themes\"\nlicense = \"MIT\"\n";
    let builtin = b"[plugin]\nname = \"lokyr/x\"\nkind = \"builtin\"\nlicense = \"MIT\"\n";
    let schema = manifest(Some("schema.json"));
    for (entries, told) in [
        (
            vec![("plugin.toml", Regular, manifest(None).as_bytes())],
            "a plugin of Luau holds its entry, `src/init.luau`",
        ),
        (
            plugin(&[("notes.txt", Regular, b"x")]),
            "`notes.txt` has no place in a package of the kind `luau`",
        ),
        (
            plugin(&[("src/init.luac", Regular, b"\x1bLua")]),
            "`src/init.luac` has no place",
        ),
        (
            plugin(&[("locales/en/hello.txt", Regular, b"x")]),
            "`locales/en/hello.txt` has no place",
        ),
        (
            plugin(&[("locales/hello.ftl", Regular, b"x")]),
            "`locales/hello.ftl` has no place",
        ),
        (
            plugin(&[("themes/nord.toml", Regular, b"x")]),
            "`themes/nord.toml` has no place",
        ),
        (
            vec![
                ("plugin.toml", Regular, themes.as_slice()),
                ("src/init.luau", Regular, b"return {}".as_slice()),
            ],
            "`src/init.luau` has no place in a package of the kind `themes`",
        ),
        (
            vec![("plugin.toml", Regular, themes.as_slice())],
            "a package of themes holds its themes",
        ),
        (
            vec![
                ("plugin.toml", Regular, schema.as_bytes()),
                ("src/init.luau", Regular, b"return {}".as_slice()),
            ],
            "the manifest's `config-schema`, `schema.json`, is not in the package",
        ),
        (
            vec![("plugin.toml", Regular, builtin.as_slice())],
            "a built-in plugin is linked in the app, never packed",
        ),
        (
            vec![("plugin.toml", Regular, b"[plugin]\nname = 3\n".as_slice())],
            "plugin.toml:2:8:",
        ),
        (
            vec![("src/init.luau", Regular, b"return {}".as_slice())],
            "the archive has no plugin.toml",
        ),
    ] {
        let told_back = refusal(&archive(&entries), bounds);
        assert!(told_back.contains(told), "{told}: {told_back}");
    }
}

#[test]
fn a_package_s_folder_of_a_link_or_out_of_its_layout_does_not_pack() {
    let package = folder("linked");
    write(&package, &[("plugin.toml", manifest(None).as_bytes())]);
    let packed = pack(&package);
    assert!(
        matches!(&packed, Err(ArchiveError::Refused(message)) if message.contains("holds its entry")),
        "{packed:?}"
    );
    write(&package, &[("src/init.luau", b"return {}\n")]);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", package.join("src/secret.luau")).unwrap();
        let packed = pack(&package);
        assert!(
            matches!(&packed, Err(ArchiveError::Refused(message)) if message.contains("`src/secret.luau` is a link")),
            "{packed:?}"
        );
    }
    let none = pack(&folder("empty"));
    assert!(matches!(none, Err(ArchiveError::Refused(_))), "{none:?}");
    // A refused archive leaves the store as it was.
    let root = folder("untouched");
    let store = Store::new(&root);
    let bad = archive(&plugin(&[("src/../x.luau", tar::EntryType::Regular, b"x")]));
    assert!(store.add(bad.as_slice()).is_err());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    for name in ["linked", "empty", "untouched"] {
        let _ = std::fs::remove_dir_all(folder(name));
    }
}

#[test]
fn a_store_s_package_changed_since_it_was_stored_is_made_anew_as_it_is_added_again() {
    let package = folder("repaired-package");
    write(
        &package,
        &[
            ("plugin.toml", manifest(None).as_bytes()),
            ("src/init.luau", b"return {}\n"),
        ],
    );
    let packed = pack(&package).unwrap();
    let root = folder("repaired-store");
    let store = Store::new(root.join("store"));
    let unpacked = store.add(packed.as_slice()).unwrap().unpacked;
    let stored = store.folder(&unpacked.digest);
    // The Finder's file, and a file that a power cut left empty.
    std::fs::write(stored.join(".DS_Store"), b"finder").unwrap();
    std::fs::write(stored.join("src/init.luau"), b"").unwrap();
    assert!(store.check(&unpacked.digest).is_err());
    // Made anew, it is not new: a failure that follows keeps it, sound.
    let repaired = store.add(packed.as_slice()).unwrap();
    assert!(!repaired.new);
    assert_eq!(repaired.unpacked, unpacked);
    store.check(&unpacked.digest).unwrap();
    assert!(!stored.join(".DS_Store").exists());
    // Nothing left beside it.
    assert_eq!(store.digests().unwrap(), [unpacked.digest]);
    assert_eq!(std::fs::read_dir(store.root()).unwrap().count(), 1);
    for name in ["repaired-package", "repaired-store"] {
        let _ = std::fs::remove_dir_all(folder(name));
    }
}

#[test]
fn a_package_that_no_unpacking_takes_does_not_pack() {
    // More files than an unpacking takes.
    let many = folder("many-files");
    write(
        &many,
        &[
            ("plugin.toml", manifest(None).as_bytes()),
            ("src/init.luau", b"return {}\n"),
        ],
    );
    for at in 0..MOST_FILES {
        std::fs::write(many.join(format!("src/m{at}.luau")), b"").unwrap();
    }
    let ArchiveError::Refused(message) = pack(&many).unwrap_err() else {
        panic!("not a refusal");
    };
    assert!(message.contains("more than 10000 files"), "{message}");
    // A source that compresses as a bomb would.
    let bomb = folder("bomb-source");
    let repeated = vec![b'x'; 3 * 1024 * 1024];
    write(
        &bomb,
        &[
            ("plugin.toml", manifest(None).as_bytes()),
            ("src/init.luau", &repeated),
        ],
    );
    let ArchiveError::Refused(message) = pack(&bomb).unwrap_err() else {
        panic!("not a refusal");
    };
    assert!(message.contains("as a bomb would"), "{message}");
    for name in ["many-files", "bomb-source"] {
        let _ = std::fs::remove_dir_all(folder(name));
    }
}
