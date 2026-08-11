#!/usr/bin/env bash
#
# Local-development helper: build panproto-c from the workspace and
# stage the library plus header under `bindings/swift/.panproto-c/`,
# where `Package.swift` looks for them by default.
#
# Run this once after every change to panproto-c or the workspace
# `Cargo.toml`. For consumers linking a released artifact instead, see
# `fetch-bindist.sh`.
#
# Usage:
#   ./bootstrap/dev-link.sh
#   PANPROTO_C_FEATURES=full ./bootstrap/dev-link.sh
#
# PANPROTO_C_FEATURES is a comma-separated cargo feature list for
# panproto-c (`full-parse`, `project`, `git`, `format-preserving`, or
# `full` for all of them). Building with features here is only half the
# job: SwiftPM also needs the matching package traits enabled so the
# gated shims compile in. The script prints the invocation to use.

set -euo pipefail

cd "$(dirname "$0")/.."
SWIFT_DIR="$(pwd)"
REPO_ROOT="$(cd ../.. && pwd)"
STAGE="$SWIFT_DIR/.panproto-c"

FEATURES="${PANPROTO_C_FEATURES:-}"
CARGO_FEATURE_ARGS=()
if [ -n "$FEATURES" ]; then
    CARGO_FEATURE_ARGS=(--features "$FEATURES")
fi

echo "building panproto-c (release${FEATURES:+, features: $FEATURES})..."
(
    cd "$REPO_ROOT" &&
        cargo build -p panproto-c --release "${CARGO_FEATURE_ARGS[@]+"${CARGO_FEATURE_ARGS[@]}"}"
)

mkdir -p "$STAGE/lib" "$STAGE/include"

# Stage whichever library artifacts the host platform produced. The
# dylib is what a `swift build` links against by default; the static
# archive is kept alongside it for consumers that prefer to link
# statically with `-Xlinker -force_load`.
staged=0
for artifact in libpanproto_c.dylib libpanproto_c.so libpanproto_c.a panproto_c.dll panproto_c.lib; do
    if [ -f "$REPO_ROOT/target/release/$artifact" ]; then
        cp -f "$REPO_ROOT/target/release/$artifact" "$STAGE/lib/"
        staged=$((staged + 1))
    fi
done
if [ "$staged" -eq 0 ]; then
    echo "error: cargo produced no library artifact in $REPO_ROOT/target/release" >&2
    exit 1
fi

cp -f "$REPO_ROOT/crates/panproto-c/include/panproto.h" "$STAGE/include/"

# The package compiles against a vendored copy of the header rather
# than the workspace one, so that an xcframework build (which has no
# workspace to read) sees the same declarations. Sync it here, and say
# so when it moves: a changed header is a changed ABI, and the raw
# shim layer has to keep up.
VENDORED="$SWIFT_DIR/Sources/CPanproto/include/panproto.h"
if ! cmp -s "$REPO_ROOT/crates/panproto-c/include/panproto.h" "$VENDORED"; then
    cp -f "$REPO_ROOT/crates/panproto-c/include/panproto.h" "$VENDORED"
    echo
    echo "note: Sources/CPanproto/include/panproto.h was out of date and has been updated."
    echo "      Commit it, and check that Sources/PanprotoFFI/Raw+*.swift still binds every"
    echo "      entry point: Scripts/parity-gate.py reports what changed."
    echo
fi

echo "staged into $STAGE/"
ls "$STAGE/lib"

if [ -n "$FEATURES" ]; then
    swift_features="$FEATURES"
    case ",$swift_features," in *,full,*) swift_features="parse,project,git" ;; esac
    swift_features="${swift_features//full-parse/parse}"
    traits=""
    case ",$swift_features," in *,parse,*) traits="PANPROTO_PARSE" ;; esac
    case ",$swift_features," in *,project,*) traits="${traits:+$traits,}PANPROTO_PROJECT" ;; esac
    case ",$swift_features," in *,git,*) traits="${traits:+$traits,}PANPROTO_GIT" ;; esac
    echo
    echo "the library carries gated symbols; build the Swift package with:"
    echo "    swift build --traits $traits"
fi
