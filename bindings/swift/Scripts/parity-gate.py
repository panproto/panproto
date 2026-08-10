#!/usr/bin/env python3
"""Fail the build when the Swift bindings drift out of parity with the C ABI.

Parity is checked in three directions, each of which has failed silently
in other bindings before:

1. Coverage: every entry point declared in the C headers has a raw shim
   in ``Sources/PanprotoFFI``. A new engine entry point that nobody
   binds is a parity hole, and release notes are not a mechanism.

2. Liveness: every raw shim is referenced from somewhere other than the
   raw layer itself, so a shim cannot rot behind an API that stopped
   calling it.

3. Exercise: every public method of the domain layer is named in at
   least one test or example. A binding nobody has ever run is a
   binding nobody has ever checked.

Run it from anywhere:

    python3 bindings/swift/Scripts/parity-gate.py

``--json`` prints a machine-readable report instead of prose.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent.parent
HEADERS = [
    PACKAGE_ROOT / "Sources/CPanproto/include/panproto.h",
    PACKAGE_ROOT / "Sources/CPanproto/include/panproto_gated.h",
]
RAW_LAYER = PACKAGE_ROOT / "Sources/PanprotoFFI"
DOMAIN_SOURCES = [
    PACKAGE_ROOT / "Sources/Panproto",
    PACKAGE_ROOT / "Sources/PanprotoStructural",
    PACKAGE_ROOT / "Sources/PanprotoVcs",
    PACKAGE_ROOT / "Sources/PanprotoParse",
    PACKAGE_ROOT / "Sources/PanprotoProject",
    PACKAGE_ROOT / "Sources/PanprotoGit",
]
CONSUMER_ROOTS = DOMAIN_SOURCES + [
    PACKAGE_ROOT / "Tests",
    PACKAGE_ROOT / "Examples",
]

# Entry points bound somewhere other than a `Raw` method, with the
# reason. Keep this list short and justified: each line is a hole in
# check 1 that a human decided was correct.
BOUND_ELSEWHERE = {
    # Buffer ownership is not a call the domain layer ever makes by
    # hand. Every owned `Vec_uint8_t` is drained and freed inside
    # `drainPpBuffer`, which is the only correct place for it.
    "pp_buf_free": "Primitives.swift (drainPpBuffer)",
}

DECLARATION = re.compile(
    r"\n(?:int32_t|void)\s*\n(pp_[a-z0-9_]+)\s*\(",
)
RAW_METHOD = re.compile(
    r"^\s*(?:@inlinable\s+)?public\s+static\s+func\s+([A-Za-z0-9_]+)",
    re.MULTILINE,
)
PUBLIC_METHOD = re.compile(
    r"^\s*public\s+(?:static\s+|nonisolated\s+|mutating\s+)*func\s+([A-Za-z0-9_]+)",
    re.MULTILINE,
)


# Entry points whose mechanical name collides with a Swift keyword.
RESERVED_RENAMES = {
    "pp_init": "initialize",
}


def swift_name(symbol: str) -> str:
    """Map a C entry point to its Swift shim name.

    The mapping is mechanical, with no acronym special-casing, so that
    this gate can compute it rather than consult a table:
    ``pp_schema_from_cbor`` becomes ``schemaFromCbor``. The one
    exception is ``pp_init``, whose mechanical name is ``init``.
    """
    if symbol in RESERVED_RENAMES:
        return RESERVED_RENAMES[symbol]
    parts = symbol.removeprefix("pp_").split("_")
    return parts[0] + "".join(part.capitalize() for part in parts[1:])


def swift_files(root: Path) -> list[Path]:
    return sorted(root.rglob("*.swift")) if root.exists() else []


def read(paths: list[Path]) -> str:
    return "\n".join(path.read_text(encoding="utf-8") for path in paths)


@dataclass
class Report:
    entry_points: int = 0
    bound: int = 0
    unbound: list[str] = field(default_factory=list)
    misnamed: list[str] = field(default_factory=list)
    dead_shims: list[str] = field(default_factory=list)
    untested: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not (self.unbound or self.misnamed or self.dead_shims or self.untested)


def collect_entry_points() -> list[str]:
    symbols: list[str] = []
    for header in HEADERS:
        if not header.exists():
            sys.exit(f"parity gate: missing header {header}")
        symbols.extend(DECLARATION.findall("\n" + header.read_text(encoding="utf-8")))
    return sorted(set(symbols))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit a JSON report")
    args = parser.parse_args()

    report = Report()
    entry_points = collect_entry_points()
    report.entry_points = len(entry_points)

    raw_files = swift_files(RAW_LAYER)
    if not raw_files:
        sys.exit(f"parity gate: no Swift sources under {RAW_LAYER}")
    raw_text = read(raw_files)
    raw_methods = set(RAW_METHOD.findall(raw_text))

    # 1. Coverage.
    for symbol in entry_points:
        expected = swift_name(symbol)
        if expected in raw_methods:
            report.bound += 1
        elif symbol in BOUND_ELSEWHERE:
            report.bound += 1
        elif symbol in raw_text:
            # The C symbol is called, but not under the name the
            # convention predicts, so the gate cannot verify it and
            # neither can a reader.
            report.misnamed.append(f"{symbol} is called but no `Raw.{expected}` declares it")
        else:
            report.unbound.append(symbol)

    # 2. Liveness.
    consumer_text = read([f for root in CONSUMER_ROOTS for f in swift_files(root)])
    expected_names = {swift_name(s) for s in entry_points}
    for method in sorted(raw_methods):
        if method not in expected_names:
            continue
        if not re.search(rf"\bRaw\.{re.escape(method)}\b", consumer_text):
            report.dead_shims.append(method)

    # 3. Exercise.
    test_text = read(
        [f for root in (PACKAGE_ROOT / "Tests", PACKAGE_ROOT / "Examples") for f in swift_files(root)]
    )
    domain_text_by_file = {
        path: path.read_text(encoding="utf-8")
        for root in DOMAIN_SOURCES
        for path in swift_files(root)
    }
    for path, text in sorted(domain_text_by_file.items()):
        for method in sorted(set(PUBLIC_METHOD.findall(text))):
            if method.startswith("_"):
                continue
            if not re.search(rf"\b{re.escape(method)}\b", test_text):
                rel = path.relative_to(PACKAGE_ROOT)
                report.untested.append(f"{rel}: {method}")

    if args.json:
        print(
            json.dumps(
                {
                    "entryPoints": report.entry_points,
                    "bound": report.bound,
                    "unbound": report.unbound,
                    "misnamed": report.misnamed,
                    "deadShims": report.dead_shims,
                    "untested": report.untested,
                    "ok": report.ok,
                },
                indent=2,
            )
        )
        return 0 if report.ok else 1

    print(f"C ABI entry points declared: {report.entry_points}")
    print(f"bound in the raw layer:      {report.bound}")

    if report.unbound:
        print(f"\nUNBOUND ({len(report.unbound)}): no Swift shim exists")
        for symbol in report.unbound:
            print(f"  {symbol}  ->  expected `Raw.{swift_name(symbol)}`")
    if report.misnamed:
        print(f"\nMISNAMED ({len(report.misnamed)})")
        for line in report.misnamed:
            print(f"  {line}")
    if report.dead_shims:
        print(f"\nDEAD SHIMS ({len(report.dead_shims)}): bound but never called")
        for method in report.dead_shims:
            print(f"  Raw.{method}")
    if report.untested:
        print(f"\nUNTESTED PUBLIC API ({len(report.untested)}): never named by a test or example")
        for line in report.untested:
            print(f"  {line}")

    if report.ok:
        print("\nparity holds")
        return 0
    print("\nparity gate failed")
    return 1


if __name__ == "__main__":
    sys.exit(main())
