#!/usr/bin/env python3
"""Assert that every version-declaring file in the workspace agrees
with the workspace version in `Cargo.toml`.

The motivating bug: `crates/panproto-py/pyproject.toml` once carried a
literal `version = "0.40.0"`, missed in a workspace bump to 0.41.0,
and silently no-op'd the PyPI publish (which had `skip-existing: true`).
This check runs in CI so a repeat is impossible: any drift fails the
build before tag.

Files inspected:

- `Cargo.toml` `[workspace.package].version` is the source of truth.
- `Cargo.toml` `[workspace.dependencies]` `panproto-*` entries must
  match (each `version = "..."` field on a path-based workspace dep).
- Every `crates/*/Cargo.toml` and `tests/*/Cargo.toml` that declares
  a literal `version = "..."` (rather than `version.workspace = true`)
  must match.
- `bindings/typescript/package.json` and `bindings/haskell/panproto.cabal`
  must match the workspace version literally.
- `bindings/python/pyproject.toml` must declare `dynamic = ["version"]`
  (maturin reads the version from `crates/panproto-py/Cargo.toml`,
  which inherits `version.workspace = true`).

Exits non-zero with a list of mismatches; otherwise silent on success
or with a one-line `OK` summary on `--verbose`.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ImportError:  # pragma: no cover
    try:
        import tomli as tomllib  # type: ignore[no-redef]
    except ImportError:
        sys.exit(
            "this script requires Python >= 3.11 (for `tomllib`) "
            "or `tomli` installed as a fallback"
        )

ROOT = Path(__file__).resolve().parents[2]


@dataclass
class Mismatch:
    path: Path
    field: str
    found: str
    expected: str

    def __str__(self) -> str:
        rel = self.path.relative_to(ROOT)
        return (
            f"  {rel}: {self.field} = {self.found!r}, "
            f"expected {self.expected!r}"
        )


def workspace_version() -> str:
    data = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return data["workspace"]["package"]["version"]


def check_root_cargo(expected: str) -> list[Mismatch]:
    """Every `panproto-*` entry in `[workspace.dependencies]` must
    pin the workspace version, since these are path-based crates we
    publish under the same release line."""
    out: list[Mismatch] = []
    data = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    deps = data.get("workspace", {}).get("dependencies", {})
    for name, spec in deps.items():
        if not name.startswith("panproto-"):
            continue
        # `spec` may be a string (`"0.41.0"`) or a table
        # (`{ version = "0.41.0", path = "crates/..." }`). Only the
        # latter is what panproto's workspace uses, but accept both.
        version = spec if isinstance(spec, str) else spec.get("version")
        if version is None:
            continue
        if version != expected:
            out.append(
                Mismatch(
                    path=ROOT / "Cargo.toml",
                    field=f"workspace.dependencies.{name}.version",
                    found=version,
                    expected=expected,
                )
            )
    return out


def check_member_cargo_files(expected: str) -> list[Mismatch]:
    """Every member crate must declare `version.workspace = true`,
    not a literal version. A literal that happens to be in sync today
    is the failure mode that bit us with `panproto-py`'s pyproject:
    sync silently rots."""
    out: list[Mismatch] = []
    for cargo in sorted({*ROOT.glob("crates/*/Cargo.toml"), *ROOT.glob("tests/*/Cargo.toml")}):
        text = cargo.read_text(encoding="utf-8")
        # `tomllib` would resolve `version.workspace = true` to a dict
        # (`{"workspace": True}`), which is fine; we want to flag a
        # literal string. Parse and inspect.
        data = tomllib.loads(text)
        version = data.get("package", {}).get("version")
        if isinstance(version, dict):
            continue  # `version.workspace = true`
        if version is None:
            continue  # virtual or unconfigured
        if version != expected:
            out.append(
                Mismatch(
                    path=cargo,
                    field="package.version",
                    found=version,
                    expected=expected,
                )
            )
        else:
            # In-sync today, but the literal is the rot vector. Flag
            # it as a warning-strength mismatch so reviewers see it.
            out.append(
                Mismatch(
                    path=cargo,
                    field="package.version (literal; should be `version.workspace = true`)",
                    found=version,
                    expected="workspace inheritance",
                )
            )
    return out


def check_grammar_exact_pins(expected: str) -> list[Mismatch]:
    """Each `crates/panproto-grammars-*/Cargo.toml` pins the sibling
    `panproto-grammars` crate at an exact `=X.Y.Z`, in lockstep with the
    workspace version. These are path-based `[dependencies]` pins (not
    `package.version`), so the other checks miss them; left unbumped they
    would make the published grammar-pack crates unresolvable against the
    new `panproto-grammars` on crates.io."""
    out: list[Mismatch] = []
    for cargo in sorted(ROOT.glob("crates/panproto-grammars-*/Cargo.toml")):
        data = tomllib.loads(cargo.read_text(encoding="utf-8"))
        spec = data.get("dependencies", {}).get("panproto-grammars")
        if spec is None:
            continue
        version = spec if isinstance(spec, str) else spec.get("version")
        if version is None:
            continue
        if version != f"={expected}":
            out.append(
                Mismatch(
                    path=cargo,
                    field="dependencies.panproto-grammars.version",
                    found=version,
                    expected=f"={expected}",
                )
            )
    return out


def check_pyproject(path: Path, expected: str, *, allow_dynamic: bool) -> list[Mismatch]:
    if not path.exists():
        return []
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    project = data.get("project", {})
    dynamic = project.get("dynamic", [])
    if "version" in dynamic:
        if not allow_dynamic:
            return [
                Mismatch(
                    path=path,
                    field="project.dynamic",
                    found="version",
                    expected="literal version",
                )
            ]
        return []
    version = project.get("version")
    if version is None:
        return [
            Mismatch(
                path=path,
                field="project.version",
                found="<missing>",
                expected=expected,
            )
        ]
    if version != expected:
        return [
            Mismatch(
                path=path,
                field="project.version",
                found=version,
                expected=expected,
            )
        ]
    return []


def check_package_json(path: Path, expected: str) -> list[Mismatch]:
    if not path.exists():
        return []
    data = json.loads(path.read_text(encoding="utf-8"))
    version = data.get("version")
    if version is None:
        return [
            Mismatch(
                path=path,
                field='"version"',
                found="<missing>",
                expected=expected,
            )
        ]
    if version != expected:
        return [
            Mismatch(
                path=path,
                field='"version"',
                found=version,
                expected=expected,
            )
        ]
    return []


# Cabal files have a fixed-form `version: X.Y.Z` line at top level.
CABAL_VERSION = re.compile(r"^version:\s*(\S+)\s*$", re.MULTILINE)


def check_cabal(path: Path, expected: str) -> list[Mismatch]:
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8")
    m = CABAL_VERSION.search(text)
    if m is None:
        return [
            Mismatch(
                path=path,
                field="version:",
                found="<missing>",
                expected=expected,
            )
        ]
    found = m.group(1)
    if found != expected:
        return [
            Mismatch(
                path=path,
                field="version:",
                found=found,
                expected=expected,
            )
        ]
    return []


def check_companion_pyprojects(expected: str) -> list[Mismatch]:
    """Each `bindings/python-grammars-<group>/pyproject.toml` pins
    `panproto>=X,<Y` as a runtime dependency. Both bounds must track
    the workspace version.

    The motivating bug: a workspace bump from 0.45 to 0.46 leaves
    every companion pinned at `panproto>=0.45,<0.46`, making the
    new wheel unsatisfiable on PyPI. Catching this in CI before tag
    push is cheaper than yanking nine wheels after the fact.

    Convention: lower bound is the same major.minor as the workspace
    (`>={major}.{minor}`); upper bound is the next minor
    (`<{major}.{minor+1}`). Patch-level bumps don't move the bounds.
    """
    expected_parts = expected.split(".")
    if len(expected_parts) < 3:
        return []  # malformed; let other checks handle it
    major, minor = expected_parts[0], expected_parts[1]
    next_minor = str(int(minor) + 1)
    expected_lower = f"{major}.{minor}"
    expected_upper = f"{major}.{next_minor}"

    out: list[Mismatch] = []
    for pyproject in sorted(ROOT.glob("bindings/python-grammars-*/pyproject.toml")):
        data = tomllib.loads(pyproject.read_text(encoding="utf-8"))
        deps = data.get("project", {}).get("dependencies", [])
        panproto_pins = [d for d in deps if d.startswith("panproto") and "grammars" not in d]
        if not panproto_pins:
            out.append(
                Mismatch(
                    path=pyproject,
                    field="project.dependencies (panproto pin)",
                    found="<missing>",
                    expected=f"panproto>={expected_lower},<{expected_upper}",
                )
            )
            continue
        pin = panproto_pins[0]
        if f">={expected_lower}" not in pin or f"<{expected_upper}" not in pin:
            out.append(
                Mismatch(
                    path=pyproject,
                    field="project.dependencies (panproto pin)",
                    found=pin,
                    expected=f"panproto>={expected_lower},<{expected_upper}",
                )
            )
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    expected = workspace_version()

    mismatches: list[Mismatch] = []
    mismatches.extend(check_root_cargo(expected))
    mismatches.extend(check_member_cargo_files(expected))
    mismatches.extend(check_grammar_exact_pins(expected))
    mismatches.extend(check_companion_pyprojects(expected))
    # `bindings/python/pyproject.toml` is the maturin project root.
    # It MUST declare `dynamic = ["version"]` so maturin reads the
    # version from `crates/panproto-py/Cargo.toml` (which inherits
    # `version.workspace = true`). A literal version here would
    # silently strand PyPI on a stale version when the workspace
    # bumps and PyPI's `skip-existing: true` masks the failure.
    mismatches.extend(
        check_pyproject(
            ROOT / "bindings/python/pyproject.toml",
            expected,
            allow_dynamic=True,
        )
    )
    mismatches.extend(check_package_json(ROOT / "bindings/typescript/package.json", expected))
    mismatches.extend(check_cabal(ROOT / "bindings/haskell/panproto.cabal", expected))

    if mismatches:
        print(
            f"version-consistency check FAILED (workspace = {expected!r})",
            file=sys.stderr,
        )
        for m in mismatches:
            print(m, file=sys.stderr)
        return 1

    if args.verbose:
        print(f"OK: every version-declaring file pins {expected!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
