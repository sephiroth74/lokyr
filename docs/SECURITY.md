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

Out of scope: a process running as the same user with full privileges. It could read the terminal's memory anyway.

## Terminal defaults

| Feature | Default | Why |
|---|---|---|
| Clipboard read (OSC 52 with `?`) | Off; can be enabled with confirmation | A remote program could read the clipboard |
| Clipboard write (OSC 52) | On, up to 1 MiB | tmux, Neovim and similar tools need it over ssh |
| Title report (CSI 21 t) | Off | The classic way to inject text into the shell's input |
| Report replies | Well-formed data only, never program-controlled text | Prevents injection through replies |
| Bracketed paste | ESC and C0 controls (except tab and newline) stripped from pasted text | Stops a paste from ending bracketed paste from the inside (`\e[201~`) |
| Multi-line paste without bracketed paste | Asks for confirmation | Avoids running commands by accident |
| OSC 8 hyperlinks | `http`, `https`, `mailto` and `file` only (configurable); the target is shown on hover; opened with ⌘-click or Ctrl-click | No schemes that launch other apps |
| Notifications from sequences (OSC 9, 777, 99) | Only when the window is not focused, rate-limited | Avoids spam |
| OSC/DCS/APC payloads | Size limits per type; a memory quota for images (F5c) | Avoids memory exhaustion |
| Secure Keyboard Entry (macOS) | Automatic when the PTY has ECHO off (password prompts), plus a manual command; a visible indicator | Stops other apps from reading keystrokes |

## Plugins

- **Capabilities** are declared by the plugin, granted by the user, stored in `permissions.toml`, and revocable ([PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md#capabilities)).
- **Luau sandbox**: one VM per plugin, with only the libraries that compute, a memory limit and a deadline per call. A plugin that fails or misses its deadline is disabled.
- **Sources only**: packages carry Luau sources, never bytecode, and the host compiles every script itself.
- **Audit log**: capability requests and grants are recorded and can be reviewed.
- **Networking** goes through a helper process (`faretra net-helper`) that enforces the allowed hosts. The app has no HTTP client.
- **First-party plugins** are our own code, reviewed and limited to the SDK.

## Packages and the registry

- **Integrity**: Faretra verifies ed25519 signatures and SHA-256 checksums. The app re-checks the hash when it loads a plugin.
- **Archives**: regular files only, no `..` or absolute paths, size limits and a compression-ratio limit.
- **Store and lockfile**: the store is content-addressed, and the lockfile is written atomically.
- **Names and versions**: `owner/name` namespaces with `saetta/*` reserved, yanked versions, and security advisories (F7).
- **Provenance**: in v0 the registry packages plugins from their tagged sources, so every artifact traces back to a commit.

Details in [PACKAGE-MANAGER.md](PACKAGE-MANAGER.md).

## IPC

- **Socket location**: a Unix socket in a directory with 0700 permissions (`$XDG_RUNTIME_DIR/saetta/` on Linux, `$TMPDIR/saetta-<uid>/` on macOS).
- **Who can connect**: every connection is checked against the peer's credentials (`getpeereid`). Only the same user is accepted.
- **Sensitive commands**: reading the scrollback or sending text to a pane requires `ipc.allow-sensitive = true` or an explicit confirmation.
- **Child processes** receive `SAETTA_SOCKET`, so scripts can use the CLI.

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

- **Locale**: when the app is launched from the Dock, `LANG` is not set. In that case we set a UTF-8 locale that matches the system preferences.
- **Login shell**: started the way Terminal.app does (`/usr/bin/login -flp $USER <shell>`).
- **TCC**:
  - Permissions requested by child processes (microphone, camera, Apple Events, protected folders, …) are attributed to the terminal.
  - The bundle therefore includes the required usage strings in `Info.plist`, worded like "A program started in the terminal wants to use…". Examples:
    - `NSMicrophoneUsageDescription`, `NSCameraUsageDescription`, `NSAppleEventsUsageDescription`;
    - `NSDesktopFolderUsageDescription`, `NSDocumentsFolderUsageDescription`, `NSDownloadsFolderUsageDescription`;
    - `NSRemovableVolumesUsageDescription`, `NSNetworkVolumesUsageDescription`.
- **Signing**:
  - Hardened Runtime and notarization.
  - One hardened-runtime exception, `com.apple.security.cs.allow-jit`, for Luau's native code generator ([ADR 0012](adr/0012-luau-plugins.md)). Without it, plugins run in Luau's interpreter.
  - No App Sandbox, which is incompatible with a terminal.

## Supply chain of our own code

- `cargo-deny` checks advisories, allowed licenses, banned crates and allowed sources.
- The toolchain is pinned in `rust-toolchain.toml`, and `Cargo.lock` is committed.
- Release builds come from CI, with an SBOM attached (e.g. `cargo-cyclonedx`).
