# Security and robustness

A terminal sees everything that is typed and printed, passwords included. That makes security a property of the whole project, not a separate module.

## Threat model

| What we defend against | Example |
|---|---|
| Untrusted output | A file or a remote host that prints malicious escape sequences (`cat`, `ssh`, `curl`) |
| Malicious paste | Text copied from a web page that hides commands or escape sequences |
| Third-party plugins | A plugin trying to read the screen, the clipboard or the network without permission |
| Tampered packages | An archive altered on the CDN, or a compromised publisher account |
| Local processes | Another user's process trying to drive the terminal through the IPC socket |
| AI agents | An agent that reads output it cannot trust, and is told by it to type into another pane through the MCP server (prompt injection) |
| The update channel | A feed or a DMG altered on its host, or a signing key stolen, to install another app over Lokyr's |

Out of scope: a process running as the same user with full privileges. It could read the terminal's memory anyway.

## Terminal defaults

| Feature | Default | Why |
|---|---|---|
| Clipboard read (OSC 52 with `?`) | Off; `[clipboard] read = "ask"` asks with an alert at each read, whose default button refuses | A remote program could read the clipboard |
| Clipboard write (OSC 52) | On, up to 1 MiB | tmux, Neovim and similar tools need it over ssh |
| Title report (CSI 21 t) | Off | The classic way to inject text into the shell's input |
| Rectangle checksums (DECRQCRA) | Off; test suites such as esctest turn them on, as iTerm2 asks | A program could read, a cell at a time, what others showed on the screen |
| Window operations (XTWINOPS 1 to 4 t, 8 to 11 t, 13 t, 15 t and 19 t, DECSLPP, DECSNLS, DECCOLM) | Off, which is stricter than xterm: its `disallowedWindowOps` lets programs move and resize the window, and read these reports, by default, and DECCOLM needs only mode 40, which programs set themselves | A program could resize, move or iconify the window at will, clear the screen with DECCOLM, and learn where the window is and how large the display is; with the operations on, a side that changes takes 1000 cells at most |
| Report replies | Well-formed data only, never program-controlled text | Prevents injection through replies |
| Pasted text | Control characters stripped, ESC and the rest of C0, DEL and C1, except tab and line breaks, with or without bracketed paste | Stops a paste from ending bracketed paste from the inside (`\e[201~`), or from sending ESC to an editor |
| A paste of several lines into a program that does not ask for bracketed paste | Asks for confirmation, with the text's first lines as the program would get them; a single line with a line break after it pastes without the line break. `[clipboard] confirm-paste` asks always, or never | Avoids running commands by accident, as a line break reaches the program as Return |
| Links: OSC 8 hyperlinks, and the URLs and the paths of the text | `http`, `https`, `mailto` and `file` only, checked by the host for every plugin (a key of the config to change them comes later); a `file` URL of another machine refused; a URL or a file asks first, showing its target in full, unless `[links]` says otherwise; opened with ⌘-click, the link that would open underlined under the pointer while ⌘ is held, and the target shown on hover later in F3; a file that would run rather than open, as a program, an application, an installer, or a file that a terminal or another runner of documents would open, shown in the Finder, selected, as the file that a symbolic link or an alias leads to; decided in the portable layer, `app-core`, from what the platform tells of the target, and decided again as a question's answer comes, the platform's own open refusing a program too | No schemes that launch other apps, and no click that runs a program that the screen named |
| What the system opens with Lokyr as the default terminal: `ssh` and `x-man-page` links, scripts and executables | Nothing until the user chooses Lokyr › Make Lokyr the Default Terminal, which macOS confirms. An `ssh` link, which any web page can give, runs `ssh`, as the app's `PATH` finds it, of the link's port, of digits alone, then `--` and its user and host, each of the characters of a name, letters, digits, `.`, `_` and `-`, not from `-` or `.`, or an IPv6 address in brackets, never through a shell; a link of anything else opens nothing. A man page's link opens its page as Open man Page does, of a page's name alone. A script runs in the user's login shell of its path in single quotes, refused of a backslash or a control character, which a shell might read otherwise; a file of no script's extension runs only if it is executable | No link that runs an option of `ssh`, as `-oProxyCommand`, or a shell's command |
| Notifications from sequences (OSC 9, 777, 99) | Only when the window is not focused, one every 2 seconds for each window at most, their title and text cut at 256 and 2,048 characters | Avoids spam |
| OSC/DCS/APC payloads | Size limits per type, 64 KiB, and per OSC command where one needs more, 1 MiB for OSC 52; a memory quota for images (F5c) | Avoids memory exhaustion |
| Secure Keyboard Entry (macOS) | Automatic while the terminal of the focused window echoes nothing in canonical mode, as a password's prompt sets it, which its output and each key tell, never a timer; not in raw mode, as vim's, which echoes itself; Lokyr › Secure Keyboard Entry keeps it on until Lokyr quits; never while another app is active; a 🔒 before the focused window's title while it is on | Stops other apps from reading keystrokes |

