//! A plugin's file saved through the save panel, on its window, written
//! where the user chose, and its image on the clipboard.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Code, Command};
use lokyr_kernel::plugins::image::Image;
use lokyr_kernel::plugins::{Cx, Event, Plugin};

use super::*;

/// What the plugin of [`Saver`] heard of its saves, and its requests'
/// errors' codes.
#[derive(Default)]
struct Heard {
    saved: Vec<Event>,
    codes: Vec<Code>,
}

/// A plugin whose `saver.save` saves a PNG through the panel, whose
/// `saver.copy` puts it on the clipboard, and whose `saver.script` saves a
/// script of a name that ends as an image's.
struct Saver {
    heard: Rc<RefCell<Heard>>,
}

const MANIFEST: &str = "[plugin]\nname = \"lokyr/saver\"\nkind = \"builtin\"\nlicense = \"MIT\"\n";

impl Plugin for Saver {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["saver.save", "saver.copy", "saver.script"] {
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
        let png = Image::blank(2.0).png.0;
        let done = match name {
            "saver.save" => cx.save_file(None, "Lokyr.png", png).map(drop),
            "saver.script" => cx
                .save_file(None, "shot.png.command", b"#!/bin/sh\necho hi\n".to_vec())
                .map(drop),
            _ => cx.set_clipboard_image(png),
        };
        if let Err(error) = done {
            self.heard.borrow_mut().codes.push(error.code);
        }
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let saved = events
            .iter()
            .filter(|event| matches!(event, Event::Saved { .. }))
            .cloned();
        self.heard.borrow_mut().saved.extend(saved);
        Ok(())
    }
}

/// The app of the plugin of [`Saver`], of a window focused.
fn saver() -> (Fake, App, WindowId, Rc<RefCell<Heard>>) {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let plugin = Saver {
        heard: Rc::clone(&heard),
    };
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None)
        .with_program(quiet())
        .with_builtin(MANIFEST, Box::new(plugin));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    (fake, app, window, heard)
}

#[test]
fn a_save_shows_its_panel_on_the_window_and_writes_the_file_where_the_user_chose() {
    let (mut fake, mut app, window, heard) = saver();
    let folder = std::env::temp_dir().join(format!("lokyr-app-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let command = |fake: &mut Fake, app: &mut App, name: &str| {
        fake.send_app(app, AppEvent::Command(name.to_owned()));
    };
    command(&mut fake, &mut app, "saver.save");
    let panels = fake.save_panels().to_vec();
    assert_eq!(panels.len(), 1);
    let (panel, shown, name, message) = panels[0].clone();
    assert_eq!((shown, name.as_str()), (window, "Lokyr.png"));
    assert_eq!(message, "lokyr/saver saves this file.");
    // A second while it shows, refused.
    command(&mut fake, &mut app, "saver.save");
    assert_eq!(heard.borrow().codes, [Code::Failed]);
    assert_eq!(fake.save_panels().len(), 1);
    // The user chooses a place: the file holds the PNG, and the plugin
    // hears its path.
    let path = folder.join("chosen.png");
    assert!(fake.close_save_panel(&mut app, panel, Some(path.clone())));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(std::fs::read(&path).unwrap(), Image::blank(2.0).png.0);
    assert!(
        lokyr_platform_core::fake::quarantined(&path),
        "marked as from elsewhere"
    );
    assert_eq!(
        heard.borrow().saved,
        [Event::Saved {
            id: match heard.borrow().saved[0] {
                Event::Saved { id, .. } => id,
                _ => unreachable!(),
            },
            path: Some(path.to_string_lossy().into_owned()),
            error: None,
        }]
    );
    heard.borrow_mut().saved.clear();
    // Cancelled: no file, a save of none.
    command(&mut fake, &mut app, "saver.save");
    let (panel, ..) = fake.save_panels()[0].clone();
    fake.close_save_panel(&mut app, panel, None);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(matches!(
        &heard.borrow().saved[..],
        [Event::Saved {
            path: None,
            error: None,
            ..
        }]
    ));
    heard.borrow_mut().saved.clear();
    // A write that fails, told.
    command(&mut fake, &mut app, "saver.save");
    let (panel, ..) = fake.save_panels()[0].clone();
    let file = folder.join("file");
    std::fs::write(&file, "x").unwrap();
    fake.close_save_panel(&mut app, panel, Some(file.join("under-a-file.png")));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(matches!(
        &heard.borrow().saved[..],
        [Event::Saved { path: None, error: Some(error), .. }] if error.contains("cannot be written")
    ));
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_plugin_s_png_goes_on_the_platform_s_clipboard() {
    let (mut fake, mut app, _, heard) = saver();
    fake.set_clipboard_text("old");
    fake.send_app(&mut app, AppEvent::Command("saver.copy".to_owned()));
    assert!(heard.borrow().codes.is_empty());
    assert_eq!(fake.clipboard_image(), Some(&Image::blank(2.0).png.0[..]));
    assert_eq!(fake.clipboard_text(), None);
}

#[test]
fn a_script_that_a_plugin_saved_asks_before_it_runs() {
    let (mut fake, mut app, window, _) = saver();
    let folder = std::env::temp_dir().join(format!("lokyr-app-scripts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    fake.send_app(&mut app, AppEvent::Command("saver.script".to_owned()));
    let (panel, ..) = fake.save_panels()[0].clone();
    let path = folder.join("shot.png.command");
    fake.close_save_panel(&mut app, panel, Some(path.clone()));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(path.exists());
    // Opened, as of a double click in the Finder: Lokyr asks first, as of a
    // download.
    let windows = fake.windows().count();
    fake.send_app(&mut app, AppEvent::OpenFiles(vec![path.clone()]));
    let asked = fake.alerts().last().cloned().expect("a question");
    assert!(
        asked.alert.message.contains("shot.png.command"),
        "{:?}",
        asked.alert
    );
    assert_eq!(
        fake.windows().count(),
        windows,
        "nothing runs before the answer"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}
