# Notifications

<sub>macOS · on by default · [`[notifications]`](CONFIGURATION.md#notifications), [`[agents]`](CONFIGURATION.md#agents), [`[bell]`](CONFIGURATION.md#bell)</sub>

Lokyr lets you know when something in a terminal needs you while you work elsewhere: a program that sends a notification, an AI agent that waits for you or is done, a long command that ends, or a bell. It uses macOS's own notifications, and a click on one brings you back to the pane that sent it.

## Try it

Run this command, then switch to another app within three seconds:

```sh
sleep 3; printf '\033]777;notify;Build;Finished in 42 seconds\033\\'
```

A notification shows "Build" and "Finished in 42 seconds". Click it: Lokyr comes forward with the pane that sent it. The first time, macOS asks whether Lokyr may show notifications.

## What it gives you

- **Programs' notifications**: a program, a script or a build tool can send one, as [Notification codes](escape-codes/notifications.md) describes.
- **AI agents**: a notification when an agent waits for you or is done, and a count of the agents that wait on the Dock's icon, as [AI agents](agents.md) describes. [`[agents] notify`](CONFIGURATION.md#notify) and [`dock-badge`](CONFIGURATION.md#dock-badge) turn them off.
- **Long commands**: a notification when a command that ran for 10 seconds or more ends, its command line as its title, with [shell integration](shell-integration.md). [`[notifications] long-command`](CONFIGURATION.md#long-command) sets how long.
- **The bell**: when a program rings the bell, the window flashes if it has the focus, the Dock's icon bounces once if Lokyr is in the background, and a window in the background shows 🔔 before its title. [`[bell]`](CONFIGURATION.md#bell) sets each, and plays the system's alert sound too.
- **A click on a notification** brings forward the pane that sent it, with its tab and its window.

## Allow Lokyr's notifications

macOS asks the first time that Lokyr shows a notification. To change your answer later, open System Settings › Notifications › Lokyr and turn on Allow notifications. There, you can also choose banners or alerts, and whether they show on the lock screen.

## How Lokyr shows them

- **Only while the window is in the background**: a notification from the window you're looking at would tell you nothing new, so it shows none.
- **One every two seconds at most** from a window's programs: a program that sends several in a burst shows the first.
- **The title** is the one that the program sends, or else the window's title. A long title or text shows its start.

## Settings

```toml
[notifications]
long-command = 10      # seconds a command runs before its end is notified; 0 turns it off

[agents]
notify = true          # an agent's notifications
dock-badge = true      # the Dock's count of the agents that wait

[bell]
sound = false          # the system's alert sound
flash = true           # the window flashes while it has the focus
bounce = true          # the Dock's icon bounces while Lokyr is in the background
mark-title = true      # 🔔 before the title of a window in the background
```

[The configuration guide](CONFIGURATION.md#notifications) tells each key's values.

## How it works

A program sends a notification by printing an escape code, OSC 9, OSC 777 or OSC 99, which Lokyr reads and never shows. [Notification codes](escape-codes/notifications.md) describes each, with what to print. Lokyr hands the notification to macOS, which shows it as it shows every app's.

## Limits

- **No buttons, icons or sounds of a program's own**: Lokyr shows a notification's title and text.
- **No notification while the window has the focus**, whichever kind it is.
- **Focus modes**, such as Do Not Disturb, hold Lokyr's notifications as they hold every app's.

## Troubleshooting

### No notification shows

**Why**: Lokyr isn't allowed to show notifications, the window had the focus, a Focus mode holds them, or the window showed another less than two seconds before.

**Fix**: allow Lokyr in System Settings › Notifications › Lokyr, turn off the Focus mode, and switch to another app before the notification comes.

**Check**: run the command of [Try it](#try-it), then switch to another app: the notification shows.

### A click on a notification opens a second Lokyr

**Why**: macOS sends the click to an app of Lokyr's identity, and another copy of Lokyr on your Mac, such as one left in Downloads, can take it.

**Fix**: keep one copy of Lokyr, in Applications, and move the others to the Trash.

**Check**: click a new notification: the Lokyr that you use comes forward, and no second icon shows in the Dock.

## See also

- [Notification codes](escape-codes/notifications.md)
- [AI agents](agents.md)
- [Shell integration](shell-integration.md)
- [Progress and status](progress-and-status.md)
