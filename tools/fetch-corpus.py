#!/usr/bin/env python3
"""Fetch tree-sitter `test/corpus/` files for grammars listed in grammars.toml.

The corpus files are the grammar authors' own test inputs: dozens to hundreds
of snippets exercising every construct. They are the right input for a real
emit-verification audit (vs. a single hand-written hello-world sample).

Copies `{directory}/test/corpus/**/*.txt` into `grammars/<name>/test/corpus/`.

Usage:
    python3 tools/fetch-corpus.py python rust kotlin   # specific grammars
    python3 tools/fetch-corpus.py --all                # every grammar in toml
"""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ImportError:
    print("Python 3.11+ required")
    sys.exit(1)

WORKSPACE_ROOT = Path(__file__).resolve().parent.parent
GRAMMARS_TOML = WORKSPACE_ROOT / "grammars.toml"
GRAMMARS_DIR = WORKSPACE_ROOT / "grammars"


def load_manifest() -> dict:
    with open(GRAMMARS_TOML, "rb") as f:
        return tomllib.load(f)


def fetch_corpus(name: str, spec: dict) -> int:
    repo = spec["repo"]
    directory = spec.get("directory", "")
    dest = GRAMMARS_DIR / name / "test" / "corpus"

    # Already present (in-repo grammars, or a prior fetch).
    if dest.exists() and any(dest.rglob("*.txt")):
        n = len(list(dest.rglob("*.txt")))
        print(f"  {name}: already have {n} corpus file(s)")
        return n

    print(f"  {name}: cloning {repo} ...", end=" ", flush=True)
    with tempfile.TemporaryDirectory() as tmp:
        clone = Path(tmp) / "repo"
        r = subprocess.run(
            ["git", "clone", "--depth=1", "--single-branch", "--quiet", repo, str(clone)],
            capture_output=True,
            text=True,
        )
        if r.returncode != 0:
            print(f"FAILED (clone: {r.stderr.strip()[:80]})")
            return 0
        root = clone / directory if directory else clone
        # tree-sitter corpus lives at test/corpus or (older) corpus/.
        # When a grammar is a subdirectory of a multi-grammar repo (directory
        # set), the corpus frequently lives at the REPO ROOT test/corpus rather
        # than the directory-scoped path (typescript, tsx, php, ocaml, fsharp).
        # Check the directory-scoped paths first, then fall back to repo root.
        candidates = [root / "test" / "corpus", root / "corpus"]
        if directory:
            candidates += [clone / "test" / "corpus", clone / "corpus"]
        src = next((c for c in candidates if c.exists()), None)
        if src is None:
            print("no corpus dir")
            return 0
        txts = list(src.rglob("*.txt")) + list(src.rglob("*.scm"))
        if not txts:
            print("empty corpus")
            return 0
        dest.mkdir(parents=True, exist_ok=True)
        for t in txts:
            rel = t.relative_to(src)
            out = dest / rel
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(t, out)
        print(f"copied {len(txts)} file(s)")
        return len(txts)


def main() -> None:
    args = sys.argv[1:]
    manifest = load_manifest()
    names = list(manifest.keys()) if (not args or args == ["--all"]) else args
    total = 0
    ok = 0
    for name in names:
        if name not in manifest:
            print(f"  {name}: not in grammars.toml")
            continue
        n = fetch_corpus(name, manifest[name])
        total += 1
        if n > 0:
            ok += 1
    print(f"\nfetched corpus for {ok}/{total} grammars")


if __name__ == "__main__":
    main()
