#!/usr/bin/env python3
"""Fetch `queries/*.scm` files from tree-sitter grammar repositories.

Lightweight companion to `fetch-grammars.py`. Shallow-clones each grammar
repo, copies `queries/*.scm` into `grammars/{lang}/queries/`, skips the C
source compilation dance.

Use this after a grammar has been fetched once (its `src/parser.c` etc. are
already vendored) and you want to update query files without re-running the
full grammar fetch.

Usage:
    python3 tools/fetch-query-files.py              # all grammars
    python3 tools/fetch-query-files.py python rust   # specific
    python3 tools/fetch-query-files.py --dry-run
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib  # type: ignore[no-redef]

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
GRAMMARS_TOML = WORKSPACE_ROOT / "grammars.toml"
GRAMMARS_DIR = WORKSPACE_ROOT / "grammars"


def load_manifest() -> dict[str, dict]:
    with open(GRAMMARS_TOML, "rb") as f:
        return tomllib.load(f)


def fetch_queries(name: str, spec: dict, dry_run: bool = False) -> bool:
    repo = spec["repo"]
    directory = spec.get("directory", "")
    dest = GRAMMARS_DIR / name / "queries"

    if dry_run:
        print(f"  {name}: {repo} -> {dest}")
        return True

    print(f"  {name}...", end=" ", flush=True)

    with tempfile.TemporaryDirectory() as tmpdir:
        clone_dir = Path(tmpdir) / "repo"
        result = subprocess.run(
            ["git", "clone", "--depth=1", "--single-branch", "--quiet", repo, str(clone_dir)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"CLONE FAILED ({result.stderr.strip()[:80]})")
            return False

        grammar_root = clone_dir / directory if directory else clone_dir
        # Try queries/ under the grammar subdirectory first (for grammars with
        # per-dialect queries like tree-sitter-ocaml), then fall back to the
        # repo root (the common case, including multi-grammar repos where
        # shared queries live at the top level).
        queries_src = grammar_root / "queries"
        if not queries_src.is_dir():
            queries_src = clone_dir / "queries"
        if not queries_src.is_dir():
            print("no queries/ dir")
            return False

        dest.mkdir(parents=True, exist_ok=True)
        copied = 0
        for item in queries_src.iterdir():
            if item.is_file() and item.suffix == ".scm":
                shutil.copy2(item, dest / item.name)
                copied += 1
            elif item.is_dir():
                for sub in item.iterdir():
                    if sub.is_file() and sub.suffix == ".scm":
                        target = dest / sub.name
                        if not target.exists():
                            shutil.copy2(sub, target)
                            copied += 1

        has_tags = (dest / "tags.scm").exists()
        print(f"OK ({copied} files{', tags.scm' if has_tags else ''})")
        return True


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch tree-sitter queries")
    parser.add_argument("languages", nargs="*", help="Specific languages (default: all)")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--skip-existing",
        action="store_true",
        help="Skip grammars that already have a non-empty queries/ directory",
    )
    args = parser.parse_args()

    manifest = load_manifest()

    if args.languages:
        for lang in args.languages:
            if lang not in manifest:
                print(f"Error: unknown language '{lang}'")
                sys.exit(1)
        languages = {k: manifest[k] for k in args.languages}
    else:
        languages = manifest

    total = len(languages)
    success = 0
    skipped = 0
    failed = []

    print(f"Fetching queries for {total} grammars into {GRAMMARS_DIR}/*/queries/\n")

    for name, spec in sorted(languages.items()):
        dest = GRAMMARS_DIR / name / "queries"
        if args.skip_existing and dest.is_dir() and any(dest.iterdir()):
            skipped += 1
            continue
        if fetch_queries(name, spec, dry_run=args.dry_run):
            success += 1
        else:
            failed.append(name)

    print(f"\nDone: {success}/{total} succeeded, {skipped} skipped")
    if failed:
        print(f"Failed ({len(failed)}): {', '.join(failed)}")


if __name__ == "__main__":
    main()
