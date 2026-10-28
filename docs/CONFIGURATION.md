# Configuration

Lokyr reads its settings from one TOML file. This guide lists every key that the file can hold: what it does, the values it takes and its default.

Every key and every section can be left out, and then takes its default: an empty file, or no file at all, gives Lokyr as it ships.

Under each key's name, a small line tells the platforms where the key applies: macOS alone for now, as Lokyr's first version runs on macOS. As Lokyr comes to other systems, the line names them, and a key that one system alone has says so.

## The file

- **Where it is**: `~/.config/lokyr/config.toml`, on macOS too, so that it can live with your dotfiles. When `XDG_CONFIG_HOME` is set to an absolute path, the file is `$XDG_CONFIG_HOME/lokyr/config.toml` instead.
  - A variable that your shell's startup files set, such as `XDG_CONFIG_HOME` in `~/.zshrc`, reaches Lokyr only when you start it from that shell: started from the Finder or the Dock, Lokyr reads `~/.config/lokyr/config.toml`.
  - When the file's directory does not exist, Lokyr makes it, empty, as it starts, so that a config that you write there later applies at once.
- **Links**: the file, or its directory, can be a symbolic link, as dotfiles managers such as GNU Stow make it. Lokyr watches the file the link points to as well, and follows a link that is pointed elsewhere: the edits of its new target apply too. A target that goes, as a checkout of the dotfiles removes and writes it, applies again as it comes back; while it is gone, the defaults apply.
- **Reloading**: Lokyr applies the file as soon as you save it, without a restart. Saving from vim, which writes another file and renames it over the config, works as well as saving from any other editor. Removing the file, or its directory, brings the defaults back.
- **All or nothing**: a file applies whole, or not at all. When it has an error, none of its values apply, and the settings in force stay as they were. At startup, an invalid file gives the defaults.
- **Errors**: an error tells the file, the line and the column, and what is wrong. For a key that Lokyr does not know, it names the nearest key that it knows:

  ```text
  lokyr: /Users/you/.config/lokyr/config.toml:3:1: unknown key `sise` in `[font]`; did you mean `size`?
  lokyr: none of the config applies, and the one in force stays
  ```

  For now the errors go to the log: to the terminal that started Lokyr, when you start it from one. A notice in the window comes later.
- **Every key at once**: `lokyr +show-config` prints the config with every key: the file's values, and the defaults of the keys that it leaves out.
  - `--default` prints the defaults alone, as an empty file gives them, and `--docs` puts the text of this guide over each key: what it does, its values and its default.
  - What it prints is a config file, which reads back as the same settings. A key whose default no value can write, as `family`'s, which is the system's font, shows as a comment.
  - A file with an error prints nothing but the error.
- **Checking a file**: `lokyr +validate-config` checks the config file, or the one that you name, as `lokyr +validate-config ~/dotfiles/lokyr.toml`, as Lokyr takes it, without applying it: its font's styles too.
  - It exits with 0 when Lokyr takes the file, and with 1 and the error, at its line and column, when it does not. A family that is not installed, or a language or an icon that Lokyr lacks, only warns, as Lokyr takes the file with its default in their place.
  - Without a config file, the defaults apply, and the check passes; a file that you name must exist.
  - In the app's bundle, the command is `Lokyr.app/Contents/MacOS/Lokyr`, as `/Applications/Lokyr.app/Contents/MacOS/Lokyr +validate-config`.

### A complete example

Every key that Lokyr reads, with the values that it takes in the comment. Most show a value other than their default; `cell-width`, `antialias` and `padding-y` show theirs:

