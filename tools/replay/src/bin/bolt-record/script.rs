//! The scripts that drive a recording: one command a line, its arguments
//! words or quoted strings, `#` starting a comment line.
//!
//! ```text
//! size 80 24
//! copy files/text.txt
//! env LESSHISTFILE=-
//! run less text.txt
//! wait "text.txt"
//! mark start
//! send " "
//! quiet 300
//! sleep 1500
//! send "q"
//! exit
//! ```

use std::time::Duration;

use anyhow::{Context, Result, bail};

/// How long a wait lasts, unless the script says.
const WAIT: Duration = Duration::from_secs(10);

/// A script, parsed.
#[derive(Debug, PartialEq, Eq)]
pub struct Script {
    pub cols: u16,
    pub rows: u16,
    /// Files to copy into the program's working directory, relative to the
    /// script's.
    pub copies: Vec<String>,
    /// Variables to set, besides those of every recording.
    pub env: Vec<(String, String)>,
    /// The program and its arguments.
    pub run: Vec<String>,
    pub steps: Vec<Step>,
}

/// What a script does once the program runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Waits until a row of the screen shows the text, and no synchronized
    /// update is under way, for so long at most.
    Wait(String, Duration),
    /// Waits until the program writes nothing for so long, with no
    /// synchronized update under way, for as long as the second says at
    /// most.
    Quiet(Duration, Duration),
    /// Lets the program run for so long, as one that redraws all the time
    /// is never quiet.
    Sleep(Duration),
    /// Types the bytes, once the program takes them, for so long at most.
    Send(Vec<u8>, Duration),
    /// Marks the moment, once no synchronized update is under way, for tests
    /// to look at the screen there, by a label of its own: not `exit`, which
    /// the end takes, and without `/`, as the tests' snapshots are named
    /// after them.
    Mark(String),
    /// Waits for the program's output to end, for so long at most.
    Exit(Duration),
}

impl Script {
    pub fn parse(text: &str) -> Result<Self> {
        let mut script = Self {
            cols: 80,
            rows: 24,
            copies: Vec::new(),
            env: Vec::new(),
            run: Vec::new(),
            steps: Vec::new(),
        };
        for (number, line) in (1..).zip(text.lines()) {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            script
                .add(&words(line)?)
                .with_context(|| format!("line {number}: `{line}`"))?;
        }
        if script.run.is_empty() {
            bail!("the script runs nothing: it needs a `run` line");
        }
        Ok(script)
    }

    fn add(&mut self, words: &[String]) -> Result<()> {
        let (command, args) = words.split_first().context("an empty command")?;
        let one = || match args {
            [arg] => Ok(arg.as_str()),
            _ => bail!("`{command}` takes one argument"),
        };
        let millis = |arg: &str| -> Result<Duration> {
            Ok(Duration::from_millis(arg.parse().context("milliseconds")?))
        };
        let wait = |args: &[String]| match args {
            [] => Ok(WAIT),
            [limit] => millis(limit),
            _ => bail!("`{command}` takes a limit in milliseconds at most"),
        };
        if !self.run.is_empty() && matches!(command.as_str(), "size" | "copy" | "env" | "run") {
            bail!("`{command}` comes before `run`");
        }
        if self.run.is_empty() && !matches!(command.as_str(), "size" | "copy" | "env" | "run") {
            bail!("`{command}` comes after `run`");
        }
        match command.as_str() {
            "size" => {
                let [cols, rows] = args else {
                    bail!("`size` takes the columns and the rows");
                };
                self.cols = cols.parse().context("columns")?;
                self.rows = rows.parse().context("rows")?;
                if self.cols == 0 || self.rows == 0 {
                    bail!("a screen has a column and a row at least");
                }
            }
            "copy" => self.copies.push(one()?.to_owned()),
            "env" => {
                let (name, value) = one()?.split_once('=').context("`env` takes NAME=VALUE")?;
                self.env.push((name.to_owned(), value.to_owned()));
            }
            "run" => {
                if args.is_empty() {
                    bail!("`run` takes a program");
                }
                self.run = args.to_vec();
            }
            "wait" => {
                let (text, rest) = args.split_first().context("`wait` takes the text")?;
                self.steps.push(Step::Wait(text.clone(), wait(rest)?));
            }
            "quiet" => {
                let (time, rest) = args.split_first().context("`quiet` takes a time")?;
                self.steps.push(Step::Quiet(millis(time)?, wait(rest)?));
            }
            "sleep" => self.steps.push(Step::Sleep(millis(one()?)?)),
            "send" => {
                let (text, rest) = args.split_first().context("`send` takes the keys")?;
                self.steps
                    .push(Step::Send(text.as_bytes().to_vec(), wait(rest)?));
            }
            "mark" => {
                let label = one()?;
                let taken = self
                    .steps
                    .iter()
                    .any(|step| matches!(step, Step::Mark(other) if other == label));
                if label.is_empty() || label == "exit" || label.contains('/') || taken {
                    bail!("a mark has a label of its own, which is not `exit` and has no `/`");
                }
                self.steps.push(Step::Mark(label.to_owned()));
            }
            "exit" => self.steps.push(Step::Exit(wait(args)?)),
            other => bail!("unknown command `{other}`"),
        }
        Ok(())
    }
}

