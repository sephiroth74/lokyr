#!/bin/sh
# The homes of the Luau plugins' footprint: `home-0`, of no plugin, and
# `home-10`, of ten plugins of the template that `sleip new` makes, each of
# a name and a namespace of its own, activated as the app starts, installed
# by the Sleip of the app given, or of `$SLEIP`, `sleip add --path`, into the home's store
# and config. The app then started of each home, `HOME` and
# `XDG_CONFIG_HOME` its own, tells a plugin's footprint as the difference.
#
#     plugins.sh <Lokyr.app> <directory>
set -eu
APP=$1
OUT=$2
SLEIP=${SLEIP:-$APP/Contents/MacOS/sleip}
for count in 0 10; do
  home=$OUT/home-$count
  rm -rf "$home"
  mkdir -p "$home/.config/lokyr"
  cp "$(dirname "$0")/config.toml" "$home/.config/lokyr/config.toml"
done
sources=$OUT/plugins
rm -rf "$sources"
mkdir -p "$sources"
cd "$sources"
HOME=$OUT/home-10 XDG_CONFIG_HOME=$OUT/home-10/.config "$SLEIP" new acme/hello > /dev/null
n=0
while [ $n -lt 10 ]; do
  folder=$sources/hello$n
  cp -R "$sources/hello" "$folder"
  # Its own name and namespace, and no `[activation]`, so that it
  # activates as the app starts.
  sed -e "s/acme\/hello/acme\/hello$n/" -e "s/hello\.say/hello$n.say/" \
    -e '/^\[activation\]/,/^on = /d' -e '/^# It activates/,/^# show before/d' \
    "$sources/hello/plugin.toml" > "$folder/plugin.toml"
  sed -e "s/hello\.say/hello$n.say/g" "$sources/hello/src/init.luau" > "$folder/src/init.luau"
  (cd "$folder" && HOME=$OUT/home-10 XDG_CONFIG_HOME=$OUT/home-10/.config "$SLEIP" add --path . > /dev/null)
  n=$((n + 1))
done
HOME=$OUT/home-10 XDG_CONFIG_HOME=$OUT/home-10/.config "$SLEIP" list
