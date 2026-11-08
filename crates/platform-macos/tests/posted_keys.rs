//! A key that the app posts comes back to its window as the keyboard's
//! would: its press and its release, of its text, stamped with its time.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use lokyr_platform_core::{
        AppEvent, Handler, KeyCode, KeyState, Platform, Size, WindowAttributes, WindowEvent,
        WindowId,
    };

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        posted: Option<Instant>,
        /// The keys that came: their state, their text, and how long after
        /// the post their time was.
        keys: Vec<(KeyState, String, Option<Duration>)>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform
                .create_window(&WindowAttributes {
                    title: "Posted keys".to_owned(),
                    size: Size {
                        width: 320.0,
                        height: 200.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("a window");
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: AppEvent) {}

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            let mut seen = self.0.borrow_mut();
            match event {
                WindowEvent::Focused(true) if seen.posted.is_none() => {
                    seen.posted = Some(Instant::now());
                    platform.post_key(window, KeyCode::KeyA, "a");
                }
                WindowEvent::Key(key) => {
                    let after = key
                        .time
                        .zip(seen.posted)
                        .map(|(time, posted)| time.saturating_duration_since(posted));
                    seen.keys.push((key.state, key.text, after));
                    if key.state == KeyState::Released {
                        platform.quit();
                    }
                }
                _ => {}
            }
        }
    }

    pub fn main() {
        common::deadline("posted_keys", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let states: Vec<(KeyState, &str)> = seen
            .keys
            .iter()
            .map(|(state, text, _)| (*state, text.as_str()))
            .collect();
        assert_eq!(
            states,
            [(KeyState::Pressed, "a"), (KeyState::Released, "a")],
            "{seen:?}"
        );
        // Stamped as it was posted, within a few milliseconds.
        for (_, _, after) in &seen.keys {
            let after = after.expect("a time");
            assert!(after < Duration::from_millis(50), "{seen:?}");
        }
        println!("posted_keys: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}
