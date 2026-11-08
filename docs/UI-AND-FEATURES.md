# UI engine and main features

This document covers our UI engine and the features the MVP cannot ship without: tabs, splits, search, iTerm2 themes and the cursor's options.

## UI engine

`ui-core` is our small, platform-independent toolkit ([ADR 0004](adr/0004-native-shell-and-gpu-chrome.md)). It draws all of the chrome on the GPU, into the same drawable as the panes. Why our own, rather than a framework of Rust's or the native toolkits, and what it takes from them: [UI-TOOLKITS.md](UI-TOOLKITS.md).

**View tree and layout**
- Plugins describe their UI as a view tree, plain data of `plugin-sdk`'s types, which the host stores as a flat arena of nodes (see [PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md#the-luau-api) and [ADR 0028](adr/0028-plugin-calls-as-messages.md)).
- A widget that holds state keeps it in the host, by the node's key, which it requires: a text field's text, caret, selection and composition, a list's scroll and selection. Its plugin learns of the changes through events, whatever its thread or language, so that no key goes through the plugin and back.
- `taffy` computes the layout, using a subset of flexbox: direction, gap, padding, alignment, sizes, grow. A size is of its content, of points, of a fraction of its parent's, or the window's: the titlebar's height and the width that the window's buttons take, which the host gives as the window changes, so that a sidebar leaves the buttons their room with no render, and none in full screen.
- Each new tree is diffed against the previous one by `key`, so only the changed parts are laid out and turned into display-list entries again. A key is unique in the whole tree, as `tab-3.close`, not among its siblings alone, since the events tell a node to its plugin by its key: a node that a render moves to another parent, or makes anew of another kind, is the same to its plugin, whose hover, drag and focus follow it.
- A tree that nests past 64 levels, or whose numbers are not finite or out of their ranges, as a negative padding or a fraction past 1, is refused, and the last one stays: the diff, the layout and the paint recurse, and a plugin's tree must not take the app down.

**Widgets**: those that the chrome and F3's plugins need, as the user chose on 1 November 2026. A plugin describes its UI with them and the host draws it: no plugin draws pixels or opens a native window, so that its UI takes the theme, the focus, the input method and accessibility, costs the frame no more than the host allows, and no plugin can pass itself off as the system.
- container, text, icon, spacer; separator, a line a point thick across its container, upright in a row and flat in a column, of the theme's `separator`;
- text field with IME, selection and clipboard, whose text the host holds:
  - one line, of a placeholder while it is empty, of the theme's `field.bg`, `field.border` and `field.selection` where its node gives no box of its own; it takes the keyboard's focus of its own, and a key, which it requires;
  - its caret moves by whole grapheme clusters, so that it never stands inside an emoji of several code points or a letter and its marks, and by words, runs of letters and digits;
  - macOS's keys of a text field: the arrows, with Option by words and with Command to the line's ends, Shift extending the selection; Backspace and Delete, with Option by words and with Command to the line's ends; Control-A, E, B, F, H, D and K; Command-A, C, X, V, Z and Shift-Command-Z, at their places of a Latin layout where the layout's own letters are not ASCII, as Russian's; the keys that it leaves, as Command-W, go to the bindings;
  - the pointer: a press places the caret, Shift extends the selection to it, a double click selects a word, a triple click the whole text, and a drag selects, which its plugin learns nothing of; Tab, as it gives it the focus, selects its whole text;
  - a paste takes the clipboard's lines on one line, each line break a space, of a CRLF one, and those at its end left out, as a line copied from the terminal ends in one; a paste of controls alone changes nothing; the field keeps its own undo, of 100 edits, typing and deleting each undone at once until the caret moves;
  - its text holds 10 000 characters at most: a paste or a plugin's text past them is cut before the grapheme cluster that would pass them, and typing past them types nothing;
  - its caret's places are each cluster's leading edge, the right of one of Hebrew or Arabic, so that the caret and the selection follow a run from right to left;
  - it scrolls across to its caret while it has the focus, and shows its text's start without it;
  - an input method's composition shows at its caret, in place of the selection, underlined as macOS marks it, the part that the input method selects of a line twice as thick; a click keeps it as it stands, which the input method then forgets; the input method's rectangle is the whole composition's, or the caret's without one, by which it places its candidates and the palette of emoji;
  - its plugin learns of its text through `input-changed`, at each change, and `submit`, as Return goes to it, and sets it through a request, which tells no `input-changed`;
- button and toggle:
  - of a label, an icon before it, or both; a button's box of the theme's `button.bg`, lighter under the pointer, `button.hover.bg`, darker as the primary button is down over it, `button.pressed.bg`, where its node gives no box of its own; a toggle of an icon alone shows no box while it is off, as a find bar's options, one of a label a button's; the tint `toggle.on.bg` and the color `toggle.on.fg` while it is on;
  - a click, or Space or Return while it has the focus, presses it: a button's `click`, at each click of a double click too; a toggle's `toggle`, of the state asked, which its plugin gives back in its next render, as a toggle's state is its plugin's and the host keeps its pressed look alone; they take the focus of their own, and no drag;
  - a disabled one is dimmed, of `text.disabled`, and takes neither the pointer's presses, nor the focus, nor accessibility's press;
- view of tabs, as a settings panel's, of several pages, apart from the window's tab bar:
  - drawn as macOS 27's tab views: a box, of the theme's `segments.box.bg` and `segments.box.border`, and inside it at its top a control of its tabs side by side, each of a label, an icon before it or both, sized as a button's, over its page, its child;
  - its tabs share the control's width as macOS's segmented controls' distributions: `fit`, each of its content's width, the control in the middle, by default; `fill`, the control of the view's width, each tab of its content's and of an equal share of the room left; `fill-equally`, of equal widths; `fill-proportionally`, each of its content's share of the room; without room for their contents, those that fill share it as their contents share theirs; the control of `segments.bg`, the tab chosen tinted of `segments.chosen.bg` and `segments.chosen.fg`, a line between two tabs that are not chosen, a disabled one dimmed of `text.disabled`;
  - a click on a tab, or Left and Right while it has the focus, past the disabled ones and to the ends, chooses it, which `select` tells its plugin, whose next render gives the tab chosen and its page; a control, as macOS has them, so that a click leaves the focus where it was, and Tab reaches it as the system's keyboard navigation reaches every control, the focus's ring round its control;
  - on Linux, from F7, the desktop's own look;
- virtualized list, of flat items and a row's template, and scroll area:
  - the list lays out and draws its rows that show alone, each of its template, whose texts and icons take its item's texts by their places; its plugin gives its count and a window of its items, and the host asks for those about the rows that show by `list-range`, 50 rows ahead each way, as they scroll past the window, so that a render carries a window, not the whole list; a frame of a list of 100 000 rows takes the interface about 25 µs, as one of 1 000 does;
  - its rows of one height, its `row-height`, or of their own, measured as they show and estimated before; its scroll is the first row that shows and the points of it above, so that the rows that show stay in place as estimates turn into heights;
  - the host keeps its scroll and its selection by its key: a click selects a row, and a double click and Return activate it, as `select` and `activate` tell its plugin; the arrows, Page Up and Page Down, Home and End move the selection and show its row; the list takes the focus, of a click too; the selection of `list.selection.bg` and `list.selection.fg` while it has the focus, of `list.selection.inactive.bg` without;
  - a scroll area scrolls its children along its direction, a column's up and down, its children keeping their heights, and a row's across, its children keeping their widths, of a wheel of one axis too; the wheel and the trackpad, of the momentum that the platform gives, scroll the innermost list or scroll area under the pointer; an indicator of the theme's `scrollbar` at the right, or at a row's bottom, shows how far, where the content overflows, unless the area hides it, as a bar of tabs; a plugin asks its view's scroll area to show a node, `reveal`, as little as shows it, once the view shows it;
