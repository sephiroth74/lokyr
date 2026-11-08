//! The app as the system's default terminal: the scripts and executables
//! that the system opens with it, each run in a window of its own, the
//! `ssh` links, a window that connects to their host, and the man pages'
//! links, the window of their page; and the menu's item that asks the
//! system to make it so.

use std::path::{Path, PathBuf};

use lokyr_kernel::commands::Code;
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::Arg;
use lokyr_platform_core::{Alert, AlertStyle, Platform, WindowId};
use lokyr_pty::Program;

use super::menus::{man_page, man_program};
use super::{App, DEFAULT_TERMINAL};
use crate::drawing::Backend;

/// The most bytes of a link that the app reads, past which it opens
/// nothing of it.
const MOST_LINK: usize = 2048;

impl<B: Backend> App<B> {
    /// Shows the menu's item of the default terminal as the system has it:
    /// ticked and dimmed while the app is the default, as it is then.
    pub(super) fn show_default_terminal(&self, platform: &mut dyn Platform) {
        let default = platform.is_default_terminal();
        platform.set_command_checked(DEFAULT_TERMINAL, default);
        platform.set_command_enabled(DEFAULT_TERMINAL, !default);
    }

    /// Asks the system to make the app its default terminal, which asks
    /// the user to confirm; outside its bundle, a notice tells why it
    /// cannot.
    pub(super) fn make_default_terminal(&mut self, platform: &mut dyn Platform) {
        if !platform.make_default_terminal() {
            let text = "Lokyr cannot be the default terminal outside its bundle, which the system opens scripts and links in";
            self.report(
                platform,
                Diagnostic::new(Code::Unavailable, text, About::Other),
            );
        }
    }

    /// The system answered the ask to make the app the default terminal:
    /// the menu's item shows it, and the log what did not change, as what
    /// the user declined.
    pub(super) fn default_terminal_made(&mut self, platform: &mut dyn Platform, failed: &[String]) {
        for failure in failed {
            eprintln!("lokyr: the default terminal did not change for {failure}");
        }
        self.show_default_terminal(platform);
    }

    /// Runs each of `scripts`, a script or an executable that the system
    /// opens with the app, in a window of its own, in its folder; one that
    /// came from the internet once the user says so, as a question asks.
    pub(super) fn run_scripts(&mut self, platform: &mut dyn Platform, scripts: Vec<PathBuf>) {
        let mut shown = None;
        for script in scripts {
            if platform.downloaded(&script) {
                self.ask_script(platform, script);
            } else if let Some(window) = self.run_script(platform, &script) {
                shown = Some(window);
            }
        }
        if let Some(window) = shown {
            platform.focus_window(window);
        }
    }

