#!/usr/bin/env python3
"""Derive CI's test jobs from `.config/verification.toml` and check it.

    verification.py buckets             job matrix of the ordinary tests
    verification.py gates TIER [BASE]   job matrix of a tier's gates; with BASE,
                                        only those a change since BASE affects
    verification.py check [TOOLCHAIN]   the checks below; the last needs a
                                        nightly TOOLCHAIN
    verification.py write               rewrite the filter in nextest.toml

`check` requires that

* the `ci` nextest profile leaves out exactly the declared gates,
* no package is in two buckets, and every declared package and test exists,
* the buckets together build every test binary of the workspace exactly once,
  each with the features a build of the whole workspace gives its package.
  Its dependencies may have fewer, since a build of fewer packages turns on
  only what those packages ask for.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NEXTEST = ROOT / ".config/nextest.toml"
TIERS = ("certificate", "corpus", "exhaustive", "kan")


def run(*command: str) -> str:
    done = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
    if done.returncode:
        sys.exit(f"{' '.join(command)}\n{done.stderr}")
    return done.stdout


def declaration() -> dict:
    with (ROOT / ".config/verification.toml").open("rb") as file:
        return tomllib.load(file)


# ---------------------------------------------------------------------------
# Buckets
# ---------------------------------------------------------------------------


def bucket_arguments(declared: dict) -> dict[str, list[str]]:
    """The cargo arguments that select each bucket."""
    arguments = {}
    listed: list[str] = []
    for bucket in declared["bucket"]:
        listed += bucket["packages"]
        arguments[bucket["name"]] = [
            *(part for package in bucket["packages"] for part in ("-p", package)),
            *(part for feature in bucket.get("features", []) for part in ("--features", feature)),
        ]
    arguments["rest"] = [
        "--workspace",
        *(part for package in listed for part in ("--exclude", package)),
    ]
    return arguments


# ---------------------------------------------------------------------------
# Gates
# ---------------------------------------------------------------------------


def expression(gate: dict) -> str:
    """The nextest expression that selects a gate's tests and no others."""
    tests = " | ".join(f"test(={test})" for test in gate["tests"])
    return f"package(={gate['package']}) & binary(={gate['binary']}) & ({tests})"


def default_filter(declared: dict) -> str:
    return "not (" + " | ".join(f"({expression(gate)})" for gate in declared["gate"]) + ")"


def jobs(gate: dict) -> list[dict]:
    """The matrix entries of one gate."""
    name = gate.get("name") or ", ".join(gate["tests"])
    entry = {
        "package": gate["package"],
        "binary": gate["binary"],
        "expression": expression(gate),
        "shard": "",
        "shard_env": "",
    }
    shards = gate.get("shards")
    if shards is None:
        return [{"name": name, **entry}]
    return [
        {
            "name": f"{name} {index}/{shards}",
            **entry,
            "shard": f"{index}/{shards}",
            "shard_env": gate["shard_env"],
        }
        for index in range(shards)
    ]


def affected_packages(base: str) -> set[str]:
    """The workspace packages a change since `base` can affect.

    A package is affected by a change to one of its files and to any package it
    depends on, for its tests and build scripts included. A file outside every
    package affects all of them, except documentation and the bindings that are
    not Rust packages, which no gate reads.
    """
    metadata = json.loads(
        run("cargo", "metadata", "--format-version", "1", "--locked", "--no-deps")
    )
    home = {
        Path(package["manifest_path"]).parent.relative_to(ROOT).as_posix(): package["name"]
        for package in metadata["packages"]
    }
    dependencies = {
        package["name"]: {dependency["name"] for dependency in package["dependencies"]}
        for package in metadata["packages"]
    }

    affected: set[str] = set()
    for file in run("git", "diff", "--name-only", f"{base}...HEAD").splitlines():
        owner = max((path for path in home if file.startswith(f"{path}/")), key=len, default=None)
        if owner is not None:
            affected.add(home[owner])
        elif not (file.endswith(".md") or file.startswith(("book/", "bindings/"))):
            return set(dependencies)
    while True:
        reached = {name for name, needs in dependencies.items() if needs & affected}
        if reached <= affected:
            return affected
        affected |= reached


def gate_matrix(declared: dict, tier: str, base: str | None) -> list[dict]:
    gates = [gate for gate in declared["gate"] if gate["tier"] == tier]
    if base is not None:
        affected = affected_packages(base)
        gates = [gate for gate in gates if gate["package"] in affected]
    return [job for gate in gates for job in jobs(gate)]


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

FILTER = re.compile(r"^default-filter = '(.*)'$", re.MULTILINE)
TABLE = re.compile(r"^\[", re.MULTILINE)


