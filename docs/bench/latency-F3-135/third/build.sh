#!/bin/sh
# The bundles of the third session, each a release of one commit of the
# work since 0.4.0, built in a worktree of its own: `m2`, the slots in the
# windows; `m3`, the live resize's pace and the view's contents placement;
# `t`, the head but for that placement, the view's layer resized to its
# bounds as before. The head's own and 0.4.0's are the second session's.
set -eu
B=/tmp/lokyr/bench-F3-135c
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
mkdir -p $B
if [ ! -d $B/src ]; then
  git -C $REPO worktree add --detach $B/src HEAD
fi
mkdir -p $B/src/target
cp -R $REPO/target/sparkle $REPO/target/terminfo $B/src/target/
set -a
. $REPO/private/lokyr.env
set +a
build() {
  name=$1
  rm -rf $B/$name
  mkdir -p $B/$name
  (cd $B/src && cargo xtask bundle > $B/build-$name.log 2>&1)
  cp -R $B/src/target/Lokyr.app $B/$name/
  echo "$name: $($B/$name/Lokyr.app/Contents/MacOS/lokyr --version)"
}
git -C $B/src checkout --quiet --detach 8661a20
build m2
git -C $B/src checkout --quiet --detach 7ae3d41
build m3
git -C $B/src checkout --quiet --detach 0acf7fd
sed -i '' '/setLayerContentsPlacement(NSViewLayerContentsPlacement::TopLeft);/d' $B/src/crates/platform-macos/src/window.rs
build t
git -C $B/src checkout --quiet -- .
