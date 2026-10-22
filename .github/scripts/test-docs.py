#!/usr/bin/env python3
"""Regressions for published documentation navigation checks."""
import sys
sys.dont_write_bytecode = True

import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("check_docs", Path(__file__).with_name("check-docs.py"))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class Links(unittest.TestCase):
    def check(self, html):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "index.html").write_text(html)
            (root / "guide").mkdir()
            (root / "guide/index.html").write_text('<h1 id="setup">Setup</h1>')
            (root / "logo.svg").write_text("<svg/>")
            return checker.problems(root, "/sonicfast")

    def test_project_site_relative_links_and_encoded_anchors(self):
        self.assertEqual(self.check('<a href="/sonicfast/guide/#%73etup">Go</a><img src="logo.svg">'), [])

    def test_broken_anchor_is_reported(self):
        self.assertIn("missing anchor", self.check('<a href="guide/#absent">Go</a>')[0])

    def test_missing_asset_is_reported(self):
        self.assertIn("missing target", self.check('<img src="/sonicfast/missing.svg">')[0])

    def test_root_relative_link_cannot_drop_project_prefix(self):
        self.assertIn("base path", self.check('<a href="/guide/">Go</a>')[0])

    def test_upstream_site_and_analytics_are_rejected(self):
        self.assertEqual(len(self.check('<a href="https://spotifast.rocks">Old</a><script src="https://stats.paolino.me/js/a.js"></script>')), 2)

    def test_external_attribution_is_allowed(self):
        self.assertEqual(self.check('<a href="https://github.com/crmne/spotifast">Credits</a>'), [])


if __name__ == "__main__":
    unittest.main()
