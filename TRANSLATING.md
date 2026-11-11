# Translating Lokyr

Lokyr speaks English and Italian, and every other language is welcome. This guide tells what there is to translate, how to improve a translation that exists, and how to add a language. You need no Rust to translate: a text editor and a Mac to try the result on are enough, and a pull request takes your files the rest of the way. [CONTRIBUTING.md](CONTRIBUTING.md) tells how to set up the project and what a pull request needs.

## What there is to translate

Every text that Lokyr shows you: its menus, the titles of its commands, its alerts and notices, the line of a window whose program ended, the find bar and the tab bar, and the prompts that macOS shows when a program in the terminal asks for the camera, the microphone or a folder. The texts are in [Project Fluent](https://projectfluent.org) catalogs, `.ftl` files, a set for each language; English is the source, which every other language follows.

Some texts stay in English, as the [decision on localization](docs/adr/0020-localization.md) says: the log, which reports of problems quote; the `sleip` command line; the documentation; and the config's keys and values, which are the same in every language.

The catalogs, each in a folder of its language's code, `en` for English, `it` for Italian:

| Catalog | Its texts |
|---|---|
| [`crates/app-core/resources/locales/en/app.ftl`](crates/app-core/resources/locales/en/app.ftl) | The menus, the app's commands, its alerts and notices |
| [`crates/app-core/resources/locales/en/infoplist.ftl`](crates/app-core/resources/locales/en/infoplist.ftl) | What macOS shows as a program in the terminal asks for the camera, the contacts and the like |
| [`crates/kernel/locales/en/kernel.ftl`](crates/kernel/locales/en/kernel.ftl) | The line of a window whose program ended, or could not start |
| [`plugins/core-actions/locales/en/core-actions.ftl`](plugins/core-actions/locales/en/core-actions.ftl) | Copy, paste, scrolling and the text's size |
| [`plugins/links/locales/en/links.ftl`](plugins/links/locales/en/links.ftl) | Opening links |
| [`plugins/tabs/locales/en/tabs.ftl`](plugins/tabs/locales/en/tabs.ftl) | The tab bar and its commands |
| [`plugins/splits/locales/en/splits.ftl`](plugins/splits/locales/en/splits.ftl) | The panes' commands |
| [`plugins/search/locales/en/search.ftl`](plugins/search/locales/en/search.ftl) | The find bar |
| [`plugins/command-palette/locales/en/command-palette.ftl`](plugins/command-palette/locales/en/command-palette.ftl) | The command palette |
| [`plugins/quick-terminal/locales/en/quick-terminal.ftl`](plugins/quick-terminal/locales/en/quick-terminal.ftl) | The quick terminal |
| [`plugins/progress/locales/en/progress.ftl`](plugins/progress/locales/en/progress.ftl) | The programs' progress |
| [`plugins/command-header/locales/en/command-header.ftl`](plugins/command-header/locales/en/command-header.ftl) | The command at a pane's top |
| [`plugins/copy-mode/locales/en/copy-mode.ftl`](plugins/copy-mode/locales/en/copy-mode.ftl) | The copy mode |

The [`keymap-macos`](docs/plugins/keymap-macos.md) plugin has no texts of its own.

## The format

A catalog is a list of messages, each an identifier, `=`, and its text. A line that starts with `#` is a comment for you, which tells where the texts show:

```fluent
# The find bar: its field's placeholder, and the count of the matches.
find-placeholder = Find
find-count = { $current } of { $count }
```

- **Translate the text, never the identifier**: `find-placeholder` stays as it is.
- **Keep the variables**, the names between `{ $` and `}`, as they are: you can move them where your language puts them, but each message of yours takes the same variables as English's, no more and no fewer.
- **Plurals** choose a text by a number, of the categories that your language has, as Unicode's [plural rules](https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html) list them; the one after `*` is the default, which every such message needs:

  ```fluent
  paste-lines-message = { $lines ->
      [one] Paste a line with its line break?
     *[other] Paste { $lines } lines?
  }
  ```

  A language of more categories, as `few` and `many`, adds them; one of a single form keeps `*[other]` alone.
- **A text on several lines** goes on the lines below its identifier, each indented.

## Style

- **Follow macOS in your language.** The menus and the alerts read best in the words that macOS's own apps use, as Copy, Paste, Select All or Bring All to Front in English: Apple's own translations are the model, as macOS shows them with your language as the system's.
- **Keep the names** Lokyr, Sleip and macOS, the names of keys, as Return or ⌘, and of commands, as `tabs.new`, and anything between backticks, which is code or a value of the config.
- **An ellipsis**, `…`, ends the title of a command that asks you something before it acts, as Rename Tab…, as macOS's guidelines want; keep it in your translation.
- **Mind the room.** Some texts sit in narrow places: the find bar's count, as `No results` and `3 of 12`, has about 95 points; a tab's title and a menu's item are best short. Try your texts in the app, as the next sections tell.
- **Quotes** are your language's own, as “” in English and «» in some others.

## Improving a translation

Edit the catalog of your language, as `crates/app-core/resources/locales/it/app.ftl`, and open a pull request with what you changed and why. The tests check that each catalog has every message of English's, and no other, each with English's variables: `cargo test -p lokyr-kernel l10n` and `cargo test -p lokyr-app-core catalogs` run them, and so does `just ci`.

When English gains a message, every other catalog needs it too, or the tests fail: whoever adds a text to English adds it to the other languages, as well as they can, and a speaker of the language makes it right later.

## Adding a language

Lokyr takes languages by their two- or three-letter code alone, as `fr`, `de` or `ja`, without a region: a system set to French of Canada reads `fr`. A language that needs two variants, as Portuguese of Brazil and of Portugal, is not possible yet; open an issue first.

1. **Make it.** One command makes your language ready to translate, of its code, as `fr`:

   ```sh
   just new-language fr
   ```

   It copies every English catalog, each `locales/en` folder of the table above, to one of your language's code, and adds your language to each list of catalogs that Lokyr builds into its binary and to the [configuration guide](docs/CONFIGURATION.md#language)'s `language` key. `infoplist.ftl` needs nothing more: building the app finds every language's folder. Without `just`, `cargo xtask new-language fr` does the same.

2. **Translate** the files of your language's folders, as the format and the style above tell.

3. **Check it**, as you go and before your pull request:

   ```sh
   just check-language fr
   ```

   It tells each catalog's messages that are missing, those that English has not, those whose variables are not English's, and those still as English has them; the first three fail it, which the tests fail too. A message can stay as English has it where your language says it so, as a name: the check tells it, and nothing fails.

4. **Run the tests**, `cargo test -p lokyr-kernel l10n` and `cargo test -p lokyr-app-core catalogs`, or `just ci`, which check every language's catalogs against English's.

5. **Try it.** Set the language in your config, which applies as you save it, the menus too:

   ```toml
   [app]
   language = "fr"
   ```

   Then go through the app: the menus, the alerts, as a paste of several lines or quitting with a program running, the tab bar's tooltips and its context menu, the find bar, and a window whose program ended. `just bundle` builds `target/Lokyr.app`, whose prompts of macOS, as a program's ask for the camera, follow the system's language rather than the config's.

6. **Open the pull request**, with `just ci` green, and tell in it how you tried it, as [CONTRIBUTING.md](CONTRIBUTING.md) asks of checks by hand. A screenshot or two of the menus and the find bar help the review.

Thank you: every language makes Lokyr someone's own terminal.
