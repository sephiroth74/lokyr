//! A global key that another process holds as its own alone is refused;
//! once that process lets it go, the app takes it.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, Command, Stdio};
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use lokyr_platform_core::{
        AppEvent, GlobalKey, Handler, KeyCode, Platform, WindowEvent, WindowId,
    };

    use crate::common;

    /// The variable that makes the probe's process the key's other holder.
    pub const HOLDER: &str = "LOKYR_PROBE_HOLD_KEY";

    #[repr(C)]
    struct EventHotKeyId {
        signature: u32,
        id: u32,
    }

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn GetApplicationEventTarget() -> *mut c_void;
        fn RegisterEventHotKey(
            code: u32,
            modifiers: u32,
            id: EventHotKeyId,
            target: *mut c_void,
            options: u32,
            registered: *mut *mut c_void,
        ) -> i32;
    }

    /// F17's virtual code, and every modifier: Command, Shift, Option and
    /// Control, as Carbon's masks give them.
    const F17: u32 = 0x40;
    const EVERY_MODIFIER: u32 = (1 << 8) | (1 << 9) | (1 << 11) | (1 << 12);

    /// Holds F17 with every modifier as its own alone, tells the probe, and
    /// waits to be killed, or for 30 seconds.
    pub fn hold() {
        let mut registered = std::ptr::null_mut();
        let id = EventHotKeyId {
            signature: u32::from_be_bytes(*b"PRBE"),
            id: 1,
        };
        // SAFETY: Carbon's calls, on the main thread; the key goes with the
        // process.
        let status = unsafe {
            RegisterEventHotKey(
                F17,
                EVERY_MODIFIER,
                id,
                GetApplicationEventTarget(),
                1,
                &raw mut registered,
            )
        };
        println!("held {status}");
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_secs(30));
    }

    struct Probe {
        refused: Rc<RefCell<Vec<Vec<usize>>>>,
        holder: Option<Child>,
        /// The other holder's line, which a thread reads, as its process
        /// waits for the app's loop to take the key.
        held: Arc<Mutex<Option<String>>>,
    }

    fn key() -> GlobalKey {
        GlobalKey {
            code: KeyCode::F(17),
            command: true,
            control: true,
            alt: true,
            shift: true,
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let mut holder = Command::new(std::env::current_exe().expect("the probe's path"))
                .env(HOLDER, "1")
                .stdout(Stdio::piped())
                .spawn()
                .expect("the other holder");
            let output = holder.stdout.take().expect("its output");
            let held = Arc::clone(&self.held);
            let waker = platform.waker();
            std::thread::spawn(move || {
                let mut line = String::new();
                let _ = BufReader::new(output).read_line(&mut line);
                *held.lock().unwrap() = Some(line);
                waker.wake();
            });
            self.holder = Some(holder);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let Some(line) = self.held.lock().unwrap().take() else {
                return;
            };
            assert_eq!(line.trim(), "held 0", "the other holder took the key");
            let mut refused = self.refused.borrow_mut();
            refused.push(platform.set_global_keys(&[key()]));
            // Let go, the key is the app's.
            if let Some(mut holder) = self.holder.take() {
                let _ = holder.kill();
                let _ = holder.wait();
            }
            refused.push(platform.set_global_keys(&[key()]));
            refused.push(platform.set_global_keys(&[]));
            platform.quit();
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        // The other holder's first connection to the window server takes
        // seconds.
        common::deadline("global_keys", 60);
        let refused = Rc::new(RefCell::new(Vec::new()));
        let probe = Probe {
            refused: Rc::clone(&refused),
            holder: None,
            held: Arc::default(),
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        assert_eq!(*refused.borrow(), [vec![0], vec![], vec![]]);
        println!("global_keys: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os(probe::HOLDER).is_some() {
            probe::hold();
            return;
        }
        probe::main();
    }
}
