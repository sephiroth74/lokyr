//! The checks' run: the app started of a home of its own, the steps, each
//! of its keys and of what it waits for, and the app's end.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::ax::{self, Element};
use crate::keys::{self, CMD, Key, Mods, SHIFT_CMD};

/// How long a step waits for what it checks, and how often it looks.
const WAIT: Duration = Duration::from_secs(10);
const LOOK_AGAIN: Duration = Duration::from_millis(100);

/// How long a new pane's shell may take to read its input, as its rc
/// files run, before the checks type in it.
const SHELL_START: Duration = Duration::from_millis(1500);

/// How deep the checks read the tree, past which no element of the app's
/// is.
const DEPTH: usize = 40;

/// The config of the app that the checks run: its texts in English, which
/// the steps read, its windows neither kept nor restored, no update's
/// check, no global key, its last window's closing its end.
const CONFIG: &str = r#"[app]
language = "en"
restore-windows = "never"
quit-after-last-window-closed = true

[updates]
check = false

[window]
remember-frame = false

[plugins.quick-terminal]
enabled = false
"#;

/// The home of the app that the checks run, a folder of its own, removed
/// as the checks pass, and kept with the app's log where they fail.
struct Home {
    path: PathBuf,
    keep: bool,
}

impl Home {
    /// A new home, of the checks' config.
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("lokyr-e2e-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        // The guard before anything is in it, so that a failure leaves
        // nothing behind.
        let home = Self { path, keep: false };
        let config = home.config().join("lokyr");
        fs::create_dir_all(&config).with_context(|| format!("making {}", config.display()))?;
        fs::write(config.join("config.toml"), CONFIG)?;
        Ok(home)
    }

    /// Its config's folder, which `XDG_CONFIG_HOME` names.
    fn config(&self) -> PathBuf {
        self.path.join(".config")
    }

