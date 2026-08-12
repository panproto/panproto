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
- `bindings/swift/Package.swift`'s release pin (`releaseXCFrameworkURL`
  and `releaseXCFrameworkChecksum`) must be well formed, must not name a
  release that has not happened, and must not skip one that has. See
  `check_swift_release_pin` for why a lag is correct here and equality is
  not required.
- No hard-coded `vX.Y.Z` release tag may appear in the Swift
  documentation unless it names the workspace version. Scanned: every
  decodable text file under `bindings/swift/` (excluding the build
  directories `.build/`, `.panproto-c/`, and `.swiftpm/`, and the binary
  fixture suffixes listed in `BINARY_SUFFIXES`), plus
  `book/src/how-to/install/swift.md`. One literal is exempt: the release
  pin in `Package.swift`, governed by the lag rule above.

Run `--self-test` to exercise the Swift pin and documentation logic
against a table of states; it reads nothing from disk and is what CI
runs alongside the check itself.

Exits non-zero with a list of mismatches; otherwise silent on success
or with a one-line `OK` summary on `--verbose`.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
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
    note: str | None = None

    def __str__(self) -> str:
        # `relative_to` raises for a path outside the workspace, which is
        # what `--self-test` passes.
        rel = self.path if not self.path.is_relative_to(ROOT) else self.path.relative_to(ROOT)
        line = (
            f"  {rel}: {self.field} = {self.found!r}, "
            f"expected {self.expected!r}"
        )
        if self.note is not None:
            line += f"\n      {self.note}"
        return line


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


# --- Swift ------------------------------------------------------------
#
# The Swift package is the one surface whose version field is written by
# a machine after the tag rather than by the bump before it, so it is
# also the one surface where equality with the workspace version is the
# wrong assertion. Everything below exists to state the right one.

SWIFT_PACKAGE = Path("bindings/swift/Package.swift")
SWIFT_INSTALL_DOC = Path("book/src/how-to/install/swift.md")
BOOTSTRAP_SCRIPT = "fetch-bindist.sh"

# The release in which `bindings/swift/` first shipped, and so the
# workspace version at which an empty pin stops being the documented
# pre-release state and becomes an unresolvable package.
SWIFT_FIRST_RELEASE = "0.70.0"


def pin_constant(name: str) -> re.Pattern[str]:
    """The pattern for one of the two constants `publish-swift.yml`
    rewrites.

    Leading whitespace and anything after the closing quote are tolerated
    so that this, the workflow's rewrite, and the workflow's mirror guard
    all accept the same set of manifests. Three patterns with three
    tolerances meant an indented declaration turned CI red over a working
    pin, and a trailing comment failed the release at the mirror guard
    with a message naming the wrong cause.
    """
    return re.compile(
        rf'^[ \t]*private let {name}\s*=\s*"([^"]*)"', re.MULTILINE
    )


PIN_URL = pin_constant("releaseXCFrameworkURL")
PIN_CHECKSUM = pin_constant("releaseXCFrameworkChecksum")

# The pinned URL must name this repository's releases, the XCFramework
# asset, and a release tag. SwiftPM resolves this URL verbatim on a
# consumer's machine, so anything else is a broken dependency.
#
# The tag group accepts everything `publish-swift.yml`'s trigger glob
# (`v[0-9]+.[0-9]+.[0-9]+*`) accepts, not just what this script can
# order. A tag the trigger admits starts a real release and the pin job
# writes it to `main` verbatim, so a narrower pattern here would turn
# `main` permanently red over a pin nobody is allowed to edit by hand.
PIN_URL_SHAPE = re.compile(
    r"^https://github\.com/panproto/panproto/releases/download/"
    r"(v[0-9][0-9A-Za-z.+-]*)"
    r"/panproto_c\.xcframework\.zip$"
)

SHA256 = re.compile(r"^[0-9a-f]{64}$")