```toml
config_version = 1

[font]
family = "CaskaydiaCove NFM"         # a family as Font Book shows it; SF Mono by default
size = 22                            # points, from 4 to 200
style = "SemiBold"                   # the face of plain text, by the font's name for it
bold-style = "Bold"                  # the faces of bold, italic and bold italic text
italic-style = "Italic"
bold-italic-style = "Bold Italic"
non-ascii-family = "Hiragino Sans"   # the family of the text past ASCII
line-height = 1.1                    # factors of the cell, from 0.5 to 3
cell-width = 1.0
ligatures = false                    # true | false
builtin-box-drawing = false          # true | false: box drawing, blocks and braille
builtin-powerline = false            # true | false: the powerline symbols
antialias = true                     # true | false
thin-strokes = "retina-dark"         # never | always | retina | dark | retina-dark

[text]
bold = false                         # true | false: bold text in the bold face
italic = false                       # true | false: italic text in the italic face
ansi-colors = false                  # true | false: the colors that programs set
bold-is-bright = true                # true | false: bold text of the first eight colors in their bright ones
blink = false                        # true | false: text that programs make blink

[cursor]
shape = "bar"                        # block | bar | underline | hollow
blink = false                        # true | false
thickness = 2                        # points, from 0.5 to 20

[window]
close-on-exit = "never"              # clean | always | never
padding-x = 6                        # points, from 0 to 100
padding-y = 4
background-opacity = 0.9             # from 0 to 1
background-blur = 20                 # points, from 0 to 100

[terminal]
term = "xterm-lokyr"                 # the TERM that programs see
unicode-widths = "8"                 # current | 8: the width tables
ambiguous-width = 2                  # 1 | 2: the cells of an ambiguous character
normalization = "nfc"                # none | nfc | nfd | hfs+

[scrollback]
scroll-on-output = true              # true | false: output brings the view back down
limit = 20                           # MiB of a window's scrollback, from 1 to 16384
budget = 200                         # MiB that the windows' scrollbacks share

[keyboard]
option-as-meta = "left"              # none | left | right | both

[clipboard]
read = "ask"                         # never | ask: a program's read of the clipboard
confirm-paste = "always"             # auto | always | never: a paste with line breaks asks first

[bell]
sound = true                         # true | false: the system's alert sound
flash = false                        # true | false: a flash of the window that has the focus
bounce = false                       # true | false: the Dock's icon, for a window in the background
mark-title = false                   # true | false: 🔔 in its title until it takes the focus

[app]
icon = "laguz"                       # jormungandr | laguz
language = "it"                      # system | en | it

[keys]
"cmd+shift+n" = "window.new"         # keys, and the command that they run
"ctrl+a>n" = { command = "window.new", when = "!alternate-screen" }
"cmd+q" = false                      # takes the binding away; the menu's Command-Q stays, for now
```

## `config_version`

<sub>Platforms: macOS</sub>

The version of the file's format. Lokyr reads version `1`, and refuses any other.

| Value | Default |
|---|---|
| `1` | `1` |

The key is the only one at the top of the file, and the only one with an underscore: the keys of the sections are written with hyphens.

## `[font]`

The font of the text: its family, its size and the faces that bold and italic text take.

A character that the font lacks, such as Japanese text, takes the font that macOS gives for it, as other apps take it: Hiragino for Japanese. A symbol takes the form that Unicode gives it:

- **Emoji**, such as ⚡, ☕ or 🦀, take Apple Color Emoji, in their own colors, even when the font has a plain form of them.
- **Symbols that have an emoji too**, such as ✔, ⚠, ℹ or ❤, stay text: the font's, or another font's that has them as text, in the color that the program gives them.
- **A variation selector** after the character chooses otherwise: U+FE0F asks for the emoji, as in ❤️, and U+FE0E for the text, as in ⚡︎. A symbol that no font has in the form asked takes the other.

### `family`

<sub>Platforms: macOS</sub>

The font family, by its name, as Font Book shows it on macOS, such as `"Menlo"` or `"CaskaydiaCove NFM"`.

- **Default**: the system's monospaced font, SF Mono.
- The case does not matter: `"menlo"` is Menlo.
- `"SF Mono"` names the system's monospaced font, which macOS installs under another name.
- A name that some faces of a family give themselves, as a font and its weight, such as `"CaskaydiaCove NFM SemiBold"`, names the whole family, and takes those faces as the regular text's.
- A family that is not installed gives the default one, and the log tells it. The styles that the file names then do not apply, as they are the missing family's.

### `size`

<sub>Platforms: macOS</sub>

The size of the text, in points.

| Values | Default |
|---|---|
| from `4` to `200`, whole or not | `13` |

