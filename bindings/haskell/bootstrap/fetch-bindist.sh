#!/usr/bin/env bash
#
# Fetch the prebuilt libpanproto_c artifact for the host platform from
# the panproto GitHub Releases.
#
# Usage:
#   ./bootstrap/fetch-bindist.sh [version]
#
# version defaults to the @VERSION@ string baked into this script by
# the release process. For development builds, pass `local` to use the
# in-tree `target/release` artifacts produced by
# `cargo build -p panproto-c --release`.
#
# After this script runs, the resulting libpanproto_c.{a,so,dylib,lib}
# is placed under .panproto-c/<target>/lib/ and the header under
# .panproto-c/<target>/include/. Add this directory to cabal via
# `extra-lib-dirs:` (already configured in cabal.project for `local`).

set -euo pipefail

VERSION="${1:-v0.40.0}"
DEST=".panproto-c"

# Detect target.
case "$(uname -sm)" in
  "Darwin arm64")   TARGET="aarch64-apple-darwin" ;;
  "Darwin x86_64")  TARGET="x86_64-apple-darwin" ;;
  "Linux x86_64")   TARGET="x86_64-unknown-linux-gnu" ;;
  "Linux aarch64")  TARGET="aarch64-unknown-linux-gnu" ;;
  *) echo "unsupported platform: $(uname -sm)" >&2; exit 1 ;;
esac

if [ "$VERSION" = "local" ]; then
  REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
  SRC="$REPO_ROOT/target/release"
  if [ ! -f "$SRC/libpanproto_c.a" ] && [ ! -f "$SRC/libpanproto_c.dylib" ] \
       && [ ! -f "$SRC/libpanproto_c.so" ]; then
    echo "no libpanproto_c artifact under $SRC; run \`cargo build -p panproto-c --release\`" >&2
    exit 1
  fi
  mkdir -p "$DEST/$TARGET/lib" "$DEST/$TARGET/include"
  cp "$REPO_ROOT/crates/panproto-c/include/panproto.h" "$DEST/$TARGET/include/"
  for f in libpanproto_c.a libpanproto_c.dylib libpanproto_c.so panproto_c.lib panproto_c.dll; do
    [ -f "$SRC/$f" ] && cp "$SRC/$f" "$DEST/$TARGET/lib/"
  done
  echo "wired $DEST/$TARGET from local build"
  exit 0
fi

ARCHIVE="panproto-c-$TARGET.tar.gz"
URL="https://github.com/panproto/panproto/releases/download/$VERSION/$ARCHIVE"

mkdir -p "$DEST"
echo "fetching $URL"
curl -fsSL "$URL" -o "$DEST/$ARCHIVE"
tar -xzf "$DEST/$ARCHIVE" -C "$DEST"
rm "$DEST/$ARCHIVE"
echo "extracted to $DEST/panproto-c-$TARGET"
