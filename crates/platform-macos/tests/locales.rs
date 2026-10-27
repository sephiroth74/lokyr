//! The user's languages and regions, as the system's preferences have them:
//! at least the system's own, each a POSIX locale's name without its
//! encoding, once each.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{AppEvent, Handler, Platform, WindowEvent, WindowId};

    use crate::common;

    struct Probe(Rc<RefCell<Vec<String>>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            *self.0.borrow_mut() = platform.locales();
            platform.quit();
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: AppEvent) {}

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    /// Whether `name` is a language and a region, as `it_IT` or `es_419`.
    fn is_locale(name: &str) -> bool {
        let Some((language, region)) = name.split_once('_') else {
            return false;
        };
        (2..=3).contains(&language.len())
            && language.bytes().all(|byte| byte.is_ascii_lowercase())
            && (2..=3).contains(&region.len())
            && region
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    }

    pub fn main() {
        common::deadline("locales", 20);
        let seen = Rc::new(RefCell::new(Vec::new()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let locales = seen.borrow();
        assert!(!locales.is_empty(), "the system's own");
        assert!(locales.iter().all(|name| is_locale(name)), "{locales:?}");
        let mut unique = locales.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), locales.len(), "{locales:?}");
        println!("locales: ok {locales:?}");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}