The size is in points, so that the text takes the same room on a Retina display, at twice the pixels, as on another.

### `style`

<sub>Platforms: macOS</sub>

The face of the text that has no style, by the name that the font gives it, such as `"SemiBold"` or `"Light"`.

- **Default**: the family's regular face: its upright face nearest to the regular weight, of the normal width when the family has several, such as Condensed faces too. A face whose font tells no width takes the one that its name tells, as Condensed or Narrow. Of faces that still tie, as Hoefler Text's Regular and Ornaments, the one that macOS gives for the family alone.
- The case, spaces, hyphens and underscores do not matter: `"semi-bold"` is `SemiBold`.
- A style that the family does not have is an error, which lists the styles that it has.

### `bold-style`, `italic-style`, `bold-italic-style`

<sub>Platforms: macOS</sub>

The faces of bold, italic and bold italic text, named as `style` names its face.

- **Default of `bold-style`**: the family's upright face heavier than the regular one, nearest to Bold's weight, or one step past the regular one's: Regular takes Bold, SemiBold takes Bold, and Bold takes ExtraBold. A family without one draws its regular face heavier.
- **Default of `italic-style`**: the family's italic face nearest to the regular one's weight. A family without italics slants its regular face.
- **Default of `bold-italic-style`**: the family's italic face nearest to the bold one's weight. A family without one slants its bold face, or draws its italic one heavier.
- The three take faces of the regular one's width. An italic is a true italic rather than an oblique when the family has both. A family of italics alone takes its heavier italic as bold. Of two faces as near to a weight, the heavier is taken for Bold's and the lighter for the regular one's, whatever order macOS lists them in.

### `non-ascii-family`

<sub>Platforms: macOS</sub>

The family of every character past ASCII, such as `"Hiragino Sans"` for Japanese text.

- **Default**: the text's own family.
- Its faces follow the weights of the text's: bold text takes its bold face. The styles that `style` and the other keys name are the text's family's, not this one's.
- A character that this family lacks takes the text's own family, and then the font that macOS gives for it: the icons of a Nerd Font, such as those of a shell's prompt, stay the font's.
- A family that is not installed leaves that text to the text's family, and the log tells it.

### `line-height`, `cell-width`

<sub>Platforms: macOS</sub>

The height and the width of the grid's cells, as factors of the face's line height and of its characters' advance.

| Values | Default |
|---|---|
| from `0.5` to `3` | `1` |

A factor below one makes a closer grid, and can make a cell smaller than the glyphs of the face, which then overlap their neighbours' cells, or are cut where the window ends; a factor above one leaves room around them.

### `ligatures`

<sub>Platforms: macOS</sub>

Whether the glyphs that a font joins into ligatures, such as `->` or `!=`, are joined.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- Programming fonts that have ligatures, such as Cascadia Code or Fira Code, join `->`, `=>`, `!=` or `===` into one drawing, each keeping the cells of its characters.
- The cell under the cursor shows its own character, so that a ligature splits where you edit it.
- `false` turns off the font's ligatures, its contextual alternates too, which some fonts use for them.
- Emoji sequences are drawn whole either way: a family joined by ZWJ, a flag, a keycap such as 1️⃣, or a skin tone.

### `builtin-box-drawing`, `builtin-powerline`

<sub>Platforms: macOS</sub>

Whether Lokyr draws these characters itself, at the cell's size and to the pixel, rather than taking them from the font:

- `builtin-box-drawing`: the box drawing characters, such as `─ │ ┌ ╋ ║ ╭`, the block elements, such as `▀ ▄ █ ░`, and braille, such as `⣿`;
- `builtin-powerline`: the powerline symbols, U+E0B0 to U+E0BF, the arrows, half circles and triangles between the segments of a prompt.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- Drawn by Lokyr, lines meet across cells without a gap, blocks tile, and the powerline symbols fill the cell's height, whatever the font and `line-height`.
- `false` takes the font's glyphs, or those of the font that macOS gives for them; some fonts draw their own as a look of their own.
- The other icons of Nerd Fonts always come from the font.

### `antialias`

<sub>Platforms: macOS</sub>

