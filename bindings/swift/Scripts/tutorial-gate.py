#!/usr/bin/env python3
"""Type-check the Swift listings a DocC tutorial shows.

A tutorial's ``@Code`` files live inside a documentation catalog, and a
catalog is a resource: SwiftPM copies it and compiles nothing in it. The
listings a reader is told to type are therefore the one part of this
package no compiler sees, and a listing that stopped compiling would go
on being published. This checks them.

Each listing is a whole program, and successive steps redeclare the same
``@main`` type, so they are checked one file at a time rather than as a
module. The check is a type-check against the modules ``swift build``
produced, in the same language mode and with the same warning policy the
package builds under.

Run it from anywhere, after a build:

    swift build
    python3 bindings/swift/Scripts/tutorial-gate.py

``--json`` prints a machine-readable report instead of prose.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent.parent
MODULE_SEARCH_PATH = PACKAGE_ROOT / ".build/debug/Modules"
MODULE_MAP = PACKAGE_ROOT / "Sources/CPanproto/module.modulemap"
CPANPROTO_INCLUDE = PACKAGE_ROOT / "Sources/CPanproto"


def listings() -> list[Path]:
    """Every Swift listing a tutorial step shows, in step order."""
    return sorted(
        path
        for catalog in PACKAGE_ROOT.glob("Sources/*/*.docc")
        for path in catalog.rglob("Resources/*.swift")
    )


def target_triple() -> str:
    """The triple ``swift build`` compiles for on this host."""
    info = json.loads(
        subprocess.run(
            ["swift", "-print-target-info"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    return info["target"]["unversionedTriple"]


def deployment_target(triple: str) -> str:
    """The triple with the package's minimum platform version pinned.

    The listings use APIs the package declares a floor for, so checking
    them against the host's default floor would accept a listing the
    package itself would reject.
    """
    if "apple-macos" in triple:
        return triple.split("apple-macos")[0] + "apple-macosx14.0"
    return triple


def check(path: Path, triple: str, sdk: str) -> str:
    """Type-check one listing, answering with the diagnostics it drew."""
    finished = subprocess.run(
        [
            "swiftc",
            "-typecheck",
            # Each listing is a program with a `@main` type, so it is
            # not the top-level-code file a bare invocation assumes.
            "-parse-as-library",
            "-swift-version",
            "6",
            "-warnings-as-errors",
            "-sdk",
            sdk,
            "-target",
            triple,
            "-I",
            str(MODULE_SEARCH_PATH),
            "-I",
            str(CPANPROTO_INCLUDE),
            "-Xcc",
            f"-fmodule-map-file={MODULE_MAP}",
            str(path),
        ],
        capture_output=True,
        text=True,
    )
    return (finished.stdout + finished.stderr).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit a JSON report")
    args = parser.parse_args()

    paths = listings()
    if not paths:
        sys.exit("tutorial gate: no tutorial listings found")
    if not MODULE_SEARCH_PATH.exists():
        sys.exit(f"tutorial gate: run `swift build` first; {MODULE_SEARCH_PATH} is missing")

    sdk = subprocess.run(
        ["xcrun", "--show-sdk-path"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    triple = deployment_target(target_triple())

    failures: dict[str, str] = {}
    for path in paths:
        diagnostics = check(path, triple, sdk)
        if diagnostics:
            failures[str(path.relative_to(PACKAGE_ROOT))] = diagnostics

    if args.json:
        print(
            json.dumps(
                {
                    "listings": len(paths),
                    "failures": failures,
                    "ok": not failures,
                },
                indent=2,
            )
        )
        return 0 if not failures else 1

    print(f"tutorial listings type-checked: {len(paths)}")
    for name, diagnostics in failures.items():
        print(f"\n{name}\n{diagnostics}")

    if failures:
        print("\ntutorial gate failed")
        return 1
    print("\nevery listing compiles")
    return 0


if __name__ == "__main__":
    sys.exit(main())
