//! A panic of the handler while an alert without a window runs its own
//! loop: the application ends, and the panic goes on from `run`.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::panic::{self, AssertUnwindSafe};

    use lokyr_platform_core::{AppEvent, Handler, Platform, WindowEvent, WindowId};

    use crate::common;

    const MESSAGE: &str = "the handler panics in the alert's loop";

    struct Probe;

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform.show_alert(None, &common::alert());
            common::wake_after(platform.waker(), 500);
        }

        fn app_event(&mut self, _: &mut dyn Platform, event: AppEvent) {
            assert!(event != AppEvent::Wake, "{MESSAGE}");
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("panic_in_modal", 20);
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| lokyr_platform_macos::run(Probe)));
        let payload = outcome.expect_err("the panic comes from `run`");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied());
        assert_eq!(message, Some(MESSAGE));
        println!("panic_in_modal: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}
