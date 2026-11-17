# Writing the documentation

This guide is for anyone who writes or changes a page of Lokyr's documentation. It covers where a page goes, how a page is laid out, and how to write it so that readers find what they need quickly. The pages are the Markdown files in `docs/`, which GitHub shows and lokyr.dev builds into its site.

## Where a page goes

Each page covers one topic, named the way a reader would name it: "Shell integration", "Keyboard", "Links". Don't name a page after a crate or a plugin, unless the plugin is the whole feature, such as the quick terminal or copy mode.

Every page belongs to one of the summary's groups:

| Group | What it holds |
|---|---|
| Start | The documentation's home, installing Lokyr, a first tour |
| Use | A page for each topic: what a feature does, how to use it, what goes wrong |
| Reference | Lists to look things up: the config's keys, the commands, the escape codes |
| Internals | How Lokyr is built, for contributors |

A Help group, for troubleshooting and common questions, joins them between Reference and Internals with its first page.

- **List a new page in [`docs/SUMMARY.md`](SUMMARY.md)**, in its group, where a reader would look for it. The site's build fails when a public page is missing from the summary. The measurements in `docs/bench/` and the release notes in `docs/releases/` are the exception: the page of their folder that the summary lists links each of them, which the build checks too.
- **Name a new page in lowercase**, with hyphens, such as `shell-integration.md`. The older pages in uppercase keep their names, because links from elsewhere point to them.
- **Update a feature's page in the same commit** as the change to the feature. When a feature has no page yet, say so in the pull request.

## How to write

- **Write to the reader as "you", in the present tense.** "Press ⌘↑ to jump to the previous prompt." "Lokyr opens the file at that line." Never refer to the reader in the third person.
- **Use the imperative for steps and task headings.** "Turn off shell integration", not "Turning off shell integration".
- **Keep one idea to a sentence**, about 25 words at most. When a sentence would need three semicolons, make it a list.
- **Choose the plain word.**
  - Use "such as" or "for example" for examples, "like" for comparisons, and "when" or "while" for time. Don't use "as" for all of them.
  - Use "of" only for possession and parts: "a tab with several panes", not "a tab of several panes".
  - Say what a program does with "reports", "sends", "shows" or "supports", not "tells".
  - Use one possessive at a time: "the tab's tooltip", not "its tab's tooltip's text".
- **Keep a table cell to a phrase or a sentence.** Anything longer becomes a section.
- **Say what a feature costs**, in memory, wakeups or latency, where it costs something.
- **Name no other terminal emulator.** Name a protocol by its number and what it does, and link its specification: "OSC 8, hyperlinks". Programs and tools that are not terminals can be named, such as vim, Gradle or Claude Code.
- **Leave out the project's plans**: no phases, no task identifiers, no decision records, and nobody's choices.
- **The Internals pages are the exception** to the last two: the design documents keep what they say, the terminals that they compare and the phases of the roadmap among it, and the measurements' file names keep their identifiers.

## The topic page

A topic page shows first, then explains. Its sections come in this order. Leave out the ones that don't apply, but don't reorder them, so that readers learn where things are.

````markdown
# Shell integration

