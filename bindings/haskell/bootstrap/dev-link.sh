#!/usr/bin/env bash
#
# Local-development helper: build panproto-c via cargo, build the
# C glue (`panproto_glue.c`) into a standalone static library, and
# stage everything under `bindings/haskell/.panproto-c/`. The cabal
# package picks them up from there without referencing paths outside
# its source tree.
#
# Why a standalone glue library rather than `c-sources` in the cabal
# file? GHC 9.12 plus a macOS arm64 toolchain that contains a stale
# `ld` (e.g. anaconda's bundled `/Users/<user>/opt/anaconda3/bin/ld`
# on a developer's PATH) fails the merge-objects pass for Haskell
# modules in packages that ship `c-sources`. Pre-building the glue
# into a separate `libpanproto_glue.a` sidesteps that interaction
# entirely. The script also tries to detect a shadowed `ld` and
# warns when one is on PATH.
#
# Run this once after every change to panproto-c, the C glue, or the
# workspace `Cargo.toml`. For consumers fetching prebuilt artifacts
# from a release, see `fetch-bindist.sh`.

set -euo pipefail

cd "$(dirname "$0")/.."
HASKELL_DIR="$(pwd)"
REPO_ROOT="$(cd ../.. && pwd)"

# Sanity check: warn if `ld` resolves to something other than the
# system linker. Anaconda and some other Python distributions ship
# their own `ld`, which is too old to recognize the @response-file
# syntax GHC's merge-objects pass uses. A shadowed `ld` is the most
# common cause of "ld: file not found: @<tmp>/ghc_tmp_*.rsp" errors.
if command -v ld >/dev/null 2>&1; then
    LD_PATH="$(command -v ld)"
    case "$LD_PATH" in
        /usr/bin/ld|/usr/local/bin/ld|/opt/homebrew/bin/ld)
            ;;
        *)
            echo "warning: ld on PATH is $LD_PATH" >&2
            echo "         the system linker is at /usr/bin/ld; if cabal build" >&2
            echo "         later fails on a 'ld: file not found' merge-objects" >&2
            echo "         error, prepend /usr/bin to PATH and retry." >&2
            ;;
    esac
fi

echo "building panproto-c (release)..."
( cd "$REPO_ROOT" && cargo build -p panproto-c --release )

mkdir -p "$HASKELL_DIR/.panproto-c/lib" "$HASKELL_DIR/.panproto-c/include"

cp -f "$REPO_ROOT/crates/panproto-c/include/panproto.h" \
      "$HASKELL_DIR/.panproto-c/include/"

# Stage panproto-c artifacts. Whichever exists for the host platform.
for f in libpanproto_c.a libpanproto_c.dylib libpanproto_c.so panproto_c.lib panproto_c.dll; do
    if [ -f "$REPO_ROOT/target/release/$f" ]; then
        cp -f "$REPO_ROOT/target/release/$f" "$HASKELL_DIR/.panproto-c/lib/"
    fi
done

# Build panproto_glue as a standalone static library so the cabal
# package does not need to ship `c-sources`. We invoke the system C
# compiler directly; on every supported platform (gcc, clang) the
# command line is the same.
echo "building panproto_glue.a..."
CC="${CC:-cc}"
GLUE_OBJ="$HASKELL_DIR/.panproto-c/panproto_glue.o"
GLUE_LIB="$HASKELL_DIR/.panproto-c/lib/libpanproto_glue.a"
"$CC" -c \
    -fPIC \
    -O2 \
    -Wall -Wextra \
    -I"$HASKELL_DIR/cbits" \
    -I"$HASKELL_DIR/.panproto-c/include" \
    "$HASKELL_DIR/cbits/panproto_glue.c" \
    -o "$GLUE_OBJ"
ar rcs "$GLUE_LIB" "$GLUE_OBJ"
rm "$GLUE_OBJ"

echo "staged into $HASKELL_DIR/.panproto-c/"
ls "$HASKELL_DIR/.panproto-c/lib"

# Write an absolute path into cabal.project.local so cabal's
# configure step finds the lib and ghc-pkg accepts the path during
# registration. The .local file is gitignored.
cat > "$HASKELL_DIR/cabal.project.local" <<EOF
package panproto-haskell
    extra-lib-dirs: $HASKELL_DIR/.panproto-c/lib
EOF
echo "wrote cabal.project.local with extra-lib-dirs"
