#!/bin/sh
# The homes of the published plugins' footprint: `home-none`, of no
# plugin, and `home-two`, of `sephiroth74/snap` and `sephiroth74/awake`,
# installed from pkg.lokyr.dev by the Sleip of the app given, or of
# `$SLEIP`, `sleip add`, their capabilities granted in the home's
# `permissions.toml`, as the user's answers to Lokyr's questions would, so
# that no question waits at the start. `awake` activates as the app starts,
# of its button in the titlebar; `snap` as its command first runs, so that
# it stays installed and asleep. The app then started of each home, `HOME`
# and `XDG_CONFIG_HOME` its own, tells their footprint as the difference.
#
#     plugins.sh <Lokyr.app> <directory>
set -eu
APP=$1
OUT=$2
SLEIP=${SLEIP:-$APP/Contents/MacOS/sleip}
for home in none two; do
  rm -rf "$OUT/home-$home"
  mkdir -p "$OUT/home-$home/.config/lokyr"
  cp "$(dirname "$0")/config.toml" "$OUT/home-$home/.config/lokyr/config.toml"
done
home=$OUT/home-two
for plugin in sephiroth74/snap sephiroth74/awake; do
  HOME=$home XDG_CONFIG_HOME=$home/.config "$SLEIP" add "$plugin"
done
cat > "$home/.config/lokyr/permissions.toml" <<'TOML'
# The capabilities that the installed plugins hold, and those refused, as
# the user answered Lokyr's questions. Remove one to be asked again.

[plugins."sephiroth74/awake"]
granted = ["keep-awake", "process-read"]
refused = []

[plugins."sephiroth74/snap"]
granted = ["clipboard-write", "process-read", "terminal-read"]
refused = []
TOML
HOME=$home XDG_CONFIG_HOME=$home/.config "$SLEIP" list
