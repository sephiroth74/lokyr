//! The application's icon while it runs: an icon file's image in place of
//! the application's own, its own again with none, and an error for a file
//! that is not an image, which leaves the icon shown as it was.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use lokyr_platform_core::{AppEvent, Error, Handler, Platform, WindowEvent, WindowId};
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    use crate::common;

    /// The icon that the application shows, as its image's TIFF data.
    fn shown() -> Vec<u8> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let image = NSApplication::sharedApplication(mtm)
            .applicationIconImage()
            .expect("an icon");
        image.TIFFRepresentation().expect("its data").to_vec()
    }

    #[derive(Default)]
    struct Seen {
        icons: Vec<Vec<u8>>,
        results: Vec<Result<(), Error>>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let icons = root.join("../app/resources/icons");
            let (laguz, jormungandr) = (
                icons.join("lokyr-laguz.icns"),
                icons.join("lokyr-jormungandr.icns"),
            );
            let mut seen = self.0.borrow_mut();
            seen.icons.push(shown());
            for icon in [Some(&laguz), Some(&jormungandr), Some(&laguz), None] {
                seen.results
                    .push(platform.set_app_icon(icon.map(PathBuf::as_path)));
                seen.icons.push(shown());
            }
            // A file that is no image, and one that does not exist.
            seen.results
                .push(platform.set_app_icon(Some(&root.join("Cargo.toml"))));
            seen.results
                .push(platform.set_app_icon(Some(&root.join("missing.icns"))));
            seen.icons.push(shown());
            platform.quit();
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: AppEvent) {}

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("app_icon", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let lengths: Vec<usize> = seen.icons.iter().map(Vec::len).collect();
        let icons = &seen.icons;
        assert!(
            seen.results[..4].iter().all(Result::is_ok),
            "{:?}",
            seen.results
        );
        // Each variant's own image, the same each time.
        assert!(icons[1] != icons[0], "a variant shows: {lengths:?}");
        assert!(icons[2] != icons[1], "another one shows: {lengths:?}");
        assert!(icons[3] == icons[1], "the first one again: {lengths:?}");
        assert!(icons[4] == icons[0], "the own icon again: {lengths:?}");
        for result in &seen.results[4..] {
            assert!(matches!(result, Err(Error::Icon(_))), "{:?}", seen.results);
        }
        assert!(icons[5] == icons[4], "unchanged: {lengths:?}");
        println!("app_icon: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}
