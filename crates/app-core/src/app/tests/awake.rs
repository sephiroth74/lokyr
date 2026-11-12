//! The Mac kept awake as a plugin asks, of the platform's power assertion,
//! and let sleep as the plugin lets go or is taken away.

use lokyr_kernel::commands::{Category, Command};
use lokyr_kernel::plugins::host::Awake;
use lokyr_kernel::plugins::{Cx, Plugin};

use super::*;

/// A plugin whose `waker.on` keeps the display awake, and whose
/// `waker.off` lets the Mac sleep.
struct Waker;

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/waker\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[capabilities]\nkeep-awake = true\n",
);

impl Plugin for Waker {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["waker.on", "waker.off"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        cx.keep_awake(if name == "waker.on" {
            Awake::Display
        } else {
            Awake::Off
        })
    }
}

#[test]
fn a_plugin_s_ask_holds_the_platform_s_assertion_until_it_lets_go_or_goes() {
    use lokyr_platform_core::Awake as Held;
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None)
        .with_program(quiet())
        .with_builtin(MANIFEST, Box::new(Waker));
    fake.start(&mut app);
    assert_eq!(fake.awake(), (Held::Off, ""));
    let command = |fake: &mut Fake, app: &mut App, name: &str| {
        fake.send_app(app, AppEvent::Command(name.to_owned()));
    };
    command(&mut fake, &mut app, "waker.on");
    assert_eq!(
        fake.awake(),
        (Held::Display, "Lokyr, kept awake by lokyr/waker")
    );
    command(&mut fake, &mut app, "waker.off");
    assert_eq!(fake.awake(), (Held::Off, ""));
    // Taken away while it keeps the Mac awake, the assertion goes with it.
    command(&mut fake, &mut app, "waker.on");
    assert_eq!(fake.awake().0, Held::Display);
    assert!(app.with_plugins(&mut fake, |plugins, cx| plugins.remove("lokyr/waker", cx)));
    app.after_plugins(&mut fake);
    assert_eq!(fake.awake(), (Held::Off, ""));
}
