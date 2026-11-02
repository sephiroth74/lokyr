//! The backend passes the checks that every platform passes: the targets
//! that open, what they are and where they lead, and the windows that are
//! not there.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{AppEvent, Handler, Platform, WindowEvent, WindowId, conformance};

    use crate::common;

    struct Probe(Rc<RefCell<Option<Vec<String>>>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let directory = std::env::temp_dir()
                .join(format!("lokyr-macos-conformance-{}", std::process::id()));
            *self.0.borrow_mut() = Some(conformance::check(platform, &directory));
            platform.quit();
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: AppEvent) {}

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("conformance", 30);
        let failed = Rc::new(RefCell::new(None));
        lokyr_platform_macos::run(Probe(Rc::clone(&failed))).expect("the application runs");
        let failed = failed.borrow_mut().take().expect("the checks ran");
        assert!(failed.is_empty(), "{failed:#?}");
        println!("conformance: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}
