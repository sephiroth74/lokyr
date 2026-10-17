# Recorded sessions

Each recording here is a real program, run in a pseudo-terminal and driven by a script, as asciicast v2: `vim`, `htop`, `tmux`, `less` and `fzf`. The tests replay them through the VT core, and look at the screen at their markers and at their end.

- **A script** (`*.script`) sets the program up, with `size`, `copy` (a file of `files/` into the program's directory), `env` and `run`, then drives it, with `wait "text"` (until the screen shows it), `quiet` (until the output stops for a while), `sleep`, `send "keys"` (once the program takes them), `mark name` and `exit`; `wait`, `quiet`, `send` and `exit` take a limit in milliseconds, ten seconds by default, and a mark's name is its own, neither `exit` nor with a `/`. The scripts say what each recording shows.
- **Snapshots** (`snapshots/`): `tools/replay/tests/goldens.rs` replays each recording and keeps the screen, its text, the attributes of its cells and the cursor, as `bolt-replay --attributes` prints them, with the terminal's replies since the point before, at each marker and at the end, `exit`, as `insta` snapshots. A change of the terminal that changes a screen fails them: `cargo insta review` (from `cargo install cargo-insta`) shows each change, or `INSTA_UPDATE=always cargo test -p bolt-replay --test goldens` writes them all, for `git diff` to show.
- **Recording again**: `just record` runs every script with `bolt-record`, which needs the five programs installed, and writes each recording next to its script. A recording changes with the programs' versions, which the tests' snapshots then show.
- **What the programs see**: the session's terminal, which answers their queries, `TERM=xterm-256color`, and no environment but a few variables, in a directory of their own that is also their home, so that no user's configuration plays a part. `htop` shows itself alone, without the user's column, from `files/htoprc`, and `tmux` a status line without the host or the time, from `files/tmux.conf`.
- **Nothing private**: the recorder refuses to write a recording that shows the user's name or full name, the host's or the computer's name, the user's home, the temporary directory, when it is the user's own, as on macOS, or the program's own home, whose path is the machine's. It looks on the screen after each read and, once the program is done, in:
  - the bytes that the program wrote, and those typed into it, as they are;
  - the text that the VT core's parser prints from the output, without its escape sequences;
  - the payloads of the OSC, DCS and APC strings, as they are and with their percent-encoding undone;
  - the titles and the working directory;
  - every row of both screens, the scrollback's too.

  A name that the recorder cannot look up, as when `hostname` fails, is said on its standard error and not checked. So are names shorter than 3 characters, which would match too much. The machine's cores, load, memory and uptime, in `htop`'s header, are not private.
