//! The packages' service: Sleip's commands of `--json`, which the app runs
//! for Lokyr's own plugins, each line that the command tells an event of
//! JSON, an object of its `event`, until the command ends.

use std::io::{BufRead, BufReader, Read as _};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use lokyr_plugin_sdk::host::Packages;
pub use serde_json::Value;
use serde_json::json;

/// The longest line of Sleip's that a job takes, in bytes: a longer one is
/// told as an error, as Sleip tells none so long.
pub const LONGEST_LINE: usize = 1 << 20;

/// The most bytes of Sleip's standard error that a job keeps, its last, to
/// tell why a command that told no error failed.
pub const KEPT_ERRORS: usize = 2048;

/// A job of the packages' service: Sleip's command of a call, whose lines
/// it reads, and the last of its standard error, which a thread reads.
pub struct Job {
    child: Arc<Mutex<Child>>,
    lines: BufReader<ChildStdout>,
    errors: Option<JoinHandle<Vec<u8>>>,
    stopped: Arc<AtomicBool>,
    told_error: bool,
}

/// What ends a job before its command does, from another thread, as the
/// user stops it.
#[derive(Clone)]
pub struct Stop(Arc<Mutex<Child>>, Arc<AtomicBool>);

impl Stop {
    /// Ends the job's command, if it runs: its lines end, and it ends of no
    /// success, of no error.
    pub fn stop(&self) {
        self.1.store(true, Ordering::Relaxed);
        let mut child = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = child.kill();
    }
}

impl Job {
    /// Starts `sleip`'s command of `call`, of `--json`, of the environment
    /// of the app and the variables of `env` beside it, its input of
    /// nothing, the last of its standard error kept.
    pub fn start(sleip: &Path, call: &Packages, env: &[(String, String)]) -> std::io::Result<Self> {
        let mut child = Command::new(sleip)
            .args(call.args())
            .envs(env.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("Sleip's output cannot be read"))?;
        let errors = child.stderr.take().map(|mut stderr| {
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                let mut part = [0; 4096];
                while let Ok(read) = stderr.read(&mut part) {
                    if read == 0 {
                        break;
                    }
                    kept.extend_from_slice(&part[..read]);
                    if kept.len() > KEPT_ERRORS {
                        kept.drain(..kept.len() - KEPT_ERRORS);
                    }
                }
                kept
            })
        });
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            lines: BufReader::new(stdout),
            errors,
            stopped: Arc::new(AtomicBool::new(false)),
            told_error: false,
        })
    }

    /// What ends it before its command does.
    pub fn stopper(&self) -> Stop {
        Stop(Arc::clone(&self.child), Arc::clone(&self.stopped))
    }

    /// The event of the next line that the command tells, an object of its
    /// `event`; an `error` of a line that is none, or past
    /// [`LONGEST_LINE`]; none once the command's output ends.
    pub fn next_line(&mut self) -> Option<Value> {
        let mut line = Vec::new();
        let read = (&mut self.lines)
            .take(LONGEST_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .ok()?;
        if read == 0 {
            return None;
        }
        if line.len() > LONGEST_LINE && !line.ends_with(b"\n") {
            // The rest of the line, to its end, goes.
            let mut rest = Vec::new();
            while (&mut self.lines)
                .take(LONGEST_LINE as u64)
                .read_until(b'\n', &mut rest)
                .is_ok_and(|read| read > 0)
                && !rest.ends_with(b"\n")
            {
                rest.clear();
            }
            return Some(error(&format!(
                "Sleip told a line past {LONGEST_LINE} bytes"
            )));
        }
        let event = match serde_json::from_slice::<Value>(&line) {
            Ok(event) if event.get("event").is_some_and(Value::is_string) => event,
            _ => error("Sleip told a line that is no event"),
        };
        self.told_error |= event["event"] == "error";
        Some(event)
    }

    /// Waits for the command's end, its lines read or not: whether it
    /// succeeded, and, of a failure that told no error and that no stop
    /// made, the error that tells it, of the last lines of Sleip's
    /// standard error or of its exit.
    pub fn finish(mut self) -> (bool, Option<Value>) {
        drop(self.lines);
        let status = {
            let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
            child.wait()
        };
        let errors = self
            .errors
            .take()
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();
        let ok = status.as_ref().is_ok_and(std::process::ExitStatus::success);
        if ok || self.told_error || self.stopped.load(Ordering::Relaxed) {
            return (ok, None);
        }
        let errors = String::from_utf8_lossy(&errors);
        let last: Vec<&str> = errors
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let last = last[last.len().saturating_sub(3)..].join(" ");
        let message = if !last.is_empty() {
            format!("Sleip failed: {last}")
        } else {
            match status.ok().and_then(|status| status.code()) {
                Some(code) => format!("Sleip ended of the exit code {code}, telling no error"),
                None => "Sleip ended of a signal, telling no error".to_owned(),
            }
        };
        (false, Some(error(&message)))
    }
}

