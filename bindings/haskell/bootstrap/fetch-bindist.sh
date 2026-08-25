#!/usr/bin/env bash
#
# Fetch the prebuilt libpanproto_c artifact for the host platform from
# the panproto GitHub Releases, then build the C glue layer and write
# cabal.project.local so `cabal build` finds the libraries without
# referencing paths outside the source tree.
#
# Usage:
#   ./bootstrap/fetch-bindist.sh [version]
#
# version defaults to the @VERSION@ string baked into this script by
# the release process. For local development against an in-tree
# `cargo build -p panproto-c --release`, use `dev-link.sh` instead.
#
# After this script runs, the resulting libpanproto_c.{a,so,dylib,lib},
# the header, and a freshly-built libpanproto_glue.a are placed under
# `bindings/haskell/.panproto-c/`, and an absolute-path
# `cabal.project.local` references that directory.

set -euo pipefail

cd "$(dirname "$0")/.."
HASKELL_DIR="$(pwd)"

VERSION="${1:-v0.72.0}"
DEST="$HASKELL_DIR/.panproto-c"

# Detect target.
case "$(uname -sm)" in
  "Darwin arm64")   TARGET="aarch64-apple-darwin" ;;
  "Darwin x86_64")  TARGET="x86_64-apple-darwin" ;;
  "Linux x86_64")   TARGET="x86_64-unknown-linux-gnu" ;;
  "Linux aarch64")  TARGET="aarch64-unknown-linux-gnu" ;;
  *) echo "unsupported platform: $(uname -sm)" >&2; exit 1 ;;
esac

ARCHIVE="panproto-c-$TARGET.tar.gz"
URL="https://github.com/panproto/panproto/releases/download/$VERSION/$ARCHIVE"

mkdir -p "$DEST/lib" "$DEST/include"

echo "fetching $URL"
curl -fsSL "$URL" -o "$DEST/$ARCHIVE"
tar -xzf "$DEST/$ARCHIVE" -C "$DEST"

# The release tarball ships under panproto-c-<target>/{lib,include}.
# Move the contents up one level so the staging layout matches what
# dev-link.sh produces.
EXTRACT="$DEST/panproto-c-$TARGET"
if [ -d "$EXTRACT" ]; then
    cp -f "$EXTRACT"/lib/* "$DEST/lib/"
    cp -f "$EXTRACT"/include/* "$DEST/include/"
    rm -rf "$EXTRACT"
fi
rm -f "$DEST/$ARCHIVE"

# Build the C glue layer locally. The glue (panproto_glue.c) is part
# of the Haskell binding, not the Rust workspace, so it is not in the
# release tarball; we always rebuild it against the just-fetched
# panproto.h.
echo "building panproto_glue.a..."
CC="${CC:-cc}"
GLUE_OBJ="$DEST/panproto_glue.o"
GLUE_LIB="$DEST/lib/libpanproto_glue.a"
"$CC" -c \
    -fPIC \
    -O2 \
    -Wall -Wextra \
    -I"$HASKELL_DIR/cbits" \
    -I"$DEST/include" \
    "$HASKELL_DIR/cbits/panproto_glue.c" \
    -o "$GLUE_OBJ"
ar rcs "$GLUE_LIB" "$GLUE_OBJ"
rm "$GLUE_OBJ"

echo "staged into $DEST/"
ls "$DEST/lib"

# Write an absolute path into cabal.project.local so cabal's
# configure step finds the lib and ghc-pkg accepts the path during
# registration. The .local file is gitignored.
cat > "$HASKELL_DIR/cabal.project.local" <<EOF
package panproto
    extra-lib-dirs: $DEST/lib
EOF
echo "wrote cabal.project.local with extra-lib-dirs"
