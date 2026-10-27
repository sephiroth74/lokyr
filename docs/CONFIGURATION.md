# Configuration

Lokyr reads its settings from one TOML file. This guide lists every key that the file can hold: what it does, the values it takes and its default.

Every key and every section can be left out, and then takes its default: an empty file, or no file at all, gives Lokyr as it ships.

Under each key's name, a small line tells the platforms where the key applies: macOS alone for now, as Lokyr's first version runs on macOS. As Lokyr comes to other systems, the line names them, and a key that one system alone has says so.

## The file

- **Where it is**: `~/.config/lokyr/config.toml`, on macOS too, so that it can live with your dotfiles. When `XDG_CONFIG_HOME` is set to an absolute path, the file is `$XDG_CONFIG_HOME/lokyr/config.toml` instead.
  - A variable that your shell's startup files set, such as `XDG_CONFIG_HOME` in `~/.zshrc`, reaches Lokyr only when you start it from that shell: started from the Finder or the Dock, Lokyr reads `~/.config/lokyr/config.toml`.
  - When the file's directory does not exist, Lokyr makes it, empty, as it starts, so that a config that you write there later applies at once.
- **Links**: the file can be a symbolic link, as dotfiles managers such as GNU Stow make it. Lokyr watches the file the link points to as well.
- **Reloading**: Lokyr applies the file as soon as you save it, without a restart. Saving from vim, which writes another file and renames it over the config, works as well as saving from any other editor. Removing the file, or its directory, brings the defaults back.
- **All or nothing**: a file applies whole, or not at all. When it has an error, none of its values apply, and the settings in force stay as they were. At startup, an invalid file gives the defaults.
- **Errors**: an error tells the file, the line and the column, and what is wrong. For a key that Lokyr does not know, it names the nearest key that it knows:

  ```text
  lokyr: /Users/you/.config/lokyr/config.toml:3:1: unknown key `sise` in `[font]`; did you mean `size`?
  lokyr: none of the config applies, and the one in force stays
  ```

  For now the errors go to the log: to the terminal that started Lokyr, when you start it from one. A notice in the window comes later.

### A complete example

Every key that Lokyr reads, each with a value other than its default where it has one, and its other values in the comment:

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

[keyboard]
option-as-meta = "left"              # none | left | right | both
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

- **Default**: the family's regular face: its upright face nearest to the regular weight, of the normal width when the family has several, such as Condensed faces too.
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

A factor below one makes a closer grid, but a cell is never smaller than the glyphs of the face.

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

## `[cursor]`

The cursor that programs start with. Programs change its shape and its blinking, as vim does in insert mode, with the escape sequence DECSCUSR; `CSI 0 SP q`, which programs send to give the cursor back, and a reset bring these settings back.

### `shape`

<sub>Platforms: macOS</sub>

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
