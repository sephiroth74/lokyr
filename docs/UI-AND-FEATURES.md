# UI engine and main features

This document covers our UI engine and the features the MVP cannot ship without: tabs, splits, search, iTerm2 themes and the cursor's options.

## UI engine

`ui-core` is our small, platform-independent toolkit ([ADR 0004](adr/0004-native-shell-and-gpu-chrome.md)). It draws all of the chrome on the GPU, into the same drawable as the panes. Why our own, rather than a framework of Rust's or the native toolkits, and what it takes from them: [UI-TOOLKITS.md](UI-TOOLKITS.md).

**View tree and layout**
- Plugins describe their UI as a view tree, plain data of `plugin-sdk`'s types, which the host stores as a flat arena of nodes (see [PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md#the-luau-api) and [ADR 0028](adr/0028-plugin-calls-as-messages.md)).
- A widget that holds state keeps it in the host, by the node's key, which it requires: a text field's text, caret, selection and composition, a list's scroll and selection. Its plugin learns of the changes through events, whatever its thread or language, so that no key goes through the plugin and back.
- `taffy` computes the layout, using a subset of flexbox: direction, gap, padding, alignment, sizes, grow.
- Each new tree is diffed against the previous one by `key`, so only the changed parts are laid out and turned into display-list entries again.

**Widgets**: those that the chrome and F3's plugins need, as the user chose on 1 November 2026. A plugin describes its UI with them and the host draws it: no plugin draws pixels or opens a native window, so that its UI takes the theme, the focus, the input method and accessibility, costs the frame no more than the host allows, and no plugin can pass itself off as the system.
- container, text, icon, spacer, separator;
- text field with IME, selection and clipboard, whose text the host holds;
- button and toggle;
- virtualized list, of flat items and a row's template, and scroll area;
- tooltip;
- `progress`, of four states, normal, error, paused and indeterminate, whose indeterminate state the window server animates, on a layer of its own as the cursor's blinking, without frames of the app's;
- floating panels, anchored to a cell, the cursor or the pointer, with the keyboard's focus inside them;
- menus. Context menus and the menu bar are native `NSMenu`s on macOS, to which plugins add their items, and drawn by us on Linux; menus drawn by `ui-core` serve inside panels alone, as a find bar's choice.
- A canvas of drawing primitives, for a plugin's own graphics, comes with F4's Luau API.

**Focus and IME**
- Focus belongs to exactly one element: the window's pane, or a node of a panel that takes it, a widget or a node of a key that a plugin marks `focusable`. A press over a panel's node gives it to the innermost node under the pointer that takes it.
- Of the keys that no binding takes, Tab and Shift-Tab move it among the panel's nodes that take it, in the tree's order, from the last round to the first; Escape gives it back to the pane; every other key goes to the element that has it alone, and a key's release where its press went. A node taken away leaves it to the node in its place, and a panel that closes, or has none left, to the pane. The ring of the focus, of the theme's `focus.ring`, goes round the node that has it, over the others.
- The keymap's contexts tell where it is: `pane.focused` while the pane has it; of a plugin's panels, `namespace.open` while one shows and `namespace.focused` while one has it, as `search.open` and `search.focused`.
- The platform IME client (`NSTextInputClient`, text-input-v3, XIM) is routed to that element, along with the cursor position for the candidate window.

**Accessibility**
- AccessKit receives a tree built from the view tree, with a role and a label on every node.
- Panes expose their visible text and cursor to VoiceOver and AT-SPI.

**Themes**: plugins use tokens (`tab.active.bg`, `search.match`, …) rather than fixed colors, so a theme restyles all the chrome. A token that a theme lacks takes the color of its name without its last part, `tab.active` for `tab.active.bg`, then `tab`, so that a theme of a few colors still covers them all; the built-in theme gives every token of the chrome, in its light and dark variants.

