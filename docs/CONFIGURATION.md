# Configuration

Lokyr reads its settings from one TOML file. This guide lists every key that the file can hold: what it does, the values it takes and its default.

Every key and every section can be left out, and then takes its default: an empty file, or no file at all, gives Lokyr as it ships.

Under each key's name, a small line tells the platforms where the key applies: macOS alone for now, as Lokyr's first version runs on macOS. As Lokyr comes to other systems, the line names them, and a key that one system alone has says so.

## The file

- **Where it is**: `~/.config/lokyr/config.toml`, on macOS too, so that it can live with your dotfiles. When `XDG_CONFIG_HOME` is set to an absolute path, the file is `$XDG_CONFIG_HOME/lokyr/config.toml` instead.
  - A variable that your shell's startup files set, such as `XDG_CONFIG_HOME` in `~/.zshrc`, reaches Lokyr only when you start it from that shell: started from the Finder or the Dock, Lokyr reads `~/.config/lokyr/config.toml`.
  - When the file's directory does not exist, Lokyr makes it, empty, as it starts, so that a config that you write there later applies at once.
- **Opening it**: Lokyr › Settings… (⌘,) opens the file in your text editor, the application that opens plain text, as TextEdit or the one that you chose in the Finder. When there is no file, it makes one first: every key, commented out with its default and its text, under its section, so that you take a key's `#` away to change it, and the others keep the defaults to come. A key without a default shows an example of its value, as `family = "Menlo"`, and `[keys]` each default binding, a line a key, under what it does. In an editor that changes the quotes that you type, as TextEdit's Edit › Substitutions › Smart Quotes does, turn that off: TOML takes straight quotes alone. When the file cannot be made, as in a folder that you cannot write, or no editor opens it, an alert tells why.
- **Links**: the file, or its directory, can be a symbolic link, as dotfiles managers such as GNU Stow make it. Lokyr watches the file the link points to as well, and follows a link that is pointed elsewhere: the edits of its new target apply too. A target that goes, as a checkout of the dotfiles removes and writes it, applies again as it comes back; while it is gone, the defaults apply.
- **Reloading**: Lokyr applies the file as soon as you save it, without a restart. Saving from vim, which writes another file and renames it over the config, works as well as saving from any other editor. Removing the file, or its directory, brings the defaults back.
- **All or nothing**: a file applies whole, or not at all. When it has an error, none of its values apply, and the settings in force stay as they were. At startup, an invalid file gives the defaults. A plugin's section, which its plugin checks, applies on its own, as [`[plugins.<namespace>]`](#pluginsnamespace) says.
- **Errors**: an error tells the file, the line and the column, and what is wrong. For a key that Lokyr does not know, it names the nearest key that it knows:

  ```text
  lokyr: error 109 (usage): /Users/you/.config/lokyr/config.toml:3:1: unknown key `sise` in `[font]`; did you mean `size`?; none of the config applies, and the one in force stays
  ```

  The error shows in a bar across the top of every window, under the titlebar, until you save a file that reads, or click its ×; it goes to the log too: the terminal that started Lokyr, when you start it from one, or else, as from the Finder or the Dock, the file `~/Library/Logs/Lokyr/lokyr.log`, which the Console app shows. A font family that is not installed shows in the bar the same way, until the file names one that is.
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
system-weight = true                 # true | false: the text of the weight of macOS's

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
blink-interval = 600                 # ms on, then off, from 100 to 5000
blink-timeout = 30                   # seconds after a key or output; 0 never stops
smooth-blink = "custom"              # off | breathing | linear | fast | slow | subtle | custom
fade-in = 300                        # with "custom": ms, from 0 to 10000
fade-in-curve = "ease-out"           # linear | ease-in | ease-out | ease-in-out
visible = 700                        # ms held visible, from 0 to 10000
fade-out = 400
fade-out-curve = "ease-in"
hidden = 200                         # ms held hidden

[window]
columns = 120                        # a new window's size, in cells, from 1 to 1000
rows = 40
remember-frame = false               # true | false: the first window where the last one was
close-on-exit = "never"              # clean | always | never
start-directory = "last"             # home | last | a path: the first window's folder
new-directory = "~/Projects"         # inherit | home | a path: a new window's folder
padding-x = 6                        # points, from 0 to 100
padding-y = 4
gutter = false                       # true | false: the commands' marks at the panes' left
background-opacity = 0.9             # from 0 to 1
background-blur = 20                 # points, from 0 to 100
title-size = false                   # true | false: the grid's size in the title
title-shortcut = false               # true | false: ⌥⌘1 at the titlebar's right
opaque-titlebar = true               # true | false: the titlebar opaque over a translucent window

[interface]
font-family = "Helvetica Neue"       # the system's interface font when absent
font-size = 14                       # points, from 4 to 200

[slots]
tab-bar = "lokyr/tabs"               # the plugin whose tab bar shows; the first that has one when absent
pane-header = "lokyr/command-header" # the plugin whose panes' headers show; likewise

[terminal]
term = "xterm-lokyr"                 # the TERM that programs see
unicode-widths = "8"                 # current | 8: the width tables
ambiguous-width = 2                  # 1 | 2: the cells of an ambiguous character
normalization = "nfc"                # none | nfc | nfd | hfs+
shell-integration = false            # true | false: the shell tells its prompts and its folder

[scrollback]
scroll-on-output = true              # true | false: output brings the view back down
limit = 20                           # MiB of a window's scrollback, from 1 to 16384
budget = 200                         # MiB that the windows' scrollbacks share

[keyboard]
option-as-meta = "left"              # none | left | right | both

[clipboard]
read = "ask"                         # never | ask: a program's read of the clipboard
confirm-paste = "always"             # auto | always | never: a paste with line breaks asks first
copy-on-select = true                # a selection by the mouse copies its text as it ends

[links]
urls = "open"                        # ask | open: a URL that ⌘-click opens
files = "reveal"                     # ask | open | reveal: a file that ⌘-click opens
search-engine = "duckduckgo"         # google | duckduckgo | bing | ecosia | kagi | startpage, or a URL of {query}

[bell]
sound = true                         # true | false: the system's alert sound
flash = false                        # true | false: a flash of the window that has the focus
bounce = false                       # true | false: the Dock's icon, for a window in the background
mark-title = false                   # true | false: 🔔 in its title until it takes the focus

[notifications]
long-command = 30                    # seconds, from 1 to 86400, or 0 for none: a notification as a command this long ends

[theme]
light = "Lokyr Light"                # the light appearance's theme, a file of themes/, held or built in
dark = "Nightfall"                   # the dark appearance's theme
minimum-contrast = 3.0               # 1 to 21: the least contrast of the terminal's text

[app]
icon = "laguz"                       # jormungandr | laguz
language = "it"                      # system | en | it
appearance = "dark"                  # system | light | dark
quit-after-last-window-closed = true # true | false: Lokyr quits as its last window closes

[updates]
check = false                        # true | false: a check for a newer version, once a day at most

[debug]
performance-overlay = true           # true | false: a panel of the performance over each window
performance-corner = "bottom-right"  # top-right | top-left | bottom-right | bottom-left

[keys]
"cmd+shift+n" = "window.new"         # keys, and the command that they run
"ctrl+a>n" = { command = "window.new", when = "!alternate-screen" }
"cmd+q" = false                      # takes the binding away; the menu's Command-Q stays, for now
"alt+backspace" = { command = "pane.send-text", args = { text = "\u0017" } }   # sends Control-W
"global:ctrl+`" = "window.toggle-quick-terminal"   # from every app: the quick terminal

# A plugin's own keys, in a section of its namespace, as its schema has them;
# `enabled` turns any plugin off:
# [plugins.<namespace>]
# enabled = false                    # true | false: the plugin runs
# key = "value"

[plugins.tabs]
position = "top"                     # top | bottom | left | right: where the tab bar goes
style = "unified"                    # standard | unified | seamless: the bar's look
hide-when-single = true              # true | false: no bar for a single tab
show-index = true                    # true | false: ⌘1 to ⌘9 on the first nine tabs
close-button = "hover"               # always | hover | never: a tab's close button
column-width = 200                   # points, 60 to 600: the bar's column on the left or the right

[plugins.splits]
dim-inactive = 0.15                  # 0 to 1: how much the panes without the keyboard fade
pane-titles = false                  # true | false: each pane's title across its top
focus-follows-mouse = false          # true | false: the keyboard goes to the pane under the pointer

[plugins.quick-terminal]
position = "bottom"                  # top | bottom | left | right | center
size = "40%,80%"                     # into the screen, then along the edge: 1% to 100%, or points as "300px"
screen = "mouse"                     # main | mouse | menu-bar
animation = 150                      # ms, from 0 to 2000
autohide = false                     # true | false: hides as it loses the focus
spaces = "remain"                    # move | remain

[plugins.search]
case = "smart"                       # smart | insensitive: whether case counts while the bar's case toggle is off
highlight-all = true                 # true | false: every match that shows highlighted
wrap-around = true                   # true | false: a step past the last match goes on from the first

[plugins.command-header]
background = "panel.bg"              # the theme's token of the pill's background, by its name
text = "text"                        # the theme's token of its text
border = "panel.border"              # the theme's token of its border, as "accent"
line = "panel.border"                # the theme's token of the line across the pane
hover = "button.hover.bg"            # the theme's token over its background under the pointer
font-size = 0                        # points, 0 to 200, of the terminal's font; 0 for its own size
font-weight = "regular"              # light | regular | medium | semibold | bold | heavy
padding-x = 8                        # points, 0 to 40, left and right of the text
padding-y = 1                        # points, 0 to 20, above and below it
align = "left"                       # left | center | right: where its pill sits along the line
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
- **Icons of the Nerd Fonts**, which prompts and tools such as `eza --icons` show, take the font's own when it is a Nerd Font, and otherwise Symbols Nerd Font Mono, which Lokyr includes, so that they show with any font, a Nerd Font installed or not. Its icons are drawn smaller where they would be wider than their cell, in its middle, so that they leave the next cell alone.

