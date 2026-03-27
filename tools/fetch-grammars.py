#!/usr/bin/env python3
"""Fetch tree-sitter grammar sources from git repos based on grammars.toml.

For each grammar entry, this script:
1. Clones the repo at the default branch (shallow)
2. Copies src/parser.c, src/scanner.c or src/scanner.cc (if present),
   src/node-types.json, and src/tree_sitter/parser.h into grammars/{lang}/src/
3. Copies the LICENSE file into grammars/{lang}/

Usage:
    python3 tools/fetch-grammars.py              # fetch all grammars
    python3 tools/fetch-grammars.py python rust   # fetch specific grammars
    python3 tools/fetch-grammars.py --dry-run     # show what would be fetched
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
    try:
        import tomli as tomllib  # type: ignore[no-redef]
    except ImportError:
        print("Python 3.11+ required (for tomllib), or install tomli: pip install tomli")
        sys.exit(1)

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
GRAMMARS_TOML = WORKSPACE_ROOT / "grammars.toml"
GRAMMARS_DIR = WORKSPACE_ROOT / "grammars"

PERMISSIVE_LICENSES = {
    "mit",
    "apache",
    "bsd",
    "isc",
    "unlicense",
    "public domain",
    "cc0",
    "0bsd",
    "zlib",
    "boost",
}


def load_manifest() -> dict[str, dict]:
    with open(GRAMMARS_TOML, "rb") as f:
        return tomllib.load(f)


def is_permissive_license(license_text: str) -> bool:
    lower = license_text.lower()
    return any(tag in lower for tag in PERMISSIVE_LICENSES)


def fetch_grammar(name: str, spec: dict, dry_run: bool = False) -> bool:
    repo = spec["repo"]
    extensions = spec.get("extensions", [])
    directory = spec.get("directory", "")
    dest = GRAMMARS_DIR / name / "src"

    if dry_run:
        ext_str = ", ".join(extensions) if extensions else "(injection)"
        print(f"  {name}: {repo} -> grammars/{name}/src/ [{ext_str}]")
        return True

    print(f"  Fetching {name} from {repo}...", end=" ", flush=True)

    with tempfile.TemporaryDirectory() as tmpdir:
        clone_dir = Path(tmpdir) / "repo"

        result = subprocess.run(
            ["git", "clone", "--depth=1", "--single-branch", "--quiet", repo, str(clone_dir)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"FAILED (clone: {result.stderr.strip()})")
            return False

        src_dir = clone_dir / directory / "src" if directory else clone_dir / "src"
        parser_c = src_dir / "parser.c"
        node_types = src_dir / "node-types.json"

        if not parser_c.exists():
            print(f"FAILED (no src/parser.c in {directory or 'root'})")
            return False

        if not node_types.exists():
            print(f"FAILED (no src/node-types.json in {directory or 'root'})")
            return False

        dest.mkdir(parents=True, exist_ok=True)

        shutil.copy2(parser_c, dest / "parser.c")
        shutil.copy2(node_types, dest / "node-types.json")

        scanner_c = src_dir / "scanner.c"
        scanner_cc = src_dir / "scanner.cc"
        if scanner_c.exists():
            shutil.copy2(scanner_c, dest / "scanner.c")
        if scanner_cc.exists():
            shutil.copy2(scanner_cc, dest / "scanner.cc")

        ts_dir = src_dir / "tree_sitter"
        if ts_dir.is_dir():
            dest_ts = dest / "tree_sitter"
            dest_ts.mkdir(parents=True, exist_ok=True)
            for header in ts_dir.glob("*.h"):
                shutil.copy2(header, dest_ts / header.name)

        # Copy shared directories (common/, etc.) from repo root into the
        # grammar's src/ directory. This handles grammars like PHP where the
        # scanner includes headers from ../../common/ relative to the subdirectory.
        for shared_name in ["common"]:
            shared_dir = clone_dir / shared_name
            if shared_dir.is_dir():
                dest_shared = dest / shared_name
                if dest_shared.exists():
                    shutil.rmtree(dest_shared)
                shutil.copytree(shared_dir, dest_shared)

        # Rewrite relative includes that point outside src/ to point to local copies.
        # e.g., #include "../../common/scanner.h" -> #include "common/scanner.h"
        for src_file in [dest / "scanner.c", dest / "scanner.cc", dest / "parser.c"]:
            if src_file.exists():
                content = src_file.read_text(errors="replace")
                import re
                rewritten = re.sub(
                    r'#include\s+"(\.\./)+common/',
                    '#include "common/',
                    content,
                )
                if rewritten != content:
                    src_file.write_text(rewritten)
                    print(f"(rewrote includes) ", end="", flush=True)

        for license_name in ["LICENSE", "LICENSE.md", "LICENSE.txt", "COPYING", "license"]:
            license_file = clone_dir / license_name
            if license_file.exists():
                license_text = license_file.read_text(errors="replace")
                if not is_permissive_license(license_text):
                    print(f"WARNING (non-permissive license)")
                    shutil.rmtree(dest.parent, ignore_errors=True)
                    return False
                shutil.copy2(license_file, dest.parent / "LICENSE")
                break

        resolved_rev = subprocess.run(
            ["git", "-C", str(clone_dir), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
        ).stdout.strip()

        (dest.parent / "REVISION").write_text(f"{resolved_rev}\n")

        print(f"OK ({resolved_rev[:8]})")
        return True


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch tree-sitter grammar sources")
    parser.add_argument("languages", nargs="*", help="Specific languages to fetch (default: all)")
    parser.add_argument("--dry-run", action="store_true", help="Show what would be fetched")
    parser.add_argument("--clean", action="store_true", help="Remove grammars/ before fetching")
    args = parser.parse_args()

    manifest = load_manifest()

    if args.languages:
        for lang in args.languages:
            if lang not in manifest:
                print(f"Error: unknown language '{lang}' (not in grammars.toml)")
                sys.exit(1)
        languages = {k: manifest[k] for k in args.languages}
    else:
        languages = manifest

    if args.clean and not args.dry_run:
        if GRAMMARS_DIR.exists():
            print(f"Cleaning {GRAMMARS_DIR}...")
            shutil.rmtree(GRAMMARS_DIR)

    GRAMMARS_DIR.mkdir(parents=True, exist_ok=True)

    total = len(languages)
    success = 0
    failed = []

    print(f"Fetching {total} grammars into {GRAMMARS_DIR}/\n")

    for name, spec in sorted(languages.items()):
        if fetch_grammar(name, spec, dry_run=args.dry_run):
            success += 1
        else:
            failed.append(name)

    print(f"\nDone: {success}/{total} succeeded")
    if failed:
        print(f"Failed ({len(failed)}): {', '.join(failed)}")
        sys.exit(1)


if __name__ == "__main__":
    main()