**Text and icons**: the chrome's text is the interface's font, the system's by default, at the optical size of its points, so that a display of twice the pixels draws the same forms twice as large and the layout's measures hold at every scale; its glyphs share the atlas of the terminal's. The icons are the codicons of the Symbols Nerd Font that the app carries, by their names (`terminal`, `close`, `search`, …), in the text's color.

**The pointer**: the node under the pointer is the topmost whose box holds it, within its ancestors' clips, as the tree draws a node over its parent and over its siblings before it. Its events go to the nodes of a key, by which their plugins know them: the hover to each node of a key that the pointer is over, its ancestors with it, so that the pointer over a tab's close is over the tab too; a click, a double click, a middle click and the context menu's to the innermost node of a key, the button down and up over it, the context menu's as it goes down. A node's style gives the pointer's shape over it, the arrow, the I-beam or the pointing hand, which its children take unless they give their own; the window server shows it as the pointer moves, of areas that the host gives it, without waking the app.

**Animations**
- Host-driven transitions (opacity, position), e.g. when tabs are reordered.
- The display link runs only while an animation is in progress.

**Display list**
- Primitives: rectangles (fill, border, radius, shadow), glyph runs from the atlas, images, clips.
- A pane's instanced cell batch includes the selection and decorations (curly, dotted and dashed underlines, strikethrough, overline); the cursor's cells and the text that blinks go into batches of their own, for layers over the window's surface, which the window server blinks.
- A window produces a display list covering chrome, panes and overlays, and one for each of those layers, drawn again only as their cells change.

## Tabs

The `tabs` plugin. Each window's tab list lives in the kernel. The plugin handles the tab bar, the commands and the interaction.

### Window integration

**macOS**
- **Tab bar in the titlebar**: `NSWindow` with `fullSizeContentView`, a transparent titlebar and a hidden title. Our engine draws the tab bar where the titlebar would be.
- **Native tabs disabled**: `tabbingMode = .disallowed` and `NSWindow.allowsAutomaticWindowTabbing = false`.
- **Traffic lights**: an empty unified `NSToolbar` vertically centers them on the bar. Moving them by hand means fighting AppKit's relayout.
- **Moving the window**: dragging the bar calls `performWindowDragWithEvent:`.
- **Double-click on the bar**: follows the system preference (`AppleActionOnDoubleClick`).
- **Fullscreen**: the bar appears together with the system titlebar.
- **Resizing**: the content follows the window's edge through a live resize, drawn with Core Animation's transaction; the grid's size, as `100 × 33`, shows at the window's centre while a resize changes it, and goes a second after the last change, or after the drag ends.
- **Dragging tabs between windows**: our own implementation, using a drag session and a ghost window.
- **The quick terminal**: a borderless, non-activating `NSPanel` that a global key, Carbon's `RegisterEventHotKey`, which asks for no Accessibility permission, slides in from an edge of a screen over every app, on every Space or on its own and over the apps in full screen, and out again, as `NSAnimationContext` moves its frame; it takes the keys as it shows, and gives the focus back to the app that had it as it hides. Its pane runs while it is hidden, which keeps its last frame, so that it shows at once. The app holds its policy until the `quick-terminal` plugin takes it in F3.

**Linux** (F7)
- The same bar acts as the CSD titlebar:
  - window buttons;
  - `xdg_toplevel.move` when it's dragged;
  - double-click to maximize;
  - right-click for `show_window_menu`.
- When the compositor provides server-side decorations, the bar sits below them.

### Appearance