### `family`

<sub>Platforms: macOS</sub>

The font family, by its name, as Font Book shows it on macOS, such as `"Menlo"` or `"CaskaydiaCove NFM"`.

- **Default**: the system's monospaced font, SF Mono.
- The case does not matter: `"menlo"` is Menlo.
- `"SF Mono"` names the system's monospaced font, which macOS installs under another name.
- A name that some faces of a family give themselves, as a font and its weight, such as `"CaskaydiaCove NFM SemiBold"`, names the whole family, and takes those faces as the regular text's.
- A family that is not installed gives the default one, and the notice bar tells it, as the log does. The styles that the file names then do not apply, as they are the missing family's.
- `lokyr +list-fonts` lists the families that are installed, and `--monospace` those that are monospaced.

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
- A style that the family does not have is an error, which lists the styles that it has, as `lokyr +list-fonts --family <name>` does.

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
- A character that this family lacks takes the text's own family, then, for an icon of the private use areas, Symbols Nerd Font Mono, which Lokyr holds, and then the font that macOS gives for it: the icons of a Nerd Font, such as those of a shell's prompt, stay the font's.
- A family that is not installed leaves that text to the text's family, and the notice bar tells it, as the log does.

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
- `false` takes the font's glyphs, and, for the powerline symbols that the font lacks, Lokyr's own, so that they fill the cell still; box drawing that the font lacks comes from another font of the system's. Some fonts draw their own as a look of their own.
- The other icons of Nerd Fonts come from the font, or from Symbols Nerd Font Mono where the font lacks them.

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

### `system-weight`

<sub>Platforms: macOS</sub>

Whether the text takes the weight of the text of macOS's own apps, as Lokyr's tabs and panels do.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- `true` draws dark text on a light background thinner, as macOS draws it: by a quarter at a scale of 1, by a sixth at 2. Light text on a dark background stays nearly as it is.
- `false` draws the glyphs' edges as the font gives them, as before, which makes dark text on a light background look heavier than macOS's.
- It costs nothing measurable, on the GPU or on the CPU.
- It takes `thin-strokes` along: the darkening that macOS adds as it smooths fonts stays, or goes, as that key says. A thin glyph, or every glyph when font smoothing is off in macOS, stays as drawn, as macOS's own text then does.
- The texts that Lokyr draws in the terminal's font, as the notices across the windows' top, the size shown as a window resizes and the performance's panel, follow it too.

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