# A release tag as it appears in prose or a shell listing. Only the
# `X.Y.Z` triple is captured: a hyphenated suffix in a release asset name
# (`panproto-c-v0.70.1-aarch64-apple-darwin.tar.gz`) is textually
# indistinguishable from a semver prerelease, so matching one as the
# other flagged lines that named the current version correctly. The
# lookbehind keeps `rev0.70.1` out, and the lookahead keeps `v0.70.10`
# from matching as `v0.70.1` while still allowing a sentence-final
# period after a tag.
DOC_RELEASE_TAG = re.compile(r"(?<![0-9A-Za-z.])v(\d+\.\d+\.\d+)(?!\.?\d)")

# `fetch-bindist.sh` also takes a bare `X.Y.Z`, which carries no `v` to
# key on. Requiring the prefix everywhere would miss it; dropping the
# prefix requirement everywhere would flag every third-party dependency
# pin (`from: "1.4.0"`). So a bare version is recognized in the one
# context where it is a release tag: an argument to that script.
BARE_VERSION_ARG = re.compile(r"^v?(\d+\.\d+\.\d+)$")

# Semver, with build metadata accepted and ignored for ordering, as the
# specification requires.
VERSION_TEXT = re.compile(
    r"^(\d+)\.(\d+)\.(\d+)"
    r"(?:-([0-9A-Za-z][0-9A-Za-z.-]*))?"
    r"(?:\+[0-9A-Za-z][0-9A-Za-z.-]*)?$"
)

SWIFT_SKIP_DIRS = {".build", ".panproto-c", ".swiftpm", ".git"}
BINARY_SUFFIXES = {".cbor", ".png", ".jpg", ".zip", ".a", ".dylib", ".so"}


@dataclass(frozen=True)
class Version:
    """A release version, ordered numerically rather than as a string.

    String comparison gets `0.9.0 < 0.10.0` and `0.6.0 < 0.60.0` wrong,
    which is the whole reason this type exists.

    Prerelease suffixes are admitted because the publish workflow's tag
    filter is `v[0-9]+.[0-9]+.[0-9]+*`, which matches `v0.71.0-rc.1`. They
    order by semver: a prerelease precedes the final release of the same
    triple, and its dot-separated identifiers compare numerically when
    they are all digits and lexically otherwise. Build metadata after a
    `+` is accepted and ignored for ordering, as semver requires, so a
    `v0.71.0+build.5` tag is orderable rather than opaque. A prerelease
    pin is otherwise treated as an ordinary release: it counts as
    published when a tag names it.
    """

    raw: str
    release: tuple[int, int, int]
    prerelease: tuple[str, ...]

    @property
    def sort_key(self) -> tuple[object, ...]:
        # A version with no prerelease outranks every prerelease of the
        # same triple, hence the `1` against the prereleases' `0`. Each
        # identifier is tagged so digits never compare against letters.
        identifiers = tuple(
            (0, int(part), "") if part.isdigit() else (1, 0, part)
            for part in self.prerelease
        )
        return (self.release, 0 if self.prerelease else 1, identifiers)


def parse_version(text: str) -> Version | None:
    m = VERSION_TEXT.fullmatch(text.removeprefix("v"))
    if m is None:
        return None
    prerelease = tuple(m.group(4).split(".")) if m.group(4) else ()
    return Version(
        raw=m.group(0),
        release=(int(m.group(1)), int(m.group(2)), int(m.group(3))),
        prerelease=prerelease,
    )


