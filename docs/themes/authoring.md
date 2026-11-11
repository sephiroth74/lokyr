# Making a theme

A theme of Lokyr is a small TOML file of colors: the terminal's background, text and 16 colors, and, if you like, any of the interface's colors, as the tab bar's, the panels' and the find bar's. A theme is for one appearance, light or dark; Lokyr derives every color that it leaves out from its terminal's colors, so that a theme of the terminal's colors alone colors all of Lokyr. Lokyr also reads iTerm2's `.itermcolors` files as they are.

A theme gives colors alone: the tab bar's look and place, the fonts, the padding and the rest stay in your config, as [the config guide](../CONFIGURATION.md#theme) tells.

## The smallest theme

```toml
[theme]
name = "Harbor"
variant = "dark"

[terminal]
background = "#14181f"
foreground = "#d8dee9"
palette = [
    "#3b4252", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d8dee9",
    "#4c566a", "#ef8a91", "#b5e09a", "#f3d28a", "#86c3ff", "#d79bea", "#7fd0db", "#eceff4",
]
```

Save it as `Harbor.toml` in Lokyr's themes folder, `~/.config/lokyr/themes/` (`$XDG_CONFIG_HOME/lokyr/themes/` where that variable is set), and name it in your config:

```toml
[theme]
dark = "Harbor"
```

Lokyr applies it as you save either file, in the windows that are open too. A file with an error tells it in a bar across the windows' top, of its line, and the theme in force stays.

[`example.toml`](example.toml) is a theme of every key and every color that this guide tells, of the values of Lokyr's own dark theme: copy it and change what you like.

## Installing and choosing a theme

- **Install** a theme by putting its file in the themes folder; or drop the file on Lokyr's icon in the Dock, or open an `.itermcolors` file with Lokyr, and Lokyr asks whether to install it: **Install** copies it into the folder, **Install and Use** names it in your config for its appearance too, and a checkbox switches Lokyr to that appearance where the theme is of the other one.
- **Choose** a theme with View › Choose Theme…, which lists the themes of the appearance in force and shows each as you move to it: Return keeps it, in your config, and Escape goes back.
- **The config** names a theme for each appearance, `[theme] light` and `dark`, by its name: a file's of the themes folder, a theme of the collection that Lokyr holds, or a built-in one, `Lokyr Light` and `Lokyr Dark`. A file of the folder takes the place of a theme of the same name. `[theme] minimum-contrast`, from 1 to 21, makes the terminal's text of at least that contrast with its background, whatever a theme gives.

## The file

A theme's file is TOML of sections. Colors are `#rrggbb`, or `#rrggbbaa` of an opacity, in sRGB.

### `[theme]`

| Key | What it is |
|---|---|
| `name` | Its name, for people: Lokyr knows a theme by its file's name, without `.toml`, which the config and the picker use |
| `variant` | `"light"` or `"dark"`: the appearance that it is for; without it, light where its background is lighter than its text |
| `extends` | The name of a theme whose colors it takes where it gives none: a file's, one of the collection or a built-in one, eight deep at most |
| `author`, `license`, `source` | Who made it, its license as an SPDX expression, and where it comes from: they tell people, and color nothing |

### `[terminal]`

| Key | What it colors | Without it |
|---|---|---|
| `background` | The terminal's default background | Needed, of the theme or of the theme that it extends |
| `foreground` | The terminal's default text | Needed, likewise |
| `palette` | The 16 colors, black, red, green, yellow, blue, magenta, cyan and white, then their bright ones | Needed, likewise |
| `cursor` | The cursor | The text's color |
| `selection` | The selection's background | The token `field.selection` |
| `selection-text` | The selected text | The text's own color |
| `bold` | Bold text of the default color | Its own color |
| `underline` | An underline's color | The text's color |
| `cursor-text`, `link` | The text under a block cursor, and a link's color | Read, and not applied yet |

### The interface's colors

The interface's colors are **tokens**, each by its name, as `panel.bg` or `tabs.active.fg`. A theme gives a token in the section of its name's first part, of the rest of its name, or in `[interface]` by its whole name:

```toml
[tabs]
active.bg = "#2b3240"          # the token tabs.active.bg
active = { fg = "#ffffff" }    # tabs.active.fg, as TOML's tables have it

[interface]
"panel.bg" = "#1c212b"
"accent" = "#61afef"
```

- `[search]` is the find bar's: `match`, every match, and `current`, the one that the find bar is at, the tokens `search.match` and `search.match.current`.
- A token and a longer one of its name in the same section need the longer one's key quoted, as `scrollbar.mark` and `scrollbar.mark.current` in `[scrollbar]`: `mark = "…"` and `"mark.current" = "…"`.
- A token that a theme does not give, nor the theme that it extends, takes Lokyr's derived color, below; a name that Lokyr does not know takes the color of its name without its last part, as `tabs.active` for `tabs.active.foo`, then of `tabs`.
- `transparent` is clear in every theme, as a configuration that names a token may name it.

#### The tokens

Each token, what it colors, and what Lokyr derives it of where a theme gives none. "The text" and "the background" are the terminal's `foreground` and `background`; "the accent" is the palette's blue, or its bright blue, whichever reads on the background, or else the text; "light / dark" values are those of a light and of a dark theme.

