use super::*;

/// The candidate `name` of `version`, of the plugin API `api` and of
/// `dependencies`.
fn candidate(
    name: &str,
    version: &str,
    api: Option<&str>,
    dependencies: &[(&str, &str)],
) -> Candidate {
    Candidate {
        name: name.to_owned(),
        version: Version::parse(version).unwrap(),
        api: api.map(str::to_owned),
        dependencies: dependencies
            .iter()
            .map(|(name, constraint)| ((*name).to_owned(), (*constraint).to_owned()))
            .collect(),
    }
}

/// Whether `constraint` holds `version`.
fn holds(constraint: &str, version: &str) -> bool {
    ranges_of(constraint)
        .unwrap()
        .contains(&Version::parse(version).unwrap())
}

#[test]
fn a_constraint_holds_the_versions_that_cargo_s_does() {
    for (constraint, held, not) in [
        (
            "^0.1",
            &["0.1.0", "0.1.9"][..],
            &["0.2.0", "0.0.9", "0.2.0-alpha"][..],
        ),
        ("0.1", &["0.1.0", "0.1.3"], &["0.2.0"]),
        (
            "^1.2.3",
            &["1.2.3", "1.9.0"],
            &["1.2.2", "2.0.0", "2.0.0-beta"],
        ),
        ("^0.0.3", &["0.0.3"], &["0.0.4", "0.0.2"]),
        ("~1.2", &["1.2.0", "1.2.9"], &["1.3.0"]),
        ("~1.2.3", &["1.2.3"], &["1.2.2", "1.3.0"]),
        ("=1.2.3", &["1.2.3"], &["1.2.4"]),
        ("=1.2", &["1.2.0", "1.2.7"], &["1.3.0"]),
        (">=1, <2", &["1.0.0", "1.99.0"], &["0.9.0", "2.0.0"]),
        (">1.2", &["1.3.0"], &["1.2.9"]),
        ("<=1.2", &["1.2.9"], &["1.3.0"]),
        ("*", &["0.0.1", "9.0.0"], &[]),
        ("1.x", &["1.4.0"], &["2.0.0"]),
    ] {
        for version in held {
            assert!(holds(constraint, version), "{constraint} holds {version}");
        }
        for version in not {
            assert!(
                !holds(constraint, version),
                "{constraint} holds not {version}"
            );
        }
    }
    assert!(ranges_of("nope").is_err());
}

#[test]
fn a_resolution_takes_the_highest_version_of_each_that_every_constraint_holds() {
    let api = Version::new(0, 1, 4);
    let candidates = [
        candidate("acme/top", "1.0.0", Some("^0.1"), &[("acme/base", "^1.2")]),
        candidate("acme/base", "1.1.0", Some("^0.1"), &[]),
        candidate("acme/base", "1.3.0", Some("^0.1"), &[]),
        candidate("acme/base", "2.0.0", Some("^0.1"), &[]),
    ];
    let picked = resolve(
        &api,
        &[("acme/top".to_owned(), "*".to_owned())],
        &candidates,
    )
    .unwrap();
    assert_eq!(
        picked,
        BTreeMap::from([
            ("acme/base".to_owned(), Version::new(1, 3, 0)),
            ("acme/top".to_owned(), Version::new(1, 0, 0)),
        ])
    );
}

#[test]
fn a_conflict_tells_its_reason() {
    let api = Version::new(0, 1, 0);
    // A plugin of another plugin API.
    let other_api = [candidate("acme/new", "1.0.0", Some("^0.2"), &[])];
    let error = resolve(&api, &[("acme/new".to_owned(), "*".to_owned())], &other_api).unwrap_err();
    assert!(error.contains("acme/new"), "{error}");
    assert!(error.contains("Lokyr's plugin API"), "{error}");
    // Two plugins of a base of versions that no one holds.
    let split = [
        candidate("acme/a", "1.0.0", None, &[("acme/base", "^1")]),
        candidate("acme/b", "1.0.0", None, &[("acme/base", "^2")]),
        candidate("acme/base", "1.0.0", None, &[]),
        candidate("acme/base", "2.0.0", None, &[]),
    ];
    let wanted = [
        ("acme/a".to_owned(), "*".to_owned()),
        ("acme/b".to_owned(), "*".to_owned()),
    ];
    let error = resolve(&api, &wanted, &split).unwrap_err();
    for told in ["acme/a", "acme/b", "acme/base"] {
        assert!(error.contains(told), "{error}");
    }
    // A plugin that no source holds.
    let error = resolve(&api, &[("acme/gone".to_owned(), "*".to_owned())], &[]).unwrap_err();
    assert!(error.contains("No source holds acme/gone."), "{error}");
    assert!(!error.contains("0.0.0") && !error.contains("-0"), "{error}");
}