Whether the edges of the glyphs are smoothed, as macOS draws text.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- `false` draws each pixel covered or not, as on old screens: crisp at small sizes on a display of a scale of 1, jagged otherwise. The glyphs that Lokyr draws itself follow it, and the shades, ░, ▒ and ▓, become patterns of a quarter, a half and three quarters of their pixels; emoji keep their smooth edges.
- `thin-strokes` needs antialiasing, and does nothing without it.

### `thin-strokes`

<sub>Platforms: macOS</sub>

Where the text is drawn of thinner strokes: without the darkening that macOS adds to the glyphs as it smooths fonts, which makes light text on a dark background look heavier.

| Values | Default |
|---|---|
| `"never"`, `"always"`, `"retina"`, `"dark"`, `"retina-dark"` | `"never"` |

- `"never"` draws the text as macOS does; `"always"` thin everywhere.
- `"retina"` thin on a display of a scale of 2 or more, as the Mac's own; `"dark"` thin where a cell's background is darker than its text; `"retina-dark"` thin where both hold.
- When font smoothing is off in macOS, as `defaults write -g AppleFontSmoothing -int 0` turns it off, every glyph is thin already.

## `[text]`

How the text's styles and colors are drawn.

### `bold`, `italic`

<sub>Platforms: macOS</sub>

Whether bold text takes the font's bold face, and italic text its italic face.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- `bold = false` draws bold text in the regular face: a program's bold then shows only by its color.
- `italic = false` draws italic text upright.

### `ansi-colors`

<sub>Platforms: macOS</sub>

Whether the colors that programs give the text and its background show.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- `false` draws all the text in the default colors, the theme's: the sixteen colors of the palette, the 256 and the RGB ones that programs set, for the text and its background, take the default text's and background's. An underline's own color, which programs set with SGR 58, as Neovim does for its diagnostics, takes the text's.
- Inverse text still swaps its colors, and the selection and the cursor keep theirs.

### `bold-is-bright`

<sub>Platforms: macOS</sub>

Whether bold text in one of the palette's first eight colors takes its bright color, of the eight after: bold red as bright red.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- Most programs choose their bright colors themselves, so that the default leaves bold text in its own color; some older ones mean bold to brighten it.
- Bold text still takes the bold face, as `bold` says.

### `blink`

<sub>Platforms: macOS</sub>

Whether text that programs make blink, with the escape sequences SGR 5 and 6, blinks.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The text blinks as the cursor does: half a second shown and half a second hidden, only in the window that has the focus.
- It stops blinking, and stays shown, 15 seconds after the last key or output, as the cursor stops, so that an idle window keeps the Mac idle.
- `false` shows it steadily.
- Each blink draws a frame of the window, as the cursor's blinking does; a later version blinks both without the app's work.

## `[cursor]`

The cursor that programs start with. Programs change its shape and its blinking, as vim does in insert mode, with the escape sequence DECSCUSR; `CSI 0 SP q`, which programs send to give the cursor back, and a reset bring these settings back.

### `shape`

<sub>Platforms: macOS</sub>

The shape of the cursor.

| Values | Default |
|---|---|
| `"block"`, `"bar"`, `"underline"`, `"hollow"` | `"block"` |

- `"block"` covers the cell, and the text under it takes the cell's background.
- `"bar"` is a vertical line at the cell's left edge.
- `"underline"` is a line at the cell's bottom edge.
- `"hollow"` is a block of its edges alone. A window without the focus always shows a hollow block, whatever the shape.

Programs can choose among the first three alone, as DECSCUSR has no hollow block.

### `blink`

<sub>Platforms: macOS</sub>

Whether the cursor blinks.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The cursor stays on for half a second and off for another.
- It stops blinking, and stays on, 15 seconds after the last key or the last output, so that an idle window keeps the Mac idle too. A key, output, or the window's focus makes it blink again.
- Only the window that has the focus blinks, and a cursor that the program hides, as full-screen programs such as `htop` hide it, does not blink.
- A program can turn blinking on or off with DECSCUSR. The escape sequence of mode 12, which some programs send as they start, turns blinking on too; its reset, which most full-screen programs send as they end, leaves this setting as it is.

### `thickness`