| Token | What it colors | Derived |
|---|---|---|
| `text` | The interface's text | The text |
| `text.secondary` | Fainter text, as a count or a hint | The text, 38% toward the background, readable on a panel |
| `text.disabled` | The text of what is dimmed | The text, 62% toward the background |
| `accent` | The accent: what is chosen, focused or running | The accent |
| `separator` | The lines between groups | The text at 10% |
| `shadow` | The shadows under panels | Black at 25% / 50% |
| `terminal.bg`, `terminal.fg` | A bar of the terminal's own colors | The background and the text |
| `panel.bg`, `panel.border` | The panels: the find bar, the palette, the menus | The background 4% / 6% toward the text; the text at 14% / 12% |
| `field.bg`, `field.border` | The text fields | The background; the text at 18% / 16% |
| `field.focus` | A field's border as it has the keyboard | The accent at 50% / 60% |
| `field.selection` | A field's selected text | The background 30% / 40% toward the accent |
| `focus.ring` | The ring of the keyboard's focus | The accent at 50% / 60% |
| `button.bg`, `button.hover.bg`, `button.pressed.bg` | The buttons, at rest, under the pointer and pressed | The text at 6% / 8%, 10% / 12% and 16% / 18% |
| `toggle.on.bg`, `toggle.on.fg` | A toggle that is on | The accent at 12% / 20%; the accent |
| `tooltip.bg`, `tooltip.fg`, `tooltip.border` | The tooltips | The background 3% / 10% toward the text; the text; the text at 15% |
| `list.selection.bg`, `list.selection.fg` | A list's selected row, as the palette's | The accent; white or black, whichever reads on it |
| `list.selection.inactive.bg` | A selected row of a list that has not the keyboard | The text at 10% / 12% |
| `scrollbar`, `scrollbar.hover`, `scrollbar.track` | The scroll bar, under the pointer, and its track | The text at 30% / 35%, 50% / 55% and 5% / 7% |
| `scrollbar.mark`, `scrollbar.mark.current` | The search's marks beside the scroll bar, and the current one | The palette's yellow; orange, of its yellow and red |
| `pane.divider` | The lines between split panes | The text at 16% |
| `pane.drop` | The half of a pane that a dragged tab lights | The accent at 25% / 30% |
| `progress.track`, `progress.normal`, `progress.error`, `progress.paused` | A program's progress, its track and its states | The text at 10% / 14%; the accent; red; yellow |
| `tabs.bar.bg`, `tabs.track.bg` | The tab bar, and the track of its pills | The background 6% toward the text; the text at 7% / 10% |
| `tabs.active.bg`, `tabs.active.fg`, `tabs.active.border`, `tabs.active.shadow` | The tab that shows | The background; the text; the text at 10% / 12%; black at 12% / 40% |
| `tabs.inactive.bg`, `tabs.inactive.fg` | The other tabs | Clear; the text 40% toward the bar, readable on it |
| `tabs.hover.bg` | A tab under the pointer | The text at 5% / 6% |
| `tabs.seamless.line`, `tabs.seamless.veil` | The seamless tab bar's line and the veil over its other tabs | The text at 12%; black at 6% / 27% |
| `segments.bg`, `segments.chosen.bg`, `segments.chosen.fg` | The segmented controls, and their chosen segment | The text at 6% / 8%; the accent; white or black, whichever reads on it |
| `segments.box.bg`, `segments.box.border` | The box under a segmented control's tabs | The text at 3%; the text at 10% |
| `search.match`, `search.match.current` | The find bar's matches, and the current one | Yellow at 50% / 30%; orange at 60% / 50% |
| `gutter.running`, `gutter.succeeded`, `gutter.failed` | The bar beside the prompt of a command that runs, that succeeded and that failed | The accent; the text at 22% / 25%; red |

#### A plugin's tokens

A plugin can take colors of a theme too: a section of the theme of a name of the plugin's gives tokens of that name, which the plugin's config names. The built-in command header names its colors so:

```toml
# In the theme
[command-header]
pill = "#2a2a2d"

# In the config
[plugins.command-header]
background = "command-header.pill"
```

A theme that lacks the token leaves the header the color of the name's first part, then none; [the command header's guide](../CONFIGURATION.md#pluginscommand-header) tells its keys.

### `extends`

```toml
[theme]
name = "Harbor Bright"
extends = "Harbor"

[terminal]
background = "#0b0e13"
```

A theme that extends another takes all of its colors and gives its own over them: the smallest change of a theme that you like is a file of `extends` and the colors that you change. A theme extends eight others at most, one after another, and none of its chain twice.

## iTerm2's `.itermcolors`

Lokyr reads an `.itermcolors` file of the themes folder as iTerm2 reads it, and never converts it: its name is the file's, without `.itermcolors`. Each color's `Color Space` tells how its components read: `sRGB` as they are, `P3` of Display P3, and `Calibrated`, `Device` or none, as old files have, of calibrated Generic RGB; each converted to sRGB and cut to its gamut, as macOS cuts it. Lokyr takes its `Ansi 0 Color` to `Ansi 15 Color`, `Background Color`, `Foreground Color`, `Cursor Color`, `Cursor Text Color`, `Selection Color`, `Selected Text Color`, `Bold Color`, `Link Color` and `Underline Color`, and derives the interface's colors of them. A file of keys of both appearances, ending in ` (Light)` and ` (Dark)`, is two themes, `<name> (Light)` and `<name> (Dark)`, each named so in the config.

## Sharing a theme

Share a theme's file as it is: anyone installs it as above. Lokyr's registry, [pkg.lokyr.dev](https://pkg.lokyr.dev), will publish themes too, each of a page of its colors, in a later version.
