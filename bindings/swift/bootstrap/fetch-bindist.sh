#!/usr/bin/env bash
#
# Fetch a prebuilt libpanproto_c from the panproto GitHub Releases and
# stage it under `bindings/swift/.panproto-c/`, so `swift build` works
# without a Rust toolchain or a workspace checkout.
#
# Usage:
#   ./bootstrap/fetch-bindist.sh [version] [variant] [--xcframework]
#
#   version   release tag (`vX.Y.Z` or `X.Y.Z`), defaulting to the
#             version this checkout is on
#   variant   `default` (the 103-entry default surface) or `full` (adds
#             the 17 parse/project/git entry points), also spelled
#             `--default` and `--full`. Defaults to `default`.
#   --tag T   fetch from a tag that is not shaped `vX.Y.Z`, which a
#             `PANPROTO_REPO` fork may well use
#
# The two positionals are recognized by shape rather than by position: a
# leading `v` or a digit makes an argument the version, `default` and
# `full` name the variant. So the variant can be chosen without naming a
# version, which is what keeps the documentation free of a release tag
# that would rot on the next bump:
#
#   ./bootstrap/fetch-bindist.sh full
#
# For an iOS or multi-platform build, fetch the XCFramework instead and
# point SwiftPM at it:
#
#   ./bootstrap/fetch-bindist.sh --xcframework
#   PANPROTO_SWIFT_XCFRAMEWORK=.panproto-c/panproto_c.xcframework swift build
#
# For local development against an in-tree cargo build, use
# `dev-link.sh` instead.

set -euo pipefail

cd "$(dirname "$0")/.."
SWIFT_DIR="$(pwd)"
STAGE="$SWIFT_DIR/.panproto-c"
REPO="${PANPROTO_REPO:-panproto/panproto}"

usage() {
    cat <<'USAGE'
usage: fetch-bindist.sh [version] [variant] [--xcframework]

  version   release tag (vX.Y.Z or X.Y.Z), defaulting to the version
            this checkout declares in the workspace Cargo.toml
  variant   default | full, also spelled --default | --full
            (full adds the parse, project, and git entry points)
  --tag T   fetch from an arbitrary tag name, for a PANPROTO_REPO
            whose tags are not vX.Y.Z

Arguments are recognized by shape, not by position, so a variant can
be selected without naming a version:

  fetch-bindist.sh full
  fetch-bindist.sh --xcframework
USAGE
}

VERSION=""
VARIANT=""
WANT_XCFRAMEWORK=0

set_version() {
    if [ -n "$VERSION" ]; then
        echo "error: two versions given: $VERSION and $1" >&2
        exit 2
    fi
    VERSION="$1"
}

# `--tag` takes the next argument, so it needs a flag rather than a
# `for` loop over "$@".
take_tag=0
for arg in "$@"; do
    if [ "$take_tag" -eq 1 ]; then
        set_version "$arg"
        take_tag=0
        continue
    fi
    case "$arg" in
    # An empty argument is what a wrapper produces from an unset
    # variable (`fetch-bindist.sh "$VERSION" full`). Treat it as absent
    # rather than as a name to reject.
    "") ;;
    --xcframework) WANT_XCFRAMEWORK=1 ;;
    --default | --full) VARIANT="${arg#--}" ;;
    default | full) VARIANT="$arg" ;;
    -h | --help)
        usage
        exit 0
        ;;
    --tag) take_tag=1 ;;
    --tag=*) set_version "${arg#--tag=}" ;;
    # A release tag, matched on its full shape rather than a leading
    # digit, so a typo is reported here instead of becoming a 404 on a
    # URL built from it.
    v[0-9]*.[0-9]*.[0-9]* | [0-9]*.[0-9]*.[0-9]*) set_version "$arg" ;;
    *)
        echo "error: unrecognized argument: $arg" >&2
        echo "       (for a tag that is not vX.Y.Z, use --tag $arg)" >&2
        usage >&2
        exit 2
        ;;
    esac
done
if [ "$take_tag" -eq 1 ]; then
    echo "error: --tag needs a tag name" >&2
    exit 2
fi

VARIANT="${VARIANT:-default}"

# Default the version to whatever this checkout declares, so a
# developer on a branch fetches a matching engine rather than whatever
# was current when this script was last edited.
if [ -z "$VERSION" ]; then
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