- tooltip, of a node of a key: its text shows below the pointer as the pointer rests over the node for a second, or above it near the window's bottom, and at once for half a second after another hid, as macOS's; a press, a key or a scroll hides it until the pointer leaves the node; of the theme's `tooltip.bg`, `tooltip.fg` and `tooltip.border`; the host wakes for it by a timer, only while one waits;
- `progress`, of four states, normal, error, paused and indeterminate, which the window server animates, on layers of its own as the cursor's blinking, without frames of the app's:
  - a bar, filled from its left, or a ring, filled clockwise from its top, of the size that its node's layout gives it, 160 by 4 points and 16 by default; a bar's corners' radius, a pill's by default, a ring's line's thickness;
  - dots, three in a triangle, one over two, in the middle of their box, 10 points by default, as a small spinner: an indeterminate one's lit in turn, clockwise from the top, each fading as the next lights, 1.2 seconds a round by default, on three layers whose opacity the window server animates, a third of the round apart; one of a value as many lit as its thirds; the dots not lit of its track's color;
  - an outline, its parent's: a line along the parent's edges, inside them, of the parent's corners' radius or its own, 2 points thick by default, filled from its left along its top and its bottom at once as far as its value, as a tab shows its program's progress; it takes its parent's whole box, over the children before it, and none of their room;
  - the fill of one that is not indeterminate on a layer of its own, where the platform has them, macOS's, so that a change of its value slides from the value that it showed, eased, in 0.2 s, at once where the system reduces motion, and costs the app no frame: the app's frame draws its track alone, and the platform keeps each fill by its view and its node, as the renders keep the node; a platform of no such layers, as the tests' and Linux's until it has them, has the frames draw it, at once;
  - its track of its style's background and its fill of its style's color, as every node's, of the theme's tokens: `progress.track`, and its state's, `progress.normal`, `progress.error` and `progress.paused`, without them;
  - an indeterminate one's moving part, a segment across the bar, an arc round the ring or a segment along the outline, of its share of the bar, of the turn or of the outline's length and of its period, 1.5 seconds a sweep of a bar, a second a turn of a ring and 2 seconds a run round an outline by default; the host hands it the platform, which has the window server move it within the bar, the ring or the outline and the clips around it, an outline's as the phase of a dash of its line; where the system reduces motion, it holds still, its opacity pulsing;
- floating panels, anchored to a cell, the cursor or the pointer, with the keyboard's focus inside them:
  - each a view tree of its own over the window's content and over the panels opened before it, the last opened on top: below a rectangle of the window, as a cell's or the cursor's, or above it where the window has no room below; at a point, as the pointer's, leftwards or upwards where it has none; or in a corner of a rectangle, as the pane's or the window's, a margin in; within the window as it resizes, laid out of the window's width where it is wider;
  - of a role, which tells the system what it is, so that a platform can show a menu as its own: a tooltip, which the pointer goes through, of no focus; a menu, which takes the keyboard as it opens, and which a click outside it closes, the click going no further, as Escape closes it even where the focus is not in it; or a dialog, as a find bar, which takes the keyboard as it opens;
  - a click outside it, or Escape while it has the focus, closes it where its plugin asks so, the focus going back to where the panel took it from, as a menu of the find bar gives it back to the bar, or to the pane; a press on a panel closes the panels over it that a click outside closes; a press anywhere keeps a field's composition, in any panel, as it stands;
  - the pointer, the keys, the input method's composition and commits, and its rectangle, to the panel under the pointer or with the focus, as to the window's content, a composition under way ending as it stands where a panel takes the focus as it opens; a panel over the titlebar takes its presses there, which would drag the window, a tooltip's aside, and while one that a press outside closes shows, the whole bar's, whose press closes it; a press's button captured by the panel that it went down over, until it goes up; the indeterminate progresses' sweeps that a panel covers left out;
  - drawn within the window, as its one drawable draws them; but a menu's panel of menu items alone, buttons and toggles of labels, separators, and submenus, containers of a text, their title, then their items, shows on macOS as a native menu at its anchor, which the system places, out of the window too: a toggle's item ticked, a disabled one dimmed, a choice told as its node's press, a `click` or a `toggle`, and never run as a command of the app's, whatever its node's key; the panel that its plugin closes closes the system's menu, and a menu that another one replaces closes, as its plugin hears, each in its turn; a menu of other nodes stays drawn, as on Linux;
- menus. Context menus and the menu bar are native `NSMenu`s on macOS, to which plugins add their items, and drawn by us on Linux; menus drawn by `ui-core` serve inside panels alone, as a find bar's choice.
- A canvas of drawing primitives, for a plugin's own graphics, comes with F4's Luau API.

**Focus and IME**
- Focus belongs to exactly one element: the window's pane, or a node of a panel that takes it, a widget or a node of a key that a plugin marks `focusable`. A press over a panel's node gives it to the innermost node under the pointer that takes it, but a control's, a button's or a toggle's, disabled or not, which leaves it where it was, as macOS's controls do: a click on a find bar's option keeps the typing in its field.
- Of the keys that no binding takes, Tab and Shift-Tab move it among the panel's nodes that take it, in the tree's order, from the last round to the first, the controls only as the system's keyboard navigation reaches every control, Keyboard navigation in System Settings on macOS, which the app reads at each key; accessibility's focus reaches them all; Escape gives it back to where its panel took it from first since the pane last had it, a node of a panel below or the pane, so that two panels or two views never give it back to each other for good; every other key goes to the element that has it alone, and a key's release where its press went, the repeats and the release of a key whose press went to a panel that closed nowhere. A node taken away leaves it to the node in its place; a panel that closes gives it back to where it took it from, and one that has none left, to the pane. The ring of the focus, of the theme's `focus.ring`, goes round the node that has it, over the others.
- Its moves come to the nodes as a blur, then a focus; after a click on a panel's node that takes no focus, Tab goes on from there.
- A slot's view, as the tab bar or the status bar, takes it as a panel does, by a press on its node, which a press over the panes gives back to the pane. While a node has it, the pane's cursor shows as unfocused, its program learning of the loss if it asked; the keymap's bindings that name no context still run, but the pane's contexts are left out; the Edit menu's Copy, Paste and Select All, and their keys, are the node's, a field's alone doing something, so that no paste goes to the pane; and a composition of the input method under way, the pane's or a field's, ends as it stands as the focus moves, as a render takes its field away or sets its text, so that no commit of it goes to the element that has the focus next.
- Accessibility's actions, as VoiceOver's: a press clicks a node, as a button's; a focus moves the keyboard's focus to it; a value set comes to it as `set-value`, each to the node or to its nearest ancestor that takes it. A node of a key that takes a press of its style, a pointer, a hover or a press of its own, as a tab of the bar or its close button, is a button to accessibility, which presses it, labelled of its texts where it has no label of its own.
- The keymap's contexts tell where it is: `pane.focused` while the pane has it; of a plugin's panels, `namespace.open` while one shows and `namespace.focused` while one has it, as `search.open` and `search.focused`.
- The platform IME client (`NSTextInputClient`, text-input-v3, XIM) is routed to that element, along with the cursor position for the candidate window.
- The Edit menu ends with the system's palette of emoji and symbols, ⌃⌘Space on macOS, whose character comes as an input method's commit: to the pane's program, or into the field that has the focus.

