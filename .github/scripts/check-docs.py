#!/usr/bin/env python3
"""Check generated site links, project base paths and inherited service URLs."""
import argparse
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class Page(HTMLParser):
    def __init__(self, text):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.links = []
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for name in ("href", "src", "poster"):
            if attrs.get(name):
                self.links.append(attrs[name])


def problems(root, baseurl):
    root = root.resolve()
    pages = {path.resolve(): Page(path.read_text()) for path in root.rglob("*.html")}
    found = []
    for path, page in sorted(pages.items()):
        for link in page.links:
            url = urlsplit(link)
            if url.hostname in {"spotifast.rocks", "stats.paolino.me"}:
                found.append(f"{path.relative_to(root)}: inherited service URL {link}")
                continue
            if url.scheme or url.netloc:
                continue
            target = unquote(url.path)
            if target.startswith("/"):
                if baseurl and target != baseurl and not target.startswith(baseurl + "/"):
                    found.append(f"{path.relative_to(root)}: link escapes site base path: {link}")
                    continue
                dest = root / target.removeprefix(baseurl).lstrip("/")
            else:
                dest = path.parent / target if target else path
            dest = dest.resolve()
            if not dest.is_relative_to(root):
                found.append(f"{path.relative_to(root)}: link escapes site directory: {link}")
                continue
            if dest.is_dir():
                dest = dest / "index.html"
            if not dest.is_file():
                found.append(f"{path.relative_to(root)}: missing target: {link}")
            elif url.fragment and url.fragment.lower() != "top" and dest in pages and unquote(url.fragment) not in pages[dest].ids:
                found.append(f"{path.relative_to(root)}: missing anchor: {link}")
    if not pages:
        found.append("No generated HTML pages found")
    return found


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("site", type=Path)
    parser.add_argument("--baseurl", default="/sonicfast")
    args = parser.parse_args()
    errors = problems(args.site, args.baseurl.rstrip("/"))
    for error in errors:
        print(error)
    if errors:
        raise SystemExit(1)
    print("Generated documentation links and base paths passed")
