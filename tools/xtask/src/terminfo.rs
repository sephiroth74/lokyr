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
//! every capability of the source back, and that the entry is in the
//! legacy format, which every ncurses reads.

use std::collections::BTreeSet;
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
fn compile(source: &Path, out: &Path) -> Result<PathBuf> {
    let tic = PathBuf::from(std::env::var_os("TIC").unwrap_or_else(|| "tic".into()));
    let infocmp = tic.with_file_name("infocmp");
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let output = Command::new(&tic)
        .arg("-x")
        .arg("-o")
        .arg(out)
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
        .arg(out)
        .arg(ENTRY)
        .output()
        .with_context(|| format!("running {}", infocmp.display()))?;
    if !output.status.success() {
        bail!(
            "infocmp did not read {ENTRY} back:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    same_names(
        &names(&fs::read_to_string(source)?),
        &names(&String::from_utf8(output.stdout)?),
    )?;
    let compiled = compiled_file(out)?;
    let bytes = fs::read(&compiled)?;
    if bytes.get(..2) != Some(&LEGACY_MAGIC[..]) {
        bail!("{} is not in the legacy format", compiled.display());
    }
    Ok(compiled)
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
/// source.
fn same_names(written: &BTreeSet<String>, read_back: &BTreeSet<String>) -> Result<()> {
    if read_back != written {
        bail!(
            "infocmp reads back other capabilities: missing {:?}, added {:?}",
            written.difference(read_back).collect::<Vec<_>>(),
            read_back.difference(written).collect::<Vec<_>>()
        );
    }
    Ok(())
}

/// The names of the capabilities in `text`, a terminfo source or what
/// `infocmp` prints, without the entry's names and those cancelled.
fn names(text: &str) -> BTreeSet<String> {
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
    fields
        .into_iter()
        .skip(1)
        .filter_map(|field| {
            if field.contains(['=', '#']) {
                field.split(['=', '#']).next()
            } else if field.is_empty() || field.ends_with('@') {
                None
            } else {
                Some(field)
            }
        })
        .map(str::to_owned)
        .collect()
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
    fn a_warning_from_tic_fails_the_task() {
        let out = temp_dir("warning");
        fs::create_dir_all(&out).unwrap();
        // A description without spaces, which older versions would take
        // for another name.
        let source = out.join("warns.terminfo");
        fs::write(&source, "xterm-bolt|Bolt,\n\tam,\n").unwrap();
        let error = compile(&source, &out.join("compiled")).unwrap_err();
        assert!(error.to_string().contains("cleanly"), "{error:#}");
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn a_capability_lost_or_added_fails_the_task() {
        let set = |names: &[&str]| names.iter().map(|&name| name.to_owned()).collect();
        assert!(same_names(&set(&["am", "cols"]), &set(&["am", "cols"])).is_ok());
        let error = same_names(&set(&["am", "cols"]), &set(&["am", "bw"])).unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"infocmp reads back other capabilities: missing ["cols"], added ["bw"]"#
        );
    }

    #[test]
    fn names_leave_out_the_entry_and_cancelled_capabilities() {
        let text = "#\tA comment, with a comma\nxterm-bolt|Bolt terminal,\n\tam, bw@,\n\tcols#80, x=a\\,b, ich=\\E[%p1%d@,\n";
        let names: Vec<String> = names(text).into_iter().collect();
        assert_eq!(names, ["am", "cols", "ich", "x"]);
    }
}