def ci_profile_span(text: str) -> tuple[int, int]:
    """Where the `[profile.ci]` table of nextest.toml starts and ends.

    Other profiles (the `miri` one) carry filters of their own, so the ci
    filter is looked for only inside its own table.
    """
    start = text.index("[profile.ci]\n")
    following = TABLE.search(text, start + 1)
    return start, following.start() if following else len(text)


def check_filter(declared: dict) -> list[str]:
    text = NEXTEST.read_text()
    start, end = ci_profile_span(text)
    found = FILTER.findall(text, start, end)
    if found == [default_filter(declared)]:
        return []
    return [
        "the ci profile of .config/nextest.toml does not leave out exactly the declared gates; "
        "run .github/scripts/verification.py write"
    ]


def check_names(declared: dict) -> list[str]:
    problems = []
    packages = {
        package["name"]: Path(package["manifest_path"]).parent
        for package in json.loads(
            run("cargo", "metadata", "--format-version", "1", "--locked", "--no-deps")
        )["packages"]
    }
    listed = [package for bucket in declared["bucket"] for package in bucket["packages"]]
    problems += [f"{name} is in two buckets" for name in set(listed) if listed.count(name) > 1]
    problems += [f"no package {name}" for name in listed if name not in packages]
    for gate in declared["gate"]:
        if gate["tier"] not in TIERS:
            problems.append(f"no tier {gate['tier']}")
        directory = packages.get(gate["package"], ROOT) / "tests"
        source = directory / f"{gate['binary']}.rs"
        if not source.exists():
            problems.append(f"no test binary {gate['package']}::{gate['binary']}")
            continue
        # A test may be in a module of the binary, in a file of its own.
        sources = "\n".join(file.read_text() for file in directory.rglob("*.rs"))
        problems += [
            f"no test {gate['package']}::{gate['binary']} {test}"
            for test in gate["tests"]
            if not re.search(rf"\bfn {re.escape(test.rsplit('::', 1)[-1])}\b", sources)
        ]
    return problems


def test_units(toolchain: str, arguments: list[str]) -> dict[tuple, list[str]]:
    """Each test binary of a build, with the features of its package."""
    graph = json.loads(
        run(
            *("cargo", f"+{toolchain}", "build", "--tests", "--locked"),
            *("--unit-graph", "-Z", "unstable-options", *arguments),
        )
    )
    return {
        (unit["pkg_id"], unit["target"]["name"], tuple(unit["target"]["kind"])): unit["features"]
        for unit in graph["units"]
        if unit["mode"] == "test"
    }


def check_units(declared: dict, toolchain: str) -> list[str]:
    problems = []
    whole = test_units(toolchain, ["--workspace"])
    built: dict[tuple, str] = {}
    for name, arguments in bucket_arguments(declared).items():
        for key, features in test_units(toolchain, arguments).items():
            label = f"{key[0].rsplit('/', 1)[-1]} {key[1]}"
            if key in built:
                problems.append(f"{label} is in the buckets {built[key]} and {name}")
            built[key] = name
            if key not in whole:
                problems.append(f"{label} of bucket {name} is not a test of the workspace")
            elif features != whole[key]:
                changed = sorted(set(features) ^ set(whole[key]))
                problems.append(f"{label} of bucket {name} differs in the features {changed}")
    problems += [
        f"{key[0].rsplit('/', 1)[-1]} {key[1]} is in no bucket"
        for key in whole.keys() - built.keys()
    ]
    return problems


def check(declared: dict, toolchain: str | None) -> None:
    problems = check_filter(declared) + check_names(declared)
    if toolchain is not None:
        problems += check_units(declared, toolchain)
    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    sys.exit(1 if problems else 0)


def main(arguments: list[str]) -> None:
    declared = declaration()
    match arguments:
        case ["buckets"]:
            matrix = bucket_arguments(declared).items()
            print(json.dumps([{"name": name, "select": " ".join(args)} for name, args in matrix]))
        case ["gates", tier] if tier in TIERS:
            print(json.dumps(gate_matrix(declared, tier, None)))
        case ["gates", tier, base] if tier in TIERS:
            print(json.dumps(gate_matrix(declared, tier, base)))
        case ["check"]:
            check(declared, None)
        case ["check", toolchain]:
            check(declared, toolchain)
        case ["write"]:
            line = f"default-filter = '{default_filter(declared)}'"
            text = NEXTEST.read_text()
            start, end = ci_profile_span(text)
            table, count = FILTER.subn(lambda _: line, text[start:end])
            if count != 1:
                sys.exit("the ci profile of nextest.toml has no one default-filter")
            NEXTEST.write_text(text[:start] + table + text[end:])
        case _:
            sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
