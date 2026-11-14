# Writing a plugin

A plugin of Lokyr is a script in [Luau](https://luau.org), a small, fast language of Lua's family, that runs in a sandbox of its own: it adds commands, keys, menu items, bars and panels, and reads and changes the terminal through the host's API, asking for the capabilities that it needs, which the person who runs it grants. [Sleip](../PACKAGE-MANAGER.md), Lokyr's package manager, makes, checks, tests, installs and publishes plugins; the registry, [pkg.lokyr.dev](https://pkg.lokyr.dev), lists the published ones.

This guide makes a plugin from nothing, runs it in Lokyr as you edit it, gives it a setting and a capability, and publishes it. Its last part, [the API at a glance](#the-api-at-a-glance), tells where everything else is.

## What you need

- **Lokyr 1.0 or later**, of the plugin API 0.1.2, which `lokyr --api-version` tells.
- **Sleip on your `PATH`**: Lokyr › Install Command Line Tools… links `lokyr` and `sleip` into `/usr/local/bin`.
- **An editor.** In VS Code, the [Luau Language Server](https://marketplace.visualstudio.com/items?itemName=JohnnyMorganz.luau-lsp) extension completes and checks the API's types, of the settings that `sleip new` writes.
- **For publishing**: a GitHub account, a public repository of the plugin, and, best, [GitHub's CLI](https://cli.github.com), `gh`, signed in.

## 1. Make it

```sh
sleip new acme/hello
cd hello
```

A plugin's name is `owner/name`, of lowercase letters, digits and `-`, from a letter; for a published plugin, the owner is your GitHub account. The second part, `hello`, is the plugin's **namespace**: its commands are named of it, as `hello.say`, its section of the config is `[plugins.hello]`, and no two plugins share one.

`sleip new` makes a plugin that works, a command that shows a notice:

| File | What it is |
|---|---|
| `plugin.toml` | The manifest: the name, version, license, the plugin API that it takes, when it activates, its capabilities and its commands |
| `src/init.luau` | The plugin: a table of callbacks that the host calls |
| `locales/en/hello.ftl` | Its texts in English, of [Project Fluent](https://projectfluent.org); other languages are folders beside it, as `locales/it/` |
| `tests/hello.luau` | Its tests |
| `lokyr.d.luau` | The API's types, the reference of every call, event and shape |
| `.luaurc`, `.vscode/settings.json` | Luau's strict mode, and the language server's settings |
| `README.md` | What the plugin does, for the people who install it |

## 2. Read it

The manifest:

```toml
[plugin]
name = "acme/hello"
version = "0.1.0"
kind = "luau"
api = "^0.1.2"
license = "MIT"
description = "A plugin of Lokyr, made by `sleip new`."

# It activates as its command first runs, which the menus and the palette
# show before.
[activation]
on = ["command:hello.say"]

[contributes]
commands = [{ name = "hello.say", title = "Say Hello", category = "view" }]
```

- `api` is the plugin API that the plugin is written against, a constraint of Cargo's grammar: a Lokyr of an older API refuses the plugin with a notice.
- `activation.on` tells when Lokyr starts the plugin: `startup`; `command:<name>` or `command:hello.*`, as one of its commands first runs; `event:<name>`, as an event first comes; or `slot:<name>`, as one of its views first shows. A plugin that waits costs nothing: no VM until then.
- `contributes.commands` shows the commands in the menu of their category and in the palette before the plugin starts; the plugin registers them again as it activates.
- The manifest's other fields, as `capabilities`, `uses` and `slots`, are in [`plugin.toml`'s reference](../PACKAGE-MANAGER.md#plugintoml).

The plugin, `src/init.luau`, returns a table of callbacks:

```lua
local Plugin: Plugin = {}

function Plugin.activate(cx: Cx)
    assert(cx.register_command({ name = "hello.say", title = "Say Hello", category = "view" }))
end

function Plugin.on_command(cx: Cx, name: string, args: Args)
    -- The command ran: open the notice's panel, and a timer that closes it.
end

function Plugin.on_event(cx: Cx, events: { Event })
    -- The timer fired, or the panel closed.
end

function Plugin.render(cx: Cx, view: ViewId): Node?
    -- The panel's view: a tree of nodes, which the host draws.
end

return Plugin
```

Every call reaches the host through `cx`, which each callback gets, as `cx.open_panel` and `cx.start_timer`. A call answers its value, or `true`; or, as it fails, `nil`, the error's message and its code: `assert` turns a failure into an error of the plugin's, which the host tells. The texts come of the plugin's catalogs in the language that Lokyr shows, `cx.message("hello-notice")`, and a command's title of the message `command-hello-say`.

## 3. Check it and test it

```sh
sleip check
sleip test
```

`sleip check` reads the manifest, the catalogs, the config's schema and the package, and checks every source's types against the API in strict mode, as Lokyr would load the plugin: each problem comes as `file:line:column`, as an editor jumps to. `sleip test` runs each function of each file of `tests/`, each on a host of its own, which plays Lokyr's part: it activates the plugin, runs its commands, gives it events, renders its views and moves the time on, and tells what the plugin did.

```lua
function test.the_command_shows_its_notice_which_goes_by_itself(host: TestHost)
    host.activate()
    host.command("hello.say")
    assert(host.panels()[1] == "notice", "the notice shows")
    local notice = host.render({ panel = "notice" })
    assert(notice ~= nil and notice.role == "alert", "the notice renders")
    host.advance(3000)
    assert(#host.panels() == 0, "the notice goes after three seconds")
end
```

A test fails at its first error, of its file and its line. The host of the tests is `TestHost` in `lokyr.d.luau`, each function of its comment.

## 4. Run it in Lokyr

With Lokyr open:

```sh
sleip dev
```

`sleip dev` checks the plugin and links its folder into Lokyr, which loads it at once: open the command palette, ⇧⌘P, and run **Say Hello**. Change the notice's text in `locales/en/hello.ftl` and save: Sleip checks the plugin again and Lokyr reloads it, so that the next Say Hello shows the new text. Control-C ends `sleip dev` and takes the plugin out of Lokyr.

## 5. Give it a setting and a capability

Let whoever uses the plugin choose the greeting, in Lokyr's config, and have it copied to the clipboard if they like.

**The setting.** A plugin's settings are its section of Lokyr's `config.toml`, which a [JSON Schema](https://json-schema.org) of the plugin's describes; Lokyr checks the section against it, and a wrong value refuses the section alone, with a notice at its line. Make `hello.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "acme/hello",
  "type": "object",
  "properties": {
    "greeting": {
      "description": "What the notice says.",
      "type": "string",
      "maxLength": 80
    },
    "copy": {
      "description": "Whether the greeting goes to the clipboard too.",
      "type": "boolean",
      "default": false
    }
  }
}
```

**The capability.** Putting text on the clipboard needs `clipboard-write`. The manifest declares it, and names the schema:

```toml
# The greeting on the clipboard too, as the config asks.
[capabilities]
clipboard-write = true

[contributes]
commands = [{ name = "hello.say", title = "Say Hello", category = "view" }]
config-schema = "hello.schema.json"
```

**The code.** The command reads the section, `cx.config()`, and keeps the greeting for the notice:

```lua
-- The greeting that the notice shows, of the config or of the catalog.
local greeting = ""

function Plugin.on_command(cx: Cx, name: string, args: Args)
    if name ~= "hello.say" then
        return
    end
    local settings: any = cx.config() or {}
    greeting = settings.greeting or cx.message("hello-notice") or "Hello!"
    if settings.copy then
        assert(cx.set_clipboard_text(greeting))
    end
    -- The notice's panel, as before.
end
```

and the view shows it: in `Plugin.render`, the text node's text is `greeting`.

**The test.**

```lua
function test.the_greeting_of_the_config_shows_and_is_copied(host: TestHost)
    host.set_config({ greeting = "Ciao!", copy = true })
    host.activate()
    host.command("hello.say")
    local notice = assert(host.render({ panel = "notice" }))
    local children = assert(notice.children)
    assert(children[1].text == "Ciao!", "the config's greeting shows")
    assert(host.clipboard() == "Ciao!", "and is copied")
end
```

`sleip check` and `sleip test` pass again, and `sleip dev` reloads it. In Lokyr's config:

```toml
[plugins.hello]
greeting = "Ciao!"
copy = true
```

The first time the plugin starts with a new capability, Lokyr asks whether the plugin may hold it, of the capability's risk; its default button refuses, and a refused capability makes its calls fail, as the plugin then tells or not. The answers stay in `permissions.toml`, beside the config, where one can be taken back. Ask for the least that the plugin needs: [the capabilities](../PLUGIN-SYSTEM.md#capabilities) list each one, what it allows and its risk.

## 6. Install it as others will

```sh
sleip add --path .
```

installs the plugin of its folder, copied into Sleip's store, as an install of the registry would: `sleip list` shows it, and `sleip remove acme/hello` takes it away. `sleip add --git <url> --tag v0.1.0` installs it of a repository's tag.

## 7. Publish it

1. Push the plugin to a public GitHub repository; its `plugin.toml`'s `repository` names it.
2. Tag the version, `v` and its `version`, and push the tag:

   ```sh
   git tag v0.1.0
   git push origin main v0.1.0
   ```

3. Propose it to the registry:

   ```sh
   sleip publish
   ```

   Sleip checks and tests the plugin as the registry does, finds the tag on GitHub, and opens the registry's pull request of the plugin's file through `gh`; without `gh`, it tells a link that proposes it on GitHub.

The registry, [`sephiroth74/sleip-registry`](https://github.com/sephiroth74/sleip-registry), checks the pull request: the plugin's file, its name, its tag's commit, its manifest and its tests, and builds its archive of the tag's commit. Its maintainer reviews a new plugin, and a version that asks for nothing new, of no new capability, command or dependency, is merged by itself. Once merged, the version is released, signed into the index and shown on `pkg.lokyr.dev`, and anyone installs it:

```sh
sleip add acme/hello
```

A new version is a new `version` in the manifest, a new tag, and `sleip publish` again. The registry's README tells its rules.

## The API at a glance

`lokyr.d.luau` is the reference: every call of `cx`, its arguments and its answer, every event and every shape, of a comment each. [The plugin reference](reference.md) lists the capabilities and their risks, the slots, the events, the limits and the errors; [the plugin system](../PLUGIN-SYSTEM.md#illustrative-draft) tells what the calls mean, at more length. An overview:

**The callbacks** of the plugin's table, each one optional: `activate`, as the plugin starts; `on_command`, a command of its own ran; `applies`, whether a command would do something now, as a menu asks before it enables an item; `on_event`, a batch of events; `render`, the tree of one of its views; `on_ui_event`, a click, a key or a change of a node of its views.

**The calls of `cx`**, by what they do:

| What | Calls |
|---|---|
| Declarations | `register_command`, `bind`, `subscribe`, `add_menu_item`, `emit` |
| The commands | `run_command`, `commands` |
| The panes' text | `selection_text`, `line_text`, `link_at`, `read_lines`, `styles`, `selection_cells`, `screen_cells`, `working_directory`, `shell_commands`, `command_line`, `viewport` |
| Search | `search`, `end_search`, `search_matches`, `current_match`, `show_matches` |
| Writing to a pane | `paste`, `send_text`, `clear`, `clear_scrollback`, `scroll`, `select_all`, `select_cells`, `deselect`, `show_cursor`, `cursor_cell` |
| Drawing over a pane | `decorate`, `undecorate`, `mark_gutter`, `unmark_gutter` |
| Windows, tabs and panes | `windows`, `tabs`, `agents`, `layout`, `pane_grid`, `pane_title`, `open_window`, `close_window`, `new_tab`, `close_tab`, `split`, `close_pane`, `activate_tab`, `focus_pane`, `move_tab`, `swap_panes`, `zoom`, `resize_pane`, `set_ratio`, `equalize`, `set_tab_title`, `set_tab_color`, and the rest of their family |
| Views and panels | `open_panel`, `close_panel`, `request_render`, `set_text`, `select`, `reveal`, `focus_node` |
| Questions, opening and texts | `ask_text`, `save_file`, `open`, `open_at`, `opens`, `message`, `keys_of`, `notice_shows` |
| The clipboard | `clipboard_text`, `set_clipboard_text`, `set_clipboard_image` |
| Images | `render_image`, `drop_image` |
| Settings and state | `config`, `state`, `set_state`, `keeps_state` |
| Time | `start_timer`, `cancel_timer`, `day` |
| The Mac | `keep_awake` |
| Themes and the font | `themes`, `preview_theme`, `set_theme`, `fonts`, `preview_font`, `set_font`, `font_size`, `set_font_size` |

**Reads, writes and requests.** A read answers at once, of things as they are; a write applies as the callback returns, so that a later read of the same callback reads them as they were; a request of the app's, as a window to open or a command to run, comes after the writes. A callback that fails, or returns an error, drops its writes and its requests.

**Events** come as a batch, each a table of its `kind`: the workspace's, as `tab-created`, `pane-focused` and `tab-changed`, which a plugin subscribes to; the pointer's, `pointer`; the commands that the shell marks, `command-started` and `command-ended`; and those of the plugin alone, as its timers', `timer`, its panels', `panel-closed`, its config's, `config-changed`, an answer, `answered`, a save, `saved`, and another plugin's of its own, `custom`.

**Views** are data: a view is a tree of nodes, each a table of its `kind`, as `container`, `text`, `button`, `toggle`, `field`, `list`, `scroll`, `progress`, `tabs` and `image`, of a layout of flexbox's terms and a style whose colors are tokens of the theme in force, as `panel.bg` and `text.secondary`, never colors of the plugin's. The host draws them, keeps their state, as a field's text, of their keys, and makes them accessible to VoiceOver. A plugin's views fill the window's **slots**, as the tab bar, a strip across the top, the status bar, the panes' headers and the titlebar's buttons, which the manifest's `slots` declares, and its **panels**, which float over the window at a cell, the cursor, the pointer, a node, or a corner of a pane or of the window.

**Capabilities** guard the calls that reach what belongs to the person who runs Lokyr: the panes' text, typing into them, the clipboard, the processes, the workspace, the notifications, the Mac's sleep, the theme and the font in the config. A call of a capability that the plugin lacks fails, of the code 302; [the capabilities](../PLUGIN-SYSTEM.md#capabilities) list them all. Each capability and its risk are in [the reference](reference.md#capabilities).

**Limits.** Each callback runs within its deadline, 16 ms for `render` and `applies`, 250 ms for a command, an event or a view's event, and a second for the activation; a plugin holds 64 MB at most: past them the plugin faults, and the host stops it, starts it again after a while, and turns it off at its third fault within a minute. A callback writes 10 000 times at most, a write past them failing; a value that the plugin gives the host carries 16 MiB of text or bytes at most; its images take 32 million pixels at once, given back at 4 million a second. [The limits](../PLUGIN-SYSTEM.md#limits-and-isolation) tell them all.

**The sandbox.** Luau's `string`, `table`, `math`, `bit32`, `utf8` and `buffer`, and `require` of the plugin's own modules, as `require("./look")` of `src/look.luau`: no files, no network, no processes.

**Versions.** The plugin API is `0.1.2`, of Lokyr 1.0: a patch adds calls and events, and the API is experimental until its own 1.0, which keeps the calls' names. The manifest's `api` is the least that the plugin needs: `^0.1.1` takes every Lokyr of the API 0.1.1 or a later 0.1, and `sleip new` writes the API of its Lokyr, `^0.1.2`.

## When something goes wrong

- **The plugin does not load**: Lokyr tells why in a notice, as a manifest that does not read, an API that it lacks, or a capability that it refuses. `sleip check` tells the same of the folder.
- **A call fails**: its second answer is the message, its third the code, whose hundreds tell the kind: 1xx a wrong call, 2xx a failure, 3xx a capability missing, 4xx a fault.
- **A callback errs**: Lokyr tells it in a notice, of the source's line, and the plugin goes on, its call's writes dropped.
- **A plugin faults**, past its deadline or its memory: Lokyr stops it and starts it again after a while; three faults within a minute turn it off until Lokyr starts again or its section turns it on.
- **Sleip itself**: `sleip doctor` checks the config's folder, the lockfile, the store and Lokyr's plugin API.
