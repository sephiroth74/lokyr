#!/bin/sh
# The bundles of F3-35's session, of the commit checked out: `h`, the app as
# it ships; `w`, the same with `--features sample-worker`, a plugin on a
# thread of its own whose status bar renders without pause, each render
# reading 24 rows of the focused pane, a request each.
set -eu
B=/tmp/lokyr/bench-F3-35
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
mkdir -p $B
set -a
. $REPO/private/lokyr.env
set +a
build() {
  name=$1
  shift
  rm -rf $B/$name
  mkdir -p $B/$name
  (cd $REPO && cargo xtask bundle "$@" > $B/build-$name.log 2>&1)
  cp -R $REPO/target/Lokyr.app $B/$name/
  echo "$name: $($B/$name/Lokyr.app/Contents/MacOS/lokyr --version)"
}
build h
build w --features sample-worker
