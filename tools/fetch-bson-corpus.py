#!/usr/bin/env python3
"""Fetch the BSON corpus of the MongoDB specifications at its recorded commit.

The corpus (`source/bson-corpus/tests` of mongodb/specifications) is
licensed under the Creative Commons Attribution-NonCommercial-ShareAlike 3.0
United States License, so it is not kept in this repository. The checked-in
manifest `crates/panproto-io/tests/fixtures/conformance/bson-corpus/MANIFEST`
records the repository, the commit and the directory on its `source` line,
and each file of the corpus with its SHA-256 digest. This script:

1. Downloads each file the manifest lists from the recorded commit;
2. Checks its SHA-256 digest against the manifest, failing on any mismatch;
3. Puts the files in `bson-corpus/tests/` beside the manifest (a directory
   git ignores) and records the commit in `bson-corpus/tests/REVISION`.

When the directory already holds every file with its recorded digest and
the recorded commit, nothing is downloaded. `conformance_bson.rs` reads the
directory; with `PANPROTO_BSON_CORPUS=required` set it fails when the
directory is absent, and otherwise reports itself skipped.

Usage:
    python3 tools/fetch-bson-corpus.py           # fetch, or confirm the fetched copy
    python3 tools/fetch-bson-corpus.py --check   # verify the fetched copy, no network
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import sys
import urllib.request
from pathlib import Path

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
CORPUS_DIR = (
    WORKSPACE_ROOT / "crates" / "panproto-io" / "tests" / "fixtures" / "conformance" / "bson-corpus"
)
MANIFEST = CORPUS_DIR / "MANIFEST"
DEST = CORPUS_DIR / "tests"
USER_AGENT = "panproto-fetch-bson-corpus"
GITHUB = "https://github.com/"


class Manifest:
    """The recorded source and the file digests of the manifest."""

    def __init__(self, path: Path) -> None:
        self.repository = ""
        self.commit = ""
        self.directory = ""
        self.digests: dict[str, str] = {}
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip() or line.startswith("#"):
                continue
            fields = line.split("\t")
            if fields[0] == "source":
                if len(fields) != 4:
                    raise ValueError(f"{path}:{number}: a source line has four fields")
                self.repository, self.commit, self.directory = fields[1:]
                continue
            if len(fields) != 6:
                raise ValueError(f"{path}:{number}: a file line has six fields")
            name, digest = fields[0], fields[5]
            if "/" in name or name.startswith(".") or len(digest) != 64:
                raise ValueError(f"{path}:{number}: malformed file line")
            self.digests[name] = digest
        if not (self.repository.startswith(GITHUB) and len(self.commit) == 40 and self.digests):
            raise ValueError(f"{path}: no source line with a GitHub repository and a full commit")

    def raw_url(self, name: str) -> str:
        slug = self.repository.removeprefix(GITHUB).removesuffix("/")
        return f"https://raw.githubusercontent.com/{slug}/{self.commit}/{self.directory}/{name}"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def problems(manifest: Manifest, directory: Path) -> list[str]:
    """Every way the files under `directory` differ from the manifest."""
    if not directory.is_dir():
        return [f"{directory} does not exist"]
    found = []
    revision = directory / "REVISION"
    if not revision.is_file() or revision.read_text(encoding="utf-8").strip() != manifest.commit:
        found.append(f"{revision} does not name commit {manifest.commit}")
    present = {p.name for p in directory.iterdir() if p.name != "REVISION"}
    for extra in sorted(present - manifest.digests.keys()):
        found.append(f"{extra} is not in the manifest")
    for name, digest in sorted(manifest.digests.items()):
        path = directory / name
        if not path.is_file():
            found.append(f"{name} is missing")
        elif sha256(path.read_bytes()) != digest:
            found.append(f"{name} does not have its recorded SHA-256")
    return found


def download(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def fetch(manifest: Manifest) -> None:
    partial = DEST.with_name(DEST.name + ".partial")
    if partial.exists():
        shutil.rmtree(partial)
    partial.mkdir()
    try:
        for name, digest in sorted(manifest.digests.items()):
            data = download(manifest.raw_url(name))
            got = sha256(data)
            if got != digest:
                raise ValueError(f"{name}: SHA-256 {got}, the manifest records {digest}")
            (partial / name).write_bytes(data)
        (partial / "REVISION").write_text(manifest.commit + "\n", encoding="utf-8")
        if DEST.exists():
            shutil.rmtree(DEST)
        partial.rename(DEST)
    finally:
        if partial.exists():
            shutil.rmtree(partial)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the fetched copy against the manifest without downloading",
    )
    args = parser.parse_args()
    manifest = Manifest(MANIFEST)
    found = problems(manifest, DEST)
    if not found:
        print(f"bson-corpus: {len(manifest.digests)} files at {manifest.commit[:8]}, verified")
        return 0
    if args.check:
        for problem in found:
            print(f"bson-corpus: {problem}", file=sys.stderr)
        return 1
    print(f"bson-corpus: fetching {len(manifest.digests)} files at {manifest.commit[:8]}")
    try:
        fetch(manifest)
    except (OSError, ValueError) as error:
        print(f"bson-corpus: {error}", file=sys.stderr)
        return 1
    found = problems(manifest, DEST)
    for problem in found:
        print(f"bson-corpus: {problem}", file=sys.stderr)
    if found:
        return 1
    print(f"bson-corpus: {len(manifest.digests)} files at {manifest.commit[:8]}, verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
