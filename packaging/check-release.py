#!/usr/bin/env python3
"""Fail early when a release tag disagrees with the source or its notes."""
import re
import sys
import tomllib
from pathlib import Path
import xml.etree.ElementTree as ET


def check(root, tag):
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?", tag):
        raise ValueError("Expected a vMAJOR.MINOR.PATCH tag (optional prerelease suffix)")
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    if tag != "v" + manifest["package"]["version"]:
        raise ValueError(f"{tag} must match Cargo.toml package.version ({manifest['package']['version']})")
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    if not any(package["name"] == manifest["package"]["name"]
               and package["version"] == tag[1:] and "source" not in package
               for package in lock.get("package", [])):
        raise ValueError(f"Update Cargo.lock for package.version {tag[1:]} before tagging; builds use --locked")
    notes = root / "packaging/release-notes" / (tag + ".md")
    if not notes.is_file() or not notes.read_text().strip():
        raise ValueError(f"Add nonempty, reviewed Sonicfast release notes at packaging/release-notes/{tag}.md "
                         "before tagging; see PACKAGING.md (Prepare a release)")
    metadata = ET.parse(root / "packaging/flatpak/org.sonicfast.Sonicfast.metainfo.xml")
    entry = metadata.find("releases/release")
    if entry is None or entry.get("version") != tag[1:]:
        raise ValueError(f"Update the first AppStream release entry in "
                         f"packaging/flatpak/org.sonicfast.Sonicfast.metainfo.xml to {tag[1:]} before tagging")


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("usage: check-release.py TAG")
        check(Path(__file__).resolve().parents[1], sys.argv[1])
    except (ValueError, OSError, ET.ParseError) as error:
        sys.exit(str(error))
