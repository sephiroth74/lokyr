# Making a theme

A theme of Lokyr is a small TOML file of colors: the terminal's background, text and 16 colors, and, if you like, any of the interface's colors, as the tab bar's, the panels' and the find bar's. A theme is for one appearance, light or dark; Lokyr colors every interface color that it leaves out of a rule of its terminal's colors and of the other interface colors, so that a theme of the terminal's colors alone colors all of Lokyr, and a theme's accent colors whatever is of the accent. Lokyr also reads iTerm2's `.itermcolors` files as they are: [Using an iTerm2 theme](#using-an-iterm2-theme) tells where to put them.

A theme gives colors alone: the tab bar's look and place, the fonts, the padding and the rest stay in your config, as [the config guide](../CONFIGURATION.md#theme) tells.

## Using an iTerm2 theme

Lokyr reads iTerm2's `.itermcolors` files as they are, without converting them: a theme of the iTerm2-Color-Schemes collection, or one that a theme's site offers for iTerm2, works in Lokyr as you download it.

First look whether Lokyr already has it: it holds 278 themes of that collection, as `Dracula`, `Catppuccin Mocha`, `Nord` or `TokyoNight Day`, which View › Choose Theme… lists with the others.

**1. Put the file in Lokyr's themes folder:**

```text
~/.config/lokyr/themes/
```

Where `XDG_CONFIG_HOME` is set, the folder is `$XDG_CONFIG_HOME/lokyr/themes/`. View › Open Themes Folder, which the command palette runs too, opens it in the Finder, making it first if it is not there, so that the file can be dragged in. From a terminal:

```sh
mkdir -p ~/.config/lokyr/themes
cp ~/Downloads/Nightfall.itermcolors ~/.config/lokyr/themes/
```

Or let Lokyr copy it: double-click the file in the Finder, or drop it on Lokyr's icon in the Dock, and choose **Install**, which copies it into the folder, or **Install and Use**, which also makes it the theme of its appearance. Where another app opens `.itermcolors` files, open it with Lokyr from the Finder's Open With menu.

**2. Choose it.** The theme's name is the file's, without `.itermcolors`: `Nightfall.itermcolors` is `Nightfall`. Choose it in View › Choose Theme…, or name it in your config for the appearance that it is for:

```toml
[theme]
dark = "Nightfall"
```

Lokyr applies it as you save the file, in the windows that are open too. A file of colors for both appearances, of keys ending in ` (Light)` and ` (Dark)`, gives two themes, `Nightfall (Light)` and `Nightfall (Dark)`. A light theme shows while Lokyr is light, and a dark one while it is dark, as macOS is unless View › Appearance chooses: the picker lists the themes of the appearance in force, and Install's question offers to switch Lokyr's appearance where the theme is of the other one.

How Lokyr reads the file's colors is told in [How Lokyr reads `.itermcolors`](#how-lokyr-reads-itermcolors).

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

Lokyr applies it as you save either file, in the windows that are open too. A file with an error tells it in a card at the windows' top right, of its line, and the theme in force stays.

[`example.toml`](example.toml) is a theme of every key and every color that this guide tells, of the values of Lokyr's own dark theme: copy it and change what you like.

To start from Lokyr's own values, **New…** on the settings window's Themes page makes a theme of every key of Lokyr Light or Lokyr Dark, as the page shows, the terminal's optional keys as comments of their defaults, and the interface colors that it leaves to their rules as comments of the rules, which a line uncommented gives. To start from a theme that you like, View › New Theme from Current… makes a new theme of every color of the one in force, and **Duplicate…** in the context menu of a theme's card on the settings window's Themes page of that theme: Lokyr asks its name, writes it whole into the themes folder, `themes/<name>.toml`, the interface colors that it leaves out as comments of their rules, of the original's author, license and source, uses it for its appearance and opens it in your editor, where each save shows at once. A name that a theme has already, of the folder, of Lokyr's own or built in, is refused.

## Installing and choosing a theme

- **Install** a theme by putting its file in the themes folder; or drop the file on Lokyr's icon in the Dock, or open an `.itermcolors` file with Lokyr, and Lokyr asks whether to install it: **Install** copies it into the folder, **Install and Use** names it in your config for its appearance too, and a checkbox switches Lokyr to that appearance where the theme is of the other one.
- **Import** themes with View › Import Themes…, or Import… on the settings window's Themes page: the open panel takes several `.itermcolors` and `.toml` files at once, which Lokyr copies into the themes folder as they are, asking once whether to replace those of a name there already.
- **Choose** a theme with View › Choose Theme…, which lists the themes of the appearance in force and shows each as you move to it: Return keeps it, in your config, and Escape goes back.
- **The config** names a theme for each appearance, `[theme] light` and `dark`, by its name: a file's of the themes folder, a theme of the collection that Lokyr holds, or a built-in one, `Lokyr Light` and `Lokyr Dark`. A file of the folder takes the place of a theme of the same name. `[theme] minimum-contrast`, from 1 to 21, makes the terminal's text of at least that contrast with its background, whatever a theme gives.