    /// The app's log, of its standard output and error.
    fn log(&self) -> PathBuf {
        self.path.join("lokyr.log")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        if self.keep {
            eprintln!(
                "lokyr-e2e: the app's home is kept in {}, its log in {}",
                self.path.display(),
                self.log().display()
            );
        } else {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// The app that the checks run: its process, the newest of the bundle's
/// executable but those that ran before the checks started it, and the
/// home of its own.
struct App {
    pid: libc::pid_t,
    home: Home,
}

/// The processes of `executable` that run now.
fn processes(executable: &Path) -> Result<Vec<libc::pid_t>> {
    let found = Command::new("/usr/bin/pgrep")
        .arg("-f")
        .arg(executable)
        .output()
        .context("running pgrep")?;
    Ok(String::from_utf8_lossy(&found.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect())
}

impl App {
    /// Starts the app of the bundle `bundle`, of a home of its own, through
    /// Launch Services, as the Finder starts it, so that it comes in front
    /// and its menus take their keys, which an app that a process in the
    /// background starts does not. A Lokyr of the same bundle that runs
    /// already is never the checks', even where the new one fails to start.
    fn start(bundle: &Path) -> Result<Self> {
        let mut home = Home::new()?;
        let bundle = fs::canonicalize(bundle)?;
        let executable = bundle.join("Contents/MacOS/lokyr");
        let log = home.log();
        let before = processes(&executable)?;
        let status = Command::new("/usr/bin/open")
            .args(["-n", "-F", "-a"])
            .arg(&bundle)
            .arg("--env")
            .arg(format!("HOME={}", home.path.display()))
            .arg("--env")
            .arg(format!("XDG_CONFIG_HOME={}", home.config().display()))
            .arg("--env")
            .arg(format!(
                "XDG_STATE_HOME={}",
                home.path.join(".local/state").display()
            ))
            .arg("--stdout")
            .arg(&log)
            .arg("--stderr")
            .arg(&log)
            .stdin(Stdio::null())
            .status()
            .context("running open")?;
        if !status.success() {
            home.keep = true;
            bail!("open could not start {}", bundle.display());
        }
        // Its process: the newest of its executable's but those before.
        let start = Instant::now();
        let pid = loop {
            let new = processes(&executable)?
                .into_iter()
                .filter(|pid| !before.contains(pid))
                .max();
            if let Some(pid) = new {
                break pid;
            }
            if start.elapsed() > WAIT {
                home.keep = true;
                bail!("{} did not start", executable.display());
            }
            thread::sleep(LOOK_AGAIN);
        };
        Ok(Self { pid, home })
    }

    fn pid(&self) -> libc::pid_t {
        self.pid
    }

    /// The app's element of its tree.
    fn element(&self) -> Result<Element> {
        Element::application(self.pid()).context("the app has no accessibility element")
    }

    /// Whether the app's process has ended.
    #[allow(unsafe_code)]
    fn ended(&mut self) -> bool {
        // SAFETY: a signal of none, which only asks whether the process is
        // there.
        unsafe { libc::kill(self.pid, 0) != 0 }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if !self.ended() {
            // SAFETY: the app's own process, which the checks started.
            #[allow(unsafe_code)]
            unsafe {
                libc::kill(self.pid, libc::SIGKILL)
            };
        }
    }
}

/// What the app shows, as its tree tells it, which the steps check.
struct Shown {
    windows: usize,
    /// The panes' text, of the windows in front first.
    terminals: Vec<String>,
    /// The tabs of the window in front's bar, by the titles of their
    /// buttons, each of a button that closes it after it.
    tabs: Vec<String>,
    /// The texts of the window in front's other elements, as the find
    /// bar's field and count.
    texts: Vec<String>,
}

impl Shown {
    /// What `app` shows now.
    fn of(app: &Element) -> Self {
        let windows = app.windows();
        let mut shown = Self {
            windows: windows.len(),
            terminals: Vec::new(),
            tabs: Vec::new(),
            texts: Vec::new(),
        };
        for (index, window) in windows.into_iter().enumerate() {
            let mut last_button = None;
            for element in window.all(DEPTH) {
                let role = element.role();
                let value = element.text("AXValue").unwrap_or_default();
                let title = element.text("AXTitle").unwrap_or_default();
                if role == "AXTextArea" {
                    shown.terminals.push(value);
                    continue;
                }
                if index != 0 {
                    continue;
                }
                if role == "AXButton" {
                    if title == "Close Tab" {
                        shown.tabs.extend(last_button.take());
                    } else {
                        last_button = Some(title.clone());
                    }
                }
                for text in [value, title] {
                    if !text.is_empty() {
                        shown.texts.push(text);
                    }
                }
            }
        }
        shown
    }

    /// Whether a pane's text holds `text`.
    fn shows(&self, text: &str) -> bool {
        self.terminals
            .iter()
            .any(|terminal| terminal.contains(text))
    }

    /// A summary for a person, of a failed step.
    fn summary(&self) -> String {
        let cut = |text: &String| {
            let lines: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            lines[lines.len().saturating_sub(4)..].join(" ⏎ ")
        };
        format!(
            "{} windows; tabs {:?}; panes {:?}; texts {:?}",
            self.windows,
            self.tabs,
            self.terminals.iter().map(cut).collect::<Vec<_>>(),
            self.texts.iter().take(12).collect::<Vec<_>>()
        )
    }
}

/// The steps, in their order: each its name, what it does, and what it
/// waits for.
struct Step {
    name: &'static str,
    act: fn(libc::pid_t),
    done: fn(&Shown) -> bool,
}

/// The text that the first command prints, which no command line holds.
const FIRST: &str = "lokyr-e2e-42";

/// Whether `text` is the find bar's count of one match of one.
fn is_count(text: &str) -> bool {
    text == "1/1" || text == "1 of 1"
}

fn steps() -> Vec<Step> {
    vec![
        Step {
            name: "the first window opens, of one pane",
            act: |_| {},
            done: |shown| shown.windows == 1 && shown.terminals.len() == 1,
        },
        Step {
            name: "a command runs in the shell and prints",
            act: |pid| {
                thread::sleep(SHELL_START);
                keys::type_text(pid, "echo lokyr-e2e-$((40+2))");
                keys::press(pid, Key::Return, Mods::default());
            },
            done: |shown| shown.shows(FIRST),
        },
        Step {
            name: "⌘T opens a tab, which the bar shows",
            act: |pid| keys::press(pid, Key::Letter('t'), CMD),
            done: |shown| shown.tabs.len() == 2 && !shown.shows(FIRST),
        },
        Step {
            name: "a command runs in the new tab",
            act: |pid| {
                thread::sleep(SHELL_START);
                keys::type_text(pid, "echo second-$((1+1))");
                keys::press(pid, Key::Return, Mods::default());
            },
            done: |shown| shown.shows("second-2"),
        },
        Step {
            name: "⇧⌘[ shows the first tab again",
            act: |pid| keys::press(pid, Key::LeftBracket, SHIFT_CMD),
            done: |shown| shown.shows(FIRST),
        },
        Step {
            name: "⌘D splits the pane",
            act: |pid| keys::press(pid, Key::Letter('d'), CMD),
            done: |shown| shown.terminals.len() == 2,
        },
        Step {
            name: "⌘W closes the new pane",
            act: |pid| keys::press(pid, Key::Letter('w'), CMD),
            done: |shown| shown.terminals.len() == 1 && shown.shows(FIRST),
        },
        Step {
            name: "⌘F opens the find bar, which finds the first command's text",
            act: |pid| {
                keys::press(pid, Key::Letter('f'), CMD);
                thread::sleep(Duration::from_millis(300));
                keys::type_text(pid, "lokyr-e2e-4");
            },
            done: |shown| shown.texts.iter().any(|text| is_count(text)),
        },
        Step {
            name: "Escape closes the find bar",
            act: |pid| keys::press(pid, Key::Escape, Mods::default()),
            done: |shown| {
                shown.windows == 1
                    && shown.terminals.len() == 1
                    && shown.shows(FIRST)
                    && !shown
                        .texts
                        .iter()
                        .any(|text| text == "lokyr-e2e-4" || is_count(text))
            },
        },
        Step {
            name: "⌘N opens a second window",
            act: |pid| keys::press(pid, Key::Letter('n'), CMD),
            done: |shown| shown.windows == 2,
        },
        Step {
            name: "⇧⌘W closes it",
            act: |pid| keys::press(pid, Key::Letter('w'), SHIFT_CMD),
            done: |shown| shown.windows == 1,
        },
    ]
}

/// What the arguments ask for: the bundle, `target/Lokyr.app` by default;
/// the tree alone, of `--dump`; and what each step saw, of `--verbose`.
struct Arguments {
    bundle: PathBuf,
    dump: bool,
    verbose: bool,
}

fn arguments() -> Result<Arguments> {
    let mut bundle = PathBuf::from("target/Lokyr.app");
    let (mut dump, mut verbose) = (false, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--app" => bundle = args.next().context("--app takes a bundle's path")?.into(),
            "--dump" => dump = true,
            "--verbose" => verbose = true,
            other => {
                bail!("`{other}` is no argument of lokyr-e2e: --app <Lokyr.app>, --dump, --verbose")
            }
        }
    }
    if !bundle.join("Contents/MacOS/lokyr").is_file() {
        bail!(
            "{} holds no Lokyr: `just bundle` builds target/Lokyr.app",
            bundle.display()
        );
    }
    Ok(Arguments {
        bundle,
        dump,
        verbose,
    })
}

/// Waits for `done` of what `app` shows, `WAIT` at most: what it showed,
/// as it came, or last, if it did not.
fn wait_for(app: &Element, done: fn(&Shown) -> bool) -> Result<Shown, Shown> {
    let start = Instant::now();
    loop {
        let shown = Shown::of(app);
        if done(&shown) {
            return Ok(shown);
        }
        if start.elapsed() > WAIT {
            return Err(shown);
        }
        thread::sleep(LOOK_AGAIN);
    }
}

/// The tree of `app`'s window in front, a line an element, 80 at most.
fn front_tree(app: &Element) -> String {
    app.windows()
        .into_iter()
        .next()
        .map(|window| {
            window
                .all(DEPTH)
                .iter()
                .take(80)
                .map(Element::line)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn checks() -> Result<()> {
    let Arguments {
        bundle,
        dump,
        verbose,
    } = arguments()?;
    if !ax::trusted() {
        bail!(
            "the checks take the Accessibility permission of the app that runs lokyr-e2e: grant it in System Settings › Privacy & Security › Accessibility, and run again"
        );
    }
    let mut app = App::start(&bundle)?;
    // Kept, with the app's log, unless every step passes.
    app.home.keep = true;
    let element = app.element()?;
    let started = steps().remove(0);
    if let Err(shown) = wait_for(&element, started.done) {
        bail!("step 1, {}, failed: {}", started.name, shown.summary());
    }
    if dump {
        // A moment for the shell's prompt.
        thread::sleep(Duration::from_secs(1));
        for window in element.windows() {
            for found in window.all(DEPTH) {
                println!("{}", found.line());
            }
        }
        app.home.keep = false;
        return Ok(());
    }
    println!("lokyr-e2e: 1. {}: ok", started.name);
    for (index, step) in steps().into_iter().enumerate().skip(1) {
        if app.ended() {
            bail!("step {}, {}: the app ended", index + 1, step.name);
        }
        (step.act)(app.pid());
        let shown = wait_for(&element, step.done);
        if let (true, Ok(shown)) = (verbose, &shown) {
            println!("lokyr-e2e:    {}", shown.summary());
        }
        if let Err(shown) = shown {
            bail!(
                "step {}, {}, failed: {}\nthe window in front's tree:\n{}",
                index + 1,
                step.name,
                shown.summary(),
                front_tree(&element)
            );
        }
        println!("lokyr-e2e: {}. {}: ok", index + 1, step.name);
    }
    // ⌘Q quits, as no program but the shells runs.
    keys::press(app.pid(), Key::Letter('q'), CMD);
    let start = Instant::now();
    while !app.ended() {
        if start.elapsed() > WAIT {
            bail!("⌘Q did not quit the app");
        }
        thread::sleep(LOOK_AGAIN);
    }
    println!("lokyr-e2e: ⌘Q quits: ok");
    app.home.keep = false;
    Ok(())
}

pub fn main() -> ExitCode {
    match checks() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lokyr-e2e: {error:#}");
            ExitCode::FAILURE
        }
    }
}