<sub>Platforms: macOS</sub>

The width of a bar and the height of an underline, in points, so that they keep their look on a Retina display.

| Values | Default |
|---|---|
| from `0.5` to `20` | the font's underline thickness |

A hollow block's edges always take the font's underline thickness.

## `[window]`

### `close-on-exit`

<sub>Platforms: macOS</sub>

What a window does when its program ends, such as the shell after `exit`.

| Values | Default |
|---|---|
| `"clean"`, `"always"`, `"never"` | `"clean"` |

- `"clean"` closes the window when the program exits with the status 0, and otherwise keeps it open, with a line that tells how the program ended, such as `[the program exited with code 1; a key closes the window]`, until you press a key, one that types or acts: a modifier, Caps Lock, Fn or a key with Command leave it open.
- `"always"` closes the window.
- `"never"` keeps it open, with the line, until you press a key.

The login shell's status is its own: on macOS, `login`, which starts it, hides it, and the shell tells it to Lokyr as it ends. A status past 128 tells the signal that ended it, as shells give it.

Closing the last window quits Lokyr.

### `padding-x`, `padding-y`

<sub>Platforms: macOS</sub>

The room between the grid and the window's edges, in points: `padding-x` at the left and the right, `padding-y` at the top and the bottom.

| Values | Default |
|---|---|
| from `0` to `100`, whole or not | `4` |

- In points, so that it keeps its look on a Retina display.
- A window is seldom a whole number of cells: the room that the cells leave over goes to the right and the bottom, past the padding, so that the text stays where it is as the window's size changes.
- The padding takes the background of the cells without a color of their own: a background that a program colors, such as a status line's, stops at the grid.

### `background-opacity`

<sub>Platforms: macOS</sub>

The opacity of the window's background, from 0, clear, to 1, opaque: below 1, what is behind the window shows through it.

| Values | Default |
|---|---|
| from `0` to `1` | `1` |

- Only the default background lets it show, with the padding and the title bar, which take it: a background that a program colors, such as a status line's, stays opaque, as do the selection, the cursor and the text.
- At `1` the window is opaque, as it is by default.

### `background-blur`

<sub>Platforms: macOS</sub>

The radius of the blur of what shows through the background, in points.

| Values | Default |
|---|---|
| from `0` to `100`, whole or not | `0`, no blur |

- The blur shows only below a `background-opacity` of 1.
- macOS has no public way to blur what is behind a window: Lokyr asks the window server through a private call, which takes whole points, so that the radius is rounded. A larger radius asks more of the window server.

## `[terminal]`

What programs are told of the terminal, and how it counts and keeps their text. The widths must match how the programs count characters, or their lines go wrong; the defaults suit most of them.

### `term`

<sub>Platforms: macOS</sub>

The `TERM` that programs see, which tells them what the terminal can do: letters, digits, `.`, `_`, `+` and `-`, 64 at most.

| Values | Default |
|---|---|
| a terminfo name, as `"xterm-256color"`, `"xterm-lokyr"`, `"xterm"`, `"vt220"` | `"xterm-256color"` |

- It changes only what programs are told, never what the terminal does, which is the same whatever the name.
- `"xterm-lokyr"` describes Lokyr itself, with the capabilities that the plain names lack, as undercurls and the keyboard protocol that reports every key; Lokyr carries its entry, and points programs to it, but a host that you reach over ssh lacks it, and its programs then fall back to plain behavior. A Lokyr that runs outside its bundle without the entry gives its windows `"xterm-256color"`, and the log tells it.
- It applies to the windows that open after the change: the programs of a window keep the `TERM` that they started with.

### `unicode-widths`

<sub>Platforms: macOS</sub>

Which width tables the terminal counts characters by, to place them in their cells.

| Values | Default |
|---|---|
| `"current"`, `"8"` | `"current"` |

- `"current"` follows the tables of the Unicode version that Lokyr knows, as current programs do: most emoji take two cells.
- `"8"` follows those of Unicode 8, for old programs that count emoji as one cell.
- It applies to the text that comes after the change, in the open windows too.

### `ambiguous-width`

<sub>Platforms: macOS</sub>