- The text blinks on the cursor's clock: shown and hidden for [`blink-interval`](#blink-interval) each, half a second by default, switched even when the cursor fades, and only in the window that has the focus.
- It stops blinking, and stays shown, [`blink-timeout`](#blink-timeout) after the last key or output, 15 seconds by default, as the cursor stops.
- `false` shows it steadily.
- macOS's window server blinks it, on a layer of its own over the rows that hold it, which wakes Lokyr no more than steady text does. The layer takes some memory of the GPU's while blinking text shows, as much as a few rows of the window, and none once it goes.

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

- The cursor stays on for [`blink-interval`](#blink-interval) and off for as long, or fades out and in, as [`smooth-blink`](#smooth-blink) says.
- It stops blinking, and stays on, [`blink-timeout`](#blink-timeout) after the last key or the last output, 15 seconds by default, so that an idle window keeps the Mac idle too. A key, output, or the window's focus makes it blink again.
- macOS's window server blinks the cursor, which wakes Lokyr no more than a steady cursor does, faded or not.
- Only the window that has the focus blinks, and a cursor that the program hides, as full-screen programs such as `htop` hide it, does not blink.
- A program can turn blinking on or off with DECSCUSR. The escape sequence of mode 12, which some programs send as they start, turns blinking on too; its reset, which most full-screen programs send as they end, leaves this setting as it is.

### `thickness`

<sub>Platforms: macOS</sub>

The width of a bar and the height of an underline, in points, so that they keep their look on a Retina display.

| Values | Default |
|---|---|
| from `0.5` to `20` | the font's underline thickness |

A hollow block's edges always take the font's underline thickness.

### `blink-interval`

<sub>Platforms: macOS</sub>

How long the cursor stays on, then off, as it blinks without [`smooth-blink`](#smooth-blink), in milliseconds.

| Values | Default |
|---|---|
| from `100` to `5000` | `500` |

Text that blinks, as the escape sequence SGR 5 makes it, takes the same interval.

### `blink-timeout`

<sub>Platforms: macOS</sub>

How long the cursor blinks after the last key or output, in seconds; then it stays on.

| Values | Default |
|---|---|
| from `0` to `3600`; `0` never stops | `15` |

A cursor that blinks for ever wakes Lokyr no more than one that stops, as the window server blinks it; the window server itself shows the window again at each change of the cursor's opacity, through each frame of a fade with [`smooth-blink`](#smooth-blink), which a blinking that stops spares it.

### `smooth-blink`

<sub>Platforms: macOS</sub>

Whether the cursor fades out and in as it blinks, rather than switching, and how: by one of five presets, or by the keys that follow.

| Values | Default |
|---|---|
| `"off"`, `"breathing"`, `"linear"`, `"fast"`, `"slow"`, `"subtle"`, `"custom"` | `"off"` |

- The presets, in milliseconds:

  | Preset | Fade in | Held visible | Fade out | Held hidden | Curves |
  |---|---|---|---|---|---|
  | `"breathing"` | 900 | 500 | 900 | 500 | ease in and out |
  | `"linear"` | 400 | 700 | 400 | 500 | linear |
  | `"fast"` | 200 | 700 | 200 | 200 | ease in and out |
  | `"slow"` | 500 | 1000 | 750 | 200 | ease in and out |
  | `"subtle"` | 150 | 900 | 150 | 200 | ease in and out |

- `"custom"` takes [`fade-in`, `fade-out`](#fade-in-fade-out), [`fade-in-curve`, `fade-out-curve`](#fade-in-curve-fade-out-curve) and [`visible`, `hidden`](#visible-hidden), whose defaults are Breathing's.
- With Reduce motion on, in System Settings › Accessibility › Display on macOS, the cursor switches rather than fading, at each fade's middle.
- What it costs: the window server composes the window at each refresh of a fade, while Lokyr itself does no more work. Measured on an M5 Pro, fades over half of the cycle, of 100, 500, 500 and 100 ms, took about a fifth of a core of the window server's CPU more, and fades over a sixth of it, about a twelfth; the cost grows with the share of the cycle that fades, from 21 % for `"subtle"` to 64 % for `"breathing"`. It lasts while the cursor blinks, in the window that has the focus, until [`blink-timeout`](#blink-timeout). A cursor that switches, the default, costs nothing measurable.

### `fade-in`, `fade-out`

<sub>Platforms: macOS</sub>

With `smooth-blink = "custom"`, how long the cursor fades in, and out, in milliseconds.

| Values | Default |
|---|---|
| from `0` to `10000` | `900` |

A fade of `0` switches. A cycle of fades and holds shorter than 50 ms does not blink.

### `fade-in-curve`, `fade-out-curve`

<sub>Platforms: macOS</sub>

With `smooth-blink = "custom"`, how each fade goes: at a constant rate, slow at its start, slow at its end, or both.

| Values | Default |
|---|---|
| `"linear"`, `"ease-in"`, `"ease-out"`, `"ease-in-out"` | `"ease-in-out"` |

### `visible`, `hidden`

<sub>Platforms: macOS</sub>

With `smooth-blink = "custom"`, how long the cursor holds visible, after it fades in, and hidden, after it fades out, in milliseconds.

| Values | Default |
|---|---|
| from `0` to `10000` | `500` |

## `[window]`

### `columns`, `rows`

<sub>Platforms: macOS</sub>

The size of a new window, in columns and rows of the font's cells.

| Values | Default |
|---|---|
| whole numbers, from `1` to `1000` | `100` columns, `30` rows |

- The window is of that many cells, with the padding around them, below the titlebar; the display that it opens on, which can be smaller, can make it smaller.
- While [`remember-frame`](#remember-frame) is on, the first window, as Lokyr starts, takes the place and the size of the last window that you moved or resized instead, and the windows that you open after it take that size, at a place of their own: these keys apply until a window keeps one.
- It applies to the windows that open after you save the file; those that are open keep their size.

### `remember-frame`

<sub>Platforms: macOS</sub>

Whether the first window, as Lokyr starts, opens where the last window that you moved or resized was, of its size.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The windows that you open after the first take the size of the last window that you moved or resized, or [`columns` and `rows`](#columns-rows) while no window kept one, down and to the right of the window that has the focus, or of the one in front, as when a question is open on it; within the display. Where Lokyr places a window is not kept, only where you move or resize it.
- A window that was on a display that is no longer there opens on one that is.
- Lokyr keeps the place in its preferences of macOS, not in the config file; a window in full screen leaves it as it was.
- It applies as you save the file, to the windows that you move or resize from then, and to the next start.

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

Closing the last window leaves Lokyr open, unless [`quit-after-last-window-closed`](#quit-after-last-window-closed) says otherwise.

### `start-directory`

<sub>Platforms: macOS</sub>

The folder that the first window's shell starts in, as Lokyr starts.

| Values | Default |
|---|---|
| `"home"`, `"last"`, or a path from `/` or `~` | `"home"` |

- `"home"` starts in your home folder.
- `"last"` starts in the folder of the last window that closed, or of the window in front as you quit Lokyr with several open: the shell's folder, or that of the program that ran in it, as `vim`'s. Lokyr keeps it in `~/Library/Application Support/lokyr/state/` on macOS, not in the config file.
- A path, as `"~/Projects"`, starts there.
- A folder that is not there, or that you cannot enter, starts in your home folder, and the log tells why.
- It applies at the next start.

### `new-directory`

<sub>Platforms: macOS</sub>

The folder that a new window's shell starts in, as ⌘N opens one, and a new tab's or a split's.

| Values | Default |
|---|---|
| `"inherit"`, `"home"`, or a path from `/` or `~` | `"inherit"` |

- `"inherit"` starts in the folder of the program in the window that has the focus, or that had it last while Lokyr is not active, as from the Dock's menu: the shell's folder, or that of the program that runs in it, as `vim`'s; in your home folder without a window, or when the folder cannot be read, as a program of another user's, as `sudo`'s.
- A new tab or a split, as ⌘T, ⌘D and ⇧⌘D open them, starts in the folder that the shell of the pane that it comes from told last, by OSC 7, where it is of this Mac and still there, so that a split of a pane where `ssh` runs starts where the shell worked before it; in the folder of the program that runs in the pane otherwise.
- `"home"` starts in your home folder.
- A path, as `"~/Projects"`, starts there; one that is not there starts in your home folder, and the log tells why.
- It applies at once, to the windows, tabs and splits that open from then.

### `padding-x`, `padding-y`

<sub>Platforms: macOS</sub>

The room between the grid and the window's edges, in points: `padding-x` at the left and the right, `padding-y` at the top and the bottom.

| Values | Default |
|---|---|
| from `0` to `100`, whole or not | `4` |

- In points, so that it keeps its look on a Retina display.
- A window is seldom a whole number of cells: the room that the cells leave over goes to the right and the bottom, past the padding, so that the text stays where it is as the window's size changes.
- The padding takes the background of the cells without a color of their own: a background that a program colors, such as a status line's, stops at the grid.
- With splits, each pane keeps its padding on every side: a divider stands between the padding of the panes on either side of it, and runs to the panes' room's edges.
- The gutter takes the padding at the cells' left: with [`gutter`](#gutter) on, a padding narrower than 4 points widens to 4, at the left and the right.

### `gutter`

<sub>Platforms: macOS</sub>

Whether each pane shows a gutter at the left of its cells, with a mark on the prompt of each command that ran: a thin bar the height of the prompt's row, faint gray for a command that succeeded, red for one that failed, the accent's color for one that still runs.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The marks need the shell to mark its prompts and its commands, as the shells' integration does ([`shell-integration`](#shell-integration)): without it, the gutter shows nothing.
- A command's mark shows its exit status: 0, or none that the shell told, is a success, any other a failure, as an interrupted command's.
- The gutter takes no column of the grid: it lies in the padding at the cells' left, which widens to 4 points, at the left and the right, if [`padding-x`](#padding-x-padding-y) is narrower.
- The marks keep their lines as output scrolls them into the scrollback and a resize reflows them, and go with them, as the scrollback's limit or a clearing takes them.
- A program of the full screen, as an editor's, shows no prompt's mark, as its screen has no shell's prompts.
- Plugins can lay marks of their own colors in the gutter, which show over a prompt's mark of the same row; they show only while it is on.
- The colors are the theme's: `gutter.succeeded`, `gutter.failed` and `gutter.running`.

### `background-opacity`

<sub>Platforms: macOS</sub>

The opacity of the window's background, from 0, clear, to 1, opaque: below 1, what is behind the window shows through it.

| Values | Default |
|---|---|
| from `0` to `1` | `1` |

- Only the default background lets it show, with the padding and the title bar, which take it, and the tab bar's background and the active tab's, as a pane's header, each of the same opacity as the panes, rather than theirs over the window's: a background that a program colors, such as a status line's, stays opaque, as do the selection, the cursor and the text.
- At `1` the window is opaque, as it is by default.

### `background-blur`

<sub>Platforms: macOS</sub>

The radius of the blur of what shows through the background, in points.

| Values | Default |
|---|---|
| from `0` to `100`, whole or not | `0`, no blur |

- The blur shows only below a `background-opacity` of 1.
- macOS has no public way to blur what is behind a window: Lokyr asks the window server through a private call, which takes whole points, so that the radius is rounded. A larger radius asks more of the window server.
- What it costs: over a video that plays behind the window, a blur of 20 took about 4 % more of a core of the window server, measured on an M5 Pro; translucency without blur, nothing measurable. Over what does not change, the window server blurs it once.

### `title-size`

<sub>Platforms: macOS</sub>

Whether the window's title ends with the size of its grid, in columns and rows, as `lokyr — zsh — 120×30`.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The title before it is the one that you give with Shell › Set Title…, `cmd+shift+i`; otherwise the program's, when it gives one, as vim and the shells' prompts can, or a man page's, as `man ls`, or the folder of the program in the foreground; then that program, as `lokyr — vim`, which Lokyr looks at as the program writes, and once more a moment after, so that a program that you run shows though your prompt set the title.
- As you resize the window, the size follows.
- It applies at once, as you save the file.

### `title-shortcut`

<sub>Platforms: macOS</sub>

Whether the titlebar shows, at its right, the keys that bring the window forward, as `⌥⌘1`; in a titlebar of tabs, after the window's buttons, as the tab bar shows them.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- Each window takes, as it opens, the lowest number that no other open window has, from 1; `alt+cmd+1` to `alt+cmd+9` bring the windows of the first nine forward, as [`[keys]`](#keys) says. The quick terminal has none.
- The keys are those that your `[keys]` bind to `window.goto` of the window's number, as the menus write them; a window whose number no binding has shows none.
- It applies at once, as you save the file.

### `opaque-titlebar`

<sub>Platforms: macOS</sub>

Whether the titlebar stays opaque over a translucent window, in the default background's color.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- The terminal's background runs under the titlebar: with `false`, the titlebar is as translucent as the background, as `background-opacity` makes it; with `true`, it is opaque, and what is behind the window shows through the background below it alone.
- A tab bar on the left that runs up through the titlebar, as [`position`](#position-1) and [`style`](#style-1) put it, keeps its own colors there: the titlebar is opaque beside it, over the panes.
- The title's color reads over the background: a light background in the dark appearance, or a dark one in the light appearance, gives the window the appearance of its background.
- It applies at once, as you save the file.

## `[interface]`

The font of Lokyr's own interface, as its tab bar and its panels, apart from the terminal's text, which takes `[font]`'s.

### `font-family`

<sub>Platforms: macOS</sub>

The family of the interface's text, by its name, as Font Book shows it on macOS, such as `"Helvetica Neue"`.

- **Default**: the system's interface font, as macOS's menus and windows show it, SF Pro.
- The case does not matter: `"helvetica neue"` is Helvetica Neue.
- A family that is not installed gives the default one.
- It applies to the views of the plugins in the window's tab bar, strip, status bar and panes' headers; Lokyr's own plugins show none yet, and the tabs bring the first.

### `font-size`

<sub>Platforms: macOS</sub>

The size of the interface's text, in points.

| Values | Default |
|---|---|
| from `4` to `200`, whole or not | `13` |

The text is measured at its size of points, as the system's font draws each size of its own, and the display's scale enlarges it. It applies as `font-family` does.

## `[slots]`

The plugins whose views fill the parts of the window that one plugin's view fills: the tab bar, and the panes' headers. The other parts, as the status bar, show the views of every plugin that fills them, in the plugins' order.

### `tab-bar`

<sub>Platforms: macOS</sub>

The plugin whose tab bar the windows show, by its name, as `"lokyr/tabs"`.

- **Default**: none, which takes the first plugin that has a tab bar.
- A plugin that is not installed, or that has no tab bar, gives the default one.
- `lokyr/tabs` has one.

### `pane-header`

<sub>Platforms: macOS</sub>

The plugin whose headers the panes show across their tops, by its name, as `"lokyr/command-header"`.

- **Default**: none, which takes the first plugin that has the panes' headers.
- A plugin that is not installed, or that has no headers, gives the default one.
- `lokyr/splits` has them, which show each pane's title as its [`pane-titles`](#pane-titles) asks.

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
- A name that no terminfo entry describes where programs look for one, in `~/.terminfo`, the directories of `TERMINFO_DIRS` and the system's, shows a notice across the windows' top, and `lokyr +validate-config` tells it: programs take a terminal that they know nothing of for a dumb one, and a shell's line editing then draws wrongly. Lokyr keeps the name, as a host that you reach over ssh can describe it.

### `unicode-widths`

<sub>Platforms: macOS</sub>

Which width tables the terminal counts characters by, to place them in their cells.

| Values | Default |
|---|---|
| `"current"`, `"8"` | `"current"` |

- `"current"` follows the tables of the Unicode version that Lokyr knows, as current programs do: most emoji take two cells.
  - A cluster of a letter and a vowel sign that spaces, as का or কা, or of consonants that a virama joins, as क्ष, takes two cells, as vim and most programs count it; so do a regional indicator alone, as 🇮, and an Arabic number sign with its digit, as ؁١.
  - The zsh of macOS counts a vowel sign that spaces as no cell: after a cluster such as का, the cursor of its line editor stands a cell short of the text.
- `"8"` follows those of Unicode 8, for old programs that count emoji as one cell: a cluster takes the cells of its first character, as का one, and a regional indicator alone one.
- vim counts a character that the emoji's variation selector makes wide, as ❤️ or ✔️, as one cell: as it writes the next character on the emoji's second cell, the emoji goes, and vim shows nothing in its place. With `"8"` the two count alike, but every emoji takes one cell, everywhere.
- It applies to the text that comes after the change, in the open windows too.

### `ambiguous-width`

<sub>Platforms: macOS</sub>

The cells of an East Asian Ambiguous character, as `α`, `→` and `○`.

| Values | Default |
|---|---|
| `1`, `2` | `1` |

- `2` suits programs and fonts that draw them wide, as some of China, Japan and Korea do; most programs count them as 1.
- The icons of Nerd Fonts, in Unicode's private area, are ambiguous too: with `2`, a prompt that shows them, as oh-my-posh's and Powerlevel10k's do, takes more cells than the shell counts, and the line that you type goes wrong, its characters doubled or out of place.
- Box drawing, the block elements and the powerline symbols are ambiguous too: with `2`, those that Lokyr draws span both cells, so that lines meet and arrows keep their shape.
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

### `shell-integration`

<sub>Platforms: macOS</sub>

Whether your shell loads Lokyr's integration, which has it tell the terminal where each prompt starts, where your command starts and ends, of its exit status, and the folder that it works in.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- It applies to zsh, bash and fish, your login shell or one of theirs that a new pane starts, without touching your files: zsh's through `ZDOTDIR`, which the integration puts back to yours as it starts, so that your `.zshenv`, `.zprofile`, `.zshrc` and `.zlogin` load as they would; bash's through `ENV`, of a bash started in POSIX mode, which leaves POSIX mode, and what it turned on, then reads your `.bash_profile` or your `.bashrc` itself; fish's through `XDG_DATA_DIRS`, of a folder that leaves it as fish starts, the variable going if you had none.
- bash 4.4 and later mark a command's output from `PS0`, which the integration ends with its mark, and set no trap, so that your own DEBUG trap, as starship's, atuin's or bash-preexec's, `$_` and the commands that your keys run through `bind -x`, as fzf's, stay as they are; an older bash marks it from a DEBUG trap, unless you have one, when its commands get no marks.
- fish 4 and later tell the prompts and the folder themselves: the integration adds nothing to them.
- macOS's own bash, `/bin/bash`, of version 3.2, reads your files and not `ENV` in POSIX mode, and gets no integration: a later bash, as Homebrew's, does. To have it in `/bin/bash`, add to your `.bashrc` `[ -n "$LOKYR_RESOURCES_DIR" ] && source "$LOKYR_RESOURCES_DIR/shell-integration/bash/lokyr.bash"`.
- The marks are OSC 133's, the folder OSC 7's; a new pane starts in the folder that its shell told last, as [`new-directory`](#new-directory) says.
- It applies to the shells that start after the change; the open ones keep what they started with.

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
- While the find bar shows a match, the view stays where the search took it, at the match, whatever the key says, and output brings it down again once the bar closes.
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
- Text that you drop on a window pastes as a paste does, and asks the same way; files that you drop paste their paths, each quoted for the shell where it needs it, as `'/tmp/a file.txt'`, apart by spaces and with one after the last.
- The question shows the text's first lines, as the program would get them, and three buttons: Paste, the default one; Paste as One Line, which puts a space for each line break and leaves out the last one; and Cancel. Its checkbox, Don't ask again, sets this key to `"never"` in your config file as you paste, keeping the rest of the file and its comments, in the file that a link points to; a config file with an error is left as it is, and the choice lasts until Lokyr quits.
- It applies as you save the file.

### `copy-on-select`

<sub>Platforms: macOS</sub>

Whether a selection that you make with the mouse copies its text to the clipboard as you let go of the button, as a double click on a word does too.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- With `false`, ⌘C copies the selection, as other Mac apps do.
- You select by dragging, the cells between where you press and where the pointer is; a double click takes a word, a triple click a line, and dragging after them goes on by words or lines; Option makes a rectangle; Shift's click extends the last selection, and Command's opens a link, as [`cmd+click`](#bindings) does; dragging past the window's top or bottom scrolls, the faster the farther; output that scrolls the text as you drag leaves the selection's start on its text. A click takes the selection away.
- A right click, or a Control click, opens a menu at the pointer: Open Link; Open man Page, which opens a window of the man page that the selection names, as `ls` or `ls(1)`, through your login shell, as your `PATH` finds the pages: black on light yellow, which it keeps once `man` has printed it, whatever `close-on-exit` says, and where the arrows, Space, the page keys, Home and End scroll; Look Up, which shows the definition of the selection, of one line, in the dictionary's panel of macOS; Copy, Paste, Select All and Clear. An item is dimmed where it would do nothing, as Open Link where no link is, or one that leads nowhere, Open man Page where the selection is no page's name, and Copy and Look Up without a selection. As the menu opens, the click keeps the selection when it is on it; else it selects the word under the pointer, which it does not copy, or nothing on a blank cell.
- A program that takes the mouse, as vim or htop can, has the clicks, the right button's too: Shift held keeps them for the selection, and for the menu. The pointer over the window is the text's I-beam, and the arrow while a program takes the clicks.
- It applies as you save the file.

## `[links]`

What a link does as you open it, with [`cmd+click`](#bindings), or as a plugin opens one. A folder always opens in the Finder, without asking. A file that would run rather than open, as a program, an application, an installer or a script that macOS would run in a terminal, never runs: it shows in the Finder, selected. A symbolic link and a Finder alias count as the file that they lead to, and a file counts as the application that would open it, which a file's own "Open With" can choose. The section also names the engine that searches the web for the selection.

### `urls`

<sub>Platforms: macOS</sub>

What a URL does: one of `http`, `https` or `mailto`, as the text writes it, or as a program's link leads to it, whose text can say something else.

| Values | Default |
|---|---|
| `"ask"`, `"open"` | `"ask"` |

- `"ask"` asks you first, showing the URL in full, with two buttons: Open, the default one, and Cancel. Its checkbox, Don't ask again for links, sets this key to `"open"` in your config file as the URL opens, keeping the rest of the file and its comments, in the file that a link points to; a config file with an error is left as it is, and the choice lasts until Lokyr quits.
- `"open"` opens it in its application, as your browser or your mail.
- It applies as you save the file.

### `files`

<sub>Platforms: macOS</sub>

What a file does: a path, or a `file` URL.

| Values | Default |
|---|---|
| `"ask"`, `"open"`, `"reveal"` | `"ask"` |

- `"ask"` asks you first, showing the path in full, with three buttons: Open, the default one, Show in Finder, and Cancel. Its checkbox, Don't ask again for files, sets this key to the button that you choose, `"open"` or `"reveal"`, in your config file, as [`urls`](#urls) does.
- `"open"` opens it in its application, as the Finder would.
- `"reveal"` shows it in the Finder, selected.
- It applies as you save the file.

### `search-engine`

<sub>Platforms: macOS</sub>

The engine of Search the Web, which the pane's context menu has for the selection of one line.

| Values | Default |
|---|---|
| `"google"`, `"duckduckgo"`, `"bing"`, `"ecosia"`, `"kagi"`, `"startpage"`, or a URL from `https://` or `http://` that holds `{query}` | `"google"` |

- A name searches that engine.
- A URL of your own takes the selection in place of its `{query}`, escaped as a URL's query takes it, as `"https://search.example/?q={query}"`.
- The search opens in your browser at once, without asking, as [`urls`](#urls) would for a program's link, since you chose it.
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

Whether a window without the focus whose bell rang shows 🔔 before its title, until it takes the focus. Where the tab bar is in the titlebar, or a single tab's title takes its place, the shown tab shows the bell's mark instead.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

## `[notifications]`

What Lokyr tells you of its own in a notification of the system's, while the window has no focus. A notification within two seconds of the last one of the window shows nothing, and a click on one brings its window forward.

### `long-command`

<sub>Platforms: macOS</sub>

How long a command runs, at least, in seconds, for a notification as it ends while its window has no focus, which tells the command, whether it succeeded or its exit status, and how long it ran.

| Values | Default |
|---|---|
| from `1` to `86400`; `0` for none | `10` |

- It needs your shell to mark its commands, as [`shell-integration`](#shell-integration) has zsh, bash and fish do: a command runs from the mark of its output's start to that of its end, as Lokyr finds them in its output, without a clock that runs meanwhile.
- The notification's title is the command as you typed it, or the window's, once its line has left the scrollback.
- It applies as you save the file, to the commands that end after.

## `[theme]`

The colors of the terminal and of Lokyr's own interface: a theme for the light appearance and one for the dark one, which follow the system's appearance, or the one that View › Appearance chooses, and switch at once.

A theme is a built-in one, `Lokyr Light` or `Lokyr Dark`, one of the themes that Lokyr holds, or a file of the `themes` folder beside the config file, `~/.config/lokyr/themes/`, named by its file without its extension: `Nightfall` is `themes/Nightfall.toml`, in Lokyr's own format below, or `themes/Nightfall.itermcolors`, the format of the largest collections of terminal themes, which Lokyr reads as it is. A file of the folder of a held or built-in theme's name takes its place, and a `.toml` one an `.itermcolors` one of its name.

Lokyr holds 278 themes, those of a permissive license of the largest collection of `.itermcolors` themes, by their names there, as `Dracula`, `Catppuccin Mocha`, `Nord`, `Rose Pine Dawn` or `TokyoNight Day`, and `<name> (Light)` and `<name> (Dark)` of those of both appearances: `Lokyr.app/Contents/Resources/licenses/themes/README.md` lists them, with their licenses and sources, and their files are in `Lokyr.app/Contents/Resources/themes`, which a theme of the folder can extend.

- **An `.itermcolors` file** of colors for the light and the dark appearance is two themes, `<name> (Light)` and `<name> (Dark)`, as `dark = "Nightfall (Dark)"`.
- Its colors read as their files say, of sRGB, Display P3 or the calibrated colors of older files, converted to sRGB; its link's, underline's and the find bar's match's colors too.

View › Choose Theme…, the command palette's theme picker, lists the themes of the appearance in force, shows each as you move through them, and names the one you choose in `[theme]`, `light` or `dark` as the appearance is.

A theme's file goes into the folder by hand, or as Lokyr installs it: on macOS, a double click on an `.itermcolors` file in the Finder, or a theme's file, `.itermcolors` or `.toml`, dropped on Lokyr's icon in the Dock, asks to install it. **Install** copies it into the folder, in place of a file of its name; **Install and Use** names it in `[theme]` too, `light` or `dark` by its variant, both for a file of both appearances; neither converts it. The variant is the one that the file says: an `.itermcolors` file of ` (Light)` and ` (Dark)` keys is of both, a `.toml` one of its `variant`, and any other is light if its background is lighter than its text, and dark otherwise. A theme of the appearance not in force changes nothing that shows until Lokyr takes its appearance: the question tells it, and its checkbox switches Lokyr to that appearance, as View › Appearance does, which sets [`appearance`](#appearance). Where another app opens `.itermcolors` files at a double click, Lokyr is in the Finder's Open With menu.

```toml
# ~/.config/lokyr/themes/Nightfall.toml
[theme]
name = "Nightfall"                   # what the theme calls itself; the file's name names it in the config
author = "Ada"                       # author, license and source tell people, and change nothing
license = "MIT"
variant = "dark"                     # light | dark; of its colors when left out
extends = "Lokyr Dark"               # a theme whose colors this one's override

[terminal]
background = "#1e1e20"               # the text's background
foreground = "#e6e6e6"               # the text
palette = ["#45454b", "#f25d62", "#58c672", "#e3bd4c", "#5aa2f7", "#c983de", "#4cc4cf", "#c4c4ca",
           "#75757d", "#ff8488", "#7edb94", "#f5d47a", "#86bcff", "#dca3ec", "#7adce4", "#f4f4f6"]
cursor = "#e6e6e6"                   # the cursor; the text's color when left out
selection = "#34445f"                # the selection's background
bold = "#ffffff"                     # bold text's color; its own when left out

[search]
match = "#ffd60a4d"                  # a match of the find bar, #rrggbbaa of an opacity
current = "#ff7a0080"                # the current match

[tabs]                               # the tab bar's colors
bar.bg = "#18181a"
active = { bg = "#1e1e20", fg = "#ffffff" }

[interface]                          # Lokyr's own colors, by their names
"panel.bg" = "#252528"
```

- **Colors** are `#rrggbb`, or `#rrggbbaa` of an opacity, in sRGB.
- **`[terminal]`** gives the terminal's colors: `background`, `foreground` and the 16 colors of `palette`, black to bright white, which every theme gives itself or through the theme that it extends; `cursor`, `cursor-text`, `selection`, `selection-text`, `bold`, `link` and `underline`, which it can give.
- **What a theme leaves out** follows its terminal's colors: the tab bar, the panels, the fields, the buttons and the find bar's highlights take colors of its background, its text and its palette, so that a theme of `[terminal]` alone colors all of Lokyr.
- **The other sections** give the interface's colors by their names: `[search]` the find bar's highlights, `match` and `current`; `[tabs]` the tab bar's, as `bar.bg`, `active.bg`, `active.fg`, `inactive.fg` and `hover.bg`; `[interface]` the rest of Lokyr's, as `panel.bg`, `field.bg`, `accent` or `text.secondary`.
- **A theme gives colors alone**: the tab bar's look, its place and the rest stay in the config, as [`[plugins.tabs]`](#pluginstabs) has them; the command's header names the theme's tokens that it takes, as [`[plugins.command-header]`](#pluginscommand-header) has them.
- **`extends`** names a theme, a file's or a built-in one, whose colors this one takes where it gives none, eight deep at most.
- **As you save** a theme's file, or the config, the theme applies at once, in the open windows too.
- **A file with an error** tells it, with its line, in the bar across the windows' top, and the theme in force stays.

### `light`

<sub>Platforms: macOS</sub>

The light appearance's theme, by its name.

| Values | Default |
|---|---|
| the name of a theme | `"Lokyr Light"` |

- A name that no file, no held theme and no built-in theme has shows in the bar, and the theme in force stays.

### `dark`

<sub>Platforms: macOS</sub>

The dark appearance's theme, by its name.

| Values | Default |
|---|---|
| the name of a theme | `"Lokyr Dark"` |

### `minimum-contrast`

<sub>Platforms: macOS</sub>

The least contrast of the terminal's text to its background, as the ratio of WCAG's guidelines for the web: a text of less moves toward white or black, whichever reads better, as little as gives it.

| Values | Default |
|---|---|
| from `1` to `21`, whole or not | `1` |

- `1` leaves the colors as the programs and the theme give them; `4.5` is the contrast that the guidelines ask of text; `21` makes every text black or white.
- It applies to the terminal's text alone, Lokyr's own following its theme.
- It applies at once, as you save the file.

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

### `appearance`

<sub>Platforms: macOS</sub>

Whether Lokyr is light or dark: the theme's colors, and Lokyr's own titlebar, menus and alerts.

| Values | Default |
|---|---|
| `"system"`, `"light"`, `"dark"` | `"system"` |

- `"system"` follows the appearance of macOS, in the system's settings, as it changes, automatic too; `"light"` and `"dark"` keep theirs, whatever the system's is.
- The View menu's Appearance sets it too: Automatic, Light or Dark, the one in force ticked. The choice goes into your config file, keeping the rest of the file and its comments, in the file that a link points to; a config file with an error is left as it is, and the choice lasts until Lokyr quits, or until you change the key.
- It applies at once, as you save the file.

### `quit-after-last-window-closed`

<sub>Platforms: macOS</sub>

Whether Lokyr quits as you close its last window.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- With `false`, Lokyr stays open without windows, as macOS's apps do, its menus and a [global key](#bindings) at hand: its icon in the Dock opens a window, and so do ⌘N and Shell › New Window, and the quick terminal, hidden, keeps its shell.
- With `true`, closing the last window quits Lokyr, the hidden quick terminal and its shell with it, asking you first while a program runs there, a global key's quick terminal too.
- It applies at once, as you save the file.

## `[updates]`

Lokyr's own updates: it checks for a newer version, which it shows in a window with what is new, to install now or as Lokyr quits, or to skip.

### `check`

<sub>Platforms: macOS</sub>

Whether Lokyr checks for a newer version now and then, once a day at most, in the background.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- Lokyr › Check for Updates… checks at once, whatever the key says.
- When the config file has an error as Lokyr starts, or names a font that is not installed, Lokyr takes the defaults but does not check for updates, as the file may have turned the checks off; it checks as the file says once you save it without its error.
- A check asks Lokyr's site, `lokyr.dev`, for the list of its versions. It sends the version of Lokyr that you have, and what any request sends, your address and your preferred languages; nothing else about you or your Mac.
- The updates' window has its own box to check automatically: the key wins over it the next time Lokyr starts, or as you save the file.
- Lokyr updates itself in its app, `Lokyr.app`, as installed from its disk image. Lokyr built from its sources, as `cargo run` runs it, has no updater: the menu's item is dimmed, and the key does nothing.
- It applies at once, as you save the file.

## `[debug]`

What Lokyr shows of its own work, to see how it does: for now, a panel of its performance.

### `performance-overlay`

<sub>Platforms: macOS</sub>

Whether a panel over each window shows Lokyr's performance, a line each: the frames that the window shows a second; the CPU's time of its frames, their median and the longest over the last second; how long a frame waited for the display to give back an image to draw into, which macOS keeps two of; the GPU's, their mean and the longest, of every window's frames; Lokyr's wakeups a second, and those that brought the processor out of idle; its memory, as macOS counts it against Lokyr; and the output of its programs, in MB a second. The first second shows Measuring…, as does a window's that opens while the panel shows.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- View › Show Performance shows and hides it too, until Lokyr quits or the key changes.
- While it shows, it changes each second, which wakes Lokyr and draws each window once a second, even when nothing else changes: Lokyr is no longer idle. Measure Lokyr with it hidden.
- While it shows, Lokyr writes its numbers to its log too, a line a window each second.
- It applies at once, as you save the file.

### `performance-corner`

<sub>Platforms: macOS</sub>

The corner of the window that the panel takes, under the titlebar at the top.

| Values | Default |
|---|---|
| `"top-right"`, `"top-left"`, `"bottom-right"`, `"bottom-left"` | `"top-right"` |

- It applies at once, as you save the file.

## `[keys]`

The bindings of keys, of the wheel and of the mouse's click, to Lokyr's commands, over the defaults. A key that a binding takes runs its command, and never reaches the program; the others go to the program, as they always do.

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
  - The key is the character that it types without any modifier, Shift neither, as `]` in `cmd+shift+]`, or `+` in `cmd++`; or a named key: `enter`, `tab`, `backspace`, `escape` or `esc`, `space`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1` to `f35`; or a step of the wheel, `wheel-up`, `wheel-down`, `wheel-left` or `wheel-right`, which a trackpad's scroll makes too, a step a row; or the left button's click, `click`, as `cmd+click`.
  - Your keyboard layout gives the characters. A key whose character no binding takes runs the binding of the character at its place on the US layout, when its own is past ASCII, as `è` on the Italian layout runs `cmd+[`, or when both are punctuation, as the Italian `+` runs `cmd+]`, so that keys that take Option on your layout still work. An ASCII letter or digit is always yours: on AZERTY, Command-A is `cmd+a`, never the `cmd+q` at its place on the US layout.
  - Two entries of the same keys, written two ways, as `alt+x` and `opt+x`, or `cmd+shift+n` and `shift+cmd+n`, are an error.
- **A sequence** of keys, each after the one before, parts them with `>`: `ctrl+a>n` is Control-A, then N. After the first keys, Lokyr waits for the next one, as long as it takes; a key that no sequence goes on with goes to the program, with the keys before it, as does a key that no binding can name, as the keypad's, and the input method's text. A step of the wheel, or a click, makes no sequence.
- **A global key**, written after `global:`, as ``"global:ctrl+`"``, runs its command from every app, Lokyr's windows too, as macOS gives Lokyr the key without the Accessibility permission, and takes it from them.
  - It is one key with `cmd`, `ctrl` or `alt`, or a function key, alone or with any of them; no sequence, no step of the wheel, no click.
  - It goes by its place on the keyboard, the US layout's, whatever your layout types there: `` ` `` is the key left of 1, or right of the left Shift on some keyboards. A character is the one that its key types without Shift: `~` is ``shift+` ``; a character that the US layout types with Shift, or has no key for, as `è`, is an error.
  - Its value is a command, or a table without `when`, which holds everywhere; a global key has no default, nor a `false`.
  - A key that another app holds as its own alone, or that the Mac's keyboards lack, as `f24`, shows in the bar across the windows' top, until the config takes it away. An app that took the key without holding it alone cannot be told, and the key then reaches one of the two.
  - Lokyr stays open as its last window closes, as [`quit-after-last-window-closed`](#quit-after-last-window-closed) has it by default, so that the key still works. With `true`, Lokyr quits all the same, and the key with it.
- **The value**:
  - a command, as `"window.new"`;
  - a table with the `command`, its `args`, as `{ command = "tabs.goto", args = { index = 1 } }`, and its context, `when`;
  - an array of such tables, for the keys in several contexts;
  - `false`, which takes away the defaults of the keys.
- **The context**, `when`, says when a binding holds: the names `pane.focused`, `selection` and `alternate-screen`, and those of the plugins' panels, with `!`, `&&`, `||` and parentheses, as `pane.focused && !selection`. A binding without `when` holds everywhere. An expression nests 32 levels deep at most.
  - `pane.focused` holds while the pane has the keyboard, and not while a panel's field or button has it; `alternate-screen` while a program such as vim or less shows its own screen.
  - A plugin's panels, as the find bar, take the plugin's namespace: `search.open` holds while one of them shows, and `search.focused` while one of them has the keyboard, as `"enter" = { command = "search.next", when = "search.focused" }`. A name of a plugin not installed reads, and never holds; `pane`, `selection` and `alternate-screen` have no names but these. No panel takes the keyboard yet: they come with the plugins that show them.
- **Which binding wins**: of those of the keys that hold, the last; your bindings come after the defaults.
- A key that a binding takes runs its command again as it repeats, held down, and its release reaches nothing.
- A click that a binding takes, a single one in the window that has the focus, runs its command before the selection, the menu of a Control click and the program; a command that takes the arguments `column` and `row` gets the cell under the pointer: its column, from `0` at the left, and the number of its row, which stays that row's as output scrolls it into the scrollback and the view scrolls, as the window showed it.
- The menus show each command's keys: of its bindings of one key, without arguments and without `when`, the first whose key no later binding takes. A menu's item takes its key before the program; a dimmed item, as Copy without a selection, takes none, and the key goes on to the bindings. A command that needs an argument, as `pane.send-text`, has no item.
- A binding of yours of a command that does not exist reads, takes its keys, and tells it in the log when you press them, as a plugin's commands come once the plugin starts.

Commands without keys of their own, for your bindings:

- `pane.send-text` sends its `text` to the program as it is, its control characters too, as keys that typed it would, which can run commands; the view comes back to the screen. In the TOML string, `\u0017` writes Control-W, and `\u001b` Escape.
- `window.toggle-quick-terminal` shows the quick terminal, or hides it, as [`[plugins.quick-terminal]`](#pluginsquick-terminal) says: bound to a global key, from every app. Shell › Quick Terminal runs it too.
- `tabs.rename` gives the tab its `title`, in place of its program's; without one, as Shell › Rename Tab… runs it, it asks you for it, and an empty one gives the tab its program's back.
- `tabs.set-color` gives the tab its `color`, as `"#ff8800"`, in place of the one that its program set; an empty one gives the program's back.
- `tabs.detach` moves the tab out to a window of its own, as Shell › Move Tab to New Window does.

For the keys of a shell's line editing, as macOS's text fields have them:

```toml
[keys]
"alt+backspace" = { command = "pane.send-text", args = { text = "\u0017" } }  # the word before: Control-W
"cmd+backspace" = { command = "pane.send-text", args = { text = "\u0015" } }  # to the line's start: Control-U
"cmd+delete" = { command = "pane.send-text", args = { text = "\u000b" } }     # to the line's end: Control-K
"alt+delete" = { command = "pane.send-text", args = { text = "\u001bd" } }    # the word after: Esc d
"alt+left" = { command = "pane.send-text", args = { text = "\u001bb" } }      # a word back: Esc b
"alt+right" = { command = "pane.send-text", args = { text = "\u001bf" } }     # a word on: Esc f
"cmd+left" = { command = "pane.send-text", args = { text = "\u0001" } }       # the line's start: Control-A
"cmd+right" = { command = "pane.send-text", args = { text = "\u0005" } }      # the line's end: Control-E
```

- `delete` is the key that deletes forward, fn-⌫ on a laptop's keyboard.
- zsh's Control-U deletes the whole line, where bash's and fish's delete to its start; `bindkey '^U' backward-kill-line` in `~/.zshrc` makes zsh's delete to the start.

The defaults, of the built-in plugin `keymap-macos`, the shortcuts of macOS's apps:

- Your bindings come after them, and win; `false` takes a default away.
- `[plugins.keymap-macos] enabled = false` takes them all away, so that your `[keys]` bind every key from nothing: the menus keep the system's own keys alone, ⌘H to hide Lokyr, ⌥⌘H to hide the others, ⌘M to minimize the window, and ⌃⌘Space for the emoji and symbols.
- A default of a command that is not there, as one of a plugin turned off, takes no key: with `[plugins.links] enabled = false`, a ⌘-click selects, as a click does.

| Keys | Command | What it does |
|---|---|---|
| `cmd+,` | `app.settings` | Opens the config file in your text editor, made first of every key commented out when there is none, as [The file](#the-file) says |
| `cmd+n` | `window.new` | Opens a window |
| `cmd+w`, `cmd+shift+w` | `window.close` | Closes the window, of all its tabs, `cmd+w` closing the tab instead while the tabs' plugin runs; while a program other than a shell runs in it, as vim or ssh, it asks you first, as closing ends the program: a job that the shell runs counts too, stopped with Control-Z or in the background, and a program that started a shell, as vim's `:sh`. tmux and screen, whose sessions go on, do not ask, nor a shell at its prompt, nor one that a shell started |
| `cmd+shift+i` | `window.set-title` | Asks for a title of the window's, which takes the place of the program's and of the window's own until you empty it; the grid's size stays after it, as [`title-size`](#title-size) says |
| `alt+cmd+1`, `alt+cmd+2`, `alt+cmd+3`, `alt+cmd+4`, `alt+cmd+5`, `alt+cmd+6`, `alt+cmd+7`, `alt+cmd+8`, `alt+cmd+9` | `window.goto` | Brings forward the window of that number, `index`, from 1: each window takes, as it opens, the lowest number that no other open window has, which its titlebar shows, as [`title-shortcut`](#title-shortcut) says; the quick terminal has none. Bound by you, as `{ command = "window.goto", args = { index = 10 } }`, it reaches past the ninth |
| `cmd+q` | `app.quit` | Quits Lokyr, asking you first while programs other than shells run in its windows; so does Quit in the Dock, and a logout waits for your answer |
| `cmd+c` | `pane.copy` | Copies the selection |
| `cmd+v` | `pane.paste` | Pastes the clipboard's text, asking you first when [`confirm-paste`](#confirm-paste) says so |
| `cmd+a` | `pane.select-all` | Selects the scrollback and the screen, to the last row that shows something |
| `cmd+k` | `pane.clear` | Clears the scrollback and the screen but the prompt, which goes to the top; on the screen of a program such as vim, it does nothing |
| `cmd+=`, `cmd+shift+=`, `cmd++` | `font.bigger` | Makes the text a point larger, in every window, up to 200 |
| `cmd+-` | `font.smaller` | Makes the text a point smaller, down to 4 |
| `cmd+0` | `font.reset` | Gives the text the size of [`size`](#size) again |
| `shift+cmd+up` | `pane.scroll-line-up` | Scrolls the view a line up |
| `shift+cmd+down` | `pane.scroll-line-down` | Scrolls the view a line down |
| `cmd+up` | `pane.prompt-previous` | Scrolls the view to the prompt above its top, of the commands that the shell marks, as [`shell-integration`](#shell-integration) has it, the prompt at the view's top |
| `cmd+down` | `pane.prompt-next` | Scrolls the view to the prompt below its top, or to the screen past the last |
| `cmd+pageup`, `shift+pageup` | `pane.scroll-page-up` | Scrolls the view a page up |
| `cmd+pagedown`, `shift+pagedown` | `pane.scroll-page-down` | Scrolls the view a page down |
| `cmd+home` | `pane.scroll-top` | Scrolls the view to the scrollback's first row |
| `cmd+end` | `pane.scroll-bottom` | Scrolls the view back to the screen |
| `cmd+click` | `links.open` | Opens what you click: the link that the program gave the text, or the URL or the path that the text holds there. A URL is of `http`, `https`, `mailto` or `file`; a path starts from `/`, from `~`, or from the directory of the program in the foreground, as `ls` lists files, and compilers and `grep -n` write a line after it, as `src/main.rs:42:7`; between quotes, a path that is there can have spaces, as `git status` and Python write one, and a URL ends at a space. A URL or a file asks you first, as [`[links]`](#links) says; a folder opens in the Finder; a program, an application or a script that would run shows in the Finder, selected, and does not run. While you hold Command, the link under the pointer that would open is underlined, and the pointer over it is the pointing hand, as you move over it or press Command with the pointer still, and goes as the program changes its text, or the view scrolls it from under the pointer |
| `cmd+f` | `search.open` | Opens the find bar at the top right of the pane, of your last search's text, selected, its field taking the keys, as [`[plugins.search]`](#pluginssearch) tells |
| `cmd+g` | `search.next` | Makes the next match of the find bar's search the current one, the one above, the last after the first, and brings it into the view; it opens the bar while it is closed |
| `cmd+shift+g` | `search.previous` | Makes the previous match the current one, the one below, the first after the last |
| `cmd+shift+p` | `command-palette.open` | Opens the command palette, whose field ranks the commands as you type |
| `up`, `down` | `command-palette.move` | In the command palette, while its field has the keys: moves the selection up or down the list |
| `cmd+t` | `tabs.new` | Opens a tab in the window, after its others, which shows it; its shell starts in the folder of the pane that has the keyboard, as [`new-directory`](#new-directory) says |
| `cmd+w` | `tabs.close` | Closes the tab, or its pane while the tab has splits; while a program other than a shell runs there, it asks you first, as closing the window does; the window's last tab closes the window, which `cmd+shift+w` closes whole |
| `cmd+shift+]`, `ctrl+tab` | `tabs.next` | Shows the next tab, the first after the last |
| `cmd+shift+[`, `ctrl+shift+tab` | `tabs.prev` | Shows the previous tab, the last before the first |
| `cmd+1`, `cmd+2`, `cmd+3`, `cmd+4`, `cmd+5`, `cmd+6`, `cmd+7`, `cmd+8`, `cmd+9` | `tabs.goto` | Shows the tab of that number, `index`, from 1 at the left; a number of no tab does nothing |
| `cmd+shift+left`, `cmd+shift+right` | `tabs.move` | Moves the tab a place to the left or to the right, `delta` of -1 or 1, as far as the bar's ends |
| `cmd+d`, `cmd+shift+d` | `splits.split` | Splits the pane, `direction` `right` or `down`: the new pane, beside it or below it, takes the keyboard |
| `alt+cmd+left`, `alt+cmd+right`, `alt+cmd+up`, `alt+cmd+down` | `splits.focus` | Gives the keyboard to the pane beside, `direction` `left`, `right`, `up` or `down`: the nearest, of the most room along the side, the first of a tie; none at the tab's edge, nor while a pane is zoomed |
| `cmd+]` | `splits.focus-next` | Gives the keyboard to the tab's next pane, the first after the last |
| `cmd+[` | `splits.focus-prev` | Gives the keyboard to the tab's previous pane, the last before the first |
| `ctrl+cmd+left`, `ctrl+cmd+right`, `ctrl+cmd+up`, `ctrl+cmd+down` | `splits.resize` | Moves a divider of the pane a cell, `cells` of 1, towards `direction`: the one on its right, or its bottom, where it has one, the other otherwise |
| `cmd+shift+enter` | `splits.zoom-toggle` | Has the pane take the tab's whole room, or gives the others theirs back |

## `[plugins.<namespace>]`

<sub>Platforms: macOS</sub>

A plugin's own keys, in a section of its namespace: the plugin's name after its owner, as `[plugins.tabs]` of `lokyr/tabs`. Each plugin tells its keys, as the tab bar's do in [`[plugins.tabs]`](#pluginstabs).

```toml
[plugins.tabs]
position = "top"
```

- **Its keys** are those of the plugin's schema, the JSON Schema file that its manifest names, with their values and their defaults: a key left out takes its default, and a section left out gives all of them.
- **A wrong value**, of another type or none of its choices, refuses that plugin's section alone: the plugin keeps the keys in force, its defaults at the start, and the rest of the file applies. The bar across the windows' top tells the error at its line, until you save a section that the plugin takes:

  ```text
  lokyr: error 109 (usage): /Users/you/.config/lokyr/config.toml:12:1: `position` is one of "top", "bottom", not "middle" in `[plugins.tabs]`; the plugin `lokyr/tabs` keeps its config in force
  ```

- **What another version would know** is ignored, as the bar tells: the section of a plugin that is not installed, and a key that the plugin's schema lacks, as one of a newer version, so that one file serves two Macs.
- **Reloading**: as you save the file, each plugin takes its section anew, and hears of a change.
- `lokyr +validate-config` checks the file's own keys and the sections' form, each a table of keys, and `lokyr +show-config` prints the sections as the file has them; their keys apply as the plugins take them, once Lokyr runs.

### `enabled`

<sub>Platforms: macOS</sub>

Whether the plugin runs: `false` turns it off, any plugin, the built-in ones too, whatever its keys.

| Value | Meaning |
|---|---|
| `true` | The plugin runs, as by default |
| `false` | The plugin is off: its commands, its keys, its bars and its panels go, and the program's text loses its marks |

Default: `true`.

- It applies as you save the file: a plugin turned off stops at once, and one turned on again starts anew.
- The plugin's other keys are not checked while it is off.
- `[plugins.core-actions] enabled = false` takes away the terminal's own actions, as copy, paste, select all and the text's size, with their keys.
- `[plugins.keymap-macos] enabled = false` takes away the default shortcuts, as [`[keys]`](#keys) says, the menus keeping the system's own keys alone.
- A value other than `true` or `false` shows in the bar, and the plugin stays as it is.

## `[plugins.tabs]`

<sub>Platforms: macOS</sub>

The tab bar of the built-in plugin `lokyr/tabs`: each window's tabs, each of its title, its color, the keys that select it and its close button, and a button of a new tab. A click on a tab shows it, on its close button closes it, asking you first while a program other than a shell runs there. A tab's title is the one that you give it with Shell › Rename Tab…, or its program's, or its folder and the program in its foreground, as `src — vim`.

### `position`

<sub>Platforms: macOS</sub>

Where the bar goes.

| Values | Default |
|---|---|
| `"top"`, `"bottom"`, `"left"`, `"right"` | `"top"` |

- At the top, as [`style`](#style-1) puts it; at the bottom, across the window's bottom, below the status bar, under a titlebar of the window's title; on the left or the right, a column from below the titlebar to the window's bottom, its tabs one below another, as wide as [`column-width`](#column-width) says, or as you drag its edge.
- On the left, in the `"unified"` and `"seamless"` looks, the column runs up through the titlebar, as macOS's sidebars do: the window's buttons stand over its top, the keys that bring the window forward after them, and the panes beside it start below the titlebar, with no title. You drag the window by the column's top, beside the window's buttons, and by the titlebar over the panes.
- It applies at once, as you save the file.

### `style`

<sub>Platforms: macOS</sub>

The bar's look.

| Values | Default |
|---|---|
| `"standard"`, `"unified"`, `"seamless"` | `"unified"` |

- `"standard"` puts the bar in a strip below a titlebar of the window's title: the tabs side by side in a rounded track, of equal widths, each its title and its keys in its middle, the one shown raised over the track, of a border and a shadow; the button of a new tab a circle after the track.
- `"unified"` puts the same tabs in the titlebar, past the window's buttons and the keys that bring the window forward, as [`title-shortcut`](#title-shortcut) shows them; the one shown is of its background alone, without a border or a shadow, the one border round a tab being its program's progress.
- `"seamless"` puts the tabs in the titlebar too, past the window's keys, which keep the titlebar's background, as cells of the titlebar's height, a line between two. The one shown has the window's own background, translucent and blurred as [`background-opacity`](#background-opacity) and [`background-blur`](#background-blur) make it, and opens onto the terminal below it; the others are a little darker, over a line. Each has its title in its middle, its color and its keys at its end, and the button of a new tab is a cell of its own.
- At the bottom, each look keeps its tabs, under a titlebar of the window's title; down a column, the tabs are rows, the one shown raised, with rounded corners and a border in `"standard"` and round ends of its background alone in `"unified"`, or of the window's background in `"seamless"`, with the rest of the column a little darker; the button of a new tab is a row of its own, its sign in its middle.
- A title too long for its tab ends in an ellipsis. A tab's background lightens under the pointer and darkens as you press it, fading in and out, at once where the system reduces motion; its close button shows at its start.
- It applies at once, as you save the file.

### `hide-when-single`

<sub>Platforms: macOS</sub>

Whether the bar hides while the window has a single tab.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- Hidden, the bar takes no room, and the window shows its tab's title in its titlebar, in its middle, with the tab's marks beside it: the lock of the protected input, the bell, and the dots of a program that runs; none in full screen, where the window has no titlebar.
- It applies at once, as you save the file.

### `show-index`

<sub>Platforms: macOS</sub>

Whether each of the first nine tabs shows the keys that select it, as `⌘1`.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The keys are those that your [`[keys]`](#keys) bind to `tabs.goto` of the tab's number, `cmd+1` to `cmd+9` by default; a tab of no binding shows none.
- It applies at once, as you save the file, and as your `[keys]` change.

### `close-button`

<sub>Platforms: macOS</sub>

When a tab shows its close button.

| Values | Default |
|---|---|
| `"always"`, `"hover"`, `"never"` | `"hover"` |

- `"hover"` shows it while the pointer is over its tab, fading in and out.
- It applies at once, as you save the file.

### `column-width`

<sub>Platforms: macOS</sub>

The width of the bar's column, on the left or the right, in points.

| Values | Default |
|---|---|
| from `60` to `600` | `200` |

- A drag of the column's edge changes its width, which Lokyr keeps for its next start; a new value in the file takes the drag's place, at once or at the next start, as the file changes while Lokyr is closed too.
- It applies at once, as you save the file.

## `[plugins.splits]`

<sub>Platforms: macOS</sub>

The panes of the built-in plugin `lokyr/splits`: how the panes of a tab of more than one show, and where the keyboard goes. Its commands split a pane, move between panes, resize, zoom and swap them, as [the keys of the defaults](#keys) tell.

### `dim-inactive`

<sub>Platforms: macOS</sub>

How much the panes that have not the keyboard fade into their background, in a tab of more than one.

| Values | Default |
|---|---|
| from `0` to `1`, whole or not | `0.15` |

- `0` leaves them as they are; `1` hides their text in their background.
- The fade brings each pane's text, its lines, its cursor and the backgrounds that its program colors toward its own background, so that a translucent window's background stays as it is; a picture of colors of its own, as an emoji, fades as its opacity, over its faded cell.
- It applies at once, as you save the file.

### `pane-titles`

<sub>Platforms: macOS</sub>

Whether each pane of a tab of more than one shows its title across its top.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- The title is its program's, or its folder and the program in its foreground, as `src — vim`, of the text's color in the pane that has the keyboard and of a fainter one in the others.
- The header takes its row of the pane's room, which its grid gives up; none while a pane is zoomed, or in a tab of one pane.
- It applies at once, as you save the file.

### `focus-follows-mouse`

<sub>Platforms: macOS</sub>

Whether the keyboard goes to the pane that the pointer comes over, without a click.

| Values | Default |
|---|---|
| `true`, `false` | `false` |

- The pointer's moves wake the app over the panes that have not the keyboard alone, so that it costs nothing while the pointer stays over the one that has it; none moves it while a button is down, as in a selection's drag.
- It applies at once, as you save the file.

## `[plugins.quick-terminal]`

<sub>Platforms: macOS</sub>

The quick terminal of the built-in plugin `lokyr/quick-terminal`: a window that a global key slides in from an edge of the screen, over every app, the ones in full screen too, and out again: a terminal at hand from anywhere. Bind its command, `window.toggle-quick-terminal`, to a key of `[keys]` that works in every app, with `global:`, which macOS gives Lokyr without the Accessibility permission, as [Bindings](#bindings) says, as ``"global:ctrl+`" = "window.toggle-quick-terminal"`` does; Shell › Quick Terminal runs it too.

```toml
[keys]
"global:ctrl+`" = "window.toggle-quick-terminal"
```

Pick a key that no other app takes: VS Code, as an example, takes Control-` for its own terminal while it is in front. The key shows the quick terminal and gives it the keys; again, it hides it, and the app that had the focus takes it back; shown without the focus, it takes the focus. Its shell runs while it is hidden, so that it shows at once, where [`new-directory`](#new-directory) says; closed, as its shell exits, the next key opens a new one.

Hidden, it keeps its shell, its screen and its scrollback, as a window does, and its last frame, so that it shows at once: some ten megabytes of memory at half the screen of a Retina laptop, more on a larger screen. It draws nothing while hidden.

These keys were the section `[quick-terminal]` before; a file that still has it reads, a notice at its line telling you to move its keys here, which it ignores until then. With `enabled = false`, the quick terminal goes, its window and its shell with it, and Shell › Quick Terminal too.

### `position`

<sub>Platforms: macOS</sub>

The edge of the screen that the quick terminal comes from, or its centre, where it fades in and out; it fades in and out in its place too where another display lies past its edge, rather than slide across that display.

| Values | Default |
|---|---|
| `"top"`, `"bottom"`, `"left"`, `"right"`, `"center"` | `"top"` |

It applies as it shows next.

### `size`

<sub>Platforms: macOS</sub>

The quick terminal's size: first, how far it reaches into the screen from its edge, its height at the top or the bottom, its width at the left or the right, and its width at the centre; then, after a comma, its length along the edge, its width at the top or the bottom, and its height at the left, the right or the centre.

| Values | Default |
|---|---|
| a percentage of the screen's, from `"1%"` to `"100%"`, or points, as `"300px"`; a second one after a comma | `"50%"` |

- A size that does not read takes `"50%"`, as the log tells.
- The screen's part is of its area below the menu bar and beside the Dock, where the quick terminal shows, centred along its edge.
- Without the second, it takes the screen's whole width at the top or the bottom, its whole height at the left or the right, and, at the centre, the first's share of the height, or its points.
- Points are those that macOS lays windows out in, two pixels each on a Retina display; a size past the screen's takes the screen's.
- It applies as it shows next.

### `screen`

<sub>Platforms: macOS</sub>

The screen that the quick terminal shows on.

| Values | Default |
|---|---|
| `"main"`, `"mouse"`, `"menu-bar"` | `"main"` |

`"main"` is the system's main screen, the one of the window in front; `"mouse"` the screen that the pointer is on; `"menu-bar"` the one that has the menu bar. It applies as it shows next.

### `animation`

<sub>Platforms: macOS</sub>

How long the quick terminal slides in and out, in milliseconds.

| Values | Default |
|---|---|
| from `0` to `2000`; `0` shows and hides it at once | `200` |

With the system's Reduce motion, it shows and hides at once. It applies at once.

### `autohide`

<sub>Platforms: macOS</sub>

Whether the quick terminal hides as it loses the focus, as you click another app or another of Lokyr's windows.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

It applies at once.

### `spaces`

<sub>Platforms: macOS</sub>

Whether the quick terminal follows you to the Space in front, as you switch Spaces, or stays on the one that it showed on.

| Values | Default |
|---|---|
| `"move"`, `"remain"` | `"move"` |

It applies as it shows next.

## `[plugins.search]`

<sub>Platforms: macOS</sub>

The find bar of the built-in plugin `lokyr/search`. `cmd+f` opens it at the top right of the pane that has the keyboard, of your last search's text, selected, so that typing replaces it, and its options, which it searches again; as you type, it searches the pane's screen and scrollback, the rows that show first, and counts the matches, as `3 of 12`, in a room of its own, which keeps the bar's width, numbers past 9999 short, as `12K`. It searches from the bottom up, as the newest output is at the bottom: `enter`, `cmd+g` and the bar's up arrow select the next match, the one above; `shift+enter`, `cmd+shift+g` and its down arrow the previous one, the one below. Every match that shows is highlighted, the current one in a color of its own, and the current one comes into the view, which stays there while the program prints on. `escape`, or its close button, closes it: the highlights go, and the current match is your selection, ready for `cmd+c`. The toggles at its field's end count case, match whole words alone, and take the text as a regex, which the next search keeps; the magnifying glass at its start lists your recent searches, which Lokyr keeps.

### `case`

<sub>Platforms: macOS</sub>

Whether case counts while the find bar's case toggle is off; on, it always counts.

| Values | Default |
|---|---|
| `smart`, `insensitive` | `smart` |

- `smart` counts case of a text of a capital letter, and not otherwise: `error` finds `Error`, and `Error` does not find `error`; `insensitive` never counts it.
- The toggles, of case, whole words and regular expressions, of the syntax of Rust's `regex`, keep their states from one search to the next, as Lokyr keeps them; a text that is no valid regular expression tells so in the bar, and finds nothing.
- It applies at the next search, as you save the file.

### `highlight-all`

<sub>Platforms: macOS</sub>

Whether every match that shows is highlighted, besides the current one.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The current match is highlighted either way, in its own color, and selected as the bar closes.
- It applies at once, as you save the file, to an open find bar too.

### `wrap-around`

<sub>Platforms: macOS</sub>

Whether a step past the last match goes on from the first, and past the first from the last.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- Without it, a step past either end stays there.
- It applies at once, as you save the file.

## `[plugins.command-header]`

<sub>Platforms: macOS</sub>

The command's header of the built-in plugin `lokyr/command-header`: while a pane's first row shows a command's output and the command's line is above the view, a pill of the command as you typed it, in the terminal's font, sits on a thin line across the pane's top, over its first row, which a click scrolls to the command's prompt. Its colors are the theme's, of the tokens that its keys name, so that they follow the light and the dark appearance.

### `background`, `text`, `border`, `line`, `hover`

<sub>Platforms: macOS</sub>

The theme's tokens of the header's colors, by their names: the pill's background, its text, its border, the line across the pane, and the color over the pill's background under the pointer.

| Values | Default |
|---|---|
| a token's name, as `"accent"`, `"panel.bg"`, `"field.bg"` or `"text.secondary"` | `"panel.bg"`, `"text"`, `"panel.border"`, `"panel.border"`, `"button.hover.bg"` |

- The tokens are those of the theme in force, as a theme's `[interface]` names them, and each follows the light and the dark appearance; a name that the theme lacks takes the color of its name without its last part, as `panel` for `panel.foo`, then none.
- `border = "accent"` draws the pill's border in the theme's accent.
- They apply at once, as you save the file, to a header that shows too.

### `font-size`

<sub>Platforms: macOS</sub>

The size of the header's text, which takes the terminal's font, as [`[font]`](#font) names it.

| Values | Default |
|---|---|
| points, from `0` to `200`; `0` for the terminal's own size | `0` |

- Of the terminal's own size, the pill is about a row of the terminal high, its padding past it; a smaller size keeps it within the row.
- It applies at once, as you save the file, to a header that shows too.

### `font-weight`

<sub>Platforms: macOS</sub>

The weight of the header's text, of the terminal's font's faces, the nearest that it has.

| Values | Default |
|---|---|
| `light`, `regular`, `medium`, `semibold`, `bold`, `heavy` | `regular` |

- `regular` is the terminal's own face, as [`style`](#style) names it; the others the family's faces nearest to their weights, which a family of a single weight does not have.
- It applies at once, as you save the file, to a header that shows too.

### `padding-x`, `padding-y`

<sub>Platforms: macOS</sub>

The room in the pill around its text, in points: left and right, and above and below.

| Values | Default |
|---|---|
| `padding-x` from `0` to `40`; `padding-y` from `0` to `20` | `8`, `1` |

- The pill's ends stay round, of half its height.
- They apply at once, as you save the file, to a header that shows too.

### `align`

<sub>Platforms: macOS</sub>

Where the header's pill sits along the line across the pane.

| Values | Default |
|---|---|
| `left`, `center`, `right` | `left` |

- `left` and `right` keep it a little in from the pane's edge; `center` in the pane's middle.
- It applies at once, as you save the file, to a header that shows too.
