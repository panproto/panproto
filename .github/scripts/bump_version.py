#!/usr/bin/env python3
"""Bump the workspace version across every version-declaring file in one shot.

The write-counterpart to ``check_version_consistency.py``: it touches exactly
the surface that check validates, plus the exact ``=X.Y.Z`` grammar-crate
dependency pins the check historically missed, so a release is one command
instead of a hand-rolled sed loop that forgets a file.

Source of truth is ``Cargo.toml``'s ``[workspace.package].version``. Everything
else derives from it:

- ``Cargo.toml`` ``[workspace.package].version`` and every ``panproto-*`` pin in
  ``[workspace.dependencies]``.
- The exact ``panproto-grammars = { version = "=X.Y.Z", ... }`` pins in
  ``crates/panproto-grammars-*/Cargo.toml`` (sibling-crate lockstep pins).
- ``bindings/typescript/package.json`` ``"version"``.
- ``bindings/haskell/panproto.cabal`` ``version:``.
- ``bindings/python-grammars-*/pyproject.toml`` ``panproto>=MAJOR.MINOR,<MAJOR.MINOR+1``.

Member crates use ``version.workspace = true`` and ``bindings/python/pyproject.toml``
is ``dynamic = ["version"]``, so neither is touched — that is the whole point of
the inheritance, and the consistency check enforces it stays that way.

Usage::

    python3 .github/scripts/bump_version.py 0.62.0            # bump + refresh lock + verify
    python3 .github/scripts/bump_version.py 0.62.0 --date 2026-08-01  # also date the CHANGELOG
    python3 .github/scripts/bump_version.py 0.62.0 --no-lock   # skip `cargo update` (CI regenerates)

After writing, it runs ``check_version_consistency.py`` and exits non-zero if
anything is still inconsistent, so a partial bump can never slip through.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ImportError:  # pragma: no cover
    sys.exit("this script requires Python >= 3.11 (for `tomllib`)")

ROOT = Path(__file__).resolve().parents[2]

SEMVER = re.compile(r"^\d+\.\d+\.\d+$")


def current_version() -> str:
    data = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return data["workspace"]["package"]["version"]


def replace_once(path: Path, old: str, new: str, *, count: int, changed: list[str]) -> None:
    """Replace ``old`` with ``new`` in ``path``, asserting the expected count.

    A surprising count means the file drifted from what the bumper knows about;
    fail loudly rather than silently miss (or over-apply) a version.
    """
    text = path.read_text(encoding="utf-8")
    found = text.count(old)
    if found != count:
        sys.exit(
            f"bump aborted: {path.relative_to(ROOT)} has {found} occurrence(s) of "
            f"{old!r}, expected {count}. The file's shape changed; update "
            f"bump_version.py to match."
        )
    path.write_text(text.replace(old, new), encoding="utf-8")
    changed.append(str(path.relative_to(ROOT)))


def bump_root_cargo(old: str, new: str, changed: list[str]) -> None:
    """The `[workspace.package].version` and every `panproto-*` workspace dep pin."""
    path = ROOT / "Cargo.toml"
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    deps = data.get("workspace", {}).get("dependencies", {})
    n_pins = sum(
        1
        for name, spec in deps.items()
        if name.startswith("panproto-")
        and (spec if isinstance(spec, str) else spec.get("version")) == old
    )
    # +1 for the [workspace.package] version itself.
    replace_once(path, f'version = "{old}"', f'version = "{new}"', count=n_pins + 1, changed=changed)


def bump_grammar_exact_pins(old: str, new: str, changed: list[str]) -> None:
    """The exact `panproto-grammars = { version = "=X.Y.Z" }` sibling pins."""
    for cargo in sorted(ROOT.glob("crates/panproto-grammars-*/Cargo.toml")):
        if f'version = "={old}"' in cargo.read_text(encoding="utf-8"):
            replace_once(cargo, f'version = "={old}"', f'version = "={new}"', count=1, changed=changed)


def bump_package_json(old: str, new: str, changed: list[str]) -> None:
    path = ROOT / "bindings/typescript/package.json"
    if path.exists():
        replace_once(path, f'"version": "{old}"', f'"version": "{new}"', count=1, changed=changed)


def bump_cabal(old: str, new: str, changed: list[str]) -> None:
    path = ROOT / "bindings/haskell/panproto.cabal"
    if not path.exists():
        return
    text = path.read_text(encoding="utf-8")
    text2, n = re.subn(rf"^(version:\s*){re.escape(old)}\s*$", rf"\g<1>{new}", text, flags=re.MULTILINE)
    if n != 1:
        sys.exit(f"bump aborted: {path.relative_to(ROOT)} version: line not found or ambiguous")
    path.write_text(text2, encoding="utf-8")
    changed.append(str(path.relative_to(ROOT)))


def bump_companion_pyprojects(old: str, new: str, changed: list[str]) -> None:
    """`panproto>=MAJOR.MINOR,<MAJOR.MINOR+1` runtime pins in the grammar companions."""
    o = old.split(".")
    n = new.split(".")
    old_pin = f'"panproto>={o[0]}.{o[1]},<{o[0]}.{int(o[1]) + 1}"'
    new_pin = f'"panproto>={n[0]}.{n[1]},<{n[0]}.{int(n[1]) + 1}"'
    if old_pin == new_pin:
        return  # patch-level bump; bounds don't move
    for pyproject in sorted(ROOT.glob("bindings/python-grammars-*/pyproject.toml")):
        if old_pin in pyproject.read_text(encoding="utf-8"):
            replace_once(pyproject, old_pin, new_pin, count=1, changed=changed)


def date_changelog(new: str, date: str, changed: list[str]) -> None:
    path = ROOT / "CHANGELOG.md"
    if not path.exists():
        return
    text = path.read_text(encoding="utf-8")
    if "## [Unreleased]" not in text:
        print("  (CHANGELOG has no [Unreleased] section; not dating)", file=sys.stderr)
        return
    text = text.replace("## [Unreleased]", f"## [{new}] - {date}", 1)
    path.write_text(text, encoding="utf-8")
    changed.append(str(path.relative_to(ROOT)))


def refresh_lockfile(changed: list[str]) -> None:
    """Update Cargo.lock for the bumped workspace crates without touching deps."""
    proc = subprocess.run(
        ["cargo", "update", "--workspace", "--offline"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        # Fall back to a non-offline update (fetches nothing new for path crates).
        proc = subprocess.run(["cargo", "update", "--workspace"], cwd=ROOT, capture_output=True, text=True)
    if proc.returncode == 0:
        changed.append("Cargo.lock")
    else:
        print(
            "  (could not refresh Cargo.lock automatically; run `cargo update --workspace`)",
            file=sys.stderr,
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("version", help="the new X.Y.Z version")
    parser.add_argument("--date", help="date the CHANGELOG [Unreleased] section as YYYY-MM-DD")
    parser.add_argument("--no-lock", action="store_true", help="do not refresh Cargo.lock")
    args = parser.parse_args()

    new = args.version
    if not SEMVER.match(new):
        sys.exit(f"not a X.Y.Z version: {new!r}")
    old = current_version()
    if old == new:
        print(f"already at {new}; nothing to bump")
    else:
        print(f"bumping {old} -> {new}")

    changed: list[str] = []
    bump_root_cargo(old, new, changed)
    bump_grammar_exact_pins(old, new, changed)
    bump_package_json(old, new, changed)
    bump_cabal(old, new, changed)
    bump_companion_pyprojects(old, new, changed)
    if args.date:
        date_changelog(new, args.date, changed)
    if not args.no_lock:
        refresh_lockfile(changed)

    for c in sorted(set(changed)):
        print(f"  updated {c}")

    # Self-verify: the checker is the authority on what "consistent" means.
    print("verifying with check_version_consistency.py ...")
    check = subprocess.run(
        [sys.executable, str(ROOT / ".github/scripts/check_version_consistency.py"), "--verbose"],
        cwd=ROOT,
    )
    return check.returncode


if __name__ == "__main__":
    sys.exit(main())
