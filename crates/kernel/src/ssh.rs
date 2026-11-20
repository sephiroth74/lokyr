//! The ssh sessions that the shells' integration wraps. Of the terminal's
//! own `TERM`, `xterm-lokyr`, which a host lacks until its terminfo entry is
//! installed there, an interactive session of a host that has not got it
//! yet installs it first, of `tic` on the host, and the host is remembered
//! in the state's folder, as one that got it or one where `tic` failed; a
//! session of a host where it failed, that the config leaves out, or that
//! does not resolve, goes of `xterm-256color`, which every host knows. Any
//! other ssh, of another `TERM`, of a remote command or of no terminal,
//! goes as it is. The shells' `ssh` asks `lokyr +ssh-term` the `TERM` of
//! its session, which [`term`] tells.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use lokyr_vt_core::TERMINFO_SOURCE;

use crate::config::TerminalConfig;

/// The terminal's own `TERM`s, the names of its entry, which a host needs.
pub const OWN_TERMS: [&str; 2] = ["xterm-lokyr", "lokyr"];

/// The `TERM` of a session of a host that lacks the terminal's own entry.
pub const FALLBACK_TERM: &str = "xterm-256color";

/// The file of the hosts that got the entry or where it failed, in the
/// state's folder, a line each: the [`Outcome`]'s word and the host's
/// [`Target::key`].
pub const HOSTS_FILE: &str = "ssh-terminfo-hosts";

/// The most hosts that the file keeps, past which the oldest go.
const MOST_HOSTS: usize = 4096;

/// The command that installs the entry on a host, of its source on the
/// standard input: `tic` of its extended capabilities, into the user's
/// own `~/.terminfo` where the system's folder is not theirs, run by `sh`
/// whatever the user's shell there.
pub const INSTALL_COMMAND: &str = "sh -c 'tic -x - >/dev/null 2>&1'";

/// ssh's exit status of a connection that failed, or of an error of its own.
pub const CONNECTION_FAILED: i32 = 255;

/// ssh's options that take an argument, as OpenSSH's have them.
const WITH_ARGUMENT: &str = "BbcDEeFIiJLlmOoPpQRSWw";

/// ssh's options of no interactive session: no command (`-N`), no terminal
/// (`-T`), a forward of the standard streams (`-W`), the config printed
/// (`-G`), the version (`-V`), a control command (`-O`), the background
/// (`-f`), a subsystem (`-s`) and a query (`-Q`).
const NO_SESSION: &str = "NTWGVOfsQ";

/// What a shell's `ssh` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// ssh as it is.
    Plain,
    /// ssh of [`FALLBACK_TERM`].
    Fallback,
    /// The entry installed on the host of the key, then ssh as it is; of
    /// [`FALLBACK_TERM`] if the install fails.
    Install(String),
}

/// How the install of the entry on a host went, which the file of the
/// hosts keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The host got the entry.
    Installed,
    /// The host could not take it, as of no `tic` or no folder to write:
    /// its sessions go of [`FALLBACK_TERM`].
    Failed,
}

