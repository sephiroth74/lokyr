# Configuration

Lokyr reads its settings from one TOML file. This guide lists every key that the file can hold: what it does, the values it takes and its default.

Every key and every section can be left out, and then takes its default: an empty file, or no file at all, gives Lokyr as it ships.

Under each key's name, a small line tells the platforms where the key applies: macOS alone for now, as Lokyr's first version runs on macOS. As Lokyr comes to other systems, the line names them, and a key that one system alone has says so.

## The file

- **Where it is**: `~/.config/lokyr/config.toml`, on macOS too, so that it can live with your dotfiles. When `XDG_CONFIG_HOME` is set to an absolute path, the file is `$XDG_CONFIG_HOME/lokyr/config.toml` instead.
  - A variable that your shell's startup files set, such as `XDG_CONFIG_HOME` in `~/.zshrc`, reaches Lokyr only when you start it from that shell: started from the Finder or the Dock, Lokyr reads `~/.config/lokyr/config.toml`.
  - When the file's directory does not exist, Lokyr makes it, empty, as it starts, so that a config that you write there later applies at once.
- **Seeing it**: Lokyr › Open Settings Window shows every key, by its section, the plugins' too, of what it does and its value in force.
- **Opening it**: Lokyr › Settings… (⌘,) opens the file in your text editor, the application that opens plain text, as TextEdit or the one that you chose in the Finder. When there is no file, it makes one first: every key, commented out with its default and its text, under its section, so that you take a key's `#` away to change it, and the others keep the defaults to come. A key without a default shows an example of its value, as `family = "Menlo"`, and `[keys]` each default binding, a line a key, under what it does. In an editor that changes the quotes that you type, as TextEdit's Edit › Substitutions › Smart Quotes does, turn that off: TOML takes straight quotes alone. When the file cannot be made, as in a folder that you cannot write, or no editor opens it, an alert tells why.
- **Links**: the file, or its directory, can be a symbolic link, as dotfiles managers such as GNU Stow make it. Lokyr watches the file the link points to as well, and follows a link that is pointed elsewhere: the edits of its new target apply too. A target that goes, as a checkout of the dotfiles removes and writes it, applies again as it comes back; while it is gone, the defaults apply.
- **Reloading**: Lokyr applies the file as soon as you save it, without a restart. Saving from vim, which writes another file and renames it over the config, works as well as saving from any other editor. Removing the file, or its directory, brings the defaults back.
- **Texts**: a name, as a font's family, a theme's or a plugin's, takes 256 characters at most, a path or an address 1024, and `term` 64; a longer one is an error, at its line.
- **All or nothing**: a file applies whole, or not at all. When it has an error, none of its values apply, and the settings in force stay as they were. At startup, an invalid file gives the defaults. A plugin's section, which its plugin checks, applies on its own, as [`[plugins.<namespace>]`](#pluginsnamespace) says.
- **Errors**: an error tells the file, the line and the column, and what is wrong. For a key that Lokyr does not know, it names the nearest key that it knows:

  ```text
  lokyr: error 109 (usage): /Users/you/.config/lokyr/config.toml:3:1: unknown key `sise` in `[font]`; did you mean `size`?; none of the config applies, and the one in force stays
  ```

  The error shows in a card at the top right of every window's panes, its title and its text in two lines, the rest of it copied whole by the card's Copy button, until you save a file that reads, or click its ×; it goes to the log too: the terminal that started Lokyr, when you start it from one, or else, as from the Finder or the Dock, the file `~/Library/Logs/Lokyr/lokyr.log`, which the Console app shows. A font family that is not installed shows in the card the same way, until the file names one that is. A failure of a command or of a plugin shows in the card too, which goes by itself after eight seconds, unless the pointer rests on it.
- **Every key at once**: `lokyr +show-config` prints the config with every key: the file's values, and the defaults of the keys that it leaves out.
  - `--default` prints the defaults alone, as an empty file gives them, and `--docs` puts the text of this guide over each key: what it does, its values and its default.
  - What it prints is a config file, which reads back as the same settings. A key whose default no value can write, as `family`'s, which is the system's font, shows as a comment.
  - A file with an error prints nothing but the error.
- **The fonts**: `lokyr +list-fonts` lists the font families that macOS has, by the names that [`family`](#family) takes, in the order of their names; it marks the monospaced families, the default one and the config's.
  - `--styles` lists each family's styles under it, by the names that [`style`](#style) takes; `--monospace` lists the monospaced families alone; and `--family <name>` one family, with its styles, by its name or a name of some of its faces, as `family` takes it, where a name of no family installed exits with 1.
  - It draws nothing, so that it runs in any terminal and in a pipe, as `lokyr +list-fonts --monospace | less`.
- **Checking a file**: `lokyr +validate-config` checks the config file, or the one that you name, as `lokyr +validate-config ~/dotfiles/lokyr.toml`, as Lokyr takes it, without applying it: its font's styles too.
  - It exits with 0 when Lokyr takes the file, and with 1 and the error, at its line and column, when it does not. A family that is not installed, or a language or an icon that Lokyr lacks, only warns, as Lokyr takes the file with its default in their place.
  - Without a config file, the defaults apply, and the check passes; a file that you name must exist.
  - In the app's bundle, the command is `Lokyr.app/Contents/MacOS/lokyr`. Lokyr's panes have its directory at the end of their `PATH`, so that `lokyr +validate-config` runs there as it is, after a `lokyr` of your own, as Homebrew's; elsewhere, name it whole, as `/Applications/Lokyr.app/Contents/MacOS/lokyr +validate-config`.

### A complete example

Every key that Lokyr reads, with the values that it takes in the comment. Most show a value other than their default; a few show theirs, such as `cell-width`, `antialias` and `padding-y`:

```toml
{{example}}
```
