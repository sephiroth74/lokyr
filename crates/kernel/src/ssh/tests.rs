use super::*;

fn args(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

/// The config of the integration on, of `ignored` hosts.
fn terminal(on: bool, ignored: &[&str]) -> TerminalConfig {
    TerminalConfig {
        ssh_integration: on,
        ssh_hosts_ignored: ignored.iter().map(|&host| host.to_owned()).collect(),
        ..TerminalConfig::default()
    }
}

fn target(hostname: &str) -> Target {
    Target {
        hostname: hostname.to_owned(),
        user: "me".to_owned(),
        port: "22".to_owned(),
    }
}

#[test]
fn an_interactive_session_is_one_destination_of_no_command_nor_option_of_no_terminal() {
    for (line, destination) in [
        ("host", Some(0)),
        ("me@host", Some(0)),
        ("-p 2222 host", Some(2)),
        ("-p2222 host", Some(1)),
        ("-i ~/.ssh/key -o ServerAliveInterval=30 host", Some(4)),
        ("-4Av host", Some(1)),
        ("-t host", Some(1)),
        ("-J jump host", Some(2)),
        ("host -p 2222", Some(0)),
        ("-L 8080:localhost:80 host", Some(2)),
        ("-- host", Some(1)),
    ] {
        assert_eq!(session(&args(line)), destination, "{line}");
    }
    // A remote command, no terminal, a forward, the config, a query, a
    // control command, the background, a subsystem, no destination.
    for line in [
        "host ls",
        "host -- ls",
        "-t host vim notes",
        "-T host",
        "-N -L 8080:localhost:80 host",
        "-W localhost:22 jump",
        "-G host",
        "-V",
        "-Q cipher",
        "-O exit host",
        "-f host",
        "-s host sftp",
        "-p 2222",
        "",
        "-vT host",
    ] {
        assert_eq!(session(&args(line)), None, "{line}");
    }
}

#[test]
fn a_host_resolves_of_its_printed_config() {
    let output = "user me\nhostname build.example.com\nport 2222\nidentityfile ~/.ssh/id\n";
    let target = Target::of_config(output).unwrap();
    assert_eq!(target.key(), "me@build.example.com:2222");
    assert_eq!(Target::of_config("user me\nport 22\n"), None);
}

#[test]
fn a_host_s_pattern_matches_of_stars_and_question_marks_whatever_its_case() {
    assert!(matches("*.example.com", "Build.EXAMPLE.com"));
    assert!(matches("build-?", "build-3"));
    assert!(matches("*", "anything"));
    assert!(matches("pi*", "pi"));
    assert!(matches("a*b*c", "aXXbYYc"));
    assert!(!matches("*.example.com", "example.com"));
    assert!(!matches("build-?", "build-10"));
    assert!(!matches("a*b*c", "aXXbYY"));
}

#[test]
fn the_plan_installs_once_falls_back_where_it_cannot_and_leaves_any_other_ssh() {
    const OWN: &str = "xterm-lokyr";
    let none = |_: &str| None;
    let resolved = |_: &[String]| Some(target("build.example.com"));
    let on = terminal(true, &[]);
    // Of the terminal's own TERM, either of its names, a host not known:
    // installed.
    for term in OWN_TERMS {
        assert_eq!(
            plan(&args("build"), term, &on, none, resolved),
            Plan::Install("me@build.example.com:22".to_owned())
        );
    }
    // A host that got it: as it is; where it failed: of the fallback.
    let got = |key: &str| (key == "me@build.example.com:22").then_some(Outcome::Installed);
    assert_eq!(plan(&args("build"), OWN, &on, got, resolved), Plan::Plain);
    let failed = |key: &str| (key == "me@build.example.com:22").then_some(Outcome::Failed);
    assert_eq!(
        plan(&args("build"), OWN, &on, failed, resolved),
        Plan::Fallback
    );
    // Another TERM, a remote command, the integration off: as it is.
    assert_eq!(
        plan(&args("build"), "xterm-256color", &on, none, resolved),
        Plan::Plain
    );
    assert_eq!(
        plan(&args("build ls"), OWN, &on, none, resolved),
        Plan::Plain
    );
    let off = terminal(false, &[]);
    assert_eq!(plan(&args("build"), OWN, &off, none, resolved), Plan::Plain);
    // A host that does not resolve, or that the config leaves out, by its
    // name or as typed: of the fallback's TERM.
    assert_eq!(
        plan(&args("build"), OWN, &on, none, |_: &[String]| None),
        Plan::Fallback
    );
    let ignored = terminal(true, &["*.example.com"]);
    assert_eq!(
        plan(&args("build"), OWN, &ignored, none, resolved),
        Plan::Fallback
    );
    let typed = terminal(true, &["build"]);
    assert_eq!(
        plan(&args("me@build"), OWN, &typed, none, resolved),
        Plan::Fallback
    );
}

#[test]
fn the_install_s_ssh_has_no_terminal_nor_the_config_s_command_and_runs_tic() {
    assert_eq!(
        install_args(&args("-p 2222 build")),
        [
            "-T",
            "-o",
            "RemoteCommand=none",
            "-p",
            "2222",
            "build",
            INSTALL_COMMAND
        ]
    );
    // The host's exit code tells the outcome: the entry there of 0, a
    // connection that failed or a signal nothing, any other a host that
    // cannot take it.
    assert_eq!(Outcome::of_status(Some(0)), Some(Outcome::Installed));
    assert_eq!(Outcome::of_status(Some(CONNECTION_FAILED)), None);
    assert_eq!(Outcome::of_status(None), None);
    assert_eq!(Outcome::of_status(Some(127)), Some(Outcome::Failed));
    assert_eq!(Outcome::of_status(Some(1)), Some(Outcome::Failed));
}

#[test]
fn a_host_s_outcome_is_remembered_once_in_its_file() {
    let folder = std::env::temp_dir().join(format!("lokyr-ssh-hosts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let file = folder.join("state").join(HOSTS_FILE);
    assert!(hosts(&file).is_empty());
    remember(&file, "me@a:22", Outcome::Failed).unwrap();
    remember(&file, "me@b:22", Outcome::Installed).unwrap();
    remember(&file, "me@a:22", Outcome::Installed).unwrap();
    remember(&file, "bad\nkey", Outcome::Installed).unwrap();
    assert_eq!(
        hosts(&file),
        [
            (Outcome::Installed, "me@b:22".to_owned()),
            (Outcome::Installed, "me@a:22".to_owned())
        ]
    );
    assert_eq!(outcome(&file, "me@a:22"), Some(Outcome::Installed));
    assert_eq!(outcome(&file, "me@c:22"), None);
    // A line of another form is left out.
    std::fs::write(&file, "failed me@a:22\nsomething else\n").unwrap();
    assert_eq!(outcome(&file, "me@a:22"), Some(Outcome::Failed));
    assert_eq!(hosts(&file).len(), 1);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_session_of_the_shell_s_term_goes_as_it_is_without_asking_ssh() {
    // Another TERM, a remote command: the TERM as it is, of no ssh run.
    let on = terminal(true, &[]);
    assert_eq!(
        term(&args("build"), "xterm-256color", &on, None).as_deref(),
        Ok("xterm-256color")
    );
    assert_eq!(
        term(&args("build ls"), "xterm-lokyr", &on, None).as_deref(),
        Ok("xterm-lokyr")
    );
}
