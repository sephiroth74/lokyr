#!/bin/sh
# The bundle of the midpoint of the work between the head that F3-135's
# third session measured, 0acf7fd, and 0.5.0: `m`, the last commit before
# the window's motion's work, of the plugins' panels and their
# accessibility, the plugins' threads, their config and state, the
# distributions, the fake host and the conformance suite, built as a
# release of the plain features in a worktree of its own.
set -eu
B=/tmp/lokyr/bench-F3-163
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
COMMIT=$(git -C $REPO rev-parse --short c905193^)
mkdir -p $B
if [ ! -d $B/src ]; then
  git -C $REPO worktree add --detach $B/src $COMMIT
fi
git -C $B/src checkout --quiet --detach $COMMIT
mkdir -p $B/src/target
cp -R $REPO/target/sparkle $REPO/target/terminfo $B/src/target/
set -a
. $REPO/private/lokyr.env
set +a
rm -rf $B/m
mkdir -p $B/m
(cd $B/src && cargo xtask bundle > $B/build-m.log 2>&1)
cp -R $B/src/target/Lokyr.app $B/m/
echo "m, of $COMMIT: $($B/m/Lokyr.app/Contents/MacOS/lokyr --version)"