The cells of an East Asian Ambiguous character, as `α`, `→` and `○`.

| Values | Default |
|---|---|
| `1`, `2` | `1` |

- `2` suits programs and fonts that draw them wide, as some of China, Japan and Korea do; most programs count them as 1.
- The icons of Nerd Fonts, in Unicode's private area, are ambiguous too: with `2`, a prompt that shows them, as oh-my-posh's and Powerlevel10k's do, takes more cells than the shell counts, and the line that you type goes wrong, its characters doubled or out of place.
- It applies to the text that comes after the change, in the open windows too.

### `normalization`

<sub>Platforms: macOS</sub>

The normal form that printed text takes, which changes how a character and its accents are kept, and what a selection copies of them.

| Values | Default |
|---|---|
| `"none"`, `"nfc"`, `"nfd"`, `"hfs+"` | `"none"` |

- `"none"` keeps the text as programs write it.
- `"nfc"` composes it: `e` and a combining acute accent become `é`.
- `"nfd"` decomposes it, the other way round; `"hfs+"` decomposes it as the file names of Apple's former file system, HFS+, are.
- It applies to the text that comes after the change, in the open windows too.

## `[scrollback]`

The scrollback holds the rows that went up past the screen's top. The wheel and the trackpad scroll through it, and with Shift held they scroll it even when a program takes the mouse.

### `scroll-on-output`

<sub>Platforms: macOS</sub>

Whether a program's output brings a view scrolled up into the scrollback back down to the screen.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- With `false`, the view stays on the rows you scrolled to while the program goes on writing, so that you can read what scrolled past.
- Typing, pasting and the input method's text always bring the view back down, whatever the key says; a modifier alone, as Shift, does not.
- A new value applies at once, to the windows that are open too.

### `limit`

<sub>Platforms: macOS</sub>

The memory that a window's scrollback takes, its screen's included, in MiB, whole or not.

| Values | Default |
|---|---|
| from `1` to `16384` | `10` |

- The oldest rows go as new ones pass it; 10 MiB keep about 10,000 rows of 120 columns, whatever their length.
- A smaller value applies to the open windows too, which give back their oldest rows as their next ones come.

### `budget`

<sub>Platforms: macOS</sub>

The memory that the scrollbacks of all the windows share, in MiB, whole or not.

| Values | Default |
|---|---|
| from `1` to `16384` | `100` |

- A window that needs more than the budget leaves gives back its own oldest rows, never another's: a busy window does not take the scrollback of a quiet one.
- A window's screen always has its rows, past the budget too.
- A new value applies at once, to the windows that are open too.

## `[keyboard]`

### `option-as-meta`

<sub>Platforms: macOS</sub>

Which Option keys act as Meta: they send an escape before the key, rather than typing the characters that the keyboard layout gives them.

| Values | Default |
|---|---|
| `"none"`, `"left"`, `"right"`, `"both"` | `"none"` |

- The default is `"none"`, as Option types characters that many layouts need, such as `@`, `#`, `[`, `]`, `{`, `}` and `~` on the Italian layout.
- `"left"` lets the right Option type those characters, and makes the left one Meta, for the shortcuts of shells and editors such as `Meta-b` and `Meta-f`.
- An Option that acts as Meta does not compose either: its dead keys, such as Option and `e` on the US layout, send Meta and `e`.
- With Meta, a key whose character is past ASCII sends the key in its place on the US layout: Meta and `ò` on the Italian layout send Meta and `;`.

## `[clipboard]`

What programs do with the clipboard. A program writes to it with the escape sequence OSC 52, as tmux and Neovim copy over ssh: the write always reaches it, up to 1 MiB of text, while the primary selection, which macOS lacks, takes nothing.

### `read`

<sub>Platforms: macOS</sub>

Whether a program can read the clipboard, with OSC 52.

| Values | Default |
|---|---|
| `"never"`, `"ask"` | `"never"` |

- `"never"` answers nothing: a program that runs elsewhere, over ssh, could otherwise read what you copied, such as a password.
- `"ask"` asks you with an alert at each read, on the program's window, whose default button refuses; the program gets the clipboard's text only if you allow it.
- It applies as you save the file, to the windows that are open too.

