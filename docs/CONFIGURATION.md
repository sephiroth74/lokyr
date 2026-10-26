# Configuration

Bolt reads its settings from one TOML file. This guide lists every key that the file can hold: what it does, the values it takes and its default.

Every key and every section can be left out, and then takes its default: an empty file, or no file at all, gives Bolt as it ships.

## The file

- **Where it is**: `~/.config/bolt/config.toml`, on macOS too, so that it can live with your dotfiles. When `XDG_CONFIG_HOME` is set to an absolute path, the file is `$XDG_CONFIG_HOME/bolt/config.toml` instead.
- **Links**: the file can be a symbolic link, as dotfiles managers such as GNU Stow make it. Bolt watches the file the link points to as well.
- **Reloading**: Bolt applies the file as soon as you save it, without a restart. Saving from vim, which writes another file and renames it over the config, works as well as saving from any other editor.
- **All or nothing**: a file applies whole, or not at all. When it has an error, none of its values apply, and the settings in force stay as they were. At startup, an invalid file gives the defaults.
- **Errors**: an error tells the file, the line and the column, and what is wrong. For a key that Bolt does not know, it names the nearest key that it knows:

  ```text
  bolt: ~/.config/bolt/config.toml:3:1: unknown key `sise` in `[font]`; did you mean `size`?
  bolt: none of the config applies, and the one in force stays
  ```

  For now the errors go to the log: to the terminal that started Bolt, when you start it from one. A notice in the window comes later.

### A complete example

```toml
config_version = 1

[font]
family = "CaskaydiaCove NFM"
style = "SemiBold"
size = 22

[cursor]
shape = "bar"
blink = true
thickness = 2

[window]
close-on-exit = "clean"

[keyboard]
option-as-meta = "left"
```

## `config_version`

The version of the file's format. Bolt reads version `1`, and refuses any other.

| Value | Default |
|---|---|
| `1` | `1` |

The key is the only one at the top of the file, and the only one with an underscore: the keys of the sections are written with hyphens.

## `[font]`

The font of the text: its family, its size and the faces that bold and italic text take.

A character that the font lacks, such as Japanese text or an emoji in most programming fonts, takes the font that macOS gives for it, as other apps take it: Hiragino for Japanese, Apple Color Emoji for emoji. A character followed by the variation selector that asks for an emoji, U+FE0F, takes the emoji even when the font has a text form of it.

### `family`

The font family, by its name, as Font Book shows it on macOS, such as `"Menlo"` or `"CaskaydiaCove NFM"`.

- **Default**: the system's monospaced font, SF Mono.
- The case does not matter: `"menlo"` is Menlo.
- `"SF Mono"` names the system's monospaced font, which macOS installs under another name.
- A name that some faces of a family give themselves, as a font and its weight, such as `"CaskaydiaCove NFM SemiBold"`, names the whole family, and takes those faces as the regular text's.
- A family that is not installed gives the default one, and the log tells it. The styles that the file names then do not apply, as they are the missing family's.

### `size`

The size of the text, in points.

| Values | Default |
|---|---|
| from `4` to `200`, whole or not | `13` |

The size is in points, so that the text takes the same room on a Retina display, at twice the pixels, as on another.

### `style`

The face of the text that has no style, by the name that the font gives it, such as `"SemiBold"` or `"Light"`.

- **Default**: the family's regular face: its upright face nearest to the regular weight.
- The case, spaces, hyphens and underscores do not matter: `"semi-bold"` is `SemiBold`.
- A style that the family does not have is an error, which lists the styles that it has.

### `bold-style`, `italic-style`, `bold-italic-style`

The faces of bold, italic and bold italic text, named as `style` names its face.

- **Default of `bold-style`**: the family's upright face heavier than the regular one, nearest to Bold's weight, or one step past the regular one's: Regular takes Bold, SemiBold takes Bold, and Bold takes ExtraBold. A family without one draws its regular face heavier.
- **Default of `italic-style`**: the family's italic face nearest to the regular one's weight. A family without italics slants its regular face.
- **Default of `bold-italic-style`**: the family's italic face nearest to the bold one's weight. A family without one slants its bold face, or draws its italic one heavier.