<sub>macOS · zsh, bash, fish · on by default ·
[`[terminal] shell-integration`](CONFIGURATION.md#shell-integration)</sub>

One paragraph: what the feature is and what it gives you, in plain words.
No history, no internals.

## Try it

The shortest thing that shows the feature working, which works when pasted
as is, and what you should see.

```sh
ls /nonexistent
```

A red bar appears beside the prompt of the command that failed. Press ⌘↑ to
jump back to it.

## What it gives you

- **Jump between prompts**: ⌘↑ and ⌘↓.
- One line a feature, each linked to its page.

## Turn it on

Task sections, one a task, each titled in the imperative: "Turn it off",
"Copy a command's output".

## Settings

The keys that control the feature, each linked to its entry in the
configuration guide.

## Commands and keys

The commands and their default keys.

## How it works

The mechanism: the sequences, the variables and the files, linked to their
reference entries.

## Limits

What the feature doesn't do, by design or not yet.

## Troubleshooting

### Commands get no marks

**Why**: … **Fix**: … **Check**: …

## For shell authors

Only where programs or shells take part, linked to the escape codes'
reference.

## See also
````

- **The marker line** under the title is one `<sub>` line: the platforms, the shells or programs it works with and their versions, whether it is on by default, the plugin that provides it, and the key that controls it.
- **"Try it" is required** on every topic page. **"Troubleshooting" is required** on every page where something can go wrong.

## The plugin page

A bundled plugin has a page in `docs/plugins/`: what it shows, its commands, its configuration, how to turn it off, and its permissions. When the plugin is a whole feature, such as the command palette or the quick terminal, its page also has a "Try it" and a "Troubleshooting" section.

## The escape code entry

An escape code's entry, on the page of its family of protocols, gives its syntax, its parameters, what Lokyr does with it, and a way to try it.

````markdown
### OSC 9;4, progress

<sub>Platforms: macOS · on by default ·
[Specification](https://learn.microsoft.com/en-us/windows/terminal/tutorials/progress-bar-sequences)</sub>

```text
ESC ] 9 ; 4 ; state ; value ST
```

| Parameter | Values | Meaning |
|---|---|---|
| `state` | `0` clear, `1` normal, `2` error, `3` indeterminate, `4` paused | Its state |
| `value` | `0` to `100` | The percentage, ignored for `0` and `3` |

**What Lokyr does**: it fills the tab's outline as the progress grows.

**Try it**

```sh
printf '\033]9;4;1;50\033\\'; sleep 2; printf '\033]9;4;0\033\\'
```

The tab's outline fills halfway, then clears.

**See**: [the `progress` plugin](plugins/progress.md)
````

Each entry has the syntax in the notation of the code block above, where the spaces only make it easier to read, its parameters, what Lokyr does and how it differs from the specification, the exact reply to a query, the key that turns it on or off, a "Try it" with what you should see, and a link to its topic page. Don't explain a standard sequence again: link xterm's [control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), and write down only what Lokyr does differently.

## The troubleshooting entry

The heading is the symptom, as the reader sees it, quoting an error message exactly when there is one. Then three short parts:

- **Why**: the cause.
- **Fix**: what to do, with commands to paste.
- **Check**: a command that shows the fix worked, and what it prints.

## Code blocks

- **Give every code block its language**: `sh` for commands, `toml` for the config, `text` for output and for an escape code's syntax, `luau` for plugins.
- **Put commands alone in a block you can copy**: no `$ ` prompt and no output. Show the output in its own `text` block after it, introduced by "You should see".
- **Write escape codes with `printf`**, `\033` for the escape and `\033\\` for the string terminator, which every `printf` supports. `\e` is not POSIX.
- **Keep config examples complete and short**: the section's header, only the keys that matter, and a comment on each value that isn't obvious. Each example must read as Lokyr reads it.
- **Use neutral paths and hosts**, such as `~/src/demo` and `host.example`.
- **Name a third-party program with the version you checked and the month**: "Gradle 9.8, checked in November 2026".

## Keys and menus

- **In the text, write keys with macOS's symbols**, in the order the menus use, ⌃⌥⇧⌘: ⇧⌘P. In the config, write the binding: `cmd+shift+p`. The first time a page names a key, give both: ⇧⌘P (`cmd+shift+p`).
- **Write menu paths with ›**, each item exactly as the English menu shows it: Lokyr › Settings…, View › Show Hints.

## Markers

- **Use one format for markers**: `<sub>Platforms: macOS · Since 1.3</sub>` under a key, a command or an entry, and the marker line under a topic page's title. A section newer than its page carries its own marker. Don't write "Added in", "New in" or "Available since" in the text.
- **Mark what no release has yet** with `· not released yet`: in the marker line of its page or section, or at the end of its list item or table row, so that readers of the last release know what they don't have yet. Never put the mark in a heading: the heading's link would change with it.
- **Mark a new key of the config** with `"x-since": "next"` in its schema, beside `x-platforms`: the configuration guide shows it as `· not released yet` in the key's line of platforms. Mark a new theme token in the comment above its rule in `crates/plugin-sdk/src/theme/rules.toml`, which the tokens' table shows.
- **The release turns every mark into its own**: in the commit that prepares it, `cargo xtask since X.Y.Z` writes `· Since X.Y`, or `· Since X.Y.Z` for a patch, and `"x-since": "X.Y"`. A mark in code, such as the ones on this page, is left alone. The release refuses to go out while a mark is left: one in another form, such as `(not released yet)`, one in a heading, or one in HTML.

## Callouts

GitHub's alerts show as callouts, on GitHub and on lokyr.dev:

```markdown
> [!TIP]
> Press ⇧⌘E to label every link on the screen.
```

Use `[!NOTE]` for something worth knowing, `[!TIP]` for a better way to do something, `[!IMPORTANT]` for something you need to know to succeed, `[!WARNING]` for a risk, and `[!CAUTION]` for something that loses data or can't be undone. Use them sparingly: a page with a callout in every section has none that stands out.

## Screenshots

- **Add a screenshot only where the eye needs it**: a feature that is visual, such as the tab bar's styles, not a dialog that the text already describes.
- **Use the format of [the screenshots page](screenshots.md)**: WebP at 1224 and 2448 pixels wide, with `srcset`, `width`, `height`, `loading="lazy"` and an alt text that says what the picture shows.
- **Take every screenshot the same way**: a window of 100 × 30 cells, the default font, Lokyr Light or Lokyr Dark, and no personal data. Show both themes only when the theme is the subject.
- **Show motion as a short, muted video or an animated WebP** with a still poster, only for drags and animations. Don't use GIFs: they are large.

## Links

- **Link every key, command and escape code to its reference entry** the first time a page names it. Reference entries link back to their topic page with "See".
- **Use relative links between pages**, such as `[the configuration guide](CONFIGURATION.md#font)`. The site turns them into its own pages' links.

## Generated pages

Some pages, or parts of them, are made from their sources, so that they can't drift from the code. Change the source, never the page; a page made whole from a source links that source in its footer, in place of the page's editor:

- **[The configuration guide](CONFIGURATION.md)** comes from the config's schemas and its template, by `cargo xtask config-guide`.
- **The theme tokens' table** in [the theme authors' guide](themes/authoring.md) comes from the tokens' rules, by `cargo xtask theme-guide`.
- **[The commands' reference](commands.md)** comes from the app's commands and its default keys, by `cargo xtask docs`, which also writes the **Default keys** column of the commands' tables of the plugins' pages, between `<!-- commands: lokyr/<plugin> -->` and `<!-- commands end -->`. Write the other columns of those tables by hand: the check fails when a table leaves out a command of its plugin, or lists one that the plugin doesn't have.

## Check your page

- `just check-links` checks that every internal link and anchor resolves.
- `just site-serve` builds the site and serves it on `http://127.0.0.1:8124`, so that you can read your page as lokyr.dev will show it, with its sidebar and its callouts. The build fails when a link breaks or a page is missing from the summary.
