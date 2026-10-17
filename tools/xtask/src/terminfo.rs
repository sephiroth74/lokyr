//! `cargo xtask terminfo`: compiles the terminfo entry of Bolt with `tic`.
//!
//! The source is `crates/vt-core/terminfo/xterm-bolt.terminfo`, which
//! XTGETTCAP answers from too. The compiled entry goes to
//! `target/terminfo/`, or to the directory that `--out` gives, in the
//! layout of the `tic` that compiles it: one directory for each first
//! letter of a name, in hexadecimal on macOS. `tic` is the one on the
//! `PATH`, or the one that `TIC` names, and `infocmp` sits next to it.
//!
//! The task then checks that `tic` warned of nothing, that `infocmp` reads
//! every capability of the source back, with its value, and that the entry
//! is in the legacy format, which every ncurses reads.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

/// The source, relative to the workspace root.
const SOURCE: &str = "crates/vt-core/terminfo/xterm-bolt.terminfo";

/// The entry's name.
const ENTRY: &str = "xterm-bolt";

/// The magic number of the legacy format, octal 0432, little-endian; the
/// format for numbers above 32767 has another, which older libraries do
/// not read.
const LEGACY_MAGIC: [u8; 2] = [0x1A, 0x01];

/// Runs the task with the remaining command-line arguments.
pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let root = workspace_root();
    let mut out = root.join("target/terminfo");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().context("`--out` needs a directory")?.into(),
            other => bail!("unknown argument `{other}`"),
        }
    }
    let compiled = compile(&root.join(SOURCE), &out)?;
    println!("terminfo: {ENTRY} compiled into {}", compiled.display());
    Ok(ExitCode::SUCCESS)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives two levels below the workspace root")
        .to_path_buf()
}

/// Compiles `source` into the directory `out`, checks the entry, and
/// returns the compiled file.
///
/// `tic` warns that a name is defined twice when the entry is in its
/// directory already, so it compiles into a new directory, where the entry
/// is checked, and the entry's files then move into `out`, past the other
/// entries that `out` may hold.
fn compile(source: &Path, out: &Path) -> Result<PathBuf> {
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let staging = Staging(out.join(format!(".{ENTRY}-{}", std::process::id())));
    let _ = fs::remove_dir_all(&staging.0);
    fs::create_dir_all(&staging.0)?;
    check(source, &staging.0)?;
    for dir in fs::read_dir(&staging.0)? {
        let dir = dir?;
        for file in fs::read_dir(dir.path())? {
            let file = file?;
            let into = out.join(dir.file_name());
            fs::create_dir_all(&into)?;
            fs::rename(file.path(), into.join(file.file_name())).with_context(|| {
                format!("moving {} into {}", file.path().display(), into.display())
            })?;
        }
    }
    compiled_file(out)
}