def published_releases() -> list[Version] | None:
    """Every `v*` tag in the repository, newest first, or `None` when the
    tag list cannot be read.

    Git tags are the publication record, and the lag rule below needs the
    publication record rather than an account of what was intended. A
    `CHANGELOG.md` heading is written by `bump_version.py` at bump time,
    before anything is published, so a version can have a heading and no
    release: two bumps before a release, or a tag whose publish aborted
    and was superseded by a patch bump, both leave a heading behind. A
    tag, by contrast, is what triggers `publish-swift.yml` in the first
    place, so it exists exactly when a release was attempted.

    Reading the CHANGELOG instead was also fragile in a way tags are not.
    Every heading had to match one fixed shape, and this repository's own
    CHANGELOG has two release headings that do not carry a date. A
    heading the pattern missed silently moved the permitted window back
    by one release, which both rejected the legal pin and accepted an
    illegal one.
    """
    try:
        proc = subprocess.run(
            ["git", "-C", str(ROOT), "tag", "--list", "v*"],
            capture_output=True,
            text=True,
            check=True,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    found = [
        parsed
        for line in proc.stdout.split()
        if (parsed := parse_version(line)) is not None
    ]
    if not found:
        return None
    return sorted(found, key=lambda v: v.sort_key, reverse=True)


def classify_release_pin(
    path: Path,
    url: str,
    checksum: str,
    expected: str,
    published: list[Version] | None,
) -> list[Mismatch]:
    """Judge one pin, given its two constants and the release record.

    `bindings/swift/Package.swift` pins the published XCFramework by URL
    and checksum, and a consumer that adds the package as an ordinary
    SwiftPM dependency resolves against exactly those two constants.

    Nothing else in the workspace writes them: `publish-swift.yml` fires
    on the tag push, waits for `build-panproto-c-bindist.yml` to attach
    `panproto_c.xcframework.zip` to the release, computes the checksum,
    and commits the rewritten constants straight to `main`. So the pin
    legitimately lags. At version-bump time the workspace has moved to N
    while the pin still names N-1, because the artifact for N does not
    exist yet; the pin catches up only once the tag is pushed and that
    job succeeds.

    The check therefore asserts the lag rather than equality:

    - Both constants empty passes only below `SWIFT_FIRST_RELEASE`, the
      release in which the Swift package shipped. That was the documented
      pre-release state; at or above that release an empty pin is a
      package no ordinary consumer can resolve, since `Package.swift`
      gates its binary target on the URL being non-empty.
    - Exactly one empty fails: a URL without a checksum, or a checksum
      without a URL, is a package that fails at consumer resolve time.
    - The URL must name this repository's releases and the XCFramework
      asset; the checksum must be a SHA-256.
    - A pin ahead of the workspace version names a release that does not
      exist, which breaks every consumer, so it fails.
    - A pin behind the workspace version fails when some release was
      published strictly between the two, because that is the pin job
      having failed on an intervening release with nobody noticing. Any
      other lag passes: bumping twice before releasing, or abandoning a
      tag and superseding it with a patch bump, both leave the pin
      several versions back with nothing wrong.

    Only a release strictly older than the workspace version counts as
    intervening. The tag for the workspace version itself may already
    exist while the pin job is still running, and that window is an hour
    wide, so treating it as a failure would turn `main` red on every
    release.
    """
    out: list[Mismatch] = []

    if not url and not checksum:
        workspace = parse_version(expected)
        floor = parse_version(SWIFT_FIRST_RELEASE)
        if workspace is None or floor is None or workspace.sort_key < floor.sort_key:
            return out
        return [
            Mismatch(
                path=path,
                field="releaseXCFrameworkURL",
                found="",
                expected="a published release URL",
                note=(
                    "an empty pin was the pre-release state, but the Swift "
                    f"package has shipped since {SWIFT_FIRST_RELEASE}, and "
                    "Package.swift gates its binary target on this constant "
                    "being non-empty. Blank, it resolves to a system library "
                    "no consumer has, so a reverted or lost pin commit would "
                    "otherwise pass forever."
                ),
            )
        ]
    if not url or not checksum:
        empty, filled = (
            ("releaseXCFrameworkURL", "releaseXCFrameworkChecksum")
            if not url
            else ("releaseXCFrameworkChecksum", "releaseXCFrameworkURL")
        )
        return [
            Mismatch(
                path=path,
                field=empty,
                found="",
                expected="a value, or both constants empty",
                note=(
                    f"{filled} is set, so this half-written pin makes "
                    "`.binaryTarget(url:checksum:)` fail at consumer resolve "
                    "time. Either constant may be empty only when both are."
                ),
            )
        ]

    shape = PIN_URL_SHAPE.fullmatch(url)
    if shape is None:
        out.append(
            Mismatch(
                path=path,
                field="releaseXCFrameworkURL",
                found=url,
                expected=(
                    "https://github.com/panproto/panproto/releases/download/"
                    "<tag>/panproto_c.xcframework.zip"
                ),
                note=(
                    "SwiftPM fetches this URL verbatim, so it has to name "
                    "this repository's releases, the XCFramework asset, and "
                    "a release tag."
                ),
            )
        )
    if not SHA256.fullmatch(checksum):
        out.append(
            Mismatch(
                path=path,
                field="releaseXCFrameworkChecksum",
                found=checksum,
                expected="64 lowercase hex digits",
                note=(
                    "`swift package compute-checksum` emits a SHA-256; "
                    "anything else fails the integrity check on resolve."
                ),
            )
        )
    if shape is None:
        return out

    # A tag the publish trigger admits but semver cannot order (`v1.2.3.4`,
    # `v1.2.3rc1`) is left alone: the release it names really happened, the
    # pin job wrote it, and no hand edit is permitted, so failing here
    # would only make `main` unfixable.
    pinned = parse_version(shape.group(1))
    workspace = parse_version(expected)
    if pinned is None or workspace is None:
        return out

    if pinned.sort_key > workspace.sort_key:
        out.append(
            Mismatch(
                path=path,
                field="releaseXCFrameworkURL (tag)",
                found=shape.group(1),
                expected=f"v{expected} or an earlier release",
                note=(
                    "the pin points forward, naming a release that has not "
                    "been published, so every consumer's resolve fails on a "
                    "missing asset. The pin is written by publish-swift.yml "
                    "after the tag, never by hand."
                ),
            )
        )
        return out

    if pinned.sort_key == workspace.sort_key or published is None:
        return out

    skipped = [
        v
        for v in published
        if pinned.sort_key < v.sort_key < workspace.sort_key
    ]
    if skipped:
        names = ", ".join(f"v{v.raw}" for v in reversed(skipped))
        out.append(
            Mismatch(
                path=path,
                field="releaseXCFrameworkURL (tag)",
                found=shape.group(1),
                expected=f"v{skipped[0].raw} or a later release",
                note=(
                    f"{names} shipped after the pinned release, so "
                    "publish-swift.yml's `pin` job failed on an intervening "
                    "tag and went unnoticed, leaving every SwiftPM consumer "
                    "resolving a stale XCFramework. Re-run that workflow for "
                    "the newest of those tags rather than editing the "
                    "constants by hand."
                ),
            )
        )
    return out


def check_swift_release_pin(expected: str) -> list[Mismatch]:
    """Read the two pin constants out of `Package.swift` and judge them."""
    path = ROOT / SWIFT_PACKAGE
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8")

    url_match = PIN_URL.search(text)
    checksum_match = PIN_CHECKSUM.search(text)
    out: list[Mismatch] = []
    for name, match in (
        ("releaseXCFrameworkURL", url_match),
        ("releaseXCFrameworkChecksum", checksum_match),
    ):
        if match is None:
            out.append(
                Mismatch(
                    path=path,
                    field=name,
                    found="<no such declaration>",
                    expected="a `private let` string constant",
                    note=(
                        "publish-swift.yml rewrites this constant by regex; "
                        "renaming or reformatting it makes that rewrite a "
                        "silent no-op."
                    ),
                )
            )
    if url_match is None or checksum_match is None:
        return out

    return out + classify_release_pin(
        path,
        url_match.group(1),
        checksum_match.group(1),
        expected,
        published_releases(),
    )


def swift_documentation_files() -> list[Path]:
    """The files the release-tag scan covers, in a stable order."""
    out: list[Path] = []
    root = ROOT / SWIFT_PACKAGE.parent
    if root.is_dir():
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            if SWIFT_SKIP_DIRS.intersection(path.relative_to(root).parts):
                continue
            if path.suffix in BINARY_SUFFIXES:
                continue
            out.append(path)
    doc = ROOT / SWIFT_INSTALL_DOC
    if doc.is_file():
        out.append(doc)
    return out


def wanted_tag(expected: str) -> str:
    """The `vX.Y.Z` a document may name.

    A prerelease workspace version is compared on its release triple, so
    a checkout on `0.71.0-rc.1` accepts documentation that says
    `v0.71.0`. The scan's job is to keep a stale release out of the
    docs, and the triple is what identifies the release.
    """
    parsed = parse_version(expected)
    if parsed is None:
        return f"v{expected}"
    return "v" + ".".join(str(part) for part in parsed.release)


def pin_line_numbers(text: str) -> set[int]:
    """The 1-based lines the two pin declarations span in `text`.

    A declaration and its value need not share a line: neither value fits
    the 100 column limit beside its name, so both sit on a continuation
    line. A `swift-format-ignore` directive cannot buy the single-line
    form back, because LineLength is enforced by the pretty-printer
    rather than by the node rules that directive suppresses. Matching the
    whole span rather than one line keeps the exemption on the value
    wherever it is written.
    """
    lines: set[int] = set()
    for pattern in (PIN_URL, PIN_CHECKSUM):
        match = pattern.search(text)
        if match is None:
            continue
        first = text.count("\n", 0, match.start()) + 1
        last = text.count("\n", 0, match.end()) + 1
        lines.update(range(first, last + 1))
    return lines


def scan_line(line: str, wanted: str, *, exempt: bool) -> list[str]:
    """Release tags on one line naming something other than `wanted`.

    `exempt` is true only for the lines the two pin declarations span in
    `Package.swift`. Anywhere else the same text is documentation, and
    pasting the `.binaryTarget` snippet into a README is the natural
    thing to do, so exempting it by shape rather than by position left a
    permanently stale example.
    """
    if exempt:
        return []
    found: list[str] = []
    for match in DOC_RELEASE_TAG.finditer(line):
        if f"v{match.group(1)}" != wanted:
            found.append(match.group(0))
    _, separator, tail = line.partition(BOOTSTRAP_SCRIPT)
    if separator:
        for token in tail.split():
            # An invocation is usually written inside inline code or ends
            # a sentence, so the argument arrives wrapped in punctuation.
            token = token.strip("`'\"(),.;:[]{}<>")
            argument = BARE_VERSION_ARG.fullmatch(token)
            if argument is None:
                continue
            if f"v{argument.group(1)}" != wanted and token not in found:
                found.append(token)
    return found


def check_swift_doc_versions(expected: str) -> list[Mismatch]:
    """No hard-coded release tag may name anything but the workspace
    version.

    Four of these had already rotted, and inconsistently: three named one
    minor behind the workspace and the fourth named another version
    again. The form that cannot rot is available in every case, since
    `bootstrap/fetch-bindist.sh` defaults its version to the workspace
    `Cargo.toml`, so a doc that names a version is choosing to rot.

    One literal is exempt: `Package.swift`'s release pin, governed by
    `check_swift_release_pin`, whose whole point is that the pin is
    allowed to lag.
    """
    out: list[Mismatch] = []
    wanted = wanted_tag(expected)
    package = ROOT / SWIFT_PACKAGE
    for path in swift_documentation_files():
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        exempt_lines = pin_line_numbers(text) if path == package else set()
        for number, line in enumerate(text.splitlines(), start=1):
            for literal in scan_line(line, wanted, exempt=number in exempt_lines):
                out.append(
                    Mismatch(
                        path=path,
                        field=f"line {number}",
                        found=literal,
                        expected=wanted,
                        note=(
                            "a release tag written into the Swift docs rots "
                            "the moment the workspace bumps. "
                            "bootstrap/fetch-bindist.sh defaults its version "
                            "to the workspace Cargo.toml and reads its "
                            "arguments by shape, so both the version and the "
                            "variant can be left unnamed: prefer the form "
                            "that names neither."
                        ),
                    )
                )
    return out


# --- self-test --------------------------------------------------------
#
# The Swift rules are the only ones in this file that judge a value
# against a record rather than comparing two strings, so they are the
# only ones with states worth enumerating. Everything below drives the
# pure functions above; it touches no files and needs no fixtures, which
# is what lets it run as a flag on the script rather than as a separate
# harness with a dependency to install.

SELF_TEST_CHECKSUM = "63ed84c9dd56e22fb5dc21807ed583987b283e3af7a23d2d8f2a8b5cc1dc55cc"
SELF_TEST_PATH = Path("/nonexistent/Package.swift")


def release_url(tag: str) -> str:
    return (
        "https://github.com/panproto/panproto/releases/download/"
        f"{tag}/panproto_c.xcframework.zip"
    )


# (label, workspace, pinned tag, published tags or None, should fail)
LAG_CASES: tuple[tuple[str, str, str, tuple[str, ...] | None, bool], ...] = (
    ("caught up at the current release", "0.70.0", "v0.70.0", ("v0.70.0",), False),
    ("lagging one release after a patch bump", "0.70.1", "v0.70.0", ("v0.70.0",), False),
    (
        "pin job has run for this release",
        "0.70.1",
        "v0.70.1",
        ("v0.70.0", "v0.70.1"),
        False,
    ),
    (
        "lagging one release after a minor bump",
        "0.71.0",
        "v0.70.1",
        ("v0.70.0", "v0.70.1"),
        False,
    ),
    (
        "pin job failed on an intervening release",
        "0.71.0",
        "v0.70.0",
        ("v0.70.0", "v0.70.1"),
        True,
    ),
    (
        "two bumps with one release between them",
        "0.72.0",
        "v0.70.1",
        ("v0.70.0", "v0.70.1"),
        False,
    ),
    (
        "abandoned tag superseded by a patch bump",
        "0.71.1",
        "v0.70.1",
        ("v0.70.0", "v0.70.1"),
        False,
    ),
    (
        "tag pushed, pin job still running",
        "0.71.0",
        "v0.70.1",
        ("v0.70.0", "v0.70.1", "v0.71.0"),
        False,
    ),
    ("pin ahead of the workspace", "0.70.1", "v0.71.0", ("v0.70.0",), True),
    ("pin ahead with no release record", "0.70.1", "v0.71.0", None, True),
    ("release record unavailable leaves the lag unjudged", "0.71.0", "v0.70.0", None, False),
    ("0.9.0 precedes 0.10.0", "0.10.0", "v0.9.0", ("v0.9.0",), False),
    ("0.6.0 precedes 0.60.0", "0.60.0", "v0.6.0", ("v0.6.0",), False),
    ("0.70.10 is ahead of 0.70.9", "0.70.9", "v0.70.10", ("v0.70.9",), True),
    (
        "a skipped release is found numerically, not lexically",
        "0.70.11",
        "v0.70.9",
        ("v0.70.9", "v0.70.10"),
        True,
    ),
    (
        "a prerelease precedes the release of its triple",
        "0.71.0",
        "v0.71.0-rc.1",
        ("v0.71.0-rc.1",),
        False,
    ),
    ("a prerelease pin may still point forward", "0.70.1", "v0.71.0-rc.1", ("v0.70.0",), True),
    (
        "build metadata parses, so a skip behind it is still caught",
        "0.71.0",
        "v0.70.0",
        ("v0.70.0", "v0.70.1+build.5"),
        True,
    ),
    ("a tag semver cannot order is left alone", "0.71.0", "v0.70.1.4", ("v0.70.0",), False),
    ("a tag with no separator is left alone", "0.71.0", "v0.70.1rc1", ("v0.70.0",), False),
)

# (label, workspace, url, checksum, should fail)
SHAPE_CASES: tuple[tuple[str, str, str, str, bool], ...] = (
    ("empty pin below the first Swift release", "0.69.1", "", "", False),
    ("empty pin at the first Swift release", "0.70.0", "", "", True),
    ("empty pin after the Swift package shipped", "0.70.1", "", "", True),
    ("checksum without a URL", "0.70.1", "", SELF_TEST_CHECKSUM, True),
    ("URL without a checksum", "0.70.1", release_url("v0.70.0"), "", True),
    (
        "URL on another host",
        "0.70.1",
        "https://example.com/panproto_c.xcframework.zip",
        SELF_TEST_CHECKSUM,
        True,
    ),
    (
        "URL naming another asset",
        "0.70.1",
        "https://github.com/panproto/panproto/releases/download/"
        "v0.70.0/panproto_c-full.xcframework.zip",
        SELF_TEST_CHECKSUM,
        True,
    ),
    (
        "uppercase checksum",
        "0.70.1",
        release_url("v0.70.0"),
        SELF_TEST_CHECKSUM.upper(),
        True,
    ),
    (
        "truncated checksum",
        "0.70.1",
        release_url("v0.70.0"),
        SELF_TEST_CHECKSUM[:32],
        True,
    ),
    ("a well-formed lagging pin", "0.70.1", release_url("v0.70.0"), SELF_TEST_CHECKSUM, False),
)

# (line, is Package.swift, offending literals) against a 0.70.1 workspace
DOC_CASES: tuple[tuple[str, bool, tuple[str, ...]], ...] = (
    ("The workspace is at v0.70.1.", False, ()),
    ("Since v0.70.1, the pin is machine-written.", False, ()),
    ("Released as (v0.70.1) yesterday.", False, ()),
    ("Use the v0.70.1-based toolchain.", False, ()),
    ("curl -LO panproto-c-v0.70.1-aarch64-apple-darwin.tar.gz", False, ()),
    ("The asset is panproto_c-v0.70.1-full.xcframework.zip", False, ()),
    ("https://github.com/panproto/panproto/tree/v0.70.1/bindings/swift", False, ()),
    ("Requires v0.70.10 of the engine.", False, ("v0.70.10",)),
    ("Superseded in v0.69.0 and again in v0.68.0.", False, ("v0.69.0", "v0.68.0")),
    ("./bootstrap/fetch-bindist.sh v0.69.0 full", False, ("v0.69.0",)),
    ("./bootstrap/fetch-bindist.sh 0.69.0 full", False, ("0.69.0",)),
    ("Run `./bootstrap/fetch-bindist.sh 0.69.0`, then build.", False, ("0.69.0",)),
    ("Run `./bootstrap/fetch-bindist.sh 0.70.1`, then build.", False, ()),
    ("`./bootstrap/fetch-bindist.sh 0.69.0 full`", False, ("0.69.0",)),
    ("./bootstrap/fetch-bindist.sh full", False, ()),
    ("./bootstrap/fetch-bindist.sh --xcframework", False, ()),
    ("./bootstrap/fetch-bindist.sh 0.70.1 full", False, ()),
    (
        '.package(url: "https://github.com/swiftlang/swift-docc-plugin", from: "1.4.0")',
        False,
        (),
    ),
    ("platforms: [.macOS(.v14), .iOS(.v17)],", False, ()),
    ("// swift-tools-version: 6.0", False, ()),
    ("rev0.69.0 is not a release tag", False, ()),
    (f'private let releaseXCFrameworkURL = "{release_url("v0.70.0")}"', True, ()),
    (
        f'private let releaseXCFrameworkURL = "{release_url("v0.70.0")}"',
        False,
        ("v0.70.0",),
    ),
)

# Manifest shapes all three of `check.py`, the pin job's rewrite, and the
# mirror guard have to read identically.
PIN_LINE_VARIANTS: tuple[str, ...] = (
    'private let releaseXCFrameworkURL = "PIN"',
    '    private let releaseXCFrameworkURL = "PIN"',
    'private let releaseXCFrameworkURL = "PIN"  // rewritten by publish-swift.yml',
    'private let releaseXCFrameworkURL = "PIN"   ',
    # The shipped form: the value does not fit beside its name inside the
    # column limit, so it sits on a continuation line.
    'private let releaseXCFrameworkURL =\n    "PIN"',
)

# `pin_line_numbers` has to find both declarations whether or not each
# shares a line with its value, because the doc scan exempts exactly the
# lines it returns and a miss turns the pin itself into a stale literal.
_SPAN_URL = f'private let releaseXCFrameworkURL =\n    "{release_url("v0.70.0")}"'
_SPAN_SUM = f'private let releaseXCFrameworkChecksum =\n    "{"a" * 64}"'
PIN_SPAN_CASES: tuple[tuple[str, str, set[int]], ...] = (
    ("both broken", f"// header\n{_SPAN_URL}\n{_SPAN_SUM}\n", {2, 3, 4, 5}),
    (
        "both inline",
        f'private let releaseXCFrameworkURL = "{release_url("v0.70.0")}"\n'
        f'private let releaseXCFrameworkChecksum = "{"a" * 64}"\n',
        {1, 2},
    ),
    (
        "mixed",
        f'private let releaseXCFrameworkURL = "{release_url("v0.70.0")}"\n{_SPAN_SUM}\n',
        {1, 2, 3},
    ),
    ("neither present", "// nothing to pin here\n", set()),
)

ORDER_CASES: tuple[tuple[str, str], ...] = (
    ("0.9.0", "0.10.0"),
    ("0.6.0", "0.60.0"),
    ("0.70.9", "0.70.10"),
    ("1.0.0-rc.2", "1.0.0-rc.10"),
    ("1.0.0-rc.1", "1.0.0"),
    ("0.70.1", "0.71.0"),
)


def self_test(*, verbose: bool) -> int:
    """Exercise the Swift pin and documentation rules against a table of
    states. Returns the number of failures."""
    failures = 0

    def report(group: str, label: str, detail: str) -> None:
        nonlocal failures
        failures += 1
        print(f"  FAIL [{group}] {label}: {detail}", file=sys.stderr)

    for lower, higher in ORDER_CASES:
        low, high = parse_version(lower), parse_version(higher)
        if low is None or high is None:
            report("ordering", f"{lower} < {higher}", "did not parse")
        elif not low.sort_key < high.sort_key:
            report("ordering", f"{lower} < {higher}", "compared the wrong way")

    for label, workspace, tag, tags, should_fail in LAG_CASES:
        published = (
            None
            if tags is None
            else sorted(
                (v for t in tags if (v := parse_version(t)) is not None),
                key=lambda v: v.sort_key,
                reverse=True,
            )
        )
        found = classify_release_pin(
            SELF_TEST_PATH, release_url(tag), SELF_TEST_CHECKSUM, workspace, published
        )
        if bool(found) != should_fail:
            report(
                "lag",
                label,
                f"workspace {workspace}, pin {tag}, published {tags}: "
                f"expected {'a failure' if should_fail else 'a pass'}, "
                f"got {[m.field for m in found] or 'a pass'}",
            )

    for label, workspace, url, checksum, should_fail in SHAPE_CASES:
        found = classify_release_pin(SELF_TEST_PATH, url, checksum, workspace, [])
        if bool(found) != should_fail:
            report(
                "shape",
                label,
                f"expected {'a failure' if should_fail else 'a pass'}, "
                f"got {[m.field for m in found] or 'a pass'}",
            )

    for line, exempt, expected_hits in DOC_CASES:
        found = tuple(scan_line(line, "v0.70.1", exempt=exempt))
        if found != expected_hits:
            report("docs", line, f"expected {expected_hits}, got {found}")

    for label, manifest, expected_lines in PIN_SPAN_CASES:
        found_lines = pin_line_numbers(manifest)
        if found_lines != expected_lines:
            report("pin span", label, f"expected {expected_lines}, got {found_lines}")

    for variant in PIN_LINE_VARIANTS:
        match = PIN_URL.search(variant)
        if match is None or match.group(1) != "PIN":
            report("pin pattern", variant, "did not yield the pinned value")

    total = len(ORDER_CASES) + len(LAG_CASES) + len(SHAPE_CASES) + len(DOC_CASES)
    total += len(PIN_LINE_VARIANTS)
    if failures:
        print(f"self-test FAILED: {failures} of {total} cases", file=sys.stderr)
    elif verbose:
        print(f"OK: {total} self-test cases pass")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verbose", action="store_true")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="exercise the Swift pin and documentation rules and exit",
    )
    args = parser.parse_args()

    if args.self_test:
        return 1 if self_test(verbose=args.verbose) else 0

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
    # The Swift pin is machine-written after the tag, so it is checked
    # against a permitted lag rather than against equality; the Swift
    # docs get no such licence.
    mismatches.extend(check_swift_release_pin(expected))
    mismatches.extend(check_swift_doc_versions(expected))

    if mismatches:
        print(
            f"version-consistency check FAILED (workspace = {expected!r})",
            file=sys.stderr,
        )
        for m in mismatches:
            print(m, file=sys.stderr)
        return 1

    if args.verbose:
        # Name the release record the Swift lag rule ran against. Without
        # tags the rule still catches a forward or malformed pin but
        # cannot see a skipped release, and a silent downgrade of that
        # coverage is exactly the rot this script exists to prevent.
        published = published_releases()
        record = (
            f"{len(published)} release tags"
            if published is not None
            else "no release tags visible, so a skipped release cannot be detected"
        )
        print(f"OK: every version-declaring file pins {expected!r} ({record})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
