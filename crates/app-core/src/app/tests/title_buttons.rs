//! The titlebar's small buttons beside the window's: a plugin's view right
//! after the window's buttons, of its own width, which a tab bar in the
//! titlebar starts after, its button's press its own.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{Slot, UiEvent, ViewId};
use lokyr_kernel::plugins::view::{Button, Kind, Layout as NodeLayout, Length, Node};
use lokyr_platform_core::{Modifiers, Point};
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// A plugin of a button in the titlebar's small buttons, 24 points wide,
/// which keeps the clicks that its nodes take, and of a tab bar of a text.
struct Awake {
    clicks: Rc<RefCell<Vec<String>>>,
}

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/sleepless\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"title-buttons\", \"tab-bar\"]\n",
);

impl plugins::Plugin for Awake {
    fn activate(&mut self, _: &mut plugins::Cx<'_>) -> Result<(), Error> {
        Ok(())
    }

    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        match view.slot()? {
            Slot::TitleButtons => Some(
                Node::new(Kind::Button(Button {
                    label: String::new(),
                    icon: "eye".to_owned(),
                    disabled: false,
                }))
                .key("awake")
                .layout(NodeLayout {
                    width: Length::Points(24.0),
                    height: Length::Points(20.0),
                    ..NodeLayout::default()
                }),
            ),
            Slot::TabBar => Some(
                Node::container(vec![Node::text("tabs")]).layout(NodeLayout {
                    grow: 1.0,
                    ..NodeLayout::default()
                }),
            ),
            _ => None,
        }
    }

    fn on_ui_event(&mut self, _: &mut plugins::Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if matches!(event.kind, lokyr_kernel::plugins::view::UiEventKind::Click) {
            self.clicks.borrow_mut().push(event.key.0.clone());
        }
        Ok(())
    }
}

#[test]
fn a_title_button_sits_after_the_window_s_buttons_and_takes_its_press() {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let plugin = Awake {
        clicks: Rc::clone(&clicks),
    };
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut config = unpadded();
    config.slots.tab_bar = Some("lokyr/sleepless".to_owned());
    let mut app = App::new(Some(drawing), config, None)
        .with_program(quiet())
        .with_builtin(MANIFEST, Box::new(plugin));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    for _ in 0..4 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    let size = fake.window(window).unwrap().size;
    let scale = size.scale as f32;
    let frame_of = |app: &App, of: fn(&ViewId) -> bool| {
        app.windows[&window]
            .chrome
            .iter()
            .find(|view| of(&view.view))
            .and_then(|view| view.frame)
            .expect("a frame")
    };
    let buttons = frame_of(&app, |view| matches!(view, ViewId::TitleButtons(_)));
    let tab_bar = frame_of(&app, |view| matches!(view, ViewId::TabBar(_)));
    // Right after the window's buttons, of its width, the titlebar's
    // height; the tab bar after it.
    assert_eq!(buttons.x, (size.buttons * size.scale) as f32);
    assert_eq!((buttons.y, buttons.height), (0.0, size.bar_pixels() as f32));
    assert_eq!(buttons.width, 24.0 * scale);
    assert_eq!(tab_bar.x, buttons.x + buttons.width);
    // Its button's press its own, of the titlebar's areas.
    let areas = &fake.window(window).unwrap().titlebar_areas;
    let at = Point {
        x: f64::from((buttons.x + buttons.width / 2.0) / scale),
        y: f64::from((buttons.y + buttons.height / 2.0) / scale),
    };
    assert!(areas.iter().any(|area| area.contains(at)), "{areas:?}");
    fake.send(&mut app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, at, Modifiers::NONE, 1));
    assert_eq!(*clicks.borrow(), ["awake"]);
}
