//! Sleip's command line: its commands, their arguments, the flags that
//! every command takes, wherever they stand, and the texts of the help.

/// The flags that every command takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Events of NDJSON on the standard output, a JSON object a line, as
    /// the app reads them, in place of text.
    pub json: bool,
    /// The local cache and store alone, of no network.
    pub offline: bool,
    /// A failure where the lockfile would change.
    pub locked: bool,
}

/// A command and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// The installed plugins.
    List,
    /// A plugin removed, by its name.
    Remove(String),
    /// The packages that no lockfile names removed from the store.
    Gc,
    /// The installation diagnosed.
    Doctor,
}

/// What a command line asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// A text of help.
    Help(String),
    /// Sleip's version.
    Version,
    /// A command, of the flags.
    Run(Options, Command),
}

/// The help of Sleip, and of its commands.
const HELP: &str = "\
Sleip, the plugin package manager for Lokyr.

Usage: sleip [options] <command> [arguments]

Commands:
  list               List the installed plugins, their versions and capabilities
  remove <plugin>    Remove a plugin, as owner/name; its package stays until `gc`
  gc                 Remove the packages of the store that no lockfile names
  doctor             Diagnose the store, the lockfile, the cache and the app's plugin API
  help [command]     Show this help, or a command's

Options:
  --json             Write events of NDJSON, a JSON object a line, in place of text
  --offline          Use the local cache and store alone
  --locked           Fail where the lockfile would change
  -h, --help         Show the help
  -V, --version      Show Sleip's version
";

/// The help of the command `name`, if it is one.
fn command_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "list" => {
            "Usage: sleip list [options]\n\nLists the plugins of plugins.lock: each one's version, kind, source and\ncapabilities.\n"
        }
        "remove" => {
            "Usage: sleip remove [options] <plugin>\n\nRemoves the plugin, as owner/name, from plugins.toml and plugins.lock,\nunless another plugin needs it. Its package stays in the store until\n`sleip gc`; the decisions on its capabilities stay in permissions.toml.\n"
        }
        "gc" => {
            "Usage: sleip gc [options]\n\nRemoves the packages of the store that plugins.lock does not name, and what\nan install that stopped left there.\n"
        }
        "doctor" => {
            "Usage: sleip doctor [options]\n\nChecks the config's folder, plugins.toml, plugins.lock and permissions.toml,\neach package of the store against its digest, the folders that `sleip dev`\nlinks, the cache, and the app's plugin API against each plugin's.\n"
        }
        _ => return None,
    })
}

/// What `args`, the command line past the program's name, asks; or the
/// text of why it is wrong.
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut options = Options::default();
    let (mut words, mut help, mut version) = (Vec::new(), false, false);
    for arg in args {
        match arg.as_str() {
            "--json" => options.json = true,
            "--offline" => options.offline = true,
            "--locked" => options.locked = true,
            "-h" | "--help" => help = true,
            "-V" | "--version" => version = true,
            flag if flag.starts_with('-') => {
                return Err(format!("`{flag}` is no option of Sleip's\n\n{HELP}"));
            }
            word => words.push(word),
        }
    }
    if version && words.is_empty() {
        return Ok(Parsed::Version);
    }
    let help_of = |name: Option<&str>| match name {
        None => Ok(Parsed::Help(HELP.to_owned())),
        Some(name) => command_help(name)
            .map(|text| Parsed::Help(text.to_owned()))
            .ok_or_else(|| format!("`{name}` is no command of Sleip's\n\n{HELP}")),
    };
    match words.as_slice() {
        [] => {
            if help {
                help_of(None)
            } else {
                Err(format!("a command is missing\n\n{HELP}"))
            }
        }
        ["help"] => help_of(None),
        ["help", name] => help_of(Some(name)),
        [name, ..] if help => help_of(Some(name)),
        ["list"] => Ok(Parsed::Run(options, Command::List)),
        ["remove", plugin] => Ok(Parsed::Run(options, Command::Remove((*plugin).to_owned()))),
        ["gc"] => Ok(Parsed::Run(options, Command::Gc)),
        ["doctor"] => Ok(Parsed::Run(options, Command::Doctor)),
        [name, ..] => match command_help(name) {
            Some(text) => Err(format!("`{name}` takes other arguments\n\n{text}")),
            None => Err(format!("`{name}` is no command of Sleip's\n\n{HELP}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> Result<Parsed, String> {
        let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        parse(&args)
    }

    #[test]
    fn the_flags_stand_anywhere_and_each_command_takes_its_arguments() {
        let all = Options {
            json: true,
            offline: true,
            locked: true,
        };
        assert_eq!(
            parsed("--json remove acme/x --offline --locked"),
            Ok(Parsed::Run(all, Command::Remove("acme/x".to_owned())))
        );
        assert_eq!(
            parsed("list"),
            Ok(Parsed::Run(Options::default(), Command::List))
        );
        assert_eq!(parsed("gc --json").map(|_| ()), Ok(()));
        assert_eq!(parsed("-V"), Ok(Parsed::Version));
        assert_eq!(parsed("--help"), Ok(Parsed::Help(HELP.to_owned())));
        assert_eq!(parsed("help"), Ok(Parsed::Help(HELP.to_owned())));
        assert!(
            matches!(parsed("help gc"), Ok(Parsed::Help(text)) if text.starts_with("Usage: sleip gc"))
        );
        assert!(
            matches!(parsed("doctor -h"), Ok(Parsed::Help(text)) if text.starts_with("Usage: sleip doctor"))
        );
        for wrong in [
            "",
            "remove",
            "remove a b",
            "list x",
            "fly",
            "--fast list",
            "help fly",
        ] {
            assert!(parsed(wrong).is_err(), "{wrong}");
        }
    }
}