### `non-ascii-family`

The family of every character past ASCII, such as `"Hiragino Sans"` for Japanese text.

- **Default**: the text's own family.
- Its faces follow the weights of the text's: bold text takes its bold face. The styles that `style` and the other keys name are the text's family's, not this one's.
- A character that this family lacks takes the font that macOS gives for it, as for the text's own family.
- A family that is not installed leaves that text to the text's family, and the log tells it.

### `line-height`, `cell-width`

The height and the width of the grid's cells, as factors of the face's line height and of its characters' advance.

| Values | Default |
|---|---|
| from `0.5` to `3` | `1` |

A factor below one makes a closer grid, but a cell is never smaller than the glyphs of the face.

### `ligatures`

Whether the glyphs that a font joins into ligatures, such as `->` or `!=`, are joined.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

Bolt reads and checks this key, but does not apply it yet: it comes with shaping, and until then no ligature is drawn.

## `[cursor]`

The cursor that programs start with. Programs change its shape and its blinking, as vim does in insert mode, with the escape sequence DECSCUSR; `CSI 0 SP q`, which programs send to give the cursor back, and a reset bring these settings back.

### `shape`

| Values | Default |
|---|---|
| `"block"`, `"bar"`, `"underline"`, `"hollow"` | `"block"` |

- `"block"` covers the cell, and the text under it takes the cell's background.
- `"bar"` is a vertical line at the cell's left edge.
- `"underline"` is a line at the cell's bottom edge.
- `"hollow"` is a block of its edges alone. A window without the focus always shows a hollow block, whatever the shape.

Programs can choose among the first three alone, as DECSCUSR has no hollow block.

### `blink`

Whether the cursor blinks.

| Values | Default |
|---|---|
| `true`, `false` | `true` |

- The cursor stays on for half a second and off for another.
- It stops blinking, and stays on, 15 seconds after the last key or the last output, so that an idle window keeps the Mac idle too. A key, output, or the window's focus makes it blink again.
- Only the window that has the focus blinks, and a cursor that the program hides, as full-screen programs such as `htop` hide it, does not blink.
- A program can turn blinking on or off with DECSCUSR. The escape sequence of mode 12, which some programs send as they start, turns blinking on too; its reset, which most full-screen programs send as they end, leaves this setting as it is.

### `thickness`

The width of a bar and the height of an underline, in points, so that they keep their look on a Retina display.

| Values | Default |
|---|---|
| from `0.5` to `20` | the font's underline thickness |

A hollow block's edges always take the font's underline thickness.

## `[window]`

### `close-on-exit`

What a window does when its program ends, such as the shell after `exit`.

| Values | Default |
|---|---|
| `"clean"`, `"always"`, `"never"` | `"clean"` |

- `"clean"` closes the window when the program exits with the status 0, and otherwise keeps it open, with a line that tells how the program ended, such as `[the program exited with code 1; a key closes the window]`, until you press a key.
- `"always"` closes the window.
- `"never"` keeps it open, with the line, until you press a key.

Closing the last window quits Bolt.

## `[keyboard]`

### `option-as-meta`

Which Option keys act as Meta: they send an escape before the key, rather than typing the characters that the keyboard layout gives them.

| Values | Default |
|---|---|
| `"none"`, `"left"`, `"right"`, `"both"` | `"none"` |

- The default is `"none"`, as Option types characters that many layouts need, such as `@`, `#`, `[`, `]`, `{`, `}` and `~` on the Italian layout.
- `"left"` lets the right Option type those characters, and makes the left one Meta, for the shortcuts of shells and editors such as `Meta-b` and `Meta-f`.
- An Option that acts as Meta does not compose either: its dead keys, such as Option and `e` on the US layout, send Meta and `e`.
- With Meta, a key whose character is past ASCII sends the key in its place on the US layout: Meta and `ò` on the Italian layout send Meta and `;`.
