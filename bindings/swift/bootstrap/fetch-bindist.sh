#!/usr/bin/env bash
#
# Fetch a prebuilt libpanproto_c from the panproto GitHub Releases and
# stage it under `bindings/swift/.panproto-c/`, so `swift build` works
# without a Rust toolchain or a workspace checkout.
#
# Usage:
#   ./bootstrap/fetch-bindist.sh [version] [variant]
#
#   version   release tag, defaulting to the version this checkout is on
#   variant   `default` (the 103-entry default surface) or `full` (adds
#             the 17 parse/project/git entry points). Defaults to
#             `default`.
#
# For an iOS or multi-platform build, fetch the XCFramework instead and
# point SwiftPM at it:
#
#   ./bootstrap/fetch-bindist.sh v0.69.0 full --xcframework
#   PANPROTO_SWIFT_XCFRAMEWORK=.panproto-c/panproto_c.xcframework swift build
#
# For local development against an in-tree cargo build, use
# `dev-link.sh` instead.

set -euo pipefail

cd "$(dirname "$0")/.."
SWIFT_DIR="$(pwd)"
STAGE="$SWIFT_DIR/.panproto-c"
REPO="${PANPROTO_REPO:-panproto/panproto}"

VERSION="${1:-}"
VARIANT="${2:-default}"
WANT_XCFRAMEWORK=0
for arg in "$@"; do
    [ "$arg" = "--xcframework" ] && WANT_XCFRAMEWORK=1
done

# Default the version to whatever this checkout declares, so a
# developer on a branch fetches a matching engine rather than whatever
# was current when this script was last edited.
if [ -z "$VERSION" ] || [ "$VERSION" = "--xcframework" ]; then
    workspace_toml="$SWIFT_DIR/../../Cargo.toml"
    if [ -f "$workspace_toml" ]; then
        VERSION="v$(sed -n 's/^version = "\(.*\)"$/\1/p' "$workspace_toml" | head -1)"
    fi
fi
if [ -z "$VERSION" ] || [ "$VERSION" = "v" ]; then
    echo "error: could not determine a release version; pass one explicitly" >&2
    exit 1
fi

suffix=""
[ "$VARIANT" = "full" ] && suffix="-full"

mkdir -p "$STAGE/lib" "$STAGE/include"

download() {
    local name="$1"
    local url="https://github.com/$REPO/releases/download/$VERSION/$name"
    echo "fetching $url"
    curl -fsSL "$url" -o "$STAGE/$name"
}

if [ "$WANT_XCFRAMEWORK" -eq 1 ]; then
    archive="panproto_c${suffix}.xcframework.zip"
    download "$archive"
    rm -rf "$STAGE/panproto_c.xcframework"
    unzip -q "$STAGE/$archive" -d "$STAGE"
    rm -f "$STAGE/$archive"
    echo "staged $STAGE/panproto_c.xcframework"
    echo
    echo "build with:"
    echo "    PANPROTO_SWIFT_XCFRAMEWORK=.panproto-c/panproto_c.xcframework swift build"
    exit 0
fi

case "$(uname -sm)" in
"Darwin arm64") TARGET="aarch64-apple-darwin" ;;
"Darwin x86_64") TARGET="x86_64-apple-darwin" ;;
"Linux x86_64") TARGET="x86_64-unknown-linux-gnu" ;;
"Linux aarch64") TARGET="aarch64-unknown-linux-gnu" ;;
*)
    echo "unsupported platform: $(uname -sm)" >&2
    exit 1
    ;;
esac

ARCHIVE="panproto-c${suffix}-$TARGET.tar.gz"
download "$ARCHIVE"
tar -xzf "$STAGE/$ARCHIVE" -C "$STAGE"
rm -f "$STAGE/$ARCHIVE"

# The tarball unpacks to panproto-c[-full]-<target>/{lib,include}; flatten
# it so the staged layout matches what dev-link.sh produces.
EXTRACT="$STAGE/panproto-c${suffix}-$TARGET"
if [ -d "$EXTRACT" ]; then
    cp -f "$EXTRACT"/lib/* "$STAGE/lib/"
    cp -f "$EXTRACT"/include/* "$STAGE/include/"
    rm -rf "$EXTRACT"
fi

# Keep the compiled-against header honest: the package vendors its own
# copy, and a bindist from a different release may declare a different
# ABI.
VENDORED="$SWIFT_DIR/Sources/CPanproto/include/panproto.h"
if [ -f "$STAGE/include/panproto.h" ] && ! cmp -s "$STAGE/include/panproto.h" "$VENDORED"; then
    echo
    echo "warning: the fetched panproto.h differs from the vendored"
    echo "         Sources/CPanproto/include/panproto.h. This checkout's raw shim layer"
    echo "         was written against a different ABI revision. Fetch a bindist matching"
    echo "         this checkout, or update the checkout to match $VERSION."
    echo
fi

echo "staged into $STAGE/"
ls "$STAGE/lib"

if [ "$VARIANT" = "full" ]; then
    echo
    echo "this is the full-feature variant; build with:"
    echo "    swift build --traits PANPROTO_PARSE,PANPROTO_PROJECT,PANPROTO_GIT"
fi
