//! The plugins' sections of the config file in the app: a plugin takes its
//! section as the app starts and as the file reloads, and the notice tells
//! a section refused at its line, or what it ignored, while the rest of the
//! file applies.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::{Cx, Data, Plugin};

use super::*;

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/keyed\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nconfig-schema = \"schema.json\"\n",
);

const SCHEMA: &str = r#"{ "type": "object", "properties": { "count": { "type": "integer", "minimum": 1, "default": 1 } } }"#;

/// A plugin of a key, `count`, whose config it reads as it activates and
/// as it changes, into `heard`.
struct Keyed {
    heard: Rc<RefCell<Vec<Data>>>,
}

impl Plugin for Keyed {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.heard.borrow_mut().push(cx.config()?);
        Ok(())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[plugins::Event]) -> Result<(), Error> {
        if events.contains(&plugins::Event::ConfigChanged) {
            self.heard.borrow_mut().push(cx.config()?);
        }
        Ok(())
    }

    fn config_schema(&self) -> Option<&'static str> {
        Some(SCHEMA)
    }
}

/// `keyed`'s config of `count`.
fn count(count: i64) -> Data {
    Data::Map([("count".to_owned(), Data::Integer(count))].into())
}

#[test]
fn a_plugin_s_section_applies_as_the_file_reloads_and_a_wrong_one_tells_its_line() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-plugin-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[plugins.keyed]\ncount = 2\n").unwrap();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut fake = Fake::default();
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_builtin(
            MANIFEST,
            Box::new(Keyed {
                heard: Rc::clone(&heard),
            }),
        );
    fake.start(&mut app);
    assert_eq!(*heard.borrow(), [count(2)]);
    assert_eq!(app.diagnostics.shown(), None);
    // A wrong value refuses its section alone, as the notice tells at its
    // line; the rest of the file applies.
    std::fs::write(
        &path,
        "[cursor]\nshape = \"bar\"\n\n[plugins.keyed]\ncount = 0\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(shown.about, About::PluginConfig("keyed".to_owned()));
    assert!(
        shown
            .text
            .contains("config.toml:5:1: `count` is at least 1, not 0")
            && shown.text.ends_with("keeps its config in force"),
        "{}",
        shown.text
    );
    assert_eq!(*heard.borrow(), [count(2)]);
    // A section that it takes takes the notice away, and the plugin hears
    // of its change.
    std::fs::write(
        &path,
        "[cursor]\nshape = \"bar\"\n\n[plugins.keyed]\ncount = 5\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    assert_eq!(*heard.borrow(), [count(2), count(5)]);
    // A key of another version, and a plugin that is not there, are
    // ignored, as the notices tell.
    std::fs::write(
        &path,
        "[plugins.keyed]\ncount = 5\nnewer = true\n\n[plugins.absent]\nx = 1\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Block);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(shown.about, About::PluginConfig("absent".to_owned()));
    assert!(
        app.diagnostics
            .tells(&About::PluginConfig("keyed".to_owned()))
    );
    assert_eq!(heard.borrow().len(), 2);
    let _ = std::fs::remove_dir_all(&directory);
}
