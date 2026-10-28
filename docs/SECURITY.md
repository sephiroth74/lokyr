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
| A paste with a line break, a final one too, into a program that does not ask for bracketed paste | Asks for confirmation, with the text's first lines as the program would get them | Avoids running commands by accident, as a line break reaches the program as Return |
| OSC 8 hyperlinks | `http`, `https`, `mailto` and `file` only (configurable); the target is shown on hover; opened with ⌘-click or Ctrl-click | No schemes that launch other apps |
| Notifications from sequences (OSC 9, 777, 99) | Only when the window is not focused, one every 2 seconds for each window at most, their title and text cut at 256 and 2,048 characters | Avoids spam |
| OSC/DCS/APC payloads | Size limits per type, 64 KiB, and per OSC command where one needs more, 1 MiB for OSC 52; a memory quota for images (F5c) | Avoids memory exhaustion |
| Secure Keyboard Entry (macOS) | Automatic when the PTY has ECHO off (password prompts), plus a manual command; a visible indicator | Stops other apps from reading keystrokes |

## Plugins

- **Capabilities** are declared by the plugin, granted by the user, stored in `permissions.toml`, and revocable ([PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md#capabilities)). So are the commands that a plugin runs, each of which needs the capabilities of its action, so that no command is a way around them; the environment's variables, the typing, the shell's history and the panes' processes have capabilities of their own ([ADR 0019](adr/0019-commands-errors-and-permissions.md)).
- **Luau sandbox**: one VM per plugin, with only the libraries that compute, a memory limit and a deadline per call. A plugin that fails or misses its deadline is disabled.
- **Sources only**: packages carry Luau sources, never bytecode, and the host compiles every script itself.
- **Audit log**: capability requests and grants are recorded and can be reviewed.
- **Networking** goes through a helper process (`sleip net-helper`) that enforces the allowed hosts. The app has no HTTP client.
- **First-party plugins** are our own code, reviewed and limited to the SDK.

## Packages and the registry

- **Integrity**: Sleip verifies ed25519 signatures and SHA-256 checksums. The app re-checks the hash when it loads a plugin.
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
  - An out-of-process crash handler (e.g. `crash-handler` + `minidumper`) writes a minidump.
  - dSYMs are archived for every release, for symbolication.
  - Nothing is uploaded automatically: the user decides whether to attach a minidump to a report.
- **Sessions**: windows, tabs, layouts and cwds are saved periodically and on quit, so they can be restored after a crash or an update.
- **Config**: reloads are all-or-nothing ([ADR 0009](adr/0009-toml-config.md)). If the new config has errors, the previous one stays active and a notice shows the file, line and column.

## macOS environment

- **Locale**: when the app is launched from the Dock, `LANG` is not set. In that case we set a UTF-8 locale that matches the system preferences, of the first of the user's languages that macOS has a UTF-8 locale of: the first of the user's regions of it that macOS has, as `it_CH.UTF-8`; otherwise the language's own region, as `it_IT`, and `en_US` for English; otherwise its first region that macOS has, as `ja_JP` for Japanese in Switzerland. A later language comes only after: English in Switzerland, then Italian in Switzerland, gives `en_US.UTF-8`, and English in Switzerland, then in the United Kingdom, `en_GB.UTF-8`. Without any, `en_US.UTF-8`.
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
- Release builds come from CI, with an SBOM attached (e.g. `cargo-cyclonedx`).