impl Outcome {
    /// The word of its line in the file of the hosts.
    fn word(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Failed => "failed",
        }
    }

    /// The outcome of the word `word`.
    fn of_word(word: &str) -> Option<Self> {
        match word {
            "installed" => Some(Self::Installed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// The outcome of the install's ssh that ended of `status`, its exit
    /// code, or none of a signal: the host got the entry of 0; none of a
    /// connection that failed or a signal, which tells nothing of the host;
    /// else the host could not take it.
    pub fn of_status(status: Option<i32>) -> Option<Self> {
        match status? {
            0 => Some(Self::Installed),
            CONNECTION_FAILED => None,
            _ => Some(Self::Failed),
        }
    }
}

/// A host as ssh's config resolves it: its name, its user and its port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The host's name, as `HostName` gives it.
    pub hostname: String,
    /// The user on the host.
    pub user: String,
    /// The port.
    pub port: String,
}

impl Target {
    /// The key that the file of the hosts keeps it by: `user@host:port`.
    pub fn key(&self) -> String {
        format!("{}@{}:{}", self.user, self.hostname, self.port)
    }

    /// The host of the printed config of `ssh -G`, `output`: its
    /// `hostname`, `user` and `port` lines; none of an output that lacks
    /// one.
    pub fn of_config(output: &str) -> Option<Self> {
        let value = |name: &str| {
            output.lines().find_map(|line| {
                let (key, value) = line.split_once(' ')?;
                (key == name).then(|| value.trim().to_owned())
            })
        };
        Some(Self {
            hostname: value("hostname").filter(|name| !name.is_empty())?,
            user: value("user").filter(|user| !user.is_empty())?,
            port: value("port").filter(|port| !port.is_empty())?,
        })
    }
}

/// The index in ssh's `args` of the destination of an interactive session,
/// which a terminal shows: one destination, after or among the options, of
/// no remote command after it nor any of [`NO_SESSION`]; none of any other
/// ssh. An option of [`WITH_ARGUMENT`] takes the rest of its word, or the
/// next one; `--` ends the options.
pub fn session(args: &[String]) -> Option<usize> {
    let mut destination = None;
    let mut options = true;
    let mut at = 0;
    while at < args.len() {
        let arg = &args[at];
        at += 1;
        if options && arg == "--" {
            options = false;
            continue;
        }
        if let Some(flags) = arg
            .strip_prefix('-')
            .filter(|flags| options && !flags.is_empty())
        {
            let flags: Vec<char> = flags.chars().collect();
            for (place, flag) in flags.iter().enumerate() {
                if NO_SESSION.contains(*flag) {
                    return None;
                }
                if WITH_ARGUMENT.contains(*flag) {
                    // Its argument, the rest of its word or the next.
                    if place + 1 == flags.len() {
                        at += 1;
                    }
                    break;
                }
            }
            continue;
        }
        // A second word past the options is a remote command.
        if destination.is_some() {
            return None;
        }
        destination = Some(at - 1);
    }
    destination.filter(|&at| !args[at].is_empty())
}

/// Whether `text` matches `pattern`, of `*` for any characters and `?` for
/// one, whatever their case, as a host's name is.
pub fn matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    // The last `*` met, and the text's place it took from, which a
    // mismatch takes one character further.
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((at, from)) => {
                    p = at + 1;
                    t = from + 1;
                    star = Some((at, from + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// What a shell's `ssh` of `args` does, of the `TERM` that its session
/// has, `term`, and the config's `terminal`: as it is, unless the
/// integration is on, `term` is one of [`OWN_TERMS`] and `args` open an
/// interactive session; then of [`FALLBACK_TERM`] for a host that
/// `resolve` does not resolve, that `ssh-hosts-ignored` names, by the
/// destination as typed or its host's name, or where the install failed,
/// as `outcome` tells; as it is for a host that got the entry; else the
/// entry installed first.
pub fn plan(
    args: &[String],
    term: &str,
    terminal: &TerminalConfig,
    outcome: impl Fn(&str) -> Option<Outcome>,
    resolve: impl FnOnce(&[String]) -> Option<Target>,
) -> Plan {
    if !terminal.ssh_integration || !OWN_TERMS.contains(&term) {
        return Plan::Plain;
    }
    let Some(destination) = session(args) else {
        return Plan::Plain;
    };
    let Some(target) = resolve(args) else {
        return Plan::Fallback;
    };
    let typed = &args[destination];
    let typed_host = typed
        .rsplit_once('@')
        .map_or(typed.as_str(), |(_, host)| host);
    let ignored = terminal
        .ssh_hosts_ignored
        .iter()
        .any(|pattern| matches(pattern, typed_host) || matches(pattern, &target.hostname));
    if ignored {
        return Plan::Fallback;
    }
    let key = target.key();
    match outcome(&key) {
        Some(Outcome::Installed) => Plan::Plain,
        Some(Outcome::Failed) => Plan::Fallback,
        None => Plan::Install(key),
    }
}

/// The arguments of the ssh that installs the entry, of the session's
/// `args`: no terminal, none of the config's remote commands, and
/// [`INSTALL_COMMAND`] as its command. The options before the session's
/// win, as ssh takes the first value of each.
pub fn install_args(args: &[String]) -> Vec<String> {
    let mut install: Vec<String> = ["-T", "-o", "RemoteCommand=none"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    install.extend(args.iter().cloned());
    install.push(INSTALL_COMMAND.to_owned());
    install
}

/// The hosts of the file `path`, each of its outcome, in the file's order;
/// none of a file that does not read, and no line of another form.
pub fn hosts(path: &Path) -> Vec<(Outcome, String)> {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter_map(|line| {
                    let (word, key) = line.split_once(' ')?;
                    Some((Outcome::of_word(word)?, key.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The outcome that the file `path` keeps of the host of `key`.
pub fn outcome(path: &Path, key: &str) -> Option<Outcome> {
    hosts(path)
        .into_iter()
        .find_map(|(outcome, kept)| (kept == key).then_some(outcome))
}

/// Remembers `outcome` of the host of `key` in the file `path`, its folder
/// made, written whole: in place of what it kept of that host, of a key of
/// no line break, [`MOST_HOSTS`] at most, the oldest going.
pub fn remember(path: &Path, key: &str, outcome: Outcome) -> std::io::Result<()> {
    if key.is_empty() || key.contains(['\n', '\r']) {
        return Ok(());
    }
    let mut hosts = hosts(path);
    hosts.retain(|(_, kept)| kept != key);
    hosts.push((outcome, key.to_owned()));
    let from = hosts.len().saturating_sub(MOST_HOSTS);
    let text: String = hosts[from..]
        .iter()
        .map(|(outcome, key)| format!("{} {key}\n", outcome.word()))
        .collect();
    crate::state::write_whole(path, text.as_bytes())
}

/// The install's ssh failed to connect: it told why, and the session would
/// fail as it did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionFailed;

/// The `TERM` of the session of ssh's `args`, of the `TERM` that the shell
/// has, `term`, the config's `terminal` and the file of the hosts, `hosts`:
/// `term`, or [`FALLBACK_TERM`], as [`plan`] tells; a host to install on
/// gets the terminal's entry first, through `ssh` of [`install_args`],
/// which asks for a password or a host's key on the terminal as a session
/// does, and its outcome remembered. A connection that fails is
/// [`ConnectionFailed`].
pub fn term(
    args: &[String],
    term: &str,
    terminal: &TerminalConfig,
    hosts: Option<&Path>,
) -> Result<String, ConnectionFailed> {
    let plan = plan(
        args,
        term,
        terminal,
        |key| hosts.and_then(|path| outcome(path, key)),
        resolve,
    );
    let fallback = || Ok(FALLBACK_TERM.to_owned());
    match plan {
        Plan::Plain => Ok(term.to_owned()),
        Plan::Fallback => fallback(),
        Plan::Install(key) => match Outcome::of_status(install(args, TERMINFO_SOURCE)) {
            None => Err(ConnectionFailed),
            Some(outcome) => {
                if let Some(path) = hosts {
                    // A file that cannot be written asks again next time.
                    let _ = remember(path, &key, outcome);
                }
                match outcome {
                    Outcome::Installed => Ok(term.to_owned()),
                    Outcome::Failed => fallback(),
                }
            }
        },
    }
}

/// The host of ssh's `args`, as `ssh -G` prints its config.
fn resolve(args: &[String]) -> Option<Target> {
    let output = Command::new("ssh")
        .arg("-G")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Target::of_config(&String::from_utf8_lossy(&output.stdout))
}

/// Installs the entry of `source` on the host of `args`: the exit code of
/// its ssh, none of a signal, or the connection's failure of an ssh that
/// does not start. Its errors go to the terminal, its output nowhere.
fn install(args: &[String], source: &str) -> Option<i32> {
    let child = Command::new("ssh")
        .args(install_args(args))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return Some(CONNECTION_FAILED);
    };
    if let Some(mut input) = child.stdin.take() {
        // A host that closes its input early tells it by the exit code.
        let _ = input.write_all(source.as_bytes());
    }
    child
        .wait()
        .map_or(Some(CONNECTION_FAILED), |status| status.code())
}

#[cfg(test)]
mod tests;