- **Styles**: `regular`, `compact` and `minimal`, like iTerm2's themes.
- **Colors**: derived from the terminal theme when not specified (see [iTerm2 themes](#iterm2-themes)).
- **Per-tab colors**: from the config, from a menu, or from iTerm2's `OSC 6;1;bg;red|green|blue;brightness;N` sequence.
- **States**: active, inactive and hover, with configurable separators and corner radius.

### Tab contents

- **Title**, in order of priority:
  1. a name set by the user;
  2. the title set by the program (OSC 0/2);
  3. the foreground process plus cwd.
- **Index** for ⌘1–9 (optional).
- **Indicators**:
  - activity (new output in a hidden tab);
  - bell;
  - progress (OSC 9;4);
  - running process;
  - an AI agent's state: working, waiting or done ([Agents](#agents));
  - protected input (Secure Keyboard Entry), which the window's title shows as a 🔒 until the tabs come.
- **Close button**: always visible, on hover only, or never. Closing asks for confirmation if a process other than the shell is running.

### Interaction

- Click to select, middle-click to close, drag to reorder (animated).
- Dragging a tab out of the bar turns it into a new window. Dropping it on another window's bar moves it there.
- Double-click to rename.
- Context menu: rename, color, close others, move to a new window.
- When there are too many tabs, each shrinks to a minimum width, and then the bar scrolls.

### Commands and shortcuts

| Command | macOS | Linux (F7) |
|---|---|---|
| `tabs.new` | ⌘T | Ctrl+Shift+T |
| `tabs.close` | ⌘W (closes the pane first when there are splits) | Ctrl+Shift+W |
| `tabs.next` / `tabs.prev` | ⌘⇧] / ⌘⇧[, ⌃Tab / ⌃⇧Tab | Ctrl+PgDn / Ctrl+PgUp |
| `tabs.goto { index }` | ⌘1 … ⌘9 | Alt+1 … Alt+9 |
| `tabs.move { delta }` | ⌘⇧← / ⌘⇧→ | Ctrl+Shift+PgUp / Ctrl+Shift+PgDn |
| `tabs.rename`, `tabs.set-color`, `tabs.detach` | menu and palette | menu and palette |

```toml
[plugins.tabs]
position = "top"          # top | bottom | left
style = "compact"         # regular | compact | minimal
hide-when-single = false
show-index = true
close-button = "hover"    # always | hover | never
min-width = 120
max-width = 260
```

## The programs' progress

The `progress` plugin, as the user asked on 1 November 2026, after iTerm2 and Ghostty: a program's progress, of OSC 9;4, as Gradle's, which the VT core reads.
- **Where**: a thin bar in the `window-strip` slot, across the window's top under the titlebar, and on the tab of its pane with the `tabs` plugin.
- **States**: normal, in the accent's color, with its percentage; error, red; paused, yellow; indeterminate, which moves without frames of the app's; and none, which takes the bar away.

## The command's sticky header

The `command-header` plugin, as the user asked on 1 November 2026, after iTerm2: the command whose output the view shows stays at the pane's top, in the `pane-overlay` slot, over the grid's top rows, which it takes no row of, so that the grid keeps its size as it shows and hides, over OSC 133's marks.
- As the scroll goes up past a command, the header gives way to the command before it; down, to the next.
- A click on it scrolls to the command.
- The actions on a command's output, to copy it, select it, tell its details, run it again, fold it or share it, come with F5a's blocks.

## Splits

The `splits` plugin. Each tab's layout tree lives in the kernel, because session restore, IPC and tmux -CC need it too. The plugin provides the commands and the interaction; the host draws the dividers from the layout tree, in the theme's tokens, and gives their drags to the plugin. Without the plugin, every tab has exactly one pane.

```
LayoutTree = Split { direction: horizontal | vertical, ratio?, first: LayoutTree, second: LayoutTree }
           | Leaf(PaneId)
plus an optional zoomed pane

A split without a ratio shares its room by its parts' panes along its
direction, so that three panes split alike take a third each; a divider's
drag gives it a ratio, between 5% and 95%, which `splits.equalize` takes away.
```

| Command | macOS (same as iTerm2) |
|---|---|
| `splits.split { direction = "right" }` | ⌘D |
| `splits.split { direction = "down" }` | ⌘⇧D |
| `splits.focus { direction }` | ⌘⌥ + arrows |
| `splits.focus-next` / `splits.focus-prev` | ⌘] / ⌘[ |
| `splits.resize { direction, cells }` | ⌃⌘ + arrows |
| `splits.zoom-toggle` | ⌘⇧↩ |
| `splits.equalize`, `splits.swap { direction }`, `splits.move-to-tab`, `splits.move-to-window` | palette |

On Linux (F7) the shortcuts use Ctrl+Shift and are defined in `keymap-linux`.

**Behavior**
- **New panes** inherit the cwd (via OSC 7, which is why basic shell integration already lands in F3) and the profile of the pane they come from.
- **Inactive panes** are dimmed (`dim-inactive`), as in iTerm2.
- **Dividers**:
  - can be dragged, with a grab area wider than the visible line;
  - reset to 50/50 on double-click.
- **PTY resizes during a drag** are coalesced: at most one every ~50 ms, plus a final one. This avoids storms of SIGWINCH.
- **Options**: per-pane titles (`pane-header`), useful with many splits; `focus-follows-mouse`.

```toml
[plugins.splits]
divider-width = 1
dim-inactive = 0.15        # 0 = no dimming
pane-titles = false
focus-follows-mouse = false
```

Alternative layouts, such as kitty's tall, fat, grid and stack, can be added later as other plugins that edit the same tree.

## Search

The kernel's `search` service (the engine) plus the `search` plugin (UI and commands).

### Engine

- **Literal search**: `memchr::memmem` over the text of logical lines; empty cells read as spaces, and the spaces that end a line go, printed or empty, so that `b ` matches nowhere at a line's end. Case-insensitive literal search folds ASCII; Unicode case folding goes through the regex engine, with the literal escaped.
- **Regex search**:
  - `regex-automata`'s meta regex, over the same text of logical lines. Its lazy DFAs, forward and reverse, fall back to its other engines where they give up, as Unicode `\b` makes them do on text that is not ASCII.
  - The text is built **one logical line at a time**, never for the whole scrollback. Alacritty instead advances its DFAs one cell at a time, because `regex-automata` has no chunked-input API; with a line's text at hand, the meta regex picks the engine and handles the fallback itself.
- **Which matches**: the matches of a logical line are its leftmost-first matches, found from its start, none overlapping another, for literal text and regexes alike: `aa` in `aaaa` is two matches, and `[0-9]+` in `12345` one. Forward gives the first that starts at the cell or after it, backward the last that starts before it, so that both ways, and the "n of m" counter, go through the same matches.
  - A match starts at a cell and ends with one: a match that starts or ends inside a cluster, as `e` in a decomposed `é`, does not count, nor does an empty match, so that `x*` finds the runs of `x`, and `^` alone finds nothing.
  - A match that does not count lets the search go on a character after its start. A line goes by once, then, from its start, unless long matches keep not counting, as `a+` with whole word does in a long line of `a`, which takes a time that grows with the square of the line's length; cancellation stops it.
- **Whole word**: for literal text and regexes alike, a filter applied after each match: the cells right before and right after the match start with neither a letter, a digit nor `_`. A pattern can still use `\b`.
- **Logical lines**: *soft-wrapped* rows count as one line, so words broken at the edge are still found.
- **Every match**: `find_all` gives the matches that start in a range of rows, in one pass, for the counter and the highlighting: each logical line is read once and its matches gone through once. It pins as many as its limit, and only counts the others, as the results keep 100k at most. `find_all` takes time in proportion to the lines that it reads when each match's search ends near the match, and to their square for a regex whose search scans to the line's end, as `a.*b|a`; a range in the middle of a long line reads all of it, so ranges of one long line read it each. A `find` step, either way, reads the whole logical line that it starts in, then goes through the line's matches from its start, so a step costs the line's length, and stepping near the end of a long line, backward as forward, costs the line's matches before it too: stepping is for nearby matches.
- **Memory**: a line's text is built with where each cell's text starts and its column, about 10 bytes a cell, and a row's page once; a search of one line that fills a 10 MiB scrollback takes a few times that, transiently.
- **Order and cancellation**: the search runs in chunks on a worker thread, starting at the viewport and moving outward so nearby results arrive first. Each keystroke in the find bar cancels the previous search: the engine looks at a flag before each logical line, and between the matches it tries in one.
- **Results**:
  - Stored as stable pins, on a match's first and last cells, which stay valid as new output arrives, and go when the cells leave the scrollback.
  - At most 100k are kept; past that, only the count.
- **Target**: 100k lines in ≤ 150 ms ([PERFORMANCE.md](PERFORMANCE.md)).

### UI

- **Find bar**: an overlay on the pane, top right as in iTerm2, with history.
- **Options**: case (Aa), regex (`.*`), whole word; an "n of m" counter.
- **Navigation**:
  - Enter / ⇧Enter;
  - ⌘G / ⇧⌘G on macOS, F3 / ⇧F3 on Linux;
  - Esc closes the bar and leaves the current match selected, ready for ⌘C.
- **Highlighting**: all visible matches (`search.match`) and the current one (`search.current`), plus marks in the scrollbar.
- **Filter mode**: iTerm2-style, showing only the matching lines. It arrives in F5b, because the renderer must first be able to show an arbitrary list of rows instead of a contiguous window.

```toml
[plugins.search]
case = "smart"            # smart | sensitive | insensitive
regex = false
highlight-all = true
wrap-around = true
```

## iTerm2 themes

The `themes` plugin (model, light/dark switching, picker) plus the `theme-iterm2` plugin (the importer).

### Internal model

| Group | Fields |
|---|---|
| Terminal | 16 ANSI colors, 256-color palette (derived or explicit), foreground, background, bold, cursor, cursor text, selection (background and text), link, underline, badge |
| Search | `search.match`, `search.current` |
| UI | Tokens for the tab bar; active, inactive and hovered tabs; dividers; overlays; accent; borders |
| Chrome | Beyond iTerm2's colors, our themes give the chrome's look too: the tab bar's layout and the tabs' style and shape, the scrollbars' style, width, colors and visibility, and the other chrome that the plugins draw |
| Options | Light/dark variants; `minimum-contrast`, the minimum ratio between text and background (as in iTerm2 and Ghostty) |

UI tokens that are not specified are **derived** from the terminal colors, the way iTerm2 does it:

| Token | Derived from |
|---|---|
| Bar background | background mixed 6% toward the foreground |
| Active tab | background |
| Inactive tab text | foreground at 60% |
| Dividers | background mixed 15% toward the foreground |
| Accent | cursor color |

This gives every imported iTerm2 theme matching tabs right away; the chrome's look that an iTerm2 theme does not give comes from the default theme.

The default theme's terminal colors are the built-in palette of F2, with a light and a dark variant, whose colors meet WCAG's contrast on their backgrounds.

### `.itermcolors` importer

- **Keys read** from the file, which is an XML plist (parsed with the `plist` crate):
  - `Ansi 0 Color` … `Ansi 15 Color`;
  - `Foreground Color`, `Background Color`, `Bold Color`;
  - `Cursor Color`, `Cursor Text Color`, `Cursor Guide Color`;
  - `Selection Color`, `Selected Text Color`;
  - `Link Color`, `Badge Color`, `Tab Color`, `Underline Color`.
- **Color components**: each color has `Red Component`, `Green Component` and `Blue Component` (floats 0–1), an optional `Alpha Component`, and a `Color Space`:
  - `sRGB`: used as is.
  - `Calibrated` (Generic RGB): converted to sRGB. On macOS this uses ColorSync or `NSColor` for exact results; elsewhere, a matrix plus a transfer curve.
  - `P3`: kept in Display P3 when the drawable is P3, converted otherwise.
  - `Device`, or missing: treated as sRGB.
- **`(Light)` / `(Dark)` suffixes**: keys from profiles that use separate colors for light and dark mode. They become the theme's two variants.
- **Missing keys**: sensible defaults (bold = foreground, cursor text = background, derived selection).
- **Theme locations**: `~/.config/lokyr/themes/`, or `themes` packages.
- **Other formats**: importers for Ghostty, kitty, Alacritty and base16 ship as separate plugins.

### Usage

```toml
[theme]
light = "Solarized Light"
dark = "Dracula"          # file names, as in the iTerm2-Color-Schemes collection
minimum-contrast = 1.0
```

- **Light/dark**: with both keys set, the theme follows the system appearance and switches live.
- **Picker**: the palette has a theme picker with live preview.
- **Native format**: TOML, with the same fields as the internal model, for anyone who also wants to define UI tokens.

### Verification

- **Full collection**: import the entire iTerm2-Color-Schemes collection with zero errors.
  - Check RGB values against computed references.
  - The collection also ships files generated for other terminals, which are useful as a cross-check.
- **Targeted tests**: for the Light/Dark keys and for each color space.
- **Licenses**: check each theme's license before bundling it in the app or in a `themes` package.

## Command palette

The `command-palette` plugin (⌘⇧P on macOS, Ctrl+Shift+P on Linux):
- fuzzy search over registered commands (title, shortcut, plugin), with recent commands first;
- theme picker with live preview;
- uses the `window-overlay` slot and an IME-capable text field.
- runs the commands as the user's, and lists every command with its plugin and keys, through the capability `user-commands`, which a palette of a third party asks for at install ([ADR 0024](adr/0024-user-commands.md)).

## Basic actions

The `core-actions` plugin:
- **Clipboard and scrollback**: copy, paste (with the rules in [SECURITY.md](SECURITY.md)), select all, clear the scrollback. A drop of files pastes their paths, quoted for the shell; a drop of text pastes it, as a paste does.
- **Font and scrolling**: font size (+, −, reset); scroll by line or page, to the top or bottom.
- **Windows**: new window, close window, quit.
- **Mouse selection**:
  - double-click selects a word, triple-click a line, and ⌥ (Alt on Linux) makes a rectangular selection;
  - copy-on-select is optional.

## The pane's context menu

A right click, or a Control click, opens a native menu at the pointer, after macOS's Terminal; a program that takes the mouse's clicks has them, but with Shift. The click selects first: within the selection it keeps it, on a word it selects the word, on a blank cell it takes the selection away. Its items are the commands that take the click's cell, then the clipboard's and the editing's; an item that would do nothing there is dimmed, as its context or its plugin tells.

The actions come with the features that they belong to, as the user asked on 29 October 2026, from Terminal's (T), iTerm2's (i) and Ghostty's (G) menus:

| Phase | Actions |
|---|---|
| F2 | Open Link (T, i); Look Up “word” (T); Open man Page for “word” (T); Copy, Paste (T, i, G); Select All, Clear (i) |
| F3, with the tabs, the splits and the plugins' items | Split Right, Left, Down and Up, Close Split (G, i); New Window and New Tab at the folder under the pointer (T); move the pane to a tab or a window (i); rename the tab (G); Search man Page Index (T); Search the Web, with the engine in the config (T, i); Copy Link Address (i) |
| F5a, with the OSC 133 marks and the blocks | mark a line, as a bookmark too (T); the command's details, run it again, select its output, fold it (i) |
| F5b | broadcast the input to the panes, lock a pane or make it read-only (i, G); annotate the selection, add a trigger, save the selection as a snippet (i) |
| F5c, with the links' smart selection | open a `file:line` in the editor, Quick Look the link, the smart selection's own actions (i) |
| Plugins, from F4 | the selection as pretty-printed JSON, Base64 decoded or encoded, the colors that it names (i) |
| Tools, later | reset the terminal, the inspector, the terminal's modes (G, i) |

Left out: downloads with scp (i), as the app has no networking code ([ADR 0008](adr/0008-separate-package-manager.md)), and editing the session (i), as the settings are the config's.

## Agents

AI agents that run in a pane, such as Claude Code, Codex, Gemini CLI, OpenCode or Aider, are recognized, and can drive the terminal ([ADR 0017](adr/0017-mcp-and-agents.md)).

- **Presence**: the pane's foreground program, by its name, from a list in the config.
- **State**: working, waiting or done, from the agent's notifications (OSC 9 and 777), the bell and its progress (OSC 9;4). For an exact state, from F3b, through its IPC, its hooks call `lokyr +notify`, and `lokyr +agent-hooks <agent>` prints the lines that its config takes, which the app never writes itself.
- **Where it shows**: on the pane's tab (F3), and as a notification while the window has no focus (F2).
- **MCP** (F3b): `lokyr +mcp` gives agents tools to list the panes, read them, open tabs and splits, run commands and wait for their exit codes, and type, within the scope of [SECURITY.md](SECURITY.md#mcp).

## Cursor

`render-metal` draws the cursor's cells into a layer of their own, over the pane's, which the window server blinks. Its options sit in the `[cursor]` section of the config, and hot reload applies them at once.

```toml
[cursor]
shape = "block"           # block | bar | underline | hollow
blink = true
thickness = 2             # a bar's width and an underline's height, in points (0.5–20); the face's underline thickness when absent
blink-interval = 500      # how long the cursor stays on, then off, in ms (100–5000)
smooth-blink = "breathing" # off | breathing | linear | fast | slow | subtle | custom
fade-in = 900             # with smooth-blink = "custom": the fades, in ms
fade-in-curve = "ease-in-out" # linear | ease-in | ease-out | ease-in-out
visible = 500             # how long it holds, visible, then hidden, between fades, in ms
fade-out = 900
fade-out-curve = "ease-in-out"
hidden = 500
blink-timeout = 15        # seconds without keys or output before blinking stops; 0 never stops
movement = "instant"      # instant | snappy | gliding
```

- **Shape**: a block, a vertical bar, an underline, or a hollow block, of its edges alone. Programs change the shape and the blinking with DECSCUSR, as vim does in insert mode, among the first three. The options are the default, which `CSI 0 SP q` and a reset bring back.
  - The text under a block takes its cell's background. A bar and an underline take `thickness`, which scales with the display as the font's size does, or else the face's underline thickness, which a hollow block's edges always take.
  - Mode 12, which `cvvis` sets in xterm's terminfo, makes the cursor blink too. Its reset, which `cnorm` sends as each full-screen program ends, leaves the configuration's blinking, so that the cursor does not stop blinking after `less` or `vim`.
- **Blinking**: on and off for `blink-interval` each. With `smooth-blink`, the cursor fades between the two instead of switching, as iTerm2's does, of its presets, Breathing, Linear, Fast, Slow and Subtle, or of six values of the user's: each fade's duration and curve, and how long the cursor holds visible and hidden between them. The cursor's cell is a layer of its own, whose opacity the window server animates, so that the blinking and its fades wake the app no more.
- **Movement**: `snappy` eases the cursor to its new position over a short time, `gliding` over a longer and softer one, and `instant` moves it at once.
  - The animation only moves the cursor that is drawn. The text that a key press changes still appears in the next frame, so the latency budget holds.
  - Each frame animates from where the cursor was drawn to where it is now, so a burst of output, or a synchronized update (mode 2026), moves it once rather than once per sequence.
- **Focus**: only the focused pane's cursor blinks. An unfocused pane, or a window in the background, shows a hollow block.
- **Power**: blinking stops after `blink-timeout` without keys or output, and the cursor stays on. The blinking and its fades take no frame of the app's, as the window server animates the cursor's layer; the display link runs only while a movement is in progress, like the other animations.
- **Blinking text**, of SGR 5 and 6, blinks on the cursor's clock, switched at `blink-interval`, and stops shown with it. It goes on a layer of its own, under the cursor's, over the rows that hold it, which the window server switches as the grid draws that text hidden, so that it takes no frame of the app's either.
- **Accessibility**: with the system's "Reduce motion" setting, the movement is instant and blinking does not fade.