### `confirm-paste`

<sub>Platforms: macOS</sub>

When a paste with line breaks asks you first: a program that does not ask for bracketed paste, as `cat` or a plain `sh`, takes each line break as Return, which can run a command.

| Values | Default |
|---|---|
| `"auto"`, `"always"`, `"never"` | `"auto"` |

- `"auto"` asks when the program does not ask for bracketed paste and the text has several lines. A single line with a line break after it, as a command copied with its line's end, pastes without the line break, so that the command waits for your Return. A shell that asks for bracketed paste, as zsh, fish and recent versions of bash do, takes any paste as typed text, which runs nothing: it is never asked about.
- `"always"` asks whenever the text has a line break, into any program.
- `"never"` pastes as it is.
- The question shows the text's first lines, as the program would get them, and three buttons: Paste, the default one; Paste as One Line, which puts a space for each line break and leaves out the last one; and Cancel. Its checkbox, Don't ask again, sets this key to `"never"` in your config file as you paste, keeping the rest of the file and its comments, in the file that a link points to; a config file with an error is left as it is, and the choice lasts until Lokyr quits.
- It applies as you save the file.

## `[bell]`

What a program's bell does, the character BEL that it prints, as `printf '\a'`, or as the agents that run in the terminal ring it when they wait for you. Each kind takes its own key; they apply as you save the file. Bells that come within a fifth of a second of the last one in the window do nothing more.

### `sound`

<sub>Platforms: macOS</sub>

Whether the bell plays the system's alert sound, the one that System Settings chooses on macOS, in Sound.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

### `flash`

<sub>Platforms: macOS</sub>

Whether the window flashes, for a tenth of a second, when its bell rings while it has the focus.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

### `bounce`

<sub>Platforms: macOS</sub>

Whether the Dock's icon bounces once when the bell of a window without the focus rings, while Lokyr is not the active application.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

### `mark-title`

<sub>Platforms: macOS</sub>

Whether a window without the focus whose bell rang shows 🔔 before its title, until it takes the focus.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

## `[app]`

### `icon`

<sub>Platforms: macOS</sub>

The variant of Lokyr's icon that shows while Lokyr runs, in the Dock and the app switcher, by its name.

| Values | Default |
|---|---|
| `"jormungandr"`, `"laguz"` | `"jormungandr"` |

- A new variant applies at once, as you save the file.
- The Finder and the Apps launcher show `"jormungandr"` whatever the key says, as do the Dock and the app switcher while Lokyr does not run: they show the app's own icon, which an app cannot change without changing the app itself.
- A name that is not one of Lokyr's variants keeps the default, and the log names the variants; the rest of the file applies.

### `language`

<sub>Platforms: macOS</sub>

The language of what Lokyr shows: its menus, the titles of its commands, and the line of a window whose program ended.

| Values | Default |
|---|---|
| `"system"`, `"en"`, `"it"` | `"system"` |

- `"system"` takes the first of your preferred languages, in the system's settings, that Lokyr has, and English when it has none of them.
- A language with its region, as `"it-IT"`, takes its language.
- A new value applies at once, as you save the file, to the menus too; a line already shown stays as it was.
- A language that Lokyr does not have takes the system's, and the log names those that it has; the rest of the file applies.
- The prompts that macOS shows when a program asks for the camera, the contacts and the like follow the system's language, whatever the key says.
- The log, for the reports of problems, stays in English.

## `[keys]`

The bindings of keys, and of the wheel, to Lokyr's commands, over the defaults. A key that a binding takes runs its command, and never reaches the program; the others go to the program, as they always do.

### Bindings

<sub>Platforms: macOS</sub>

Each entry of the section is a binding: its name is the keys, in quotes, and its value the command that they run.

```toml
[keys]
"cmd+shift+n" = "window.new"
"ctrl+a>n" = { command = "window.new", when = "!alternate-screen" }
"cmd+q" = false
```