## The file

A theme's file is TOML of sections, of 1 MiB at most, as an `.itermcolors` file is. Colors are `#rrggbb`, or `#rrggbbaa` of an opacity, in sRGB.

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
- A token that a theme does not give, nor the theme that it extends, takes the color of its rule, below; a name that Lokyr does not know takes the color of its name without its last part, as `tabs.active` for `tabs.active.foo`, then of `tabs`.
- A rule that names another token follows the color that the theme gives that token: a theme's `accent` colors `focus.ring`, `list.selection.bg` and the others whose rules name the accent, and its `text` those that name the text, unless the theme gives them too.
- `transparent` is clear in every theme, as a configuration that names a token may name it.
- A name of the sections of Lokyr's tokens, or of `[interface]`, that Lokyr does not know, as `tabs.actve.bg`, is told in a card at the windows' top right, of its line, and colors nothing that Lokyr draws.

#### Colors of other colors

A token's value is a color, or an expression of other colors, which follows them as they change:

```toml
[interface]
"accent" = "#c678dd"
"focus.ring" = "accent / 40%"                      # the accent at 40%
"tooltip.bg" = "mix(background, foreground, 8%)"   # the background, 8% toward the text
"list.selection.fg" = "contrasting(list.selection.bg)"
```

- **A color**: `#rrggbb` or `#rrggbbaa`, and `transparent`.
- **The terminal's colors**: `background`, `foreground`, `cursor`, `selection`, and the 16 colors by their names, `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, then `bright-black` to `bright-white`.
- **A token**, by its name, as `accent` or `panel.bg`, of the theme's color or of its rule's.
- `X / N%`: X at an opacity of N%, whatever its own.
- `mix(X, Y, N%)`: X mixed toward Y by N%, opaque.
- `readable(X, on: Y)`: X, moved toward white or black as little as it takes to read on Y, of three quarters of the contrast of the theme's text on its background, 4.5 at most.
- `contrasting(X)`: white or black, whichever reads better on X.
- `first(X, Y, …, on: Z, at: R)`: the first of the colors of a contrast of R at least on Z, from 1 to 21, else the last.

An expression that does not read, a name of no color, and tokens that name each other, as `button.bg` of `button.hover.bg` and `button.hover.bg` of `button.bg`, are told in the card, of their line, the token taking its rule's color; the rest of the theme is in force.

An expression is 1024 bytes at most, its functions nested 32 deep at most; tokens that name each other go 64 deep at most; and a file gives 512 tokens at most, those past it told and left out.

#### The tokens

