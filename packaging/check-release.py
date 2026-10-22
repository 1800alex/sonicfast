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
        raise ValueError("Tag must match Cargo.toml package.version")
    notes = root / "packaging/release-notes" / (tag + ".md")
    if not notes.is_file() or not notes.read_text().strip():
        raise ValueError("Add reviewed Sonicfast release notes for this tag")
    metadata = ET.parse(root / "packaging/flatpak/org.sonicfast.Sonicfast.metainfo.xml")
    entry = metadata.find("releases/release")
    if entry is None or entry.get("version") != tag[1:]:
        raise ValueError("Update the first AppStream release entry for this tag")


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("usage: check-release.py TAG")
        check(Path(__file__).resolve().parents[1], sys.argv[1])
    except (ValueError, OSError, ET.ParseError) as error:
        sys.exit(str(error))
