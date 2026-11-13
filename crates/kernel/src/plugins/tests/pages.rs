//! The settings tab's pages of Lokyr's own plugins: listed of their
//! manifests, titled of their catalogs, shown at a plugin's ask of its own
//! page alone, rendered while the tab is open, a waiting plugin activated
//! as its page renders, and told to their plugins alone as they show.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_plugin_sdk::view::Node;

use super::*;

/// The manifest of the built-in plugin `<name>`, of its pages `pages`, ids
/// and icons, and of its activation `on`.
fn manifest(name: &str, pages: &[(&str, &str)], on: &[&str]) -> Manifest {
    let pages: Vec<String> = pages
        .iter()
        .map(|(id, icon)| format!("{{ id = \"{id}\", icon = \"{icon}\" }}"))
        .collect();
    let on: Vec<String> = on.iter().map(|on| format!("\"{on}\"")).collect();
    Manifest::parse(&format!(
        "[plugin]\nname = \"{name}\"\nkind = \"builtin\"\nlicense = \"MIT\"\n[activation]\non = [{}]\n[contributes]\npages = [{}]\n",
        on.join(", "),
        pages.join(", ")
    ))
    .unwrap()
}

/// The catalogs of the test's plugin: the page `plugins` titled in
/// English and Italian, `registry` of no title.
static PAGES: [(&str, &str); 2] = [
    ("en", "page-plugins = Plugins\n"),
    ("it", "page-plugins = Plugin\n"),
];

/// A plugin of [`PAGES`], whose command `manager.show` shows the page that
/// its argument `page` names, and which tells what it did in `did`: its
/// activation, each answer of its showing, each batch of events and each
/// render; its pages' events come to it of no subscription.
struct Manager {
    did: Rc<RefCell<Vec<String>>>,
}

impl Plugin for Manager {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.did.borrow_mut().push("activate".to_owned());
        cx.register_command(Command {
            params: vec![Param {
                name: "page".to_owned(),
                kind: Kind::String,
                default: None,
            }],
            ..command("manager.show")
        })
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, args: Args) -> Result<(), Error> {
        let Some(Value::String(page)) = args.get("page") else {
            return Ok(());
        };
        let answer = match cx.show_page(page) {
            Ok(()) => "shown".to_owned(),
            Err(error) => format!("{:?}", error.code),
        };
        self.did.borrow_mut().push(answer);
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            let told = match event {
                Event::PageShown(page) => format!("shown {page}"),
                Event::PageHidden(page) => format!("hidden {page}"),
                other => other.name().to_owned(),
            };
            self.did.borrow_mut().push(told);
        }
        Ok(())
    }

    fn render(&mut self, _: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        self.did.borrow_mut().push(format!("render {view:?}"));
        Some(Node::text("page"))
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&PAGES)
    }
}

/// The manager plugin, and what it did.
fn manager() -> (Box<dyn Plugin>, Rc<RefCell<Vec<String>>>) {
    let did = Rc::new(RefCell::new(Vec::new()));
    let plugin = Manager {
        did: Rc::clone(&did),
    };
    (Box::new(plugin), did)
}

/// The arguments of `manager.show` of `page`.
fn page(page: &str) -> Args {
    let mut args = Args::new();
    args.insert("page".to_owned(), Value::String(page.to_owned()));
    args
}

