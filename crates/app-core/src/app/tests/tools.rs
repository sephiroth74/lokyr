//! The command line tools, `lokyr` and `sleip`, linked into a folder of
//! the shells' `PATH`: the menu's item, what is there looked at, the
//! question before what is another program's is replaced, and the
//! platform's outcome told.

use lokyr_platform_core::{LinkOutcome, ToolLink};

use super::*;

/// An app started on `fake`, its command line tools in `base/bundle`, both
/// there, and their links' folder `base/bin`.
fn tools_app(fake: &mut Fake, base: &std::path::Path) -> App {
    let bundle = base.join("bundle");
    std::fs::create_dir_all(&bundle).unwrap();
    for name in ["lokyr", "sleip"] {
        std::fs::write(bundle.join(name), name).unwrap();
    }
    let mut app = app(Config::default(), quiet());
    app.tools_places.folder = base.join("bin");
    app.tools_places.source = Some(bundle);
    fake.start(&mut app);
    app
}

/// The links of both tools, of `base`.
fn links(base: &std::path::Path) -> Vec<ToolLink> {
    ["lokyr", "sleip"]
        .iter()
        .map(|name| ToolLink {
            link: base.join("bin").join(name),
            target: base.join("bundle").join(name),
        })
        .collect()
}

/// The last alert's message and detail.
fn last_alert(fake: &Fake) -> (String, String) {
    let alert = &fake.alerts().last().expect("an alert").alert;
    (alert.message.clone(), alert.detail.clone())
}

#[test]
fn the_item_links_both_tools_and_tells_how_it_ended() {
    let base = real_directory("tools-link");
    let mut fake = Fake::default();
    let mut app = tools_app(&mut fake, &base);
    // In the app's menu, after the default terminal.
    let items = menu_items(&fake, 0);
    let at = |title: &str| items.iter().position(|item| item == title);
    assert_eq!(
        at("Install Command Line Tools…"),
        at("Make Lokyr the Default Terminal").map(|default| default + 1),
        "{items:?}"
    );
    // Nothing there: the platform makes both, of a reason that names the
    // folder, which the system's question tells.
    fake.send_app(&mut app, AppEvent::Command(INSTALL_TOOLS.to_owned()));
    let [(asked, reason)] = fake.tool_links() else {
        panic!("one ask: {:?}", fake.tool_links());
    };
    assert_eq!(*asked, links(&base));
    assert!(
        reason.contains(&base.join("bin").display().to_string()),
        "{reason}"
    );
    assert!(fake.alerts().is_empty());
    // Made: an alert says so; the user's cancel of the system's question
    // says nothing; a failure, why.
    fake.send_app(&mut app, AppEvent::ToolsLinked(LinkOutcome::Linked));
    assert_eq!(last_alert(&fake).0, "The command line tools are installed");
    let shown = fake.alerts().len();
    fake.send_app(&mut app, AppEvent::ToolsLinked(LinkOutcome::Cancelled));
    assert_eq!(fake.alerts().len(), shown);
    fake.send_app(
        &mut app,
        AppEvent::ToolsLinked(LinkOutcome::Failed("Operation not permitted".to_owned())),
    );
    assert_eq!(
        last_alert(&fake),
        (
            "The command line tools could not be installed".to_owned(),
            "Operation not permitted".to_owned()
        )
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn links_of_this_app_s_are_told_and_another_program_s_replaced_once_the_user_says_so() {
    let base = real_directory("tools-there");
    let mut fake = Fake::default();
    let mut app = tools_app(&mut fake, &base);
    let bin = base.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for link in links(&base) {
        std::os::unix::fs::symlink(&link.target, &link.link).unwrap();
    }
    // Both this app's: an alert says so, and nothing is asked of the
    // platform.
    fake.send_app(&mut app, AppEvent::Command(INSTALL_TOOLS.to_owned()));
    assert!(fake.tool_links().is_empty());
    assert_eq!(
        last_alert(&fake).0,
        "The command line tools are installed already"
    );
    // Another program's file and a link to another bundle: a question
    // names both, and Cancel leaves them.
    std::fs::remove_file(bin.join("lokyr")).unwrap();
    std::fs::write(bin.join("lokyr"), "another").unwrap();
    std::fs::remove_file(bin.join("sleip")).unwrap();
    std::os::unix::fs::symlink("/Applications/Old.app/sleip", bin.join("sleip")).unwrap();
    fake.send_app(&mut app, AppEvent::Command(INSTALL_TOOLS.to_owned()));
    let question = fake.alerts().last().unwrap().clone();
    assert_eq!(
        question.alert.message,
        format!("Replace what is in {}?", bin.display())
    );
    assert_eq!(
        question.alert.detail,
        format!(
            "{} is a file of another program.\n{} is a link to /Applications/Old.app/sleip.",
            bin.join("lokyr").display(),
            bin.join("sleip").display()
        )
    );
    assert_eq!(question.alert.buttons, ["Replace", "Cancel"]);
    fake.answer_alert(&mut app, question.id, 1);
    assert!(fake.tool_links().is_empty());
    // Replace has the platform make them.
    fake.send_app(&mut app, AppEvent::Command(INSTALL_TOOLS.to_owned()));
    let question = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, question, 0);
    assert_eq!(fake.tool_links().len(), 1);
    assert_eq!(fake.tool_links()[0].0, links(&base));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_folder_in_the_way_or_a_tool_not_beside_the_app_is_refused() {
    let base = real_directory("tools-refused");
    let mut fake = Fake::default();
    let mut app = tools_app(&mut fake, &base);
    // A folder where a link goes: left alone.
    std::fs::create_dir_all(base.join("bin/sleip")).unwrap();
    fake.send_app(&mut app, AppEvent::Command(INSTALL_TOOLS.to_owned()));
    assert!(fake.tool_links().is_empty());
    assert_eq!(
        last_alert(&fake).0,
        format!("{} is a folder", base.join("bin/sleip").display())
    );
    // Sleip not beside the app, as outside its bundle.
    std::fs::remove_file(base.join("bundle/sleip")).unwrap();
    fake.send_app(&mut app, AppEvent::Command(INSTALL_TOOLS.to_owned()));
    assert!(fake.tool_links().is_empty());
    assert_eq!(
        last_alert(&fake),
        (
            "The command line tools are not beside Lokyr".to_owned(),
            format!(
                "{} is not there, as when Lokyr runs outside its bundle.",
                base.join("bundle/sleip").display()
            )
        )
    );
    let _ = std::fs::remove_dir_all(base);
}
