# Links

<sub>macOS · on by default · the [`links`](plugins/links.md) plugin · [`[links]`](CONFIGURATION.md#links), [`[plugins.links]`](CONFIGURATION.md#pluginslinks)</sub>

⌘-click a URL, a path or a program's link to open it, without copying it first. A path followed by a line number, such as a compiler's error, opens at that line in your editor. From the keyboard, hints label every link on the screen, so that a letter or two opens one.

## Try it

Run this command:

```sh
echo "Read https://lokyr.dev/docs/ or /etc/hosts"
```

Hold ⌘ and move the pointer over the address: it's underlined, and the pointer becomes a pointing hand. Click: Lokyr asks whether to open it, then opens it in your browser.

## What it gives you

- **⌘-click** on a URL, a path, or a link that a program printed.
- **A path with a line**, such as `src/main.rs:42`, opens at that line in your editor.
- **The context menu** opens a link, copies its address, or opens a window or a tab in its folder.
- **A question before opening**, which you can turn off for URLs, for files, or for both.
- **Hints**: ⇧⌘E labels every URL, path, git hash and IP address on the screen, to open or copy one from the keyboard. · not released yet
- **Your own patterns**, such as your issue tracker's ticket numbers, which hints label and open. · not released yet

## Open a link with ⌘-click

While you hold ⌘, the link under the pointer is underlined, so that you see what a click would open. These open:

- **A program's link**: text that a program printed with a link behind it, such as the files that GNU's `ls --hyperlink` lists. The link opens, whatever the text says.
- **A URL** that starts with `http`, `https`, `mailto` or `file`. It ends at a space, and the brackets and the punctuation around it, such as a sentence's last period, stay out.
- **A path** that starts with `/` or `~`, or a path relative to the folder of the program in front, such as a file that `ls` lists. Only a path that exists on this Mac opens.
- **A path between quotes** can hold spaces, as `git status` writes them, and so can a path whose spaces have backslashes, as your shell's completion writes them.

A URL opens in your browser or your mail app, a file in its app, and a folder in the Finder. A program, an app or a script never runs: the Finder shows it, selected.

**Try it**: run this command, which prints a link the way programs do:

```sh
printf '\033]8;;https://lokyr.dev/\033\\the site\033]8;;\033\\\n'
```

⌘-click `the site`: `https://lokyr.dev/` opens.

## Open a path at its line in your editor

Compilers, test runners, `grep -n` and AI agents write a path with its line, and sometimes its column. Lokyr underlines them together, in each of these forms:

| Form | Example |
|---|---|
| A colon and the line | `src/main.rs:42` |
| The line and the column | `src/main.rs:42:7` |
| Between parentheses | `src/app.ts(42,7)` |
| A fragment, as on the web | `src/main.rs#L42` |

By default, the file opens in its app, at no line. Name your editor in [`[links] editor`](CONFIGURATION.md#editor), and the file opens there, at its line and column:

```toml
[links]
editor = "vscode"   # or "cursor", "zed", "sublime", "textmate", and more
```

For another editor, give the address that it answers to, with `{path}`, `{line}` and `{column}` where they go:

```toml
[links]
editor = "jetbrains://idea/navigate/reference?path={path}&line={line}"
```

A file opens in your editor even when a terminal claims its type, such as a script, as nothing runs. Where the editor isn't installed, the file opens in its app.

**Try it**: with `editor = "vscode"` in your config, run `echo /etc/hosts:3` and ⌘-click the path: VS Code opens the file at line 3.

## Choose whether Lokyr asks first

Before it opens a URL or a file, Lokyr asks you, showing it in full, so that a link whose text says something else can't surprise you. A folder opens at once.

- **For URLs**, the question has Open and Cancel. Check Don't ask again for links to open them at once from then on.
- **For files**, it has Open, Show in Finder and Cancel. Check Don't ask again for files, and the button you click becomes what files do.

The checkboxes write [`urls`](CONFIGURATION.md#urls) and [`files`](CONFIGURATION.md#files) in your config, which you can also set yourself:

```toml
[links]
urls = "open"      # "ask" asks first
files = "reveal"   # "ask" asks first, "open" opens the file in its app
```

**Try it**: ⌘-click `/etc/hosts` in the output of [Try it](#try-it) above: the question offers Open, Show in Finder and Cancel. Click Show in Finder: the Finder shows the file, selected.

## Use the context menu

Right-click a link, a URL or a path, or ⌃-click it, for these items:

- **Open Link** opens it, as a ⌘-click does.
- **Copy Link Address** copies the link, the URL, or the path from `/`.
- **New Window at Folder** and **New Tab at Folder** open a shell in the path's folder, or in the folder that holds its file.

An item that doesn't apply is dimmed: a URL of the web has no folder. For a selection of one line, the menu also has Search the Web, which opens the search engine that [`search-engine`](CONFIGURATION.md#search-engine) names.

**Try it**: run `echo /usr/share/man`, right-click the path and choose New Tab at Folder: a tab opens, its shell in `/usr/share/man`.

## Label links with hints

<sub>Platforms: macOS · not released yet</sub>

Press ⇧⌘E, or choose View › Show Hints. Every URL, every path that exists, with or without its line, every git hash and every IP address on the screen is underlined, and gets a label of one or two letters over its first letters. The labels nearest the cursor are a single letter. Type a label to pick its hint:

- **The label** opens the hint, as a ⌘-click would, a path at its line in your editor. A hash or an address, which open nothing, is copied.
- **The label typed with Shift** copies the hint instead.
- **Tab** changes what a pick does, shown in the small field at the pane's corner: Open, Copy, Type, which types the hint at the prompt, and Select. ⇧Tab goes back.

Typing a label's first letter keeps the labels that start with it. Escape, or a click elsewhere, takes the labels away.

Hints also find a URL right after a bracket, a quote or a `=`, such as in Markdown's `[docs](https://…)` or HTML's `href="https://…"`, and a path with spaces between quotes or after backslashes. [`alphabet`](CONFIGURATION.md#alphabet) sets the labels' letters, and the theme's `hints.label.bg` and `hints.label.fg` their colors.

**Try it**: run this command, then press ⇧⌘E:

```sh
echo "https://lokyr.dev/docs/ /etc/hosts 10.0.0.1 3fa2b9c"
```

Each of the four gets a label. Type the URL's label with Shift: the labels go, and `pbpaste` prints `https://lokyr.dev/docs/`.

## Add your own patterns

<sub>Platforms: macOS · not released yet</sub>

Besides URLs, paths, hashes and addresses, the hints can label text of your own, such as your issue tracker's ticket numbers, and open a page for each. Each pattern is a table of the config, under `[plugins.links.hints]`, named as you like:

```toml
# A ticket number, such as PROJ-1234, opens the ticket's page.
[plugins.links.hints.tickets]
pattern = 'PROJ-\d+'
url = "https://jira.example.com/browse/$0"
```

- **The table's name**, `tickets` here, is yours to choose. Lokyr uses it in the notices about that pattern, and when two patterns match the same text, the one whose name comes first wins, capital letters before lowercase ones, so that `Zeta` comes before `alpha`.
- **`pattern`** is a [regular expression](https://docs.rs/regex/latest/regex/#syntax). It is case-sensitive; start it with `(?i)` to ignore case. Write it between single quotes, as above, so that TOML keeps its backslashes as they are. Between double quotes, each backslash is doubled: `"PROJ-\\d+"`.
- **`url`** is what picking the hint opens. In it, `$0` stands for the whole text that the pattern matched, `$1` to `$9` for the parts of the pattern between parentheses, and `$$` for a `$`. It must open an `http`, `https`, `mailto` or `file` URL, or a path from `/` or `~`. A `url` that starts with another scheme, such as `vscode:` or `jira:`, is left out, and a notice reports it.
- **Without a `url`**, picking the hint copies its text.

**Try it**: add the table above to your config and save it, then run this command and press ⇧⌘E:

```sh
echo "Fixed in PROJ-42, see PROJ-7 too"
```

`PROJ-42` and `PROJ-7` get a label each, and typing a label opens `https://jira.example.com/browse/PROJ-42` or `https://jira.example.com/browse/PROJ-7`.

When only part of the match goes into the URL, put that part between parentheses and use `$1`. The ticket's number alone, for example:

```toml
# PROJ-1234 opens https://tracker.example.com/issue?id=1234.
[plugins.links.hints.tracker]
pattern = 'PROJ-(\d+)'
url = "https://tracker.example.com/issue?id=$1"
```

More patterns:

```toml
# A GitHub issue, such as #128, of one repository.
[plugins.links.hints.issues]
pattern = '#(\d+)'
url = "https://github.com/acme/app/issues/$1"

# A UUID, which picking copies.
[plugins.links.hints.uuids]
pattern = '(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'
```

A pattern that doesn't read is left out, and a notice reports why. A pick that fails, such as a `url` that starts with `$0` and turns into a URL that doesn't open, shows a notice too, and the labels go. The [`hints`](CONFIGURATION.md#hints) key of the configuration guide lists the same rules.

You can also add, change and remove your patterns in the settings window, which Lokyr › Open Settings Window opens, under Hints in its Links section:

- **Add a pattern**: type its name, its `pattern` and its `url` in the fields of New entry, then click Add.
- **Change a pattern**: edit its `pattern` or its `url`. Lokyr saves it when you stop typing, press Return or leave the field. Empty the `url` to make picking copy the text.
- **Rename a pattern**: edit its name, then press Return.
- **Remove a pattern**: click Remove beside its name.

Lokyr writes each pattern to your config file as a table of its own, such as `[plugins.links.hints.tickets]`, and keeps the rest of the file and its comments as they are.

## Settings

```toml
[links]
urls = "ask"            # "open" opens URLs without asking
files = "ask"           # "open" or "reveal" without asking
editor = "system"       # the editor of a path's line, such as "vscode"
search-engine = "google"
```

[`[links]`](CONFIGURATION.md#links) tells each key, and [`[plugins.links]`](CONFIGURATION.md#pluginslinks) the hints' [`alphabet`](CONFIGURATION.md#alphabet) and [`hints`](CONFIGURATION.md#hints).

In the settings window, which Lokyr › Open Settings Window opens, the Links section holds these keys, and under Hints the hints' keys, your patterns among them. · not released yet

## Commands and keys

| Keys | In the config | What they do |
|---|---|---|
| ⌘-click | `cmd+click` | Open the link under the pointer |
| ⇧⌘E | `cmd+shift+e` | Show the hints · not released yet |
| Tab, ⇧Tab | `tab`, `shift+tab` | Change what a pick does, while the hints show · not released yet |

[The `links` plugin](plugins/links.md#commands) lists its commands, those of no keys too, such as `links.copy`, which you can bind in [`[keys]`](CONFIGURATION.md#bindings).

## How it works

A program prints a link with OSC 8, [hyperlinks in terminals](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda): an escape code before the text gives the link, and another after it ends it. Lokyr keeps the link on the text's cells, and never shows the code. A `file` URL opens when it names this Mac: with no host, `localhost` or the Mac's name, as `ls --hyperlink`, ripgrep and `fd` write it.

For a URL or a path in the text, Lokyr reads the line under the pointer, joined across the rows that it wraps over, when you hold ⌘ or click. A relative path is read from the folder of the program in front, and opens only if it's there.

Hints read the screen's lines once, when you press ⇧⌘E, and cost nothing while they're hidden.

## Limits

- **macOS's own `ls` prints no links**: GNU's does, with `--hyperlink`, such as Homebrew's `gls`.
- **A URL that a program cuts with its own line breaks** opens only the part you click. Lokyr joins only the rows that a line wraps over.
- **A path on another machine**, such as one that a program prints over `ssh`, doesn't open: Lokyr looks for it on this Mac.
- **Hints label the screen**, not the scrollback above it: scroll to bring a link into view first. · not released yet
- **Patterns open only** `http`, `https`, `mailto` and `file` URLs and paths, not other schemes such as `vscode:`. · not released yet

## Troubleshooting

### ⌘-click on a path does nothing

**Why**: the path doesn't exist on this Mac, or it's relative to a folder other than the one of the program in front, such as a path that `make` prints from a subfolder.

**Fix**: run the command from the folder that the paths start from, or ⌘-click a path from `/`.

**Check**: hold ⌘ over the path: it's underlined.

### A path with its line opens the file at its start

**Why**: [`editor`](CONFIGURATION.md#editor) is `"system"`, which opens the file in its app, at no line.

**Fix**: name your editor:

```toml
[links]
editor = "vscode"
```

**Check**: ⌘-click the path again: the editor opens at the line.

### A pattern of your own gets no labels

<sub>Platforms: macOS · not released yet</sub>

**Why**: the pattern doesn't read, its `url` starts with a scheme that doesn't open, or its backslashes were lost between double quotes. A notice reports the first two when you save the config.

**Fix**: write the pattern between single quotes, such as `pattern = 'PROJ-\d+'`, and make the `url` start with `https://`.

**Check**: run `echo PROJ-42` and press ⇧⌘E: `PROJ-42` gets a label.

## See also

- [The `links` plugin](plugins/links.md), its commands and permissions
- [Mouse and selection](mouse-and-selection.md)
- [Escape codes](escape-codes.md), OSC 8 among them
- [AI agents](agents.md), whose paths open at their lines
