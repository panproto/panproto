#!/usr/bin/env python3
"""Fetch tree-sitter grammar sources from git repos based on grammars.toml.

For each grammar entry, this script:
1. Clones the repo (shallow) at the entry's `rev` commit, or at the default
   branch when the entry names none
2. Replaces any Git LFS pointer among the fetched sources with the object it
   names, downloaded through the repository's LFS batch API and checked
   against the pointer's sha256 and size
3. Copies src/parser.c, src/scanner.c or src/scanner.cc (if present),
   src/node-types.json, and src/tree_sitter/parser.h into grammars/{lang}/src/
4. Copies the LICENSE file into grammars/{lang}/

Usage:
    python3 tools/fetch-grammars.py              # fetch all grammars
    python3 tools/fetch-grammars.py python rust   # fetch specific grammars
    python3 tools/fetch-grammars.py --dry-run     # show what would be fetched
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
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
    # The BSD licence texts do not name themselves; their first condition does.
    "redistribution and use in source and binary forms",
}

LFS_POINTER_HEADER = b"version https://git-lfs.github.com/spec/v1\n"
LFS_MEDIA_TYPE = "application/vnd.git-lfs+json"
USER_AGENT = "panproto-fetch-grammars"


def load_manifest() -> dict[str, dict]:
    with open(GRAMMARS_TOML, "rb") as f:
        return tomllib.load(f)


def is_permissive_license(license_text: str) -> bool:
    lower = license_text.lower()
    return any(tag in lower for tag in PERMISSIVE_LICENSES)


def read_lfs_pointer(path: Path) -> tuple[str, int] | None:
    """Return the (sha256, size) a Git LFS pointer file names, or None.

    A pointer is a small text file that starts with the LFS spec line and
    carries `oid sha256:<hex>` and `size <bytes>` lines. A checkout made
    without git-lfs (or with GIT_LFS_SKIP_SMUDGE=1) holds the pointer in
    place of the real file.
    """
    if path.stat().st_size > 1024:
        return None
    data = path.read_bytes()
    if not data.startswith(LFS_POINTER_HEADER):
        return None
    oid = None
    size = None
    for line in data.decode("ascii", errors="replace").splitlines():
        key, _, value = line.partition(" ")
        if key == "oid" and value.startswith("sha256:"):
            oid = value.removeprefix("sha256:")
        elif key == "size" and value.isdigit():
            size = int(value)
    if oid is None or size is None:
        raise ValueError(f"malformed Git LFS pointer at {path}")
    return oid, size


def lfs_batch_url(repo: str) -> str:
    base = repo.removesuffix("/").removesuffix(".git")
    return f"{base}.git/info/lfs/objects/batch"


def resolve_lfs_pointer(repo: str, path: Path) -> bool:
    """Replace the LFS pointer at `path` with the object it names.

    Asks the repository's LFS server (the batch API every LFS host serves)
    for a download action, streams the object into place, and checks its
    sha256 and size against the pointer. Returns False when `path` is not a
    pointer; raises on any download or verification failure.
    """
    pointer = read_lfs_pointer(path)
    if pointer is None:
        return False
    oid, size = pointer
    request_body = json.dumps(
        {
            "operation": "download",
            "transfers": ["basic"],
            "objects": [{"oid": oid, "size": size}],
        }
    ).encode()
    request = urllib.request.Request(
        lfs_batch_url(repo),
        data=request_body,
        method="POST",
        headers={
            "Accept": LFS_MEDIA_TYPE,
            "Content-Type": LFS_MEDIA_TYPE,
            "User-Agent": USER_AGENT,
        },
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        batch = json.load(response)
    objects = batch.get("objects", [])
    if len(objects) != 1 or "error" in objects[0]:
        raise RuntimeError(f"LFS server refused object {oid}: {objects}")
    download = objects[0]["actions"]["download"]
    headers = dict(download.get("header", {}))
    headers["User-Agent"] = USER_AGENT
    object_request = urllib.request.Request(download["href"], headers=headers)

    digest = hashlib.sha256()
    received = 0
    partial = path.with_name(path.name + ".lfs-download")
    with urllib.request.urlopen(object_request, timeout=600) as response, open(partial, "wb") as out:
        while chunk := response.read(1 << 20):
            digest.update(chunk)
            received += len(chunk)
            out.write(chunk)
    if received != size or digest.hexdigest() != oid:
        partial.unlink()
        raise RuntimeError(
            f"LFS object for {path.name} does not match its pointer: "
            f"got {received} bytes with sha256 {digest.hexdigest()}, "
            f"pointer names {size} bytes with sha256 {oid}"
        )
    partial.replace(path)
    return True


def clone_at(repo: str, rev: str | None, clone_dir: Path) -> subprocess.CompletedProcess:
    """Shallow-clone `repo` into `clone_dir`, at `rev` when one is given.

    The LFS filter is switched off for these commands, overriding any global
    git-lfs configuration, so that LFS-tracked files arrive as pointers on
    every machine, with or without git-lfs installed; `resolve_lfs_pointer`
    then fetches and verifies them.
    """
    env = {**os.environ, "GIT_LFS_SKIP_SMUDGE": "1"}
    git = [
        "git",
        "-c",
        "filter.lfs.process=",
        "-c",
        "filter.lfs.smudge=",
        "-c",
        "filter.lfs.required=false",
    ]
    if rev is None:
        return subprocess.run(
            [*git, "clone", "--depth=1", "--single-branch", "--quiet", repo, str(clone_dir)],
            capture_output=True,
            text=True,
            env=env,
        )
    clone_dir.mkdir(parents=True)
    for command in (
        [*git, "init", "--quiet"],
        [*git, "remote", "add", "origin", repo],
        [*git, "fetch", "--depth=1", "--quiet", "origin", rev],
        [*git, "checkout", "--quiet", "FETCH_HEAD"],
    ):
        result = subprocess.run(command, cwd=clone_dir, capture_output=True, text=True, env=env)
        if result.returncode != 0:
            return result
    return result


def fetch_grammar(name: str, spec: dict, dry_run: bool = False) -> bool:
    repo = spec["repo"]
    rev = spec.get("rev")
    extensions = spec.get("extensions", [])
    directory = spec.get("directory", "")
    dest = GRAMMARS_DIR / name / "src"

    if dry_run:
        ext_str = ", ".join(extensions) if extensions else "(injection)"
        at = f"@{rev[:8]}" if rev else ""
        print(f"  {name}: {repo}{at} -> grammars/{name}/src/ [{ext_str}]")
        return True

    # If the grammar is authored in-repo (`grammars/<name>/grammar.js`
    # is checked in alongside a generated `src/parser.c`), skip the
    # fetch entirely. This catches the self-referential entries for
    # `tidal_mini` and `strudel_mini`, whose `repo` URL points at
    # panproto itself purely so the manifest is self-consistent; doing
    # an actual clone-and-copy here would download the whole panproto
    # repo just to copy back files it already contains.
    local_grammar_js = GRAMMARS_DIR / name / "grammar.js"
    local_parser_c = dest / "parser.c"
    if local_grammar_js.exists() and local_parser_c.exists():
        print(f"  {name}: skipping (authored in-repo, parser.c already present)")
        return True

    print(f"  Fetching {name} from {repo}...", end=" ", flush=True)

    with tempfile.TemporaryDirectory() as tmpdir:
        clone_dir = Path(tmpdir) / "repo"

        result = clone_at(repo, rev, clone_dir)
        if result.returncode != 0:
            print(f"FAILED (clone: {result.stderr.strip()})")
            return False

        grammar_root = clone_dir / directory if directory else clone_dir
        src_dir = grammar_root / "src"

        if src_dir.is_dir():
            for item in sorted(src_dir.rglob("*")):
                if item.is_file() and not item.is_symlink():
                    try:
                        if resolve_lfs_pointer(repo, item):
                            print(f"(LFS {item.name}) ", end="", flush=True)
                    except (OSError, RuntimeError, ValueError, KeyError) as error:
                        print(f"FAILED (LFS {item.name}: {error})")
                        return False
        parser_c = src_dir / "parser.c"
        node_types = src_dir / "node-types.json"

        # If parser.c is missing, run tree-sitter generate to produce it.
        if not parser_c.exists():
            grammar_js = grammar_root / "grammar.js"
            grammar_json = grammar_root / "grammar.json"
            if grammar_js.exists() or grammar_json.exists():
                print("(generating) ", end="", flush=True)
                gen_result = subprocess.run(
                    ["tree-sitter", "generate"],
                    cwd=str(grammar_root),
                    capture_output=True,
                    text=True,
                )
                if gen_result.returncode != 0:
                    print(f"FAILED (tree-sitter generate: {gen_result.stderr.strip()[:200]})")
                    return False
            else:
                print(f"FAILED (no src/parser.c or grammar.js in {directory or 'root'})")
                return False

        if not parser_c.exists():
            print(f"FAILED (parser.c not generated)")
            return False

        if not node_types.exists():
            print(f"FAILED (no src/node-types.json in {directory or 'root'})")
            return False

        dest.mkdir(parents=True, exist_ok=True)

        shutil.copy2(node_types, dest / "node-types.json")

        # Copy all C/C++ source files, headers, and subdirectories from src/.
        # This captures parser.c, scanner.c, scanner.cc, auxiliary .c files
        # (e.g., yaml's schema.core.c), headers (e.g., html's tag.h), and
        # subdirectories with include files (e.g., comment's tree_sitter_comment/).
        for item in src_dir.iterdir():
            if item.name == "tree_sitter":
                continue  # Handled separately below
            dest_item = dest / item.name
            if item.is_dir():
                if dest_item.exists():
                    shutil.rmtree(dest_item)
                shutil.copytree(item, dest_item)
            elif item.suffix in (".c", ".cc", ".h", ".json"):
                shutil.copy2(item, dest_item)

        ts_dir = src_dir / "tree_sitter"
        if ts_dir.is_dir():
            dest_ts = dest / "tree_sitter"
            dest_ts.mkdir(parents=True, exist_ok=True)
            for header in ts_dir.glob("*.h"):
                shutil.copy2(header, dest_ts / header.name)

        # Copy the grammar's queries/ directory (if present) into
        # grammars/{lang}/queries/. We care primarily about tags.scm, which
        # defines named-scope detection (@definition.function, @name, etc.),
        # the canonical primitive consumed by GitHub code navigation, Helix,
        # and the `tree-sitter tags` CLI. Copying the whole directory keeps
        # highlights.scm and locals.scm available for future use.
        queries_src = grammar_root / "queries"
        if queries_src.is_dir():
            dest_queries = dest.parent / "queries"
            if dest_queries.exists():
                shutil.rmtree(dest_queries)
            dest_queries.mkdir(parents=True, exist_ok=True)
            for item in queries_src.iterdir():
                if item.is_file() and item.suffix == ".scm":
                    shutil.copy2(item, dest_queries / item.name)
                elif item.is_dir():
                    # Some grammars nest queries under queries/<dialect>/;
                    # copy one level deep and flatten .scm files we recognize.
                    for sub in item.iterdir():
                        if sub.is_file() and sub.suffix == ".scm":
                            shutil.copy2(sub, dest_queries / sub.name)

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

    if not args.dry_run:
        resolve_cross_grammar_deps()

    print(f"\nDone: {success}/{total} succeeded")
    if failed:
        print(f"Failed ({len(failed)}): {', '.join(failed)}")
        sys.exit(1)


def resolve_cross_grammar_deps() -> None:
    """Make each grammar self-contained by copying missing headers from siblings.

    After all grammars are fetched, scan each grammar's C/C++ source files for
    #include "header.h" directives that reference files not present locally.
    For each missing header, search all sibling grammars' src/ directories and
    copy the header into the grammar's own src/. This ensures each grammar
    directory is fully self-contained and compiles with only -I on its own src/.
    """
    import re

    print("\nResolving cross-grammar header dependencies...")

    # Build index: header_name -> list of grammar src/ paths that contain it.
    header_index: dict[str, list[Path]] = {}
    for grammar_dir in sorted(GRAMMARS_DIR.iterdir()):
        src_dir = grammar_dir / "src"
        if not src_dir.is_dir():
            continue
        for header in src_dir.glob("*.h"):
            if header.name in ("parser.h", "alloc.h", "array.h"):
                continue  # Skip standard tree-sitter headers (in tree_sitter/)
            header_index.setdefault(header.name, []).append(src_dir)

    include_re = re.compile(r'#include\s+"([^"]+)"')
    copied_count = 0

    for grammar_dir in sorted(GRAMMARS_DIR.iterdir()):
        src_dir = grammar_dir / "src"
        if not src_dir.is_dir():
            continue

        for src_name in ("scanner.c", "scanner.cc", "parser.c"):
            src_file = src_dir / src_name
            if not src_file.exists():
                continue

            content = src_file.read_text(errors="replace")
            for match in include_re.finditer(content):
                header_path = match.group(1)
                if header_path.startswith("tree_sitter/"):
                    continue
                # Check if the header exists locally.
                if (src_dir / header_path).exists():
                    continue
                # Look up the header basename in our index.
                header_basename = Path(header_path).name
                sources = header_index.get(header_basename, [])
                donor = next((s for s in sources if s != src_dir), None)
                if donor:
                    dest_header = src_dir / header_basename
                    shutil.copy2(donor / header_basename, dest_header)
                    copied_count += 1
                    print(f"  {grammar_dir.name}: copied {header_basename} from {donor.parent.name}")

    if copied_count:
        print(f"  Resolved {copied_count} cross-grammar dependencies")
    else:
        print("  No cross-grammar dependencies found")


if __name__ == "__main__":
    main()
