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
    /// A plugin made, by its name.
    New(String),
    /// The plugin of the current folder checked.
    Check,
    /// The tests of the plugin of the current folder run.
    Test,
    /// The plugin of the current folder linked into the app's plugins.
    Dev,
    /// A plugin added, of its name if given, from a folder or a git
    /// repository.
    Add(Option<String>, lokyr_pkg_model::list::Wanted),
    /// The installed plugins updated, or the one of the name given.
    Update(Option<String>),
    /// The installed plugins.
    List,
    /// A plugin removed, by its name.
    Remove(String),
    /// The packages that no lockfile names removed from the store.
    Gc,
    /// The installation diagnosed.
    Doctor,
    /// The registry's plugins that every word finds.
    Search(Vec<String>),
    /// A plugin of the registry told, by its name.
    Info(String),
    /// One of the registry's tools, which its CI runs, hidden from the help.
    Registry(Registry),
}

/// A tool of the registry's, of `sleip registry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registry {
    /// A pull request of the registry checked, of its files in `head`
    /// against those of the branch that it merges into in `base`, opened by
    /// `author`, a public member of `orgs`: each plugin's version that it
    /// publishes packaged into `out`, checked and tested, and whether it
    /// merges by itself, written to `report` too.
    Verify {
        head: std::path::PathBuf,
        base: std::path::PathBuf,
        author: String,
        orgs: Vec<String>,
        out: std::path::PathBuf,
        report: Option<std::path::PathBuf>,
        /// Repositories of `file://` taken, as the tests' are.
        local: bool,
        /// The files judged alone, no version cloned, checked, tested nor
        /// packaged, as the job that merges checks again.
        files_alone: bool,
    },
    /// The archive of the version `version` of the plugin `plugin`, of the
    /// registry in `registry`, packaged into `out`.
    Build {
        plugin: String,
        version: String,
        registry: std::path::PathBuf,
        out: std::path::PathBuf,
        local: bool,
    },
    /// Every version of the registry in `registry`, of its release's names.
    Versions { registry: std::path::PathBuf },
    /// A new key of ed25519, its secret written to a new file `out` that no
    /// one else reads, its public part told.
    Keygen { out: std::path::PathBuf },
    /// `keys.json` of the index keys `keys`, signed by the root key of the
    /// file `root`, written to `out`.
    SignKeys {
        root: std::path::PathBuf,
        keys: Vec<String>,
        out: std::path::PathBuf,
    },
    /// The index of the registry in `registry`, of the archives of its
    /// releases in `archives`, signed by the index key that
    /// `SLEIP_INDEX_KEY` holds, written into `out` with `config.json` and
    /// the registry's `keys.json`, which the root keys of the file `roots`
    /// sign, or Sleip's own; its archives' address of `dl`, or the
    /// registry's.
    Index {
        registry: std::path::PathBuf,
        archives: std::path::PathBuf,
        out: std::path::PathBuf,
        roots: Option<std::path::PathBuf>,
        dl: Option<String>,
    },
    /// The registry's site of its index in `out`, which `index` wrote, of
    /// the registry in `registry` and of the archives of its releases in
    /// `archives`, its keys checked against the root keys of the file
    /// `roots`, or Sleip's own.
    Site {
        registry: std::path::PathBuf,
        archives: std::path::PathBuf,
        out: std::path::PathBuf,
        roots: Option<std::path::PathBuf>,
    },
    /// The list of the plugins of the README `readme`, the registry's
    /// `README.md` if none, of the index in `index`, of the registry in
    /// `registry` and of the archives of its releases in `archives`, its
    /// keys checked against the root keys of the file `roots`, or
    /// Sleip's own.
    Readme {
        registry: std::path::PathBuf,
        archives: std::path::PathBuf,
        index: std::path::PathBuf,
        readme: Option<std::path::PathBuf>,
        roots: Option<std::path::PathBuf>,
    },
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
  new <plugin>       Make a plugin of Luau, as owner/name, in a folder of its name
  check              Check the plugin of the current folder: its manifest, texts and types
  test               Run the tests of the plugin of the current folder, tests/*.luau
  dev                Link the plugin of the current folder into Lokyr, reloaded at each save
  add [<plugin>] --path <folder> | --git <url> (--tag <tag> | --rev <commit>)
                     Add a plugin from a folder or a git repository, and install it
  update [<plugin>]  Fetch the installed plugins' folders and repositories again
  list               List the installed plugins, their versions and capabilities
  remove <plugin>    Remove a plugin, as owner/name; its package stays until `gc`
  gc                 Remove the packages of the store that no lockfile names
  doctor             Diagnose the store, the lockfile, the cache and the app's plugin API
  search <words>     Search the registry's plugins, of their names and descriptions
  info <plugin>      Tell a plugin of the registry: its versions, capabilities and checksums
  help [command]     Show this help, or a command's

Options:
  --json             Write events of NDJSON, a JSON object a line, in place of text
  --offline          Use the local cache and store alone
  --locked           Fail where the lockfile would change
  -h, --help         Show the help
  -V, --version      Show Sleip's version
";

/// The help of `sleip registry`, which Sleip's own help leaves out.
const REGISTRY_HELP: &str = "\
Usage: sleip [options] registry verify <head> --base <folder> --author <login> [--orgs <org,...>] --out <folder> [--report <file>] [--files-alone]
       sleip [options] registry build <owner/name> <version> [--registry <folder>] --out <folder>
       sleip [options] registry versions [--registry <folder>]
       sleip [options] registry keygen --out <file>
       sleip [options] registry sign-keys --root <file> --keys <key,...> --out <file>
       sleip [options] registry index [--registry <folder>] --archives <folder> --out <folder> [--roots <file>] [--dl <address>]
       sleip [options] registry site [--registry <folder>] --archives <folder> --out <folder> [--roots <file>]
       sleip [options] registry readme [--registry <folder>] --archives <folder> --index <folder> [--readme <file>] [--roots <file>]

The registry's tools, which its CI runs.
  verify    Checks a pull request of the registry, a git repository at its head in <head>,
            against the branch that it merges into, one at its head in <base>, each of its whole
            history: the changes of the pull request since the commit that both share, each
            judged against <base>; the plugins' files that it changes, their owner the author
            or one of its organizations, their names, the versions published left as they were,
            and each new version's tag, manifest, check and tests, its archive packaged into
            <out>; whether it merges by itself, or the maintainer reviews it, and a report of
            Markdown in <report>. A failure of a pull request refused. Of --files-alone, the
            files alone, no version cloned, checked, tested nor packaged, as the job that
            merges checks again.
  build     Packages the archive of a version of the registry's plugin into <out>, of the
            commit that the registry's file pins, whatever became of its tag.
  versions  Lists every version of the registry, of its release's tag and archive.
  keygen    Makes a key of ed25519: its secret in the new file <out>, which no one else
            reads, and its public part told.
  sign-keys Writes keys.json to <out>, the index keys of --keys, signed by the root key
            whose secret the file <root> holds, kept offline.
  index     Writes the registry's index into <out>: config.json, the registry's keys.json,
            checked against Sleip's root keys or those of <roots>, a line each, and a file
            of each plugin, index/<owner>/<name>, a line of each version whose archive is in
            <archives>/<release>/, signed whole by the index key that SLEIP_INDEX_KEY holds.
  site      Writes the registry's site into <out>, beside the index that `index` wrote there:
            index.html of every plugin, which Pagefind searches, and plugins/<owner>/<name>/
            of each, of its versions, capabilities, checksums and README.
  readme    Rewrites the list of the plugins of the registry's README.md, or of <readme>,
            between its marks <!-- plugins:start --> and <!-- plugins:end -->, of the index in
            <index>: each plugin's name, version, description and capabilities, as text alone.
  --local   Takes repositories of file://, as the tests' are.
";

/// The help of the command `name`, if it is one.
fn command_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "new" => {
            "Usage: sleip new [options] <plugin>\n\nMakes a plugin of Luau that works, as owner/name, in a folder of its name\nafter the owner in the current folder: its manifest, src/init.luau of a\ncommand that shows a notice, its texts in English, a test, the plugin API's\ntypes and luau-lsp's settings.\n"
        }
        "check" => {
            "Usage: sleip check [options]\n\nChecks the plugin of the current folder: its manifest, the capabilities that\nthe app takes, the plugin API's version, its texts' catalogs, its package's\nlayout and sizes, and its sources' types against the plugin API's, as\nluau-lsp checks them, each problem of its file, line and column.\n"
        }
        "test" => {
            "Usage: sleip test [options]\n\nRuns the tests of the plugin of the current folder: each function of the\ntable that each file tests/*.luau returns, of a TestHost of its own, the\nplugin made anew on a host of the tests; a failure tells its file and line.\n"
        }
        "dev" => {
            "Usage: sleip dev [options]\n\nChecks the plugin of the current folder and links it into Lokyr's plugins,\nan entry of plugins.lock of its folder, which Lokyr reloads at each save;\nchecks it again at each change, telling its problems. Ctrl-C ends it, the\nentry taken back, and an installed plugin of its name given back.\n"
        }
        "add" => {
            "Usage: sleip add [options] [<plugin>] --path <folder>\n       sleip add [options] [<plugin>] --git <url> (--tag <tag> | --rev <commit>)\n\nAdds a plugin to plugins.toml, of the folder or the git repository's tag or\ncommit given, as owner/name, its manifest's if left out; resolves every\nplugin of plugins.toml against Lokyr's plugin API; stores each package; and\nwrites plugins.lock, which Lokyr reloads. A failure changes nothing.\n"
        }
        "update" => {
            "Usage: sleip update [options] [<plugin>]\n\nFetches the folders and the git repositories of the plugins of plugins.toml\nagain, or the one given, resolves them, stores their packages and writes\nplugins.lock. A failure changes nothing.\n"
        }
        "search" => {
            "Usage: sleip search [options] <words>...\n\nSearches the registry's plugins: each that every word finds in its name or\nits description, of the registry's signed list, which Sleip checks and keeps\nin its cache; with --offline, of the cache alone.\n"
        }
        "info" => {
            "Usage: sleip info [options] <plugin>\n\nTells a plugin of the registry, as owner/name: its version, description,\ncapabilities, the commands it runs, its dependencies, and its versions with\ntheir checksums, of its signed index file; with --offline, of the cache.\n"
        }
        "list" => {
            "Usage: sleip list [options]\n\nLists the plugins of plugins.lock: each one's version, kind, source and\ncapabilities.\n"
        }
        "remove" => {
            "Usage: sleip remove [options] <plugin>\n\nRemoves the plugin, as owner/name, from plugins.toml and plugins.lock,\nunless another plugin needs it. Its package stays in the store until\n`sleip gc`; the decisions on its capabilities stay in permissions.toml.\n"
        }
        "gc" => {
            "Usage: sleip gc [options]\n\nRemoves the packages of the store that plugins.lock does not name, and what\nan install that stopped left there.\n"
        }
        "registry" => REGISTRY_HELP,
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
    // The options of a value, by their names.
    let mut valued: Vec<(&str, &str)> = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--offline" => options.offline = true,
            "--locked" => options.locked = true,
            "-h" | "--help" => help = true,
            "-V" | "--version" => version = true,
            name @ ("--path" | "--git" | "--tag" | "--rev") => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("`{name}` takes a value\n\n{HELP}"))?;
                valued.push((name, value));
            }
            "registry" if words.is_empty() && !help => {
                let rest: Vec<&String> = args.collect();
                if rest.iter().any(|arg| *arg == "-h" || *arg == "--help") {
                    return Ok(Parsed::Help(REGISTRY_HELP.to_owned()));
                }
                return registry(&rest)
                    .map(|command| Parsed::Run(options, Command::Registry(command)));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("`{flag}` is no option of Sleip's\n\n{HELP}"));
            }
            word => words.push(word),
        }
    }
    let value = |name: &str| {
        valued
            .iter()
            .find(|(given, _)| *given == name)
            .map(|(_, value)| (*value).to_owned())
    };
    if !valued.is_empty() && words.first() != Some(&"add") {
        return Err(format!(
            "`{}` is an option of `sleip add` alone\n\n{HELP}",
            valued[0].0
        ));
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
        ["new", plugin] => Ok(Parsed::Run(options, Command::New((*plugin).to_owned()))),
        ["check"] => Ok(Parsed::Run(options, Command::Check)),
        ["test"] => Ok(Parsed::Run(options, Command::Test)),
        ["dev"] => Ok(Parsed::Run(options, Command::Dev)),
        ["add", rest @ ..] if rest.len() <= 1 => {
            use lokyr_pkg_model::list::{GitAt, Wanted};
            let from = match (
                value("--path"),
                value("--git"),
                value("--tag"),
                value("--rev"),
            ) {
                (Some(path), None, None, None) => Wanted::Path(path),
                (None, Some(url), Some(tag), None) => Wanted::Git {
                    url,
                    at: GitAt::Tag(tag),
                },
                (None, Some(url), None, Some(rev)) => Wanted::Git {
                    url,
                    at: GitAt::Rev(rev),
                },
                _ => {
                    return Err(format!(
                        "`sleip add` takes `--path <folder>`, or `--git <url>` and one of `--tag` and `--rev`\n\n{HELP}"
                    ));
                }
            };
            Ok(Parsed::Run(
                options,
                Command::Add(rest.first().map(|name| (*name).to_owned()), from),
            ))
        }
        ["update"] => Ok(Parsed::Run(options, Command::Update(None))),
        ["update", plugin] => Ok(Parsed::Run(
            options,
            Command::Update(Some((*plugin).to_owned())),
        )),
        ["list"] => Ok(Parsed::Run(options, Command::List)),
        ["remove", plugin] => Ok(Parsed::Run(options, Command::Remove((*plugin).to_owned()))),
        ["gc"] => Ok(Parsed::Run(options, Command::Gc)),
        ["doctor"] => Ok(Parsed::Run(options, Command::Doctor)),
        ["search", words @ ..] if !words.is_empty() => Ok(Parsed::Run(
            options,
            Command::Search(words.iter().map(|word| (*word).to_owned()).collect()),
        )),
        ["info", plugin] => Ok(Parsed::Run(options, Command::Info((*plugin).to_owned()))),
        [name, ..] => match command_help(name) {
            Some(text) => Err(format!("`{name}` takes other arguments\n\n{text}")),
            None => Err(format!("`{name}` is no command of Sleip's\n\n{HELP}")),
        },
    }
}

/// The tool that `args`, the command line past `registry`, asks; or the
/// text of why it is wrong.
fn registry(args: &[&String]) -> Result<Registry, String> {
    use std::path::PathBuf;

    let (mut words, mut local, mut files_alone) = (Vec::new(), false, false);
    let mut valued: Vec<(&str, &str)> = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--local" => local = true,
            "--files-alone" => files_alone = true,
            name @ ("--base" | "--author" | "--orgs" | "--out" | "--report" | "--registry"
            | "--root" | "--keys" | "--archives" | "--roots" | "--dl" | "--index"
            | "--readme") => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("`{name}` takes a value\n\n{REGISTRY_HELP}"))?;
                valued.push((name, value.as_str()));
            }
            flag if flag.starts_with('-') => {
                return Err(format!(
                    "`{flag}` is no option of `sleip registry`\n\n{REGISTRY_HELP}"
                ));
            }
            word => words.push(word),
        }
    }
    let value = |name: &str| {
        valued
            .iter()
            .find(|(given, _)| *given == name)
            .map(|(_, value)| (*value).to_owned())
    };
    let needed =
        |name: &str| value(name).ok_or_else(|| format!("`{name}` is needed\n\n{REGISTRY_HELP}"));
    let registry = || PathBuf::from(value("--registry").unwrap_or_else(|| ".".to_owned()));
    match words.as_slice() {
        ["verify", head] => Ok(Registry::Verify {
            head: PathBuf::from(head),
            base: PathBuf::from(needed("--base")?),
            author: needed("--author")?,
            orgs: value("--orgs")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|org| !org.is_empty())
                .map(str::to_owned)
                .collect(),
            out: PathBuf::from(needed("--out")?),
            report: value("--report").map(PathBuf::from),
            local,
            files_alone,
        }),
        ["build", plugin, version] => Ok(Registry::Build {
            plugin: (*plugin).to_owned(),
            version: (*version).to_owned(),
            registry: registry(),
            out: PathBuf::from(needed("--out")?),
            local,
        }),
        ["versions"] => Ok(Registry::Versions {
            registry: registry(),
        }),
        ["keygen"] => Ok(Registry::Keygen {
            out: PathBuf::from(needed("--out")?),
        }),
        ["sign-keys"] => Ok(Registry::SignKeys {
            root: PathBuf::from(needed("--root")?),
            keys: needed("--keys")?
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(str::to_owned)
                .collect(),
            out: PathBuf::from(needed("--out")?),
        }),
        ["index"] => Ok(Registry::Index {
            registry: registry(),
            archives: PathBuf::from(needed("--archives")?),
            out: PathBuf::from(needed("--out")?),
            roots: value("--roots").map(PathBuf::from),
            dl: value("--dl"),
        }),
        ["site"] => Ok(Registry::Site {
            registry: registry(),
            archives: PathBuf::from(needed("--archives")?),
            out: PathBuf::from(needed("--out")?),
            roots: value("--roots").map(PathBuf::from),
        }),
        ["readme"] => Ok(Registry::Readme {
            registry: registry(),
            archives: PathBuf::from(needed("--archives")?),
            index: PathBuf::from(needed("--index")?),
            readme: value("--readme").map(PathBuf::from),
            roots: value("--roots").map(PathBuf::from),
        }),
        _ => Err(format!(
            "`sleip registry` takes `verify`, `build`, `versions`, `keygen`, `sign-keys`, `index`, `site` or `readme`, of their arguments\n\n{REGISTRY_HELP}"
        )),
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
        assert_eq!(
            parsed(
                "--json registry verify pr --base main --author ann --orgs acme,,bob --out dist --files-alone"
            ),
            Ok(Parsed::Run(
                Options {
                    json: true,
                    ..Options::default()
                },
                Command::Registry(Registry::Verify {
                    head: "pr".into(),
                    base: "main".into(),
                    author: "ann".to_owned(),
                    orgs: vec!["acme".to_owned(), "bob".to_owned()],
                    out: "dist".into(),
                    report: None,
                    local: false,
                    files_alone: true,
                })
            ))
        );
        assert_eq!(
            parsed("registry build acme/x 0.1.0 --out dist --local"),
            Ok(Parsed::Run(
                Options::default(),
                Command::Registry(Registry::Build {
                    plugin: "acme/x".to_owned(),
                    version: "0.1.0".to_owned(),
                    registry: ".".into(),
                    out: "dist".into(),
                    local: true,
                })
            ))
        );
        // The registry's tools hidden, no line of the help a command of them.
        assert!(
            !HELP
                .lines()
                .any(|line| line.trim_start().starts_with("registry"))
        );
        assert!(
            matches!(parsed("help registry"), Ok(Parsed::Help(text)) if text.contains("registry verify"))
        );
        for wrong in [
            "registry",
            "registry verify pr --author ann --out dist",
            "registry build acme/x --out dist",
            "registry versions --fast",
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
