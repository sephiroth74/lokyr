# AI agents in the terminal

Lokyr knows the AI coding agents that run in its panes, as Claude Code, Codex, Gemini CLI or Aider, and shows on each tab what its agent is doing: at work, waiting for you, or done. You see at a glance which of your agents needs you, across every tab and window, with nothing to set up.

## What a tab shows

Lokyr recognizes an agent by its program in the pane's foreground: Claude Code, Codex, Gemini CLI, OpenCode, Aider, Cursor's agent, Copilot, Amp, Goose, Qwen, Crush and Droid, as [`[agents] programs`](CONFIGURATION.md#programs) lists them, and any program that you add there. Its tab then shows one of three marks:

- **At work**, a sparkle: while the agent prints of its own, apart from the echo of your keys, tells a progress, or turns its title's spinner.
- **Waiting for you**, a dot: once it asks for you, by a notification of its own or its bell, as when it asks for a permission, and until you type into its pane. The tab's tooltip tells what it asked, as "Claude needs your permission to use Bash", and in a tab of several panes a ring goes round the pane that waits.
- **Done**, a tick: once it shows no work for a while after you asked it something, until it works again, you type into its pane, or you look at its tab.

A tab of several agents shows the most urgent: one that waits before one at work, and one at work before one done.

## Finding the agent that needs you

- **Notifications**: while its window has no focus, an agent that waits for you or is done tells it in a notification of the system's, of its program and its tab's title. A click on it brings the agent's pane forward, with its tab and its window.
- **The Dock**: Lokyr's icon counts the agents that wait for you, in every window.
- **Go to Next Agent**, Window › Go to Next Agent or ⌥⌘A, brings forward the next agent that waits for you, or, of none, one that is done, across every window.
- **Show Agents**, Window › Show Agents… or ⇧⌥⌘A, lists every agent in the command palette, the most urgent first, each with what it asked or its tab, its state and how long it has been in it; Return brings it forward.

## Claude Code's exact state

Without any setup, Lokyr reads an agent's state from its output, its quiet, its notifications and its bell. For Claude Code it can know exactly: Lokyr › Install Claude Code Integration… adds hooks to Claude Code's settings, asking before it changes each of its configurations and keeping a copy of the file as it was. Claude Code then tells its tab when you submit a request, when it asks for a permission, when it ends its answer, and when an error stops it, which shows as a red mark of its own.

The hooks do nothing in another terminal. To take them away, remove the groups whose commands hold `lokyr-agent:` from the `hooks` of Claude Code's `settings.json`; every detail is in [the config guide](CONFIGURATION.md#agents).

## Opening what an agent points to

Agents, compilers and tests print paths with a line, as `src/main.rs:42:7`. Hold ⌘ over one and click: Lokyr opens the file at that line in the editor that [`[links] editor`](CONFIGURATION.md#editor) names, as VS Code, Cursor, Windsurf, Zed, Sublime Text or TextMate.

## Settings

The `[agents]` section of the config tunes it all, each key as [the config guide](CONFIGURATION.md#agents) tells:

```toml
[agents]
programs = ["claude", "codex", "gemini", "aider"]   # the agents' programs; an empty list turns this off
done-after = 3                                       # seconds of no work before an agent is done
notify = true                                        # the notifications while its window has no focus
dock-badge = true                                    # the Dock's count of the agents that wait
```

## Next: Lokyr as an MCP server

In version 1.2, Lokyr becomes a server of the Model Context Protocol: an agent starts `lokyr +mcp` and can open a pane of its own, run a command in it, and read its output and its exit code, within its own panes, any other after you confirm. The [roadmap](ROADMAP.md#f3b--mcp-and-agents) tells what comes.