## Plugins

- **Capabilities** are declared by the plugin, granted by the user, stored in `permissions.toml`, and revocable ([PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md#capabilities)). So are the commands that a plugin runs, each of which needs the capabilities of its action, so that no command is a way around them; the environment's variables, the typing, the shell's history and the panes' processes have capabilities of their own ([ADR 0019](adr/0019-commands-errors-and-permissions.md)).
- **Luau sandbox**: one VM per plugin, with only the libraries that compute, a memory limit and a deadline per call. A plugin that fails or misses its deadline is disabled.
- **Sources only**: packages carry Luau sources, never bytecode, and the host compiles every script itself.
- **Audit log**: capability requests and grants are recorded and can be reviewed.
- **Networking**, from F4: a plugin has none of its own; its requests go to the app, which checks their capability and their hosts, and logs them, then to a helper process, `sleip net-helper`, of the Sleip that the bundle holds, which enforces the allowed hosts too and parses the responses outside the app's process. Lokyr's own code has no HTTP client; Sparkle's updater is the app's one network client ([ADR 0026](adr/0026-app-updates.md)).
- **First-party plugins** are our own code, reviewed and limited to the SDK.

## Packages and the registry

- **Integrity**: Sleip verifies ed25519 signatures and SHA-256 checksums. The app re-checks the hash when it loads a plugin.
- **The registry's keys**, as [ADR 0031](adr/0031-registry-v0.md) decided: Sleip holds the public part of the registry's root key, offline, which signs the list of the index keys; it refuses an index file that a key of the list did not sign, or older than the one it holds, so that neither the index's host taken over nor an index key stolen makes it install what the registry did not publish.
- **Archives**: regular files only, no `..` or absolute paths, size limits and a compression-ratio limit.
- **Store and lockfile**: the store is content-addressed, and the lockfile is written atomically.
- **Names and versions**: `owner/name` namespaces with `lokyr/*` reserved, yanked versions, and security advisories (F7).
- **Provenance**: in v0 the registry packages plugins from their tagged sources, so every artifact traces back to a commit.

Details in [PACKAGE-MANAGER.md](PACKAGE-MANAGER.md).

## IPC

IPC comes in F3b ([ROADMAP](ROADMAP.md#f3b--mcp-and-agents)).

- **Socket location**: a Unix socket in a directory with 0700 permissions (`$XDG_RUNTIME_DIR/lokyr/` on Linux, `$TMPDIR/lokyr-<uid>/` on macOS).
- **Who can connect**: every connection is checked against the peer's credentials (`getpeereid`). Only the same user is accepted.
- **Sensitive commands**: reading the scrollback or sending text to a pane requires `ipc.allow-sensitive = true` or an explicit confirmation.
- **Child processes** receive `LOKYR_SOCKET`, so scripts can use the CLI, and `LOKYR_PANE_ID`, their pane's.

## MCP

The MCP server lets AI agents drive the terminal ([ADR 0017](adr/0017-mcp-and-agents.md)).

- **Transport**: `lokyr +mcp` speaks MCP over stdio. The agent starts it, and it reaches the app through the IPC socket, with the socket's checks. The app opens no port and speaks no HTTP.
- **An agent's scope**:
  - by default, its own pane, the one whose program it descends from, and the panes that it opened;
  - `lokyr +mcp` names its pane from `LOKYR_PANE_ID`, and the app checks it: the socket's peer process must descend from that pane's program;
  - reading or typing into another pane asks the user first, a pane at a time, with the agent's name, as MCP's `initialize` gives it, and the pane's.
- **The config**: the `[mcp]` section widens the scope, or turns the server off.
- **The agents' files**: the app never writes them. The lines that their hooks need are in the documents, and `lokyr +agent-hooks <agent>` prints them.

## Robustness

- **Panics**:
  - The release profile uses `panic = "unwind"`, with `catch_unwind` at the boundary of native plugins. A faulty plugin is disabled while the terminal keeps running.
  - A panic inside the core is still a bug to fix, not a handled case.
- **Crashes**:
  - A panic writes its backtrace to the app's log file, `~/Library/Logs/Lokyr/lokyr.log` on macOS, with the version, the time and the thread, before the process ends or its plugin is disabled; the app's standard error goes there too when no terminal reads it, as from the Finder. macOS's own crash reports keep the others, which `cargo xtask symbolicate` reads with the release's dSYM. Help › Report an Issue opens a new issue of the repository with the version and the system's.
  - An out-of-process crash handler (e.g. `crash-handler` + `minidumper`) writes a minidump, later.
  - dSYMs are archived for every release, for symbolication.
  - Nothing is uploaded automatically: the user decides whether to attach a minidump to a report.
- **Sessions**: windows, tabs, layouts and cwds are saved as Lokyr quits, in the state directory, so that they open again after an update or a restart of the Mac, as `[app] restore-windows` says; a crash or a force quit restores those of the quit before, as the user chose on 8 November 2026. A file of a newer format's version, or that does not read, is left aside with a notice.
- **Config**: reloads are all-or-nothing ([ADR 0009](adr/0009-toml-config.md)). If the new config has errors, the previous one stays active and a notice shows the file, line and column.

## macOS environment

- **Locale**: when the app is launched from the Dock, `LANG` is not set. In that case we set a UTF-8 locale that matches the system preferences, of the first of the user's languages that macOS has a UTF-8 locale of: the first of the user's regions of it that macOS has, as `it_CH.UTF-8`; otherwise the language's own region, as `it_IT`, and `en_US` for English; otherwise its first region that macOS has, as `ja_JP` for Japanese in Switzerland. A later language comes only after: English in Switzerland, then Italian in Switzerland, gives `en_US.UTF-8`, and English in Switzerland, then in the United Kingdom, `en_GB.UTF-8`. Without any, `en_US.UTF-8`.
- **`PATH`**: the bundle's `Contents/MacOS`, the directory of the app's own executable, `lokyr`, at the end of the panes' `PATH`, unless it has it: the app's command line runs in a pane as `lokyr`, and no command of the user's is shadowed by it.
- **Login shell**: started the way Terminal.app does, through `login` (`/usr/bin/login -flp $USER /bin/bash --noprofile --norc -c "exec -l -a <name> <shell>"`).
- **TCC**:
  - Permissions requested by child processes (microphone, camera, Apple Events, protected folders, …) are attributed to the terminal.
  - The bundle therefore includes the required usage strings in `Info.plist`, worded like "A program started in the terminal wants to use…". Examples:
    - `NSMicrophoneUsageDescription`, `NSCameraUsageDescription`, `NSAppleEventsUsageDescription`;
    - `NSDesktopFolderUsageDescription`, `NSDocumentsFolderUsageDescription`, `NSDownloadsFolderUsageDescription`;
    - `NSRemovableVolumesUsageDescription`, `NSNetworkVolumesUsageDescription`.
- **Signing**:
  - Hardened Runtime and notarization.
  - The hardened runtime's entitlements of the resources that the child processes ask for, without which it refuses them with no prompt: `com.apple.security.automation.apple-events`, `com.apple.security.device.audio-input` and `com.apple.security.device.camera`, and `com.apple.security.personal-information.addressbook`, `.calendars`, `.location` and `.photos-library`. They let a program ask; macOS still asks the user, with the usage strings above.
  - One hardened-runtime exception, `com.apple.security.cs.allow-jit`, for Luau's native code generator ([ADR 0012](adr/0012-luau-plugins.md)). Without it, plugins run in Luau's interpreter.
  - No App Sandbox, which is incompatible with a terminal.

## Supply chain of our own code

- `cargo-deny` checks advisories, allowed licenses, banned crates and allowed sources.
- The toolchain is pinned in `rust-toolchain.toml`, and `Cargo.lock` is committed.
- Release builds come from `just release`, on the maintainer's Mac, which holds the signing identities and the updates' key, from a commit that CI checked; each carries an SBOM (e.g. `cargo-cyclonedx`) and the notices of its third-party licenses.
- **Updates** ([ADR 0026](adr/0026-app-updates.md), [ADR 0029](adr/0029-update-feed-and-releases.md)): the feed, at `https://lokyr.dev/appcast.xml`, is signed with EdDSA, whose private key stays in the maintainer's keychain, with a copy offline; Sparkle checks that signature and Apple's of the new app before it replaces the old one, and reaches only the feed's and the releases' addresses. Each release's dSYMs are kept, to read its crashes.