/// The event of an error of `message`, as Sleip tells one.
pub fn error(message: &str) -> Value {
    json!({ "event": "error", "message": message })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Sleip of a shell's `script`, in a folder of the test's `name`.
    fn script(name: &str, script: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let folder =
            std::env::temp_dir().join(format!("lokyr-service-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("sleip");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_job_runs_sleip_s_command_of_its_call_and_reads_its_events() {
        let sleip = script(
            "lines",
            r#"echo "{\"event\":\"args\",\"args\":\"$*\",\"lokyr\":\"$SLEIP_LOKYR\"}"
echo 'not an event'
echo '{"no":"event"}'
echo '{"event":"done","changed":1}'
exit 0"#,
        );
        let call = Packages::Add {
            plugin: "acme/hello".to_owned(),
            constraint: Some("^0.2".to_owned()),
        };
        let env = [(
            "SLEIP_LOKYR".to_owned(),
            "/Applications/Lokyr.app".to_owned(),
        )];
        let mut job = Job::start(&sleip, &call, &env).unwrap();
        let first = job.next_line().unwrap();
        assert_eq!(first["event"], "args");
        assert_eq!(first["args"], "add --json acme/hello@^0.2");
        assert_eq!(first["lokyr"], "/Applications/Lokyr.app");
        for _ in 0..2 {
            let wrong = job.next_line().unwrap();
            assert_eq!(wrong["event"], "error");
            assert_eq!(wrong["message"], "Sleip told a line that is no event");
        }
        assert_eq!(job.next_line().unwrap()["changed"], 1);
        assert_eq!(job.next_line(), None);
        assert_eq!(job.finish(), (true, None));
    }

    #[test]
    fn a_job_that_fails_or_that_is_stopped_ends_of_no_success() {
        let failing = script(
            "failing",
            r#"echo '{"event":"error","message":"no"}'; exit 2"#,
        );
        let mut job = Job::start(&failing, &Packages::List, &[]).unwrap();
        assert_eq!(job.next_line().unwrap()["message"], "no");
        assert_eq!(job.next_line(), None);
        assert_eq!(job.finish(), (false, None));
        // A failure that told no error is told of its standard error's last
        // lines, or of its exit.
        let silent = script(
            "silent",
            "echo 'first' >&2; echo 'the store is locked' >&2; exit 3",
        );
        let mut job = Job::start(&silent, &Packages::List, &[]).unwrap();
        assert_eq!(job.next_line(), None);
        let (ok, told) = job.finish();
        assert!(!ok);
        assert_eq!(
            told.unwrap()["message"],
            "Sleip failed: first the store is locked"
        );
        let mute = script("mute", "exit 4");
        let job = Job::start(&mute, &Packages::List, &[]).unwrap();
        assert_eq!(
            job.finish().1.unwrap()["message"],
            "Sleip ended of the exit code 4, telling no error"
        );
        // Stopped as it waits, its lines end.
        // Of `exec`, so that the process stopped is the one that holds the
        // output, as no shell's child is left holding it.
        let waiting = script("waiting", "exec sleep 30");
        let mut job = Job::start(&waiting, &Packages::List, &[]).unwrap();
        let stop = job.stopper();
        let stopping = std::thread::spawn(move || stop.stop());
        assert_eq!(job.next_line(), None);
        // Stopped, of no error.
        assert_eq!(job.finish(), (false, None));
        stopping.join().unwrap();
        // A Sleip not there does not start.
        assert!(Job::start(Path::new("/no/sleip"), &Packages::List, &[]).is_err());
    }

    #[test]
    fn a_line_past_the_longest_is_an_error_and_the_next_reads() {
        let long = script(
            "long",
            &format!(
                "head -c {} /dev/zero | tr '\\\\0' a; echo; echo '{{\"event\":\"done\"}}'",
                LONGEST_LINE + 10
            ),
        );
        let mut job = Job::start(&long, &Packages::List, &[]).unwrap();
        let wrong = job.next_line().unwrap();
        assert_eq!(wrong["event"], "error");
        assert_eq!(job.next_line().unwrap()["event"], "done");
        assert_eq!(job.finish(), (true, None));
    }
}
