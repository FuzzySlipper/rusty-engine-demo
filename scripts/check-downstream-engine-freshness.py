#!/usr/bin/env python3
"""Fail when the rolling Rusty Engine facade lock is not current."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys
import tomllib


DEFAULT_REPOSITORY = "https://github.com/FuzzySlipper/rusty-engine"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("lockfile", type=Path)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--repository", default=DEFAULT_REPOSITORY)
    parser.add_argument("--branch", default="main")
    parser.add_argument("--expected-sha")
    return parser.parse_args()


def validate_manifest(manifest: Path, repository: str, branch: str) -> None:
    with manifest.open("rb") as source:
        cargo = tomllib.load(source)
    dependencies = cargo.get("workspace", {}).get("dependencies", {})
    canonical = [
        (alias, specification)
        for alias, specification in dependencies.items()
        if isinstance(specification, dict) and specification.get("git") == repository
    ]
    if len(canonical) != 1:
        raise ValueError(
            "Cargo.toml must declare exactly one dependency from the canonical Engine repository"
        )
    alias, specification = canonical[0]
    if specification.get("package", alias) != "rusty-engine":
        raise ValueError("the canonical Engine dependency must be rusty-engine")
    if specification.get("branch") != branch or "rev" in specification or "tag" in specification:
        raise ValueError(f"rusty-engine must follow rolling branch {branch!r}")


def resolved_revision(lockfile: Path, repository: str, branch: str) -> str:
    with lockfile.open("rb") as source:
        lock = tomllib.load(source)
    packages = [package for package in lock.get("package", []) if package.get("name") == "rusty-engine"]
    if len(packages) != 1:
        raise ValueError("Cargo.lock must contain exactly one rusty-engine package")
    expected_prefix = f"git+{repository}?branch={branch}#"
    source = packages[0].get("source")
    if not isinstance(source, str) or not source.startswith(expected_prefix):
        raise ValueError(f"rusty-engine lock source must use {expected_prefix}<sha>")
    revision = source.removeprefix(expected_prefix)
    if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        raise ValueError("rusty-engine lock source must end in a 40-character SHA")
    return revision


def branch_head(repository: str, branch: str) -> str:
    completed = subprocess.run(
        ["git", "ls-remote", repository, f"refs/heads/{branch}"],
        check=True,
        capture_output=True,
        text=True,
    )
    fields = completed.stdout.split()
    if len(fields) != 2 or re.fullmatch(r"[0-9a-f]{40}", fields[0]) is None:
        raise ValueError(f"could not resolve {repository} branch {branch}")
    return fields[0]


def main() -> int:
    args = parse_args()
    try:
        validate_manifest(args.manifest.resolve(), args.repository, args.branch)
        resolved = resolved_revision(args.lockfile.resolve(), args.repository, args.branch)
        expected = args.expected_sha or branch_head(args.repository, args.branch)
        if re.fullmatch(r"[0-9a-f]{40}", expected) is None:
            raise ValueError("expected revision must be a 40-character lowercase SHA")
        if resolved != expected:
            raise RuntimeError(
                f"downstream Engine lock is stale: resolved {resolved}, current {args.branch} is {expected}"
            )
    except (OSError, subprocess.CalledProcessError, ValueError, RuntimeError) as error:
        print(f"Engine freshness check failed: {error}", file=sys.stderr)
        return 1
    print(f"Engine freshness check passed: {resolved}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