**Accessibility**
- Nothing is built until an accessibility client, as VoiceOver, asks for a window's elements or its focus, so that a window that no client reads costs nothing; AccessKit's adapter then answers of the window's tree, on macOS of Lokyr's own view, not a subclass. Some apps that hold the Accessibility permission ask for the focus as a window becomes key, without reading on: the tree then follows the screen too, which costs a fraction of a millisecond at each change, and nothing at rest.
- AccessKit receives a tree built from the view trees, the window's content's, its chrome's and its panels', each node's id its slot's in its tree's arena: a role, its kind's, as a field a text field, a toggle a button that stays pressed, a list a list box, a view of tabs a tab group, or its node's own, as a group or a heading; a label, its node's own, its text, a button's or a toggle's label, or its tooltip, which tells it otherwise; a widget's state, a field's text, a toggle's, a progress's value, a list's rows that show, of their items' texts and their places among them all, the one selected, a view's tabs, the one chosen; and its place in the window. A spacer, a separator and an icon of no label are left out, as decorations; a container and a scroll area of no role of their own are their parent's children, as a client goes through them; a panel is a group, a tooltip's a tooltip. The keyboard's focus is the tree's.
- A client's press, focus and value set reach the node, as accessibility's actions do above: a row's press selects it, a tab's chooses it.
- Panes expose their visible text and cursor to VoiceOver and AT-SPI: each pane of the window's tab is a terminal, a text area on macOS, labelled "Terminal", whose text is its visible rows, each ended by a newline unless the program's text wraps on to the next, its trailing blanks dropped but up to the cursor; the cursor is the text's selection, collapsed at its cell; each character keeps its cell's place, so that a client shows where it reads. A client's focus of a pane gives it the keyboard.
- The tree follows the panes' screens and the keyboard's moves to another pane at most five times a second, of the nodes that the screens' changes touch.

**Themes**: plugins use tokens (`tabs.active.bg`, `search.match`, …) rather than fixed colors, so a theme restyles all the chrome. A token that a theme lacks takes the color of its name without its last part, `tabs.active` for `tabs.active.bg`, then `tabs`, so that a theme of a few colors still covers them all; the built-in theme gives every token of the chrome, in its light and dark variants. `transparent` is no color in every theme, as the tokens that fall back to it are: a progress's track of it shows nothing.

**Text and icons**: the chrome's text is the interface's font, the system's by default, at the optical size of its points, so that a display of twice the pixels draws the same forms twice as large and the layout's measures hold at every scale; its glyphs share the atlas of the terminal's, and take the weight of macOS's text: the GPU, which blends a glyph's coverage as it is drawn, makes dark text on a light background a quarter heavier than the system's at a scale of 1, and so thins it by a curve of the text color's luminance, as the system does, leaving light text on a dark background nearly as it is, of the glyphs drawn of the font smoothing alone, as text without it, or of thin strokes, is macOS's as drawn; each glyph falls where the shaping puts it, to a third of a pixel, as a glyph drawn past its pixel's edge, so that a display of a scale of 1 keeps the font's spacing. The terminal's text keeps its glyphs as drawn, unless `[font] system-weight` asks for the system's weight. The icons are the codicons of the Symbols Nerd Font that the app carries, by their names (`terminal`, `close`, `search`, …), in the text's color.

**The pointer**: over a window's slots' views, the pointer goes to them before the panes; its moves without a button come to the app over them alone, of areas that the host gives the platform, for their hover and their tooltips, with its leaving them, the moves over the panes waking it only for a program that asks for them all; a press over a view keeps the pointer there until its button goes up, and a press over the panes, a selection's or a program's, keeps the pointer for them, over the views too. The node under the pointer is the topmost whose box holds it, within its ancestors' clips, as the tree draws a node over its parent and over its siblings before it. Its events go to the nodes of a key, by which their plugins know them: the hover to each node of a key that the pointer is over, its ancestors with it, so that the pointer over a tab's close is over the tab too; a click, a double click, a middle click and the context menu's to the innermost node of a key, the button down and up over it, the context menu's as it goes down; a button and a toggle take the primary button's as their own, a `click` or a `toggle` at each click, and a disabled one takes none, nor do the nodes around it. The primary button held over a node of a key, past 4 points, drags it: its start, its moves, of where it started, and its end come to that node alone, wherever the pointer goes, and no click; Tab's move of the focus, or the window's loss of it, cancels the drag, which its node learns of. A reorderable node's drag moves it among its siblings that are reorderable too, along its parent's direction: the host shows it under the pointer, within their room and over them, the others sliding out of its way in 0.2 s as it passes their middles, or reaches their edges, and a scroll area around them scrolling as it goes past the area's edges, the faster the further; it keeps its press and its hover until the button goes up, and the others show neither; Escape puts it back, its drag cancelled; a drop at another place gives it a `reorder`, of its new index, before its drag's end, and it slides to its new place, where its plugin's render of the new order keeps it, or, without one within half a second, slides back with the others. A wheel's or a trackpad's scroll goes to the innermost node of a key under the pointer, in points. A node's style gives the pointer's shape over it, the arrow, the I-beam or the pointing hand, which its children take unless they give their own; the window server shows it as the pointer moves, of areas that the host gives it, without waking the app.

**Animations**
- Host-driven transitions (opacity, position), e.g. when tabs are reordered: a node of a key whose style gives a `transition`, of its time and its curve, linear, ease-in, ease-out or ease-in-out, slides from where it showed to the place that the layout of a new render gives it, with its children, and fades from the opacity that it showed to the `opacity` that a render gives it; a node of a transition inside a sliding one slides with it, of its own move within it alone; a change during a transition goes on from where it is; a scroll, a resize and another display's scale move it without one; the pointer finds it at its place.
- The display link runs only while an animation is in progress: the host draws a frame at each refresh of the display while one runs, and none once they all end; where the system reduces motion, Reduce motion in macOS's accessibility settings, the transitions end at once.

**Display list**
- Primitives: rectangles (fill, border, radius, shadow), glyph runs from the atlas, images, clips.
- A pane's instanced cell batch includes the selection and decorations (curly, dotted and dashed underlines, strikethrough, overline); the cursor's cells and the text that blinks go into batches of their own, for layers over the window's surface, which the window server blinks.
- A window produces a display list covering chrome, panes and overlays, and one for each of those layers, drawn again only as their cells change.

## Tabs

The `tabs` plugin. Each window's tab list lives in the kernel. The plugin handles the tab bar, the commands and the interaction.

### Window integration

