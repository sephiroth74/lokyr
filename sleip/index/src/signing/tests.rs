use super::*;

/// A version of `acme/hello`, of `vers` and of `yanked`.
fn entry(vers: &str, yanked: bool) -> Entry {
    Entry {
        v: VERSION,
        name: "acme/hello".to_owned(),
        vers: vers.to_owned(),
        kind: "luau".to_owned(),
        api: Some("^0.1".to_owned()),
        deps: vec![Dependency {
            name: "acme/base".to_owned(),
            req: "^1".to_owned(),
        }],
        caps: vec!["clipboard-write".to_owned()],
        uses: vec!["pane.clear".to_owned()],
        cksum: format!("sha256:{}", "a".repeat(64)),
        yanked,
    }
}

#[test]
fn a_time_writes_and_reads_back_and_another_form_is_refused() {
    for text in [
        "1970-01-01T00:00:00Z",
        "2026-11-10T09:30:05Z",
        "2000-02-29T23:59:59Z",
        "2100-12-31T12:00:00Z",
    ] {
        assert_eq!(Timestamp::parse(text).unwrap().to_string(), text);
    }
    assert_eq!(
        Timestamp::parse("1970-01-02T00:00:01Z"),
        Ok(Timestamp(86_401))
    );
    for wrong in [
        "2026-11-10 09:30:05Z",
        "2026-11-10T09:30:05",
        "2026-11-10T09:30:05+01:00",
        "2026-02-30T00:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-11-10T24:00:00Z",
        "2026-1a-10T09:30:05Z",
    ] {
        assert!(Timestamp::parse(wrong).is_err(), "{wrong}");
    }
}

#[test]
fn a_key_writes_and_reads_back_and_signs() {
    let secret = SecretKey::generate().unwrap();
    let again = SecretKey::parse(&secret.file_text()).unwrap();
    assert_eq!(again.public(), secret.public());
    let public = PublicKey::parse(&secret.public().to_string()).unwrap();
    let signature = secret.sign(b"message");
    assert!(public.verifies(b"message", &signature));
    assert!(!public.verifies(b"messages", &signature));
    assert!(
        !SecretKey::generate()
            .unwrap()
            .public()
            .verifies(b"message", &signature)
    );
    assert!(PublicKey::parse("ed25519:short").is_err());
    assert!(SecretKey::parse("ed25519:AAAA").is_err());
    // Its text told nowhere but its file.
    assert!(!format!("{secret:?}").contains(secret.file_text().trim()));
}

#[test]
fn keys_json_is_taken_of_a_root_key_and_not_older_than_the_one_held() {
    let root = SecretKey::generate().unwrap();
    let index = SecretKey::generate().unwrap();
    let at = Timestamp::parse("2026-11-10T09:00:00Z").unwrap();
    let text = sign_keys(&root, &[index.public()], at);
    let keys = verify_keys(&text, &[root.public()], None).unwrap();
    assert_eq!(keys.keys, [index.public()]);
    assert_eq!(keys.generated, at);
    // Of the same time, held: taken; of a later one: refused.
    assert!(verify_keys(&text, &[root.public()], Some(at)).is_ok());
    let later = Timestamp(at.0 + 1);
    let why = verify_keys(&text, &[root.public()], Some(later)).unwrap_err();
    assert!(
        why.contains("before the one that this Sleip holds"),
        "{why}"
    );
    // Of another root, or of a key added after its signature.
    let other = SecretKey::generate().unwrap();
    let why = verify_keys(&text, &[other.public()], None).unwrap_err();
    assert!(why.contains("signed by no root key"), "{why}");
    let mut file: serde_json::Value = serde_json::from_str(&text).unwrap();
    file["keys"]
        .as_array_mut()
        .unwrap()
        .push(other.public().to_string().into());
    let tampered = serde_json::to_string(&file).unwrap();
    assert!(verify_keys(&tampered, &[root.public()], None).is_err());
    // Of no root at all, as a Sleip of none.
    assert!(verify_keys(&text, &[], None).is_err());
}

#[test]
fn an_index_file_is_taken_of_a_listed_key_whole_of_its_plugin_and_not_older() {
    let root = SecretKey::generate().unwrap();
    let index = SecretKey::generate().unwrap();
    let at = Timestamp::parse("2026-11-10T09:00:00Z").unwrap();
    let keys = verify_keys(
        &sign_keys(&root, &[index.public()], at),
        &[root.public()],
        None,
    )
    .unwrap();
    let entries = [entry("0.1.0", true), entry("0.1.1", false)];
    let text = sign_index("acme/hello", &entries, &index, at);
    assert_eq!(text.lines().count(), 3);
    let read = verify_index(&text, "acme/hello", &keys, None).unwrap();
    assert_eq!(read.entries, entries);
    assert_eq!(read.generated, at);
    // Of no version, as a plugin of none released yet.
    let empty = sign_index("acme/hello", &[], &index, at);
    assert!(
        verify_index(&empty, "acme/hello", &keys, None)
            .unwrap()
            .entries
            .is_empty()
    );
    // Of a key that keys.json does not list.
    let stranger = SecretKey::generate().unwrap();
    let why = verify_index(
        &sign_index("acme/hello", &entries, &stranger, at),
        "acme/hello",
        &keys,
        None,
    )
    .unwrap_err();
    assert!(why.contains("which keys.json does not list"), "{why}");
    // Older than the one held.
    let why = verify_index(&text, "acme/hello", &keys, Some(Timestamp(at.0 + 60))).unwrap_err();
    assert!(
        why.contains("before the one that this Sleip holds"),
        "{why}"
    );
    // A line changed, a yank taken back.
    let tampered = text.replacen("\"yanked\":true", "\"yanked\":false", 1);
    let why = verify_index(&tampered, "acme/hello", &keys, None).unwrap_err();
    assert!(why.contains("not as its signature says"), "{why}");
    // A line taken away.
    let mut lines: Vec<&str> = text.lines().collect();
    lines.remove(0);
    let shorter = lines.join("\n") + "\n";
    assert!(verify_index(&shorter, "acme/hello", &keys, None).is_err());
    // Served as another plugin's file.
    let why = verify_index(&text, "bob/hello", &keys, None).unwrap_err();
    assert!(why.contains("not as its signature says"), "{why}");
    // Of no signature, or of no line's end.
    assert!(verify_index(&text.replace('\n', "\r\n"), "acme/hello", &keys, None).is_err());
    assert!(verify_index(text.trim_end(), "acme/hello", &keys, None).is_err());
}

#[test]
fn sleip_s_root_keys_read() {
    assert_eq!(roots().len(), ROOT_KEYS.len());
}