- **The keys**: modifiers, each followed by `+`, and the key: `cmd+shift+n`.
  - The modifiers are `cmd`, `ctrl`, `alt` or `opt`, and `shift`; `command` and `super` are `cmd` too, `control` is `ctrl`, and `option` is `alt`.
  - The key is the character that it types without any modifier, Shift neither, as `]` in `cmd+shift+]`, or `+` in `cmd++`; or a named key: `enter`, `tab`, `backspace`, `escape` or `esc`, `space`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1` to `f35`; or a step of the wheel, `wheel-up`, `wheel-down`, `wheel-left` or `wheel-right`, which a trackpad's scroll makes too, a step a row.
  - Your keyboard layout gives the characters. A key whose character no binding takes runs the binding of the character at its place on the US layout, when its own is past ASCII, as `è` on the Italian layout runs `cmd+[`, or when both are punctuation, as the Italian `+` runs `cmd+]`, so that keys that take Option on your layout still work. An ASCII letter or digit is always yours: on AZERTY, Command-A is `cmd+a`, never the `cmd+q` at its place on the US layout.
  - Two entries of the same keys, written two ways, as `alt+x` and `opt+x`, or `cmd+shift+n` and `shift+cmd+n`, are an error.
- **A sequence** of keys, each after the one before, parts them with `>`: `ctrl+a>n` is Control-A, then N. After the first keys, Lokyr waits for the next one, as long as it takes; a key that no sequence goes on with goes to the program, with the keys before it, as does a key that no binding can name, as the keypad's, and the input method's text. A step of the wheel makes no sequence.
- **The value**:
  - a command, as `"window.new"`;
  - a table with the `command`, its `args`, as `{ command = "tabs.goto", args = { index = 1 } }`, and its context, `when`;
  - an array of such tables, for the keys in several contexts;
  - `false`, which takes away the defaults of the keys.
- **The context**, `when`, says when a binding holds: the names `pane.focused`, `selection` and `alternate-screen`, with `!`, `&&`, `||` and parentheses, as `pane.focused && !selection`. `alternate-screen` holds while a program such as vim or less shows its own screen. A binding without `when` holds everywhere. An expression nests 32 levels deep at most.
- **Which binding wins**: of those of the keys that hold, the last; your bindings come after the defaults.
- A key that a binding takes runs its command again as it repeats, held down, and its release reaches nothing.
- For now, the keys of the menus' items, Command-N and Command-Q, stay theirs: the menus take them before the bindings see them.
- A binding of a command that does not exist reads, and tells it in the log when you press its keys, as a plugin's commands come once the plugin starts.

The defaults:

| Keys | Command | What it does |
|---|---|---|
| `cmd+n` | `window.new` | Opens a window |
| `cmd+w` | `window.close` | Closes the window; while a program other than a shell runs in it, as vim or ssh, it asks you first, as closing ends the program. tmux and screen, whose sessions go on, do not ask |
| `cmd+q` | `app.quit` | Quits Lokyr, asking you first while programs other than shells run in its windows; so does Quit in the Dock, and a logout waits for your answer |
| `cmd+c` | `pane.copy` | Copies the selection |
| `cmd+v` | `pane.paste` | Pastes the clipboard's text, asking you first when [`confirm-paste`](#confirm-paste) says so |
| `cmd+a` | `pane.select-all` | Selects the scrollback and the screen, to the last row that shows something |
| `cmd+k` | `pane.clear` | Clears the scrollback and the screen but the prompt, which goes to the top; on the screen of a program such as vim, it does nothing |
| `cmd+=`, `cmd+shift+=`, `cmd++` | `font.bigger` | Makes the text a point larger, in every window, up to 200 |
| `cmd+-` | `font.smaller` | Makes the text a point smaller, down to 4 |
| `cmd+0` | `font.reset` | Gives the text the size of [`size`](#size) again |
| `cmd+up`, `cmd+down` | `pane.scroll-line-up`, `pane.scroll-line-down` | Scrolls the view a line up or down |
| `cmd+pageup`, `shift+pageup`, `cmd+pagedown`, `shift+pagedown` | `pane.scroll-page-up`, `pane.scroll-page-down` | Scrolls the view a page up or down |
| `cmd+home`, `cmd+end` | `pane.scroll-top`, `pane.scroll-bottom` | Scrolls the view to the scrollback's first row, or back to the screen |
