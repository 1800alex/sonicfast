#!/usr/bin/env python3
"""Allow crates.io, retained upstream forks and the public Libresonic repository.

Dependencies must build without repository-external filesystem paths. This reads
every tracked Cargo.toml, Cargo.lock and .cargo/config.toml.
"""

import os
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath, PureWindowsPath
from urllib.parse import urlsplit

CRATES_IO = {
    "registry+https://github.com/rust-lang/crates.io-index",
    "sparse+https://index.crates.io/",
}
MANIFESTS = ("/Cargo.toml", "/Cargo.lock", "/.cargo/config.toml", "/.cargo/config")


def owned(url):
    """True for retained upstream repositories and the approved Libresonic source."""
    parts = urlsplit(url)
    segments = parts.path.strip("/").removesuffix(".git").split("/")
    return (
        parts.scheme == "https"
        and parts.netloc == "github.com"
        and len(segments) == 2
        and (segments[0] == "crmne" or segments == ["1800alex", "libresonic"])
        and segments[1] not in ("", ".", "..")
        and "%" not in parts.path
    )


def git_urls(value, table=()):
    """Yields (table, url) for every git key in a parsed manifest."""
    if isinstance(value, dict):
        for key, item in value.items():
            if key == "git" and isinstance(item, str):
                yield ".".join(table), item
            else:
                yield from git_urls(item, (*table, key))
    elif isinstance(value, list):
        for item in value:
            yield from git_urls(item, table)


def replacements(data):
    """Yields (section, crate, spec) for every [patch] and [replace] entry."""
    for entries in data.get("patch", {}).values():
        for crate, spec in entries.items():
            yield "patch", crate, spec
    for crate, spec in data.get("replace", {}).items():
        yield "replace", crate, spec


def dependency_paths(value, table=()):
    if isinstance(value, dict):
        for key, item in value.items():
            if key == "path" and isinstance(item, str) and any(
                part in ("dependencies", "dev-dependencies", "build-dependencies") for part in table
            ):
                yield ".".join(table), item
            else:
                yield from dependency_paths(item, (*table, key))


def in_tree(manifest, dependency):
    if PurePosixPath(dependency).is_absolute() or (PureWindowsPath(dependency).drive or PureWindowsPath(dependency).root):
        return False
    depth = 0
    for part in (PurePosixPath(manifest).parent / dependency.replace("\\", "/")).parts:
        depth += -1 if part == ".." else 1
        if depth < 0:
            return False
    return True


def problems(files):
    """Returns (path, message) for every disallowed source in {path: TOML text}."""
    found = []
    for path, text in sorted(files.items()):
        data = tomllib.loads(text)
        if path.endswith("Cargo.lock"):
            for package in data.get("package", []):
                source = package.get("source")
                if source and source not in CRATES_IO and not (source.startswith("git+") and owned(source[4:])):
                    found.append((path, f"{package['name']} comes from {source}"))
            continue
        for table, url in git_urls(data):
            if not owned(url):
                found.append((path, f"{table} points at {url}"))
        for table, dependency in dependency_paths(data):
            if not in_tree(path, dependency):
                found.append((path, f"{table} requires a filesystem path outside this repository"))
        for section, crate, spec in replacements(data):
            if not (isinstance(spec, dict) and "git" in spec):
                found.append((path, f"[{section}] {crate} must name an approved git repository"))
        for section in ("source", "registries"):
            if section in data:
                found.append((path, f"[{section}] adds a crate source besides crates.io"))
    return found


def tracked_manifests(root):
    listing = subprocess.run(["git", "ls-files", "-z"], cwd=root, check=True, capture_output=True, text=True).stdout
    paths = [path for path in listing.split("\0") if f"/{path}".endswith(MANIFESTS)]
    return {path: (root / path).read_text() for path in paths if (root / path).is_file()}


def main():
    found = problems(tracked_manifests(Path(__file__).resolve().parents[2]))
    for path, message in found:
        print(f"::error file={path}::{message}" if os.environ.get("GITHUB_ACTIONS") else f"{path}: {message}")
    if found:
        print(
            "Dependencies come from crates.io, retained crmne repositories or "
            "https://github.com/1800alex/libresonic. Keep filesystem dependencies inside the repository; "
            "see CONTRIBUTING.md."
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