    /// Runs `script` in a window of its own, in its folder: the user's
    /// login shell runs it, of the user's environment, itself where it is
    /// executable, else of its interpreter, its first line's or its
    /// extension's; and the window keeps the screen once it ends, whatever
    /// `close-on-exit` says, until a key closes it. The window, if it
    /// opened; a notice tells why one does not run.
    pub(super) fn run_script(
        &mut self,
        platform: &mut dyn Platform,
        script: &Path,
    ) -> Option<WindowId> {
        let command = match script_command(script, is_executable(script), &first_line(script)) {
            Ok(command) => command,
            Err(why) => {
                let text = format!("{} does not run, as {why}", script.display());
                self.report(platform, Diagnostic::new(Code::Failed, text, About::Other));
                return None;
            }
        };
        let program = Program::LoginCommand {
            command: command.into(),
        };
        let folder = script.parent().map(Path::to_path_buf);
        let window = self.open_window_running(platform, &program, None, folder)?;
        if let Some(pane) = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.active_pane(state.window))
        {
            self.scripts.insert(pane);
        }
        Some(window)
    }

    /// Asks whether to run `script`, which came from the internet, as macOS
    /// asks of an app downloaded: it runs once the user says so.
    fn ask_script(&mut self, platform: &mut dyn Platform, script: PathBuf) {
        let name = script
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self
                .texts
                .text_with("script-downloaded-message", &[("name", Arg::Text(&name))]),
            detail: self.texts.text("script-downloaded-detail"),
            buttons: vec![
                self.texts.text("script-downloaded-run"),
                self.texts.text("script-downloaded-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(None, &alert);
        self.script_asks.insert(alert, script);
    }

    /// The question of a downloaded script closed of `button`: the script
    /// runs of the first, Run.
    pub(super) fn script_answered(
        &mut self,
        platform: &mut dyn Platform,
        script: &Path,
        button: usize,
    ) {
        if button == 0
            && let Some(window) = self.run_script(platform, script)
        {
            platform.focus_window(window);
        }
    }

    /// Opens each of `links` that the system opens with the app: an `ssh`
    /// link, a window that runs `ssh` to its host; a man page's, the
    /// window of its page; any other, nothing, as the log tells.
    pub(super) fn open_links(&mut self, platform: &mut dyn Platform, links: &[String]) {
        let mut shown = None;
        for link in links {
            let opened = if let Some(args) = ssh_args(link) {
                let program = Program::command(&self.ssh, args);
                self.open_window_running(platform, &program, None, None)
            } else if let Some((title, args)) = man_link(link) {
                let program = man_program(self.man.as_deref(), &args);
                self.open_window_running(platform, &program, Some(format!("man {title}")), None)
            } else {
                eprintln!(
                    "lokyr: nothing opens of the link {:?}, which is no ssh link nor a man page's",
                    link.chars().take(200).collect::<String>()
                );
                None
            };
            shown = opened.or(shown);
        }
        if let Some(window) = shown {
            platform.focus_window(window);
        }
    }
}

/// The interpreters of the scripts that run of their extensions, where a
/// script is not executable and names none on its first line, as the
/// login shell finds them on the user's `PATH`.
const INTERPRETERS: [(&str, &str); 15] = [
    ("command", "/bin/sh"),
    ("tool", "/bin/sh"),
    ("sh", "/bin/sh"),
    ("bash", "bash"),
    ("zsh", "zsh"),
    ("ksh", "ksh"),
    ("csh", "tcsh"),
    ("tcsh", "tcsh"),
    ("fish", "fish"),
    ("py", "python3"),
    ("rb", "ruby"),
    ("pl", "perl"),
    ("php", "php"),
    ("lua", "lua"),
    ("tcl", "tclsh"),
];

/// The interpreter of the scripts of `path`'s extension, whatever its case.
pub(super) fn interpreter_of(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    INTERPRETERS
        .iter()
        .find(|(name, _)| extension.eq_ignore_ascii_case(name))
        .map(|&(_, interpreter)| interpreter)
}

/// Whether the file at `path` is executable by someone.
pub(super) fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// The interpreter's words that the first line of the file at `path` names,
/// `#!/usr/bin/env python3`'s, as the system runs an executable script of;
/// none for a file of no such line, or one that does not read.
pub(super) fn first_line(path: &Path) -> Vec<String> {
    use std::io::Read;
    let mut head = [0u8; 256];
    let read = std::fs::File::open(path)
        .and_then(|mut file| file.read(&mut head))
        .unwrap_or(0);
    let head = &head[..read];
    let Some(rest) = head.strip_prefix(b"#!") else {
        return Vec::new();
    };
    let line = rest.split(|&byte| byte == b'\n').next().unwrap_or_default();
    String::from_utf8_lossy(line)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// `text` in single quotes, which sh, bash, zsh, fish and tcsh read alike,
/// a quote in it closed, escaped and opened again; `None` for a text of a
/// backslash, which fish reads otherwise in quotes, of `!`, which tcsh and
/// csh take as history's even there, or of a control character.
fn quoted(text: &str) -> Option<String> {
    if text.contains(['\\', '!']) || text.chars().any(char::is_control) {
        return None;
    }
    Some(format!("'{}'", text.replace('\'', "'\\''")))
}

/// The command that the login shell runs `script` of: `exec` of its path,
/// where it is `executable`; else of its interpreter, of `named`, the words
/// of its first line, or of its extension's; why it cannot, else.
fn script_command(
    script: &Path,
    executable: bool,
    named: &[String],
) -> Result<String, &'static str> {
    let path = script
        .to_str()
        .and_then(quoted)
        .ok_or("its path holds a backslash, a `!` or a control character, or is not text, which a shell may read otherwise")?;
    if executable {
        return Ok(format!("exec {path}"));
    }
    let interpreter = if named.is_empty() {
        vec![
            interpreter_of(script)
                .ok_or("it is not executable, and names no interpreter on its first line nor of its extension")?
                .to_owned(),
        ]
    } else {
        named.to_vec()
    };
    let words: Option<Vec<String>> = interpreter.iter().map(|word| quoted(word)).collect();
    let words = words.ok_or(
        "its first line names an interpreter of a character that a shell may read otherwise",
    )?;
    Ok(format!("exec {} {path}", words.join(" ")))
}

/// The arguments of `ssh` for `link`, `ssh://[user[;parameters]@]host[:port][/]`:
/// the port, if one, then `--` and the user and the host, so that no part
/// of the link reads as an option; `None` for any other link, or one of a
/// user or a host of another character than a name's, as a link that a
/// web page gives can hold anything.
fn ssh_args(link: &str) -> Option<Vec<String>> {
    if link.len() > MOST_LINK {
        return None;
    }
    let (scheme, rest) = link.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("ssh") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let (user, place) = match authority.rsplit_once('@') {
        // The parameters after a `;`, as a key's fingerprint, are not
        // `ssh`'s.
        Some((user, place)) => (Some(user.split(';').next()?), place),
        None => (None, authority),
    };
    let (host, port) = if let Some(inner) = place.strip_prefix('[') {
        // An IPv6 address, in brackets.
        let (address, after) = inner.split_once(']')?;
        let ipv6 = !address.is_empty()
            && address
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
        if !ipv6 {
            return None;
        }
        let port = match after {
            "" => None,
            after => Some(after.strip_prefix(':')?),
        };
        (address, port)
    } else {
        match place.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (place, None),
        }
    };
    let name = |text: &str| {
        !text.is_empty()
            && !text.starts_with(['-', '.'])
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    let named_host = host.contains(':') || name(host);
    if !named_host || user.is_some_and(|user| !name(user)) {
        return None;
    }
    let mut args = Vec::new();
    if let Some(port) = port {
        let number: u16 = port.parse().ok().filter(|&number| number > 0)?;
        args.extend(["-p".to_owned(), number.to_string()]);
    }
    args.push("--".to_owned());
    args.push(match user {
        Some(user) => format!("{user}@{host}"),
        None => host.to_owned(),
    });
    Some(args)
}

/// The man page of `link`, `x-man-page://[section/]name`, as a page's
/// title, as `ls(1)`, and `man`'s arguments; `None` for any other link, or
/// one of no page's name.
fn man_link(link: &str) -> Option<(String, Vec<String>)> {
    if link.len() > MOST_LINK {
        return None;
    }
    let (scheme, rest) = link.split_once(':')?;
    if !scheme.eq_ignore_ascii_case("x-man-page") {
        return None;
    }
    let rest = rest.trim_start_matches('/');
    let rest = rest.split(['?', '#']).next()?.trim_end_matches('/');
    let title = match rest.split_once('/') {
        Some((section, name)) => format!("{name}({section})"),
        None => rest.to_owned(),
    };
    let args = man_page(&title)?;
    Some((title, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ssh_link_gives_ssh_its_host_never_an_option() {
        let args = |link: &str| ssh_args(link);
        let given = |parts: &[&str]| Some(parts.iter().map(|&part| part.to_owned()).collect());
        assert_eq!(args("ssh://example.com"), given(&["--", "example.com"]));
        assert_eq!(
            args("ssh://ada@example.com:2222/"),
            given(&["-p", "2222", "--", "ada@example.com"])
        );
        assert_eq!(
            args("SSH://ada;fingerprint=ssh-rsa-xx@host"),
            given(&["--", "ada@host"])
        );
        assert_eq!(args("ssh://[::1]:22"), given(&["-p", "22", "--", "::1"]));
        assert_eq!(args("ssh://[fe80::1]"), given(&["--", "fe80::1"]));
        // What could read as an option, or a shell's, opens nothing.
        for wrong in [
            "ssh://-oProxyCommand=open%20-a%20Calculator",
            "ssh://-oProxyCommand=x@host",
            "ssh://host:-1",
            "ssh://host:0",
            "ssh://host:99999",
            "ssh://ho$t",
            "ssh://ada%20b@host",
            "ssh://.host",
            "ssh://[::1",
            "ssh://[::1]x",
            "ssh://[a b]",
            "ssh://",
            "ssh://@host",
            "http://host",
            "ssh:host",
        ] {
            assert_eq!(args(wrong), None, "{wrong}");
        }
        assert_eq!(args(&format!("ssh://{}", "a".repeat(MOST_LINK))), None);
    }

    #[test]
    fn a_man_page_s_link_gives_its_page() {
        let page = |link: &str| man_link(link).map(|(title, _)| title);
        assert_eq!(page("x-man-page://ls"), Some("ls".to_owned()));
        assert_eq!(page("x-man-page://1/open"), Some("open(1)".to_owned()));
        assert_eq!(page("x-man-page:///printf/"), Some("printf".to_owned()));
        assert_eq!(page("X-MAN-PAGE://3/printf"), Some("printf(3)".to_owned()));
        assert_eq!(
            man_link("x-man-page://1/open").map(|(_, args)| args),
            Some(vec!["--".to_owned(), "1".to_owned(), "open".to_owned()])
        );
        for wrong in [
            "x-man-page://-k",
            "x-man-page://ls;rm",
            "x-man-page://a/b/c",
            "x-man-page://",
            "ssh://ls",
        ] {
            assert_eq!(page(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn a_script_runs_of_its_path_in_quotes_or_of_its_interpreter() {
        let command = |path: &str, executable, named: &[&str]| {
            let named: Vec<String> = named.iter().map(|word| (*word).to_owned()).collect();
            script_command(Path::new(path), executable, &named)
        };
        assert_eq!(
            command("/Users/ada/run me.command", true, &[]).as_deref(),
            Ok("exec '/Users/ada/run me.command'")
        );
        assert_eq!(
            command("/tmp/it's.sh", true, &[]).as_deref(),
            Ok("exec '/tmp/it'\\''s.sh'")
        );
        // Not executable: of its first line's interpreter, or its
        // extension's.
        assert_eq!(
            command("/tmp/a.py", false, &["/usr/bin/env", "python3"]).as_deref(),
            Ok("exec '/usr/bin/env' 'python3' '/tmp/a.py'")
        );
        assert_eq!(
            command("/tmp/deploy.ZSH", false, &[]).as_deref(),
            Ok("exec 'zsh' '/tmp/deploy.ZSH'")
        );
        assert_eq!(
            command("/tmp/plain.sh", false, &[]).as_deref(),
            Ok("exec '/bin/sh' '/tmp/plain.sh'")
        );
        // Of no interpreter, or of characters that a shell reads otherwise.
        assert!(command("/tmp/notes", false, &[]).is_err());
        assert!(command("/tmp/a\\b.sh", true, &[]).is_err());
        assert!(command("/tmp/Install!.command", true, &[]).is_err());
        assert!(command("/tmp/a\nb.sh", true, &[]).is_err());
        assert!(command("/tmp/a.sh", false, &["/bin/sh;", "rm!"]).is_err());
    }

    #[test]
    fn a_script_s_first_line_names_its_interpreter() {
        let directory =
            std::env::temp_dir().join(format!("lokyr-first-line-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let with = directory.join("a.py");
        std::fs::write(&with, "#!/usr/bin/env python3 -u\nprint(1)\n").unwrap();
        assert_eq!(first_line(&with), ["/usr/bin/env", "python3", "-u"]);
        let without = directory.join("b.py");
        std::fs::write(&without, "print(1)\n").unwrap();
        assert!(first_line(&without).is_empty());
        assert!(first_line(&directory.join("gone")).is_empty());
        assert_eq!(interpreter_of(Path::new("x.Rb")), Some("ruby"));
        assert_eq!(interpreter_of(Path::new("x.txt")), None);
        let _ = std::fs::remove_dir_all(directory);
    }
}