**macOS**
- **Tab bar in the titlebar**: `NSWindow` with `fullSizeContentView`, a transparent titlebar and a hidden title. Our engine draws the tab bar where the titlebar would be, past the window's buttons, in the unified and seamless styles; the standard style keeps a titlebar of the window's title and draws the bar below it, of its view's height ([Appearance](#appearance)).
- **The window's number**: as the user asked on 4 November 2026, after iTerm2's, each window has a number, the lowest that no other open window has as it opens, and ⌥⌘1 to ⌥⌘9 bring the windows of the first nine forward, `window.goto { index }`; the titlebar shows its shortcut, as the keymap binds it, ⌥⌘1: at its right in a titlebar of the window's title, and after the window's buttons in a titlebar of tabs, where the bar's plugin draws it, as `WindowInfo` gives the plugins the window's number and keys; the config's [`title-shortcut`](CONFIGURATION.md#title-shortcut) can hide it; the quick terminal has none.
- **Native tabs disabled**: `tabbingMode = .disallowed` and `NSWindow.allowsAutomaticWindowTabbing = false`.
- **Traffic lights**: an empty compact unified `NSToolbar` (`unifiedCompact`) vertically centers them on the bar, which is 40 points high on macOS 26 and later, against 32 for the titlebar alone. Moving them by hand means fighting AppKit's relayout. The platform tells the room that they take at the bar's left, with a margin after them as wide as the one before them.
- **Moving the window**: dragging the bar's room that its tabs leave calls `performWindowDragWithEvent:`. The host gives the platform the parts of the bar that are its view's own, the boxes of the outermost nodes under the root that take a press, each within the clips around it, where a press, its drags and its release go to the view: a node of a key of a button's, a toggle's, a field's, a list's or a view of tabs' kind, or of a pointer, a hover or a press of its own style, as a tab, so that a group of tabs of a key, as a transition needs, leaves the window the room around them, and the pointer's moves over the bar come to the app while the tabs are in it; the rest of the bar, the root's own box too, is the window's. The content's view lets AppKit drag the window from none of its presses, whose own default of a translucent window's view would drag the window from a tab's drag in the titlebar too: it drags the window itself, from the bar's room that is not the view's, as of an opaque window.
- **Double-click on the bar**: follows the system preference (`AppleActionOnDoubleClick`), on the bar's room that its tabs leave; on a tab, it is the tab's.
- **Fullscreen**: as the user chose on 3 November 2026, after iTerm2's, which shows its tab bar in full screen unless asked not to: the bar stays at the screen's top, of its view's height, without the window's buttons; the menu bar and the system's titlebar slide down over it as the pointer reaches the top, the empty toolbar hiding with them (`autoHideToolbar`), and take no room of the content's, as the window draws its chrome in its one drawable. `tabs` always shows the bar in full screen, as the user chose on 6 November 2026: no option hides it.
- **Resizing**: the content follows the window's edge through a live resize, drawn with Core Animation's transaction; each pane's terminal takes its grid with its program's signal, at once and then once every 25 ms at most, as at the resize's end, so that a shell's prompt never shows cleared before the shell draws it again; the grid's size, as `100 × 33`, shows at the window's centre while a resize changes it, and goes a second after the last change, or after the drag ends.
- **Dragging tabs between windows**: our own implementation rather than AppKit's dragging sessions, as the content's view keeps the pointer's drag wherever it goes, out of the window too: past a margin of about a bar's height across the bar, or out of the window, the tab leaves its bar, and its image follows the pointer, a borderless window over every other, those in full screen too, which the pointer goes through, of a Metal layer that the renderer draws once: a card of the tab, as its bar paints it, over its panes as they showed, a hidden tab's too, drawn once into a texture of their own, of the window's size, with its smaller copies, and scaled down to 320 points across, so that the text keeps its strokes. `NSWindow.windowNumberAtPoint:belowWindowWithWindowNumber:` finds the window under the pointer, of any app; Escape, or a drop over a window elsewhere than its bar, slides the image back to the tab's place, as the window server animates the image's frame. The texture goes as the drag ends.
- **The quick terminal**: a borderless, non-activating `NSPanel` that a global key, Carbon's `RegisterEventHotKey`, which asks for no Accessibility permission, slides in from an edge of a screen over every app, on every Space or on its own and over the apps in full screen, and out again, as `NSAnimationContext` moves its frame; it takes the keys as it shows, and gives the focus back to the app that had it as it hides. Its pane runs while it is hidden, which keeps its last frame, so that it shows at once. The app holds its policy until the `quick-terminal` plugin takes it in F3.

**Linux** (F7)
- The same bar acts as the CSD titlebar:
  - window buttons;
  - `xdg_toplevel.move` when it's dragged;
  - double-click to maximize;
  - right-click for `show_window_menu`.
- When the compositor provides server-side decorations, the bar sits below them.

### Appearance

