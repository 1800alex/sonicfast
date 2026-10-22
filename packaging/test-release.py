#!/usr/bin/env python3
"""Ensure invalid releases fail before building or publishing artifacts."""
import sys
sys.dont_write_bytecode = True

import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("check_release", Path(__file__).with_name("check-release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class Release(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text('[package]\nname = "sonicfast"\nversion = "1.2.3"\n')
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "sonicfast"\nversion = "1.2.3"\n')
        (self.root / "packaging/release-notes").mkdir(parents=True)
        (self.root / "packaging/release-notes/v1.2.3.md").write_text("Reviewed notes")
        self.metadata = self.root / "packaging/flatpak/org.sonicfast.Sonicfast.metainfo.xml"
        self.metadata.parent.mkdir()
        self.metadata.write_text('<component><releases><release version="1.2.3"/></releases></component>')

    def test_matching_release(self):
        release.check(self.root, "v1.2.3")

    def test_tag_disagrees_with_package(self):
        with self.assertRaisesRegex(ValueError, "package.version"):
            release.check(self.root, "v1.2.4")

    def test_outdated_lockfile(self):
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "sonicfast"\nversion = "1.2.2"\n')
        with self.assertRaisesRegex(ValueError, r"Cargo.lock.*1\.2\.3.*--locked"):
            release.check(self.root, "v1.2.3")

    def test_missing_reviewed_notes(self):
        (self.root / "packaging/release-notes/v1.2.3.md").unlink()
        with self.assertRaisesRegex(ValueError, r"release notes at packaging/release-notes/v1\.2\.3\.md"):
            release.check(self.root, "v1.2.3")

    def test_empty_reviewed_notes(self):
        (self.root / "packaging/release-notes/v1.2.3.md").write_text(" \n")
        with self.assertRaisesRegex(ValueError, r"release notes at packaging/release-notes/v1\.2\.3\.md"):
            release.check(self.root, "v1.2.3")

    def test_checked_in_release_metadata(self):
        root = Path(__file__).resolve().parents[1]
        manifest = release.tomllib.loads((root / "Cargo.toml").read_text())
        release.check(root, "v" + manifest["package"]["version"])

    def test_outdated_appstream(self):
        self.metadata.write_text('<component><releases><release version="1.2.2"/></releases></component>')
        with self.assertRaisesRegex(ValueError, "AppStream"):
            release.check(self.root, "v1.2.3")

    def test_missing_appstream_release(self):
        self.metadata.write_text('<component/>')
        with self.assertRaisesRegex(ValueError, "AppStream"):
            release.check(self.root, "v1.2.3")


if __name__ == "__main__":
    unittest.main()