/// A directory that is removed as it drops.
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Compiles `source` into `dir`, which holds no entry, and checks it.
fn check(source: &Path, dir: &Path) -> Result<()> {
    let tic = PathBuf::from(std::env::var_os("TIC").unwrap_or_else(|| "tic".into()));
    let infocmp = tic.with_file_name("infocmp");
    let output = Command::new(&tic)
        .arg("-x")
        .arg("-o")
        .arg(dir)
        .arg(source)
        .output()
        .with_context(|| format!("running {}", tic.display()))?;
    let warnings = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || !warnings.trim().is_empty() {
        bail!(
            "tic did not compile {} cleanly:\n{warnings}",
            source.display()
        );
    }
    let output = Command::new(&infocmp)
        .args(["-x", "-1", "-A"])
        .arg(dir)
        .arg(ENTRY)
        .output()
        .with_context(|| format!("running {}", infocmp.display()))?;
    if !output.status.success() {
        bail!(
            "infocmp did not read {ENTRY} back:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    same_capabilities(
        &capabilities(&fs::read_to_string(source)?)?,
        &capabilities(&String::from_utf8(output.stdout)?)?,
    )?;
    let compiled = compiled_file(dir)?;
    let bytes = fs::read(&compiled)?;
    if bytes.get(..2) != Some(&LEGACY_MAGIC[..]) {
        bail!("{} is not in the legacy format", ENTRY);
    }
    Ok(())
}

/// The compiled entry in `out`, under its first letter or the letter's
/// code in hexadecimal.
fn compiled_file(out: &Path) -> Result<PathBuf> {
    let first = ENTRY.as_bytes()[0];
    [format!("{}", char::from(first)), format!("{first:02x}")]
        .into_iter()
        .map(|dir| out.join(dir).join(ENTRY))
        .find(|path| path.is_file())
        .with_context(|| format!("no compiled {ENTRY} in {}", out.display()))
}

/// Fails unless the capabilities that `infocmp` reads back are those of the
/// source, with the same values.
fn same_capabilities(
    written: &BTreeMap<String, Value>,
    read_back: &BTreeMap<String, Value>,
) -> Result<()> {
    let missing: Vec<_> = written
        .keys()
        .filter(|name| !read_back.contains_key(*name))
        .collect();
    let added: Vec<_> = read_back
        .keys()
        .filter(|name| !written.contains_key(*name))
        .collect();
    let changed: Vec<_> = written
        .iter()
        .filter(|&(name, value)| read_back.get(name).is_some_and(|back| back != value))
        .map(|(name, _)| name)
        .collect();
    if !missing.is_empty() || !added.is_empty() || !changed.is_empty() {
        bail!(
            "infocmp reads back other capabilities: missing {missing:?}, added {added:?}, changed {changed:?}"
        );
    }
    Ok(())
}

/// What a capability holds, as `tic` reads it, whichever way it is written.
#[derive(Debug, PartialEq, Eq)]
enum Value {
    Flag,
    Number(u32),
    /// The string's bytes, its escapes undone, its parameters as they are.
    Text(Vec<u8>),
}

/// The capabilities in `text`, a terminfo source or what `infocmp` prints,
/// without the entry's names and those cancelled.
fn capabilities(text: &str) -> Result<BTreeMap<String, Value>> {
    let text: String = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let mut fields = Vec::new();
    let (mut start, mut at) = (0, 0);
    let bytes = text.as_bytes();
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 1,
            b',' => {
                fields.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    let mut capabilities = BTreeMap::new();
    for field in fields.into_iter().skip(1) {
        let (name, value) = if let Some((name, value)) = field.split_once('=') {
            (name, Value::Text(unescape(value)))
        } else if let Some((name, number)) = field.split_once('#') {
            (name, Value::Number(parse_number(number)?))
        } else if field.is_empty() || field.ends_with('@') {
            continue;
        } else {
            (field, Value::Flag)
        };
        capabilities.insert(name.to_owned(), value);
    }
    Ok(capabilities)
}

/// A number as terminfo(5) writes it: in decimal, or in hexadecimal after
/// `0x`, or in octal after `0`.
fn parse_number(text: &str) -> Result<u32> {
    let parsed = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else if text.len() > 1 && text.starts_with('0') {
        u32::from_str_radix(&text[1..], 8)
    } else {
        text.parse()
    };
    parsed.with_context(|| format!("`{text}` is not a number"))
}

/// The bytes of a string, its escapes undone, as terminfo(5) lists them:
/// `\\E` for ESC, `^X` for a control character, three octal digits, and a
/// backslash before a character that stands for itself.
fn unescape(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        at += 1;
        match byte {
            b'\\' => {
                let Some(&escaped) = bytes.get(at) else {
                    out.push(b'\\');
                    break;
                };
                at += 1;
                out.push(match escaped {
                    b'E' | b'e' => 0x1B,
                    b'n' | b'l' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'f' => 0x0C,
                    b's' => b' ',
                    b'0'..=b'7' => {
                        let digits = bytes[at - 1..]
                            .iter()
                            .take(3)
                            .take_while(|digit| matches!(digit, b'0'..=b'7'))
                            .count();
                        let octal = bytes[at - 1..at - 1 + digits]
                            .iter()
                            .fold(0_u8, |n, digit| {
                                n.wrapping_mul(8).wrapping_add(digit - b'0')
                            });
                        at += digits - 1;
                        // A string ends at a NUL, so `\\0` stands for 0x80.
                        if octal == 0 { 0x80 } else { octal }
                    }
                    other => other,
                });
            }
            b'^' => match bytes.get(at) {
                Some(&control) => {
                    at += 1;
                    out.push(if control == b'?' {
                        0x7F
                    } else {
                        control & 0x1F
                    });
                }
                None => out.push(b'^'),
            },
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bolt-terminfo-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_entry_compiles_cleanly_and_reads_back_whole() {
        let out = temp_dir("entry");
        let compiled = compile(&workspace_root().join(SOURCE), &out).unwrap();
        assert!(compiled.ends_with(ENTRY));
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn the_entry_compiles_again_into_a_directory_that_has_it() {
        let out = temp_dir("again");
        let source = workspace_root().join(SOURCE);
        let first = compile(&source, &out).unwrap();
        // Another entry, which a directory such as `~/.terminfo` holds.
        let other = first.with_file_name("other");
        fs::write(&other, b"another entry").unwrap();
        let again = compile(&source, &out).unwrap();
        assert_eq!(again, first);
        assert_eq!(fs::read(&other).unwrap(), b"another entry");
        let names: Vec<String> = fs::read_dir(&out)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|name| !name.starts_with('.')), "{names:?}");
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn a_warning_from_tic_fails_the_task() {
        let out = temp_dir("warning");
        fs::create_dir_all(&out).unwrap();
        // A description without spaces, which older versions would take
        // for another name.
        let source = out.join("warns.terminfo");
        fs::write(&source, "xterm-bolt|Bolt,\n\tam,\n").unwrap();
        let compiled = out.join("compiled");
        let error = compile(&source, &compiled).unwrap_err();
        assert!(error.to_string().contains("cleanly"), "{error:#}");
        // An entry that fails is not installed.
        assert_eq!(fs::read_dir(&compiled).unwrap().count(), 0);
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn a_capability_lost_added_or_changed_fails_the_task() {
        let read = |text: &str| capabilities(&format!("name|A terminal,\n\t{text}\n")).unwrap();
        let written = read("am, cols#80, x=\\E[1m,");
        assert!(same_capabilities(&written, &read("am, cols#80, x=\\E[1m,")).is_ok());
        assert!(
            same_capabilities(&written, &read("am, cols#80, x=\\E[2m,")).is_err(),
            "a value alone"
        );
        let error = same_capabilities(&written, &read("bw, cols#81, x=\\E[1m,")).unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"infocmp reads back other capabilities: missing ["am"], added ["bw"], changed ["cols"]"#
        );
    }

    #[test]
    fn capabilities_are_read_as_tic_reads_them() {
        let text = "#\tA comment, with a comma\nxterm-bolt|Bolt terminal,\n\tam, bw@,\n\tcols#80, it#010, lines#0x18, x=a\\,b, ich=\\E[%p1%d@,\n\ty=\\E[4\\:%p1%dm^G\\177,\n";
        let read = capabilities(text).unwrap();
        let names: Vec<&String> = read.keys().collect();
        assert_eq!(names, ["am", "cols", "ich", "it", "lines", "x", "y"]);
        assert_eq!(read["it"], Value::Number(8));
        assert_eq!(read["lines"], Value::Number(24));
        assert_eq!(read["x"], Value::Text(b"a,b".to_vec()));
        // An escaped colon, a control character and octal are their bytes,
        // however written.
        assert_eq!(read["y"], Value::Text(b"\x1b[4:%p1%dm\x07\x7f".to_vec()));
        assert_eq!(
            capabilities("n|N t,\n\ty=\\e[4:%p1%dm\\007^?,\n").unwrap()["y"],
            read["y"]
        );
        assert!(capabilities("n|N t,\n\tcols#eighty,\n").is_err());
    }
}
