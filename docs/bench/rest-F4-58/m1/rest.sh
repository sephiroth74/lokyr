#!/bin/sh
# Whether the Mac is at rest for the measures: its load, its busiest
# processes, Spotlight's indexing, the screen's lock and a Defender's.
uptime
ps -Ao pcpu,comm -r | sed -n 2,6p
mdutil -s / 2>/dev/null | tail -1
ioreg -n Root -d1 -a 2>/dev/null | grep -A1 CGSSessionScreenIsLocked | tail -1 | grep -q true && echo "the screen is locked" || echo "the screen is unlocked"
pgrep -fl "wdav|Defender|jamf" | head -3 || true
sw_vers -productVersion
sysctl -n machdep.cpu.brand_string