- **Styles**: `standard`, `unified` and `seamless`, after iTerm2's themes, whose names they leave to it, as the user chose on 6 November 2026: `standard`, the bar below a titlebar of the window's title; `unified`, the tabs in the titlebar, of the theme's colors of the chrome; `seamless`, the tabs in the titlebar too, the active one of the window's own background, translucent as the window is, and the others under a veil that darkens it.
- **Position**: as the user asked on 4 November 2026, after iTerm2's, the bar at the window's top, bottom, left or right. At the top, as the styles put it; at the bottom, across the window's bottom, below the status bar, under a titlebar of the window's title; on the left or the right, a column from below a titlebar of the window's title to the window's bottom, its tabs one below another, each of its title and its shortcut, which scrolls past the window's height, of a width that the config gives and a drag of its edge changes. On the left, as the user chose on 6 November 2026, `unified` and `seamless` put the column through the titlebar, as macOS's sidebars: the window's buttons over its top, the window's keys after them, and the panes beside it below the titlebar, of no title, as the user chose after a first try of the panes up to the window's top, whose titlebar AppKit dragged the window from, a selection's drag there too, the window's drag the column's top, beside the buttons, and the titlebar over the panes; on the right, where the window's buttons would stand over the panes, the column stays below a titlebar of the window's title. `unified` and `seamless` put the tabs in the titlebar at the top alone, and elsewhere give the tabs' look alone. The host lays the panes out in the room that the bar leaves, the bar's place told by its plugin, and with a single tab the bar, a column too, takes no room.
- **Colors**: the theme's tokens alone, `tabs.bar.bg`, `tabs.active.bg`, `tabs.inactive.bg`, `tabs.hover.bg` and their texts', so that the themes, still to define, restyle the bar; the window draws nothing of its own in it, and shows the terminal's background where the bar's view leaves room; derived from the terminal theme when not specified (see [iTerm2 themes](#iterm2-themes)).
- **Per-tab colors**: from the config, from a menu, or from iTerm2's `OSC 6;1;bg;red|green|blue;brightness;N` sequence.
- **States**: active, inactive and hover, with configurable separators and corner radius. Under the pointer, as the user asked on 4 November 2026, a tab's hover background fades in, and out as the pointer leaves, of about 0.15 s, which the host draws of the node's hover style, as the pointer moves, without a render of the plugin's; Reduce motion makes it instant.

### Tab contents

- **Title**, in order of priority:
  1. a name set by the user;
  2. the title set by the program (OSC 0/2);
  3. the foreground process plus cwd.
- **Shortcut**, as the user asked on 4 November 2026, after iTerm2's: on each of the first nine tabs, the keys that select it, `tabs.goto` of its index, as the keymap binds them and the platform writes them, ⌘1 to ⌘9 by default, so that a binding of the user's shows as it is; none on a tab of no binding (optional, `show-index`).
- **New tab**: a button at the end of the tabs, as iTerm2's.
- **Indicators**, before the title, each of what VoiceOver calls it, as the host tells them of each tab, which its showing clears of what came while it hid:
  - activity, new output in a hidden tab: a dot of the accent's color;
  - bell, rung in a hidden tab: a bell;
  - progress, OSC 9;4's, of the tab's active pane or another: the tab's outline, filled from its left along its top and its bottom at once as far as its percentage, of its state's color, as the window's strip fills; a failure or a pause of no percentage full; one of no known end a segment that runs round it;
  - running process, a program other than a shell in the tab's foreground, to a plugin that reads the processes: a spinner of dots, as the user chose on 6 November 2026, after Nyx's, of a program that tells no progress, while it prints, as the user chose that day, over the activity's dot, which shows once the program ends or stops printing; a ring, which the activity's dot takes the place of, of a program that prints nothing of its own, as ssh, a REPL or an agent waiting for the user, or `sleep`, of a full-screen program, of the alternate screen, as vim, htop or less, which may stay for hours, or of one that tells its progress, which the outline shows. A program prints while output came within the last 3 seconds that is not the echo of the user's input, nor the program's answer to it or to a new size, which comes within half a second of them;
  - an AI agent's state ([Agents](#agents)): working, a sparkle of the accent's color; waiting, an `i` of the paused yellow; done, a check;
  - protected input (Secure Keyboard Entry), of the tab that shows in the window with the focus: a lock, which the window's title leaves to the tab bar where one shows, and keeps as a 🔒 where none does.
- **Close button**: always visible, on hover only, or never. Closing asks for confirmation if a process other than the shell is running.

### Interaction

- Click to select, middle-click to close, drag to reorder (animated): the tab follows the pointer along the bar, across it or down its column, the others sliding out of its way, the bar scrolling as the tab goes past its ends; Escape during a drag puts the tab back.
- A tab dragged out of the bar follows the pointer as an image of itself over an image of its window's panes, as the user asked on 4 November 2026: as they showed when the drag started, not live, scaled down, which the host draws once as the drag starts, a hidden tab's too; the window shows the tab next to it, as if it had closed, once the tab that it showed leaves the bar. Where it is dropped decides, as the user chose on 4 November 2026, after iTerm2's:
  - on a window's bar, its own or another's, it goes there, at the place under the pointer;
  - on a window's panes, its own or another's, it becomes a split of the pane under the pointer, in its half nearest the pointer, left, right, top or bottom, which lights up as the pointer passes; a tab of splits goes in whole, its layout in that half;
  - outside every window of Lokyr, it becomes a new window, of its window's size, the tab under the point where it was dropped; a window of a single tab, whose bar shows of `hide-when-single = false`, moves there itself, as the user chose on 6 November 2026, and, dropped on another window's bar, closes as its tab goes there;
  - on a window whose bar hides, of a single tab, over its titlebar, it goes after that window's tab.

  The bar of another window opens a tab's room under the pointer as the tab passes over it, its tabs sliding aside as a reorder slides them, as the user chose on 6 November 2026; the image is a card 320 points wide, the tab at its top as its bar paints it, its panes under it, of its window's height in proportion.
- With a single tab, the bar hides, by default: the window shows its tab's title in its titlebar, which `tabs` draws in the system's title's place, as the user chose on 6 November 2026, after Nyx's, centered between the room of the window's buttons on either side, with the tab's marks beside it, the lock, the bell and the spinner of a program that runs and prints, and its program's progress, OSC 9;4's, in a ring filled as far as its value, of its state's color, which the window server slides, as the user chose on 6 November 2026, an indeterminate one's arc turning round it, in place of the outline that a tab of a bar shows; the window keeps its own title for its menu and the system's lists of windows; and the tab moves by Move Tab to New Window, in the Window menu and the palette, which no key takes by default.
- Double-click to rename, in the question of its title, as Rename Tab… asks it; the title that it asked with, unchanged, leaves the tab's folder and program its own.
- Context menu, the platform's: rename, the tab's color, of a submenu of macOS's system colors, each of a colored dot, and none, move to a new window, close, close others; a picker of any color later, as the host has none.
- When there are too many tabs, each shrinks to a minimum width, 120 points across the bar, and then the bar scrolls, across or down a column, as the tab shown scrolls into view.

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
position = "top"          # top | bottom | left | right
style = "unified"         # standard | unified | seamless
hide-when-single = true
show-index = true
close-button = "hover"    # always | hover | never
min-width = 120
max-width = 260
```

## The programs' progress

The `progress` plugin, as the user asked on 1 November 2026, after iTerm2 and Ghostty: a program's progress, of OSC 9;4, as Gradle's, which the VT core reads. Some programs, Gradle, cargo and Claude Code among them, send it only to the terminals that they recognize by their environment's variables, which Lokyr is not among yet: the README's [Programs' progress](../README.md#programs-progress) tells how to turn each on.
- **Where**: a thin bar in the `window-strip` slot, across the window's top under the titlebar, and on the tab of its pane with the `tabs` plugin, as the user asked on 4 November 2026, after iTerm2's: the tab's outline, of its corners' radius, filled from its left end along its top and its bottom at once as far as the percentage, as the strip fills.
- **Motion**: a change of the percentage slides the fill from where it was, eased, of about 0.2 s, in the strip and on the tab; the slides and the indeterminate sweeps go on layers that the window server animates, so that a program's percentage, of many changes a second, costs the app no frames.
- **States**: normal, in the accent's color, with its percentage; error, red; paused, yellow; indeterminate, which moves without frames of the app's; and none, which takes the bar away.

## The gutter

A few points at the left of each pane's cells, in its padding, which takes no column of the grid, with a thin bar on the prompt of each command that ran, of the commands that the shell marks with OSC 133: faint gray for a success, red for a failure, the accent while it runs, the theme's `gutter.succeeded`, `gutter.failed` and `gutter.running`; plugins lay marks of their own colors in it by the rows' numbers, which keep their rows as output and a reflow move them. `[window] gutter` turns it off.

## The scroll bar

Each pane's, drawn by the host, as the dividers and a search's matches are, over the pane's right edge, as macOS's overlay scroll bars look: a knob whose length and place tell how much of the scrollback and the screen the view shows, and where, of the theme's `scrollbar` tokens. It takes no column of the grid.

- **When it shows**: as macOS's Show scroll bars says, `[window] scrollbar = "system"` by default: as the view moves, through its rows and from the screen at once, so that output at the screen moves none, a thin knob that fades a second after, at once with Reduce motion; as the pointer comes over it, a wider knob over a track; or always, as macOS has it with a mouse. `"always"` and `"never"` override it.
- **The pointer**: a drag of the knob moves the view to the row that its place gives, the screen at the bottom, which the view follows again; a click on the track pages toward it, or jumps there, the knob's middle at the click, which the drag then holds, as macOS's Click in the scroll bar to says, Option the other way.
- **A search's marks**: the matches that the view shows, along the bar, the current one apart, of `scrollbar.mark` and `scrollbar.mark.current`, which keep it shown: the terminal finds them for each of the bar's runs of rows by a binary search, of one walk of its pages, however many they are, and takes its lock for them only while a search shows matches.
- **Its cost**: a frame looks at each pane's view, from its snapshot; a fade takes a timer a second after the view stops, and its frames for 0.3 s; at rest, nothing.

## A long command's end

A command that the shell marks with OSC 133 and that runs for `[notifications] long-command` seconds at least, 10 by default, tells its end in a notification of the system's while its window has no focus: the command as it was typed, whether it succeeded or its exit status, and how long it ran. Its time runs from its output's mark to its end's, as the workspace finds them in the pane's output, a frame's pace apart at most, without a timer meanwhile; a click on the notification brings its window forward.

## The command's sticky header

The `command-header` plugin, as the user asked on 1 November 2026, after iTerm2: the command whose output the view shows stays at the pane's top, over the grid's top rows, which it takes no row of, so that the grid keeps its size as it shows and hides, over OSC 133's marks.
- **Its look**, as the user chose on 8 November 2026: a pill of the command's line, in the terminal's font, of its size and its regular face, on a hairline across the pane, the pill a little in from the left; `[plugins.command-header]` names the theme's tokens of its colors, `background`, `text`, `border`, `line` and `hover`, the panels' and the interface's by default, as `border = "accent"`, and sets its `font-size`, `font-weight`, `padding-x`, `padding-y` and `align`, as the user chose: the theme holds no tokens of its own for it.
- **When it shows**: while the view's first row shows a command's output and the command's line is above the view; the alternate screen shows none.
- As the scroll goes up past a command, the header gives way to the command before it; down, to the next.
- A click on it scrolls to the command's prompt, at the view's top.
- **The host's parts**: a panel across a pane's top, `Anchor::PaneTop`, of the pane's width, which hides while its pane does not show; the panel role `overlay`, which takes the pointer and not the keyboard, a scroll over it going to the pane under it; the scroll to a row's number, `Scroll::Row`, as the terminal is when the write applies; and a node's text of the terminal's family, of a size in points and of a weight, `Style::font_family`, `font_size` and `font_weight`, which its children take.
- The actions on a command's output, to copy it, select it, tell its details, run it again, fold it or share it, come with F5a's blocks.

## Splits

The `splits` plugin. Each tab's layout tree lives in the kernel, because session restore, IPC and tmux -CC need it too. The plugin provides the commands; the host draws the dividers from the layout tree, in the theme's `pane.divider`, and takes their drags itself, giving their splits ratios in the workspace, which the plugins learn of by `layout-changed`. Without the plugin, every tab has exactly one pane.

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

On macOS the shortcuts are the `keymap-macos` plugin's, which binds the keys of every plugin's commands; on Linux (F7) they use Ctrl+Shift and are defined in `keymap-linux`.

**Behavior**
- **The commands**, as `splits` gives them: a split takes the keyboard to its new pane, right or below; the focus by direction goes to the nearest pane whose side faces the pane's, of the most room shared along it, the first in the layout's order of a tie, and nowhere at the tab's edge or while a pane is zoomed; next and previous go round the tab's panes in its layout's order; a resize moves the divider on the pane's right, or its bottom, where it has one, as iTerm2's, the other otherwise, by `cells` cells, 1 by default, of the share of its split's room that the pane's grid gives a cell; the zoom toggles; swap exchanges the pane with the one beside it, each taking the other's room, the keyboard staying with the pane; to a tab puts the pane in a tab of its own after its tab, and to a window in a window of its own, a pane alone in its window's one tab staying.
- **New panes** inherit the cwd of the pane they come from: the folder that its shell told last by OSC 7, which is why basic shell integration already lands in F3, where it is of this Mac, as a pane of `ssh` works in the client's, its program's otherwise; its profile with the profiles, in F5b.
- **Inactive panes** are dimmed (`dim-inactive`, 0.15 by default), as in iTerm2: the host brings their text's colors, their lines, their cursor and their cells' own backgrounds toward their background, as `splits` asks for it; a veil of the background over them, the user's first choice, darkened a translucent window's panes and left their padding out, which the user saw on 6 November 2026.
- **Dividers**, a line of a point, between the padding of the panes on either side, as each pane keeps its padding on every side, which the user asked for on 6 November 2026, and as long as the panes' room, the lines meeting:
  - can be dragged, with a grab area of 6 points, wider than the visible line, the pointer a column's or a row's resize over it, and held at it while the drag goes on;
  - reset to 50/50 on double-click.
- **PTY resizes during a drag** are coalesced: at most one every 25 ms, the window's live resize's pace, plus a final one. This avoids storms of SIGWINCH.
- **Options**: per-pane titles, in the `pane-header` slot, useful with many splits, of the pane's program's title or its folder and program, which `splits` reads and draws; `focus-follows-mouse`, which the host does as `splits` asks for it, the pointer's moves waking the app over the panes without the keyboard alone, as the user chose on 6 November 2026.

```toml
[plugins.splits]
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
- **Order and cancellation**: the search runs in chunks on a worker thread, 1000 rows under the pane's lock at a time, starting at the viewport and moving outward, a chunk below it and one above in turn, so nearby results arrive first, each chunk's as an event of the plugin's; it searches the rows that the scrollback holds as it starts, and ends at a resize, which renumbers them. Each keystroke in the find bar cancels the previous search, as a plugin's next search of a pane ends its last: the engine looks at a flag before each logical line, and between the matches it tries in one.
- **Results**:
  - Stored as stable pins, on a match's first and last cells, which the terminal keeps by the search, which stay valid as new output arrives, and go when the cells leave the scrollback.
  - At most 100k are kept; past that, only the count.
  - Shown as the plugin asks, `show_matches`: at each frame, the terminal finds those that the view shows by a binary search, as they lie in reading order, so that a frame's cost is of the view's matches, however many the search found. A search's matches that showed stay, stopped, as the next search of the pane starts, until that one shows its own, in the same frame, so that typing never leaves the view without them.
  - The worker takes the lock in turn, and the main thread, a frame's copy or a plugin's request, ahead of its next chunk.
- **Target**: 100k lines in ≤ 150 ms ([PERFORMANCE.md](PERFORMANCE.md)).

### UI

- **Find bar**: an overlay on the pane, top right as in iTerm2, with history.
- **Options**: case (Aa), regex (`.*`), whole word; an "n of m" counter.
- **Navigation**, from the bottom up, as the newest output is at the bottom: the first match is the last of the nearest part to the view, the one nearest the bottom; the next is the one above, the previous the one below, past the ends as `wrap-around` says:
  - Enter / ⇧Enter, the next and the previous;
  - ⌘G / ⇧⌘G on macOS, F3 / ⇧F3 on Linux, which open the bar while it is closed;
  - the bar's up and down arrows;
  - each match the current one, highlighted apart, and brought into the view, where the view stays as the program prints on, whatever `scroll-on-output` says, until the bar closes;
  - Esc closes the bar, its highlights going, and leaves the current match selected, ready for ⌘C.
- **History**: the bar opens of the last search's text, selected, so that typing replaces it, and its options, which it searches again; ⌘F on an open bar selects its text too; the magnifying glass of its field lists the recent searches, twenty at most, which the plugin's state keeps with the options.
- **Layout**, as VS Code's find widget, of macOS's search field: the field, its glass at its start and its toggles of case, whole words and regexes at its end; the count, in a room of its own, which keeps the bar's width whatever it tells, `n of m`, of numbers past 9999 short, as `12K`; the arrows and the close button. The buttons and the toggles have no box at rest, their hover and press fading in, a toggle on of its own fill.
- **Highlighting**: the visible matches (`search.match`), unless `highlight-all` is off, and the current one (`search.match.current`), the theme's translucent colors over each cell's own; [the scroll bar](#the-scroll-bar) marks them along it.
- **Filter mode**: iTerm2-style, showing only the matching lines. It arrives in F5b, because the renderer must first be able to show an arbitrary list of rows instead of a contiguous window.

```toml
[plugins.search]
case = "smart"            # smart | insensitive, while the case toggle is off
highlight-all = true
wrap-around = true
```

## Copy mode

The `copy-mode` plugin, of the host's selection by a range, its scroll to a row, and a cursor of the plugin's own: a cursor of the keyboard's over the scrollback and the screen, which vi's keys move, as tmux's copy mode has them, to select and copy without the mouse.

- **Starting**: ⇧⌘C, as iTerm2's, or Edit › Copy Mode, in the pane that has the keyboard, at the terminal's cursor, or at the view's last row while the view is up in the scrollback; not on the alternate screen, whose rows have no scrollback.
- **The keys**: a badge at the pane's bottom right, an overlay panel, holds the keys back from the program, `takes_keys`, while it shows and its pane has the keyboard: the keys that no binding takes go nowhere, and the input method's text with them. The pane keeps the keyboard, so that its cursor keeps its look and the program hears no change of the focus, as a dialog's panel would have given it. Its bindings hold in `copy-mode.open && pane.focused`, so that a panel's field, as the find bar's, keeps its keys.
- **The moves**: by cells and rows; by words and big words, through the rows that a line wraps over, a line's end parting two words; to a line's start, its first character and its last; by paragraphs; to the scrollback's top and the last row that shows something; to the view's rows; by half a page and a page, the view with the cursor; with counts. They read the logical lines a few rows at a time, so that a line of many rows costs no more.
- **Selecting**: of cells, of whole lines, or of a block, from where it started to the cursor, the host's selection, which ⌘C copies as any other; `y` copies it, or the cursor's line without one, and ends.
- **Its end**: another pane of its tab that takes the keyboard, the alternate screen, a resize, which numbers the rows anew, and its pane's move to another tab end it; a tab in front leaves it as it is, its panel hidden and its keys with it, as `<namespace>.open` holds of a panel that shows alone.

## iTerm2 themes

The host's, as it draws them: the app reads `[theme]` and the themes' files, light/dark switching, the importer of `.itermcolors`; the palette's picker, a plugin's, writes `[theme]` ([ADR 0035](adr/0035-themes-are-the-hosts.md)).

### Internal model

| Group | Fields |
|---|---|
| Terminal | 16 ANSI colors, 256-color palette (derived or explicit), foreground, background, bold, cursor, cursor text, selection (background and text), link, underline, badge |
| Search | `search.match`, `search.match.current` |
| UI | Tokens for the tab bar; active, inactive and hovered tabs; dividers; overlays; accent; borders |
| Chrome | The colors of the chrome that the plugins draw, a section of each plugin's, as the tab bar's: the bar, the active, inactive and hovered tabs, their marks. The chrome's look, as the tab bar's style, place and shape, stays in the config, never in a theme ([ADR 0034](adr/0034-themes-native-format-and-itermcolors.md)) |
| Options | Light/dark variants; `minimum-contrast`, the minimum ratio between text and background (as in iTerm2 and Ghostty) |

UI tokens that are not specified are **derived** from the terminal colors, the way iTerm2 derives its tab bar, each of the opacities of the built-in theme of its appearance, a theme dark where its background is darker than its text:

| Token | Derived from |
|---|---|
| The tab bar's background, `tabs.bar.bg` | background mixed 6% toward the foreground |
| A panel, `panel.bg` | background mixed 4% (light) or 6% (dark) toward the foreground |
| The active tab, a field | background |
| Text | foreground |
| Secondary text, an inactive tab's | foreground mixed 38% and 40% toward the background, of three quarters of the theme's own contrast at least, 4.5 at most |
| Dividers, separators, borders, the buttons' and the hovers' veils | the foreground at 10% to 18% |
| Accent, a focus ring, a list's selection, the progress | the palette's blue, or its bright blue, where it reads on the background at 3:1, else the foreground; the text on it white or black, whichever reads better |
| The search's matches | the palette's yellow; the current one between its yellow and its red |
| The progress's error and pause | the palette's red and yellow |

This gives every imported iTerm2 theme matching tabs right away; the chrome's colors that an iTerm2 theme does not give derive from its terminal's colors. The tokens' names are the theme's sections': the tab bar's `tabs.*`, the plugin's namespace, the segmented control's `segments.*`, the host's own as `panel.*` or `field.*`.

**`minimum-contrast`** is the least contrast ratio of a cell's text to the background under it, from 1, none, to 21: a text of less moves toward white or black, whichever reads better, as little as gives it, its opacity kept.

The default theme's terminal colors are the built-in palette of F2, with a light and a dark variant, whose colors meet WCAG's contrast on their backgrounds.

### `.itermcolors` importer

- **Keys read** from the file, which is an XML plist (parsed with the `plist` crate):
  - `Ansi 0 Color` … `Ansi 15 Color`;
  - `Foreground Color`, `Background Color`, `Bold Color`;
  - `Cursor Color`, `Cursor Text Color`, `Cursor Guide Color`;
  - `Selection Color`, `Selected Text Color`;
  - `Link Color`, `Badge Color`, `Tab Color`, `Underline Color`, `Match Background Color`.
- **Color components**: each color has `Red Component`, `Green Component` and `Blue Component` (floats 0–1), an optional `Alpha Component`, and a `Color Space`:
  - `sRGB`: used as is.
  - `P3`: converted to sRGB.
  - `Calibrated`, `Device`, or missing: calibrated Generic RGB, of a gamma of 1.8, as iTerm2 reads them, its own old files of no color space among them, converted to sRGB. On macOS this uses ColorSync for exact results; elsewhere, a matrix plus a transfer curve.
  - The `Alpha Component` is kept; the cursor guide, of none, takes 0.25, as iTerm2 gives it.
- **`(Light)` / `(Dark)` suffixes**: keys from profiles that use separate colors for light and dark mode. A file of them is two themes, `<name> (Light)` and `<name> (Dark)`, as a theme is a file a variant.
- **Missing keys**: sensible defaults (bold = foreground, cursor text = background, derived selection).
- **Read as they are**: a user's `.itermcolors` is never converted; the importer reads it at each start and as it changes.
- **No other format**: `.itermcolors` is the one format imported, the source of the largest collection, iTerm2-Color-Schemes, whose formats generated for other terminals lose its variants, its opacity and some keys, and take its Calibrated colors as sRGB ([ADR 0034](adr/0034-themes-native-format-and-itermcolors.md)).

### Where themes come from

- **Held by the app**: the collection's schemes of a permissive license, as its `licenses.yml` tells, and those whose license is checked at their source, converted into the native format as the app builds, of a pinned commit, by the same importer, each with its author, license and source; their notices in the bundle's `Contents/Resources/licenses`; and Lokyr's own light and dark themes.
- **The user's**: the files of `~/.config/lokyr/themes/`, `.toml` and `.itermcolors`, each named by its file; one of a name that the app holds takes its place; a file with an error tells it with its line, the theme in force staying.
- **From the Finder**: a double click on an `.itermcolors` file, or a file dropped on Lokyr's icon in the Dock, asks to install it into the themes' folder. Lokyr opens `.itermcolors` files of an alternate rank, so that iTerm2 keeps them where it is installed. A file dropped on a pane gives the program its path, as for every file.
- **The whole collection**, those that the app cannot hold too, with Sleip in F4.

### Usage

```toml
[theme]
light = "Rose Pine Dawn"
dark = "Dracula"          # the names of the collection's schemes, or of the themes' folder's files
minimum-contrast = 1.0
```

- **Light/dark**: with both keys set, the theme follows the system appearance and switches live.
- **Picker**: the palette has a theme picker with live preview.
- **Native format**: TOML, a file a variant, of the internal model's fields, all but the terminal's background, foreground and palette optional, the rest derived:

  ```toml
  [theme]
  name = "Nightfall"
  author = "Ada"
  license = "MIT"
  variant = "dark"
  extends = "Dracula"            # optional: a theme of either format, whose values these override

  [terminal]
  background = "#1e1e20"
  foreground = "#e6e6e6"
  palette = ["#45454b", "#f25d62", "#58c672", "#e3bd4c", "#5aa2f7", "#c983de", "#4cc4cf", "#c4c4ca",
             "#75757d", "#ff8488", "#7edb94", "#f5d47a", "#86bcff", "#dca3ec", "#7adce4", "#f4f4f6"]
  cursor = "#e6e6e6"
  selection = "#34445f"
  link = "#5aa2f7"

  [search]
  match = "#ffd60a4d"            # #rrggbbaa, of an opacity
  current = "#ff7a0080"

  [tabs]                         # the tab bar's tokens, tabs.*; its look is the config's
  bar.bg = "#18181a"
  active = { bg = "#1e1e20", fg = "#e6e6e6" }
  inactive = { fg = "#9a9aa0" }

  [interface]
  "panel.bg" = "#252528"
  ```

### Verification

- **Full collection**: import the entire iTerm2-Color-Schemes collection with zero errors.
  - Check RGB values against computed references.
  - The collection also ships files generated for other terminals, which are useful as a cross-check of its sRGB and P3 schemes; those of Calibrated RGB and of no color space differ from them by design, as their generator takes them as sRGB.
- **Targeted tests**: for the Light/Dark keys and for each color space.
- **Licenses**: check each theme's license before bundling it in the app or in a `themes` package.

## Command palette

The `command-palette` plugin (⌘⇧P on macOS, Ctrl+Shift+P on Linux):
- fuzzy search over registered commands (title, shortcut, plugin), with recent commands first;
- theme picker with live preview;
- shows in a panel over the window, of an IME-capable text field.
- runs the commands as the user's, and lists every command with its plugin and keys, through the capability `user-commands`, which a palette of a third party asks for at install ([ADR 0024](adr/0024-user-commands.md)).

## Windows restored

As Lokyr quits, the windows that are open, the quick terminal's and the man pages' aside, are kept in the state's directory, in `windows.json`, and open again as it starts, as `[app] restore-windows` says: as macOS restores an app's windows, of its setting "Close windows when quitting an application", by default, or always, or never, as the user chose on 8 November 2026.
- **What opens again**: each window at its frame, out of full screen at the one that it had before, within the screen that its titlebar's middle lies on, or, where that screen went, within the nearest one, of its size within the screen's area for windows, as `ScreenRect::onto` finds it; its tabs in their order, the one shown, each tab's title and color of its own, its splits and their ratios, the pane of the keyboard and the one zoomed; each pane's program anew, the configured one, in the folder that its shell told last, by OSC 7, of this Mac, or that its program worked in, as a new pane's. The window in front takes the focus, and the windows' numbers follow their order.
- **What does not**: the panes' text and their programs, whose scrollback comes back with F5b; full screen itself, B-580 of the backlog.
- **When**: as Lokyr quits, alone, as the user chose: no write while it runs, so that a crash or a force quit opens the windows of the quit before. A quit of no window, or with the windows not kept, forgets them.
- **The file**: JSON, of the format's version first, `"version": 1`, which a later format raises; a file of a newer version, as a later Lokyr writes, or that does not read, or of more than 256 panes, is left aside with a notice, the start going on as without it, and this version writes its own as it quits. Fields that it does not know are read past, so that a later version may add some of its own without raising it.
- The benchmarks open none of the kept windows, and neither keep nor forget them as they quit; the last window's close that quits, as `[app] quit-after-last-window-closed` says, forgets them, as a quit of no window does. A file left aside tells it in a notice of its own.

## The Finder's services and the Dock

A window, or a tab, at a folder, from the Finder, as the user's daily path into a project:
- **The services**: New Lokyr Window Here and New Lokyr Tab Here, in the Finder's menu of a folder, under Quick Actions or Services, and in the app menu's Services. The bundle declares them in `NSServices`, on `public.folder` alone, of an empty `NSRequiredContext`, which has macOS show them without the user turning them on in the keyboard's settings; their titles are the catalogs' `infoplist.ftl`, each language's `ServicesMenu.strings`. The app's provider, `ServicesProvider`, answers `openWindow:userData:error:` and `openTab:userData:error:` of the pasteboard's file URLs, as `AppEvent::OpenFolders`: a window at each folder, or a tab at each after the tabs of the window in front, the quick terminal's and the man pages' aside, a window without one; a file's folder for a file; the last window brought forward with the app, as the Finder keeps the focus otherwise.
- **The Dock**: a folder dropped on the app's icon opens a window there, as the bundle declares `public.folder` among its document types, of the rank `None`, so that the Finder never opens folders with Lokyr of its own; a theme's file dropped with it still asks to install.
- **At the start**: a start that macOS makes to open a folder, a file or a link, whose events come before the start's end, opens no new window of its own, the folder's alone, beside the windows restored; one whose files open no window, as a theme's, opens a new window after a second. AppKit's `NSApplicationLaunchIsDefaultLaunchKey` tells a start from a shell as no default one too, and so is not read: a service that starts Lokyr comes after the start, beside a new window.

## Lokyr as the default terminal

Lokyr › Make Lokyr the Default Terminal, as the user asked on 6 November 2026, asks macOS, through Launch Services' default handlers, `NSWorkspace`'s `setDefaultApplicationAtURL:`, to open in Lokyr the shell scripts, Terminal's `com.apple.terminal.shell-script`, `.command` and `.tool`, and `public.shell-script`, `.sh`, the executables, `public.unix-executable`, and the `ssh` and `x-man-page` links; macOS asks the user to confirm the changes, one after the other, and what the user declines stays as it was, as the log tells. The item is ticked and dimmed while Lokyr is the default of them all, as the platform asks Launch Services on a thread of its own after the start and as the app becomes active, as another app may take the default meanwhile, out of their way, and again as the changes end; nothing changes but at the user's choice.
- **The bundle**: the scripts and the executables among its document types, of the Shell role and the rank `Alternate`, so that Lokyr is in the Finder's Open With and opens them of its own once it is the default; Terminal's type imported; `CFBundleURLTypes` of the two schemes; their names in the catalogs' `infoplist.ftl`.
- **A script**: a window of its own in the script's folder, which the user's login shell runs, `exec` of its path, so that it has the user's environment; the window keeps what it printed once it ends, whatever `close-on-exit` says, as Terminal's does. A file of a script's extension runs, `.command`, `.tool`, `.sh`, `.zsh`, `.py`, `.rb` and the others of an interpreter, and one of no such extension if it is executable or its first line is `#!`; one not executable runs of its first line's interpreter, or of its extension's, `/bin/sh` of `.command` and `.sh`. A path of `!`, a backslash or a control character does not run, as a notice tells, as a shell may read them otherwise. A script downloaded from the internet, of macOS's quarantine that Gatekeeper has not approved, asks first, Run or Cancel, at each open.
- **An `ssh` link**: `ssh://[user[;parameters]@]host[:port]`, a window that runs `ssh` to the host, as [SECURITY.md](SECURITY.md#terminal-defaults) tells of its checks.
- **A man page's link**: `x-man-page://[section/]name`, the window of its page, as Open man Page opens it.

## Basic actions

The `core-actions` plugin:
- **Clipboard and scrollback**: copy, paste (with the rules in [SECURITY.md](SECURITY.md)), select all, clear the scrollback. A drop of files pastes their paths, quoted for the shell; a drop of text pastes it, as a paste does.
- **Font and scrolling**: font size (+, −, reset); scroll by line or page, to the top or bottom.
- **Windows**: new window, close window, quit; Window › Float on Top, the app's `window.toggle-float-on-top`, as the user asked on 6 November 2026, keeps a window above the other applications' windows, at AppKit's floating level, each window of its own, its item ticked as the window in front floats and dimmed for the quick terminal, which floats of its own; a window in full screen takes the normal level there, and floats again as it leaves it.
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

Its items show no keys, as macOS's context menus and Terminal's, the menu bar telling them, as the user chose on 5 November 2026.

Its groups, from the top: what opens at the cell, Open Link, Copy Link Address, New Window and New Tab at Folder, and the man pages of the selection; Look Up and Search the Web, of the selection; the clipboard's; the editing's; then the plugins' items, a group of each plugin, as `tabs`' Rename Tab… and `splits`' Split Right, Left, Down and Up, the moves of the pane to a tab or a window, and Close Pane. A plugin adds its items through the HostApi, each a command of its arguments, as it adds them to the menu bar's menus, where they show their keys.

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
movement = "instant"      # instant | snappy
```

- **Shape**: a block, a vertical bar, an underline, or a hollow block, of its edges alone. Programs change the shape and the blinking with DECSCUSR, as vim does in insert mode, among the first three. The options are the default, which `CSI 0 SP q` and a reset bring back.
  - The text under a block takes its cell's background. A bar and an underline take `thickness`, which scales with the display as the font's size does, or else the face's underline thickness, which a hollow block's edges always take.
  - Mode 12, which `cvvis` sets in xterm's terminfo, makes the cursor blink too. Its reset, which `cnorm` sends as each full-screen program ends, leaves the configuration's blinking, so that the cursor does not stop blinking after `less` or `vim`.
- **Blinking**: on and off for `blink-interval` each. With `smooth-blink`, the cursor fades between the two instead of switching, as iTerm2's does, of its presets, Breathing, Linear, Fast, Slow and Subtle, or of six values of the user's: each fade's duration and curve, and how long the cursor holds visible and hidden between them. The cursor's cell is a layer of its own, whose opacity the window server animates, so that the blinking and its fades wake the app no more.
- **Movement**: `snappy` eases the cursor to its new position in 80 ms, slowing as it comes, and `instant`, the default, moves it at once.
  - The animation only moves the cursor that is drawn. The text that a key press changes still appears in the next frame, so the latency budget holds.
  - Each frame animates from where the cursor was drawn to where it is now, so a burst of output, or a synchronized update (mode 2026), moves it once rather than once per sequence.
  - The window server slides the cursor's layer, as it blinks it, from where it shows, in the middle of a slide too, so that a movement takes no frame of the app's and no display link: the frame tells the layer its new place with the slide.
  - It slides for a move of the cursor's own alone, in the pane that showed it, of the same view, cells and grid's place, of rows that did not scroll; a scroll of the view or of the screen, a new size, another pane, and a cursor shown again after it was hidden go at once. Where the cursor has no layer of its own, it moves at once.
- **Focus**: only the focused pane's cursor blinks. An unfocused pane, or a window in the background, shows a hollow block.
- **Power**: blinking stops after `blink-timeout` without keys or output, and the cursor stays on. The blinking and its fades take no frame of the app's, as the window server animates the cursor's layer; a movement takes no frame of the app's either, as the window server slides the layer.
- **Blinking text**, of SGR 5 and 6, blinks on the cursor's clock, switched at `blink-interval`, and stops shown with it. It goes on a layer of its own, under the cursor's, over the rows that hold it, which the window server switches as the grid draws that text hidden, so that it takes no frame of the app's either.
- **Accessibility**: with the system's "Reduce motion" setting, the movement is instant and blinking does not fade.