#[test]
fn the_pages_are_listed_of_their_manifests_in_the_language_shown_but_a_plugin_s_turned_off() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (plugin, _) = manager();
    plugins
        .add_declared(
            manifest(
                "lokyr/manager",
                &[("plugins", "extensions"), ("registry", "cloud")],
                &[],
            ),
            plugin,
        )
        .unwrap();
    plugins
        .add_declared(
            manifest("lokyr/themes", &[("gallery", "paintcan")], &[]),
            scripted(|_| Ok(()), |_, _, _| Ok(())),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let listed = |plugins: &Plugins, language: &str| -> Vec<(String, String, String, String)> {
        plugins
            .pages(language)
            .into_iter()
            .map(|page| (page.plugin, page.id, page.icon, page.title))
            .collect()
    };
    let owned = |plugin: &str, id: &str, icon: &str, title: &str| {
        (
            plugin.to_owned(),
            id.to_owned(),
            icon.to_owned(),
            title.to_owned(),
        )
    };
    // Each titled of its catalogs, the language's or the source's, or of
    // its id without a title.
    assert_eq!(
        listed(&plugins, "it"),
        [
            owned("lokyr/manager", "plugins", "extensions", "Plugin"),
            owned("lokyr/manager", "registry", "cloud", "registry"),
            owned("lokyr/themes", "gallery", "paintcan", "gallery"),
        ]
    );
    assert_eq!(listed(&plugins, "en")[0].3, "Plugins");
    // A plugin turned off has none.
    let off = crate::config::Config::parse(
        "[plugins.themes]\nenabled = false\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    assert!(plugins.configure(&off.plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(listed(&plugins, "en").len(), 2);
}

#[test]
fn a_plugin_shows_a_page_of_its_own_alone_which_renders_while_the_tab_is_open() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let mut plugins = Plugins::new();
    let (plugin, did) = manager();
    plugins
        .add_declared(
            manifest("lokyr/manager", &[("plugins", "extensions")], &[]),
            plugin,
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // Its own page goes to the app, by the plugin's name; another is
    // refused.
    for shown in ["plugins", "other"] {
        plugins
            .run("manager.show", page(shown), &mut app.context())
            .unwrap();
    }
    assert_eq!(did.borrow()[1..], ["shown", "NotFound"]);
    let requests = plugins.take_requests();
    let pages: Vec<(&str, &str)> = requests
        .iter()
        .filter_map(|request| match request {
            Request::ShowPage { plugin, page } => Some((plugin.as_str(), page.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(pages, [("lokyr/manager", "plugins")]);
    // Its page renders while the settings tab is open alone.
    let view = ViewId::Page("plugins".to_owned());
    plugins
        .render("lokyr/manager", &view, &mut app.context())
        .unwrap();
    assert!(plugins.tree("lokyr/manager", &view).is_none());
    app.workspace.open_settings(window, "Settings");
    plugins
        .render("lokyr/manager", &view, &mut app.context())
        .unwrap();
    assert!(plugins.tree("lokyr/manager", &view).is_some());
    // A page that it lacks does not.
    let other = ViewId::Page("other".to_owned());
    plugins
        .render("lokyr/manager", &other, &mut app.context())
        .unwrap();
    assert!(plugins.tree("lokyr/manager", &other).is_none());
    // Its page shown and hidden, told to it alone.
    plugins.page_shown("lokyr/manager", "plugins", true);
    plugins.page_shown("lokyr/manager", "plugins", false);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    let did = did.borrow();
    assert_eq!(did[did.len() - 2..], ["shown plugins", "hidden plugins"]);
}

#[test]
fn a_plugin_that_waits_for_its_page_activates_as_the_page_renders() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 30");
    let mut plugins = Plugins::new();
    let (plugin, did) = manager();
    plugins
        .add_declared(
            manifest(
                "lokyr/manager",
                &[("plugins", "extensions")],
                &["page:plugins"],
            ),
            plugin,
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("lokyr/manager"), Some(Status::Inactive));
    // Listed while it waits.
    assert_eq!(plugins.pages("en").len(), 1);
    // Not while the tab is closed.
    let view = ViewId::Page("plugins".to_owned());
    plugins
        .render("lokyr/manager", &view, &mut app.context())
        .unwrap();
    assert_eq!(plugins.status("lokyr/manager"), Some(Status::Inactive));
    app.workspace.open_settings(window, "Settings");
    plugins
        .render("lokyr/manager", &view, &mut app.context())
        .unwrap();
    assert_eq!(plugins.status("lokyr/manager"), Some(Status::Active));
    assert_eq!(
        *did.borrow(),
        ["activate".to_owned(), format!("render {view:?}")]
    );
}
