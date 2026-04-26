#!/usr/bin/env python3
"""Fetch grammar.json (only) for every grammar in grammars.toml.

Tree-sitter's `grammar.json` is the machine-readable production-rule
table that the panproto-grammars build uses to distill emit-tables for
panproto-parse's `emit_pretty`. Most upstream tree-sitter-* repos ship
it under `src/grammar.json`; the few that ship only `grammar.js` are
regenerated via `tree-sitter generate`.

Unlike `tools/fetch-grammars.py`, this script does NOT touch
`parser.c`, `scanner.c`, `node-types.json`, or `tree_sitter/parser.h`
in `grammars/{lang}/src/`. It only writes `grammars/{lang}/src/grammar.json`.

Usage:
    python3 tools/fetch-grammar-json.py            # fetch all
    python3 tools/fetch-grammar-json.py rust go    # fetch specific
    python3 tools/fetch-grammar-json.py --skip-existing
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


def fetch_grammar_json(name: str, spec: dict, root: Path, skip_existing: bool) -> str:
    """Fetch grammar.json for a single grammar.

    Returns one of: "ok", "skip-existing", "no-source", "fail".
    """
    dest = root / "grammars" / name / "src" / "grammar.json"
    if skip_existing and dest.exists():
        return "skip-existing"

    repo = spec["repo"]
    directory = spec.get("directory")

    with tempfile.TemporaryDirectory() as tmp:
        clone_dir = Path(tmp) / name
        result = subprocess.run(
            ["git", "clone", "--depth", "1", repo, str(clone_dir)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"  {name}: clone failed: {result.stderr.strip()[:120]}")
            return "fail"

        grammar_root = clone_dir / directory if directory else clone_dir
        src_dir = grammar_root / "src"

        upstream_json = src_dir / "grammar.json"
        if upstream_json.exists():
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(upstream_json, dest)
            return "ok"

        # Fall back to regeneration when the upstream repo ships only grammar.js.
        grammar_js = grammar_root / "grammar.js"
        if not grammar_js.exists():
            return "no-source"

        gen = subprocess.run(
            ["tree-sitter", "generate"],
            cwd=str(grammar_root),
            capture_output=True,
            text=True,
        )
        if gen.returncode != 0:
            print(f"  {name}: tree-sitter generate failed: {gen.stderr.strip()[:120]}")
            return "fail"

        if upstream_json.exists():
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(upstream_json, dest)
            return "ok"

        return "fail"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("languages", nargs="*", help="Specific languages to fetch (default: all)")
    parser.add_argument(
        "--skip-existing",
        action="store_true",
        help="Skip languages whose grammar.json already exists locally.",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    manifest = tomllib.loads((root / "grammars.toml").read_text())

    if args.languages:
        manifest = {k: v for k, v in manifest.items() if k in args.languages}
        missing = set(args.languages) - set(manifest)
        if missing:
            print(f"unknown languages: {sorted(missing)}", file=sys.stderr)
            return 2

    counts = {"ok": 0, "skip-existing": 0, "no-source": 0, "fail": 0}
    failures: list[str] = []
    no_source: list[str] = []

    total = len(manifest)
    for i, (name, spec) in enumerate(sorted(manifest.items()), 1):
        print(f"[{i:3d}/{total}] {name:30s} ", end="", flush=True)
        outcome = fetch_grammar_json(name, spec, root, args.skip_existing)
        counts[outcome] += 1
        if outcome == "fail":
            failures.append(name)
        elif outcome == "no-source":
            no_source.append(name)
        if outcome != "fail":
            print(outcome)

    print()
    print(f"ok:            {counts['ok']}")
    print(f"skip-existing: {counts['skip-existing']}")
    print(f"no-source:     {counts['no-source']}")
    print(f"failed:        {counts['fail']}")
    if no_source:
        print(f"\nno upstream grammar.js or grammar.json for:\n  {', '.join(no_source)}")
    if failures:
        print(f"\nfailed:\n  {', '.join(failures)}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