Each token, what it colors, and its rule, the color that it takes where a theme gives none, of the terms that [Colors of other colors](#colors-of-other-colors) tells. A token of two rules has one of a light theme and one of a dark one, a theme dark where its background is darker than its text.

<!-- The tokens' table, of the rules: `cargo xtask theme-guide` writes it. -->

| Token | What it colors | Its rule |
|---|---|---|
| `text` | The interface's text | `foreground` |
| `text.secondary` | Fainter text, as a count or a hint | `readable(mix(text, background, 38%), on: panel.bg)` |
| `text.disabled` | The text of what is dimmed | `mix(text, background, 62%)` |
| `text.success` | Text that tells that something works or went well, as a plugin's state, active | `readable(green, on: background)` |
| `text.warning` | Text that tells that something does not work or needs a look, as a plugin's state, disabled | `readable(yellow, on: background)` |
| `accent` | The accent: what is chosen, focused or running | `first(blue, bright-blue, foreground, on: background, at: 3)` |
| `separator` | The lines between groups | `text / 10%` |
| `pane.divider` | The lines between split panes | `text / 16%` |
| `pane.drop` | The half of a pane that a dragged tab lights | light `accent / 25%`, dark `accent / 30%` |
| `shadow` | The shadows under panels | light `#000000 / 25%`, dark `#000000 / 50%` |
| `panel.bg` | The panels' background: the find bar, the palette, the menus | light `mix(background, foreground, 4%)`, dark `mix(background, foreground, 6%)` |
| `panel.border` | The panels' border | light `text / 14%`, dark `text / 12%` |
| `field.bg` | The text fields' background | `background` |
| `field.border` | The text fields' border | light `text / 18%`, dark `text / 16%` |
| `field.focus` | A text field's border as it has the keyboard | light `accent / 50%`, dark `accent / 60%` |
| `field.selection` | A text field's selected text, and the terminal's selection where the theme gives none | light `mix(background, accent, 30%)`, dark `mix(background, accent, 40%)` |
| `focus.ring` | The ring of the keyboard's focus | light `accent / 50%`, dark `accent / 60%` |
| `button.bg` | A button at rest | light `text / 6%`, dark `text / 8%` |
| `button.hover.bg` | A button under the pointer | light `text / 10%`, dark `text / 12%` |
| `button.pressed.bg` | A button pressed | light `text / 16%`, dark `text / 18%` |
| `toggle.on.bg` | A toggle that is on | light `accent / 12%`, dark `accent / 20%` |
| `toggle.on.fg` | The mark of a toggle that is on | `accent` |
| `tooltip.bg` | The tooltips' background | light `mix(background, foreground, 3%)`, dark `mix(background, foreground, 10%)` |
| `tooltip.fg` | The tooltips' text | `text` |
| `tooltip.border` | The tooltips' border | `text / 15%` |
| `list.selection.bg` | A list's selected row, as the palette's | `accent` |
| `list.selection.fg` | The text of a list's selected row | `contrasting(list.selection.bg)` |
| `list.selection.inactive.bg` | A selected row of a list that has not the keyboard | light `text / 10%`, dark `text / 12%` |
| `scrollbar` | The scroll bar | light `text / 30%`, dark `text / 35%` |
| `scrollbar.hover` | The scroll bar under the pointer | light `text / 50%`, dark `text / 55%` |
| `scrollbar.track` | The scroll bar's track | light `text / 5%`, dark `text / 7%` |
| `scrollbar.mark` | The find's marks beside the scroll bar | `yellow` |
| `scrollbar.mark.current` | The current match's mark beside the scroll bar | `mix(yellow, red, 50%)` |
| `progress.track` | The track of a program's progress | light `text / 10%`, dark `text / 14%` |
| `progress.normal` | A program's progress as it runs | `accent` |
| `progress.error` | A program's progress that failed | `red` |
| `progress.paused` | A program's progress paused | `yellow` |
| `tabs.bar.bg` | The tab bar | `mix(background, foreground, 6%)` |
| `tabs.track.bg` | The track of the tab bar's pills | light `text / 7%`, dark `text / 10%` |
| `tabs.active.bg` | The tab that shows | `background` |
| `tabs.active.fg` | The title of the tab that shows | `text` |
| `tabs.active.border` | The border of the tab that shows | light `text / 10%`, dark `text / 12%` |
| `tabs.active.shadow` | The shadow of the tab that shows | light `#000000 / 12%`, dark `#000000 / 40%` |
| `tabs.inactive.bg` | The other tabs | `transparent` |
| `tabs.inactive.fg` | The other tabs' titles | `readable(mix(text, background, 40%), on: tabs.bar.bg)` |
| `tabs.hover.bg` | A tab under the pointer | light `text / 5%`, dark `text / 6%` |
| `tabs.seamless.line` | The seamless tab bar's line | `text / 12%` |
| `tabs.seamless.veil` | The veil over the seamless tab bar's other tabs | light `#000000 / 6%`, dark `#000000 / 27%` |
| `segments.bg` | A segmented control | light `text / 6%`, dark `text / 8%` |
| `segments.chosen.bg` | A segmented control's chosen segment | `accent` |
| `segments.chosen.fg` | The text of a segmented control's chosen segment | `contrasting(segments.chosen.bg)` |
| `segments.box.bg` | The box under a segmented control's tabs | `text / 3%` |
| `segments.box.border` | The border of the box under a segmented control's tabs | `text / 10%` |
| `search.match` | The find bar's matches | light `yellow / 50%`, dark `yellow / 30%` |
| `search.match.current` | The find bar's current match | light `mix(yellow, red, 50%) / 60%`, dark `mix(yellow, red, 50%) / 50%` |
| `gutter.running` | The bar beside the prompt of a command that runs | `accent` |
| `gutter.succeeded` | The bar beside the prompt of a command that succeeded | light `text / 22%`, dark `text / 25%` |
| `gutter.failed` | The bar beside the prompt of a command that failed | `red` |
| `terminal.bg` | A bar of the terminal's own background | `background` |
| `terminal.fg` | A bar of the terminal's own text | `foreground` |
| `cursor.guide` | The band across the cursor's row, as the cursor guide shows it | `foreground / 10%` |

<!-- The tokens' table's end. -->

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

## How Lokyr reads `.itermcolors`

Lokyr reads an `.itermcolors` file of the themes folder as iTerm2 reads it, and never converts it: its name is the file's, without `.itermcolors`. Each color's `Color Space` tells how its components read: `sRGB` as they are, `P3` of Display P3, and `Calibrated`, `Device` or none, as old files have, of calibrated Generic RGB; each converted to sRGB and cut to its gamut, as macOS cuts it. Lokyr takes its `Ansi 0 Color` to `Ansi 15 Color`, `Background Color`, `Foreground Color`, `Cursor Color`, `Cursor Text Color`, `Selection Color`, `Selected Text Color`, `Bold Color`, `Link Color`, `Underline Color` and `Cursor Guide Color`, the token `cursor.guide`, and colors the interface of them by the tokens' rules. A file of keys of both appearances, ending in ` (Light)` and ` (Dark)`, is two themes, `<name> (Light)` and `<name> (Dark)`, each named so in the config.

## Sharing a theme

Share a theme's file as it is: anyone installs it as above.