/// The words of `line`: runs of characters without spaces, or strings in
/// double quotes, with the escapes `\e`, `\r`, `\n`, `\t`, `\\`, `\"` and
/// `\xHH`.
fn words(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word = String::new();
        if c == '"' {
            chars.next();
            loop {
                match chars.next().context("a string that does not end")? {
                    '"' => break,
                    '\\' => {
                        word.push(match chars.next().context("an escape that does not end")? {
                            'e' => '\x1b',
                            'r' => '\r',
                            'n' => '\n',
                            't' => '\t',
                            'x' => {
                                let hex: String = chars.by_ref().take(2).collect();
                                let byte = u8::from_str_radix(&hex, 16)
                                    .with_context(|| format!("`\\x{hex}` is not a byte"))?;
                                if !byte.is_ascii() {
                                    bail!("`\\x{hex}` is past ASCII: write the character itself");
                                }
                                char::from(byte)
                            }
                            c @ ('\\' | '"') => c,
                            other => bail!("unknown escape `\\{other}`"),
                        });
                    }
                    c => word.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
        }
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_sets_the_program_up_then_drives_it() {
        let script = Script::parse(concat!(
            "# A comment.\n",
            "size 100 30\n",
            "copy files/a.txt\n",
            "env LESS=\n",
            "run sh -c \"printf 'a b'; read x\"\n",
            "\n",
            "wait \"a b\" 500\n",
            "mark start\n",
            "send \"\\e[A\\x03\\\"\\\\\\r\"\n",
            "quiet 200\n",
            "quiet 100 900\n",
            "sleep 50\n",
            "exit\n",
        ))
        .unwrap();
        assert_eq!(
            script,
            Script {
                cols: 100,
                rows: 30,
                copies: vec!["files/a.txt".to_owned()],
                env: vec![("LESS".to_owned(), String::new())],
                run: ["sh", "-c", "printf 'a b'; read x"]
                    .map(String::from)
                    .to_vec(),
                steps: vec![
                    Step::Wait("a b".to_owned(), Duration::from_millis(500)),
                    Step::Mark("start".to_owned()),
                    Step::Send(b"\x1b[A\x03\"\\\r".to_vec(), WAIT),
                    Step::Quiet(Duration::from_millis(200), WAIT),
                    Step::Quiet(Duration::from_millis(100), Duration::from_millis(900)),
                    Step::Sleep(Duration::from_millis(50)),
                    Step::Exit(WAIT),
                ],
            }
        );
    }

    #[test]
    fn mistakes_say_where_they_are() {
        for (text, error) in [
            ("wait x\n", "line 1: `wait x`: `wait` comes after `run`"),
            (
                "run a\nsize 1 1\n",
                "line 2: `size 1 1`: `size` comes before `run`",
            ),
            ("run a\nsend \"x\n", "a string that does not end"),
            ("run a\nsend \"\\q\"\n", "unknown escape `\\q`"),
            ("run a\nsend \"\\xff\"\n", "past ASCII"),
            ("run a\nquiet soon\n", "milliseconds"),
            ("run a\njump\n", "unknown command `jump`"),
            ("size 0 1\nrun a\n", "a column and a row"),
            ("size 80 24\n", "it needs a `run` line"),
            ("run a\nmark exit\n", "a label of its own"),
            ("run a\nmark a/b\n", "a label of its own"),
            ("run a\nmark x\nmark x\n", "a label of its own"),
            ("run a\nsend x soon\n", "milliseconds"),
        ] {
            let message = format!("{:#}", Script::parse(text).unwrap_err());
            assert!(message.contains(error), "{text:?}: {message}");
        }
    }
}
