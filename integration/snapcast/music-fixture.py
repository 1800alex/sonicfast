#!/usr/bin/env python3
"""Minimal synthetic Subsonic fixture for the real App -> decoder -> sender proof.

This is a test double, not another supported real-world music server. Requests
and authentication targets are never logged. The only media is an original tone.
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
from urllib.parse import urlsplit

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        endpoint = urlsplit(self.path).path.rsplit("/", 1)[-1].removesuffix(".view")
        if endpoint == "stream":
            payload = Path("/usr/local/share/sonicfast-tone.flac").read_bytes()
            content_type = "audio/flac"
        else:
            data = {"status": "ok", "version": "1.16.1", "type": "Sonicfast test double", "openSubsonic": True}
            if endpoint == "getOpenSubsonicExtensions":
                data["openSubsonicExtensions"] = []
            elif endpoint == "getSong":
                data["song"] = {"id": "tone", "isDir": False, "title": "Original 660 Hz tone", "artist": "Sonicfast fixture",
                                "album": "Snapcast proof", "duration": 2, "suffix": "flac", "contentType": "audio/flac",
                                "path": "Sonicfast fixture/Snapcast proof/tone.flac", "size": 35025}
            elif endpoint == "getArtists":
                data["artists"] = {"index": []}
            elif endpoint == "getPlaylists":
                data["playlists"] = {"playlist": []}
            elif endpoint == "getAlbumList2":
                data["albumList2"] = {"album": []}
            elif endpoint == "getStarred2":
                data["starred2"] = {"song": [], "album": [], "artist": []}
            elif endpoint == "search3":
                data["searchResult3"] = {"song": [], "artist": [], "album": []}
            payload = json.dumps({"subsonic-response": data}).encode()
            content_type = "application/json"
        # Original files offer byte ranges, as Navidrome's raw streams do, so
        # receivers that fetch the original (MPD, Cast) can seek.
        status = 200
        if endpoint == "stream":
            wanted = self.headers.get("Range", "")
            if wanted.startswith("bytes=") and "," not in wanted:
                first, _, last = wanted[6:].partition("-")
                if first.isdigit() and int(first) < len(payload):
                    end = min(int(last), len(payload) - 1) if last.isdigit() else len(payload) - 1
                    if int(first) <= end:
                        status = 206
                        content_range = f"bytes {first}-{end}/{len(payload)}"
                        payload = payload[int(first):end + 1]
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        if endpoint == "stream":
            self.send_header("Accept-Ranges", "bytes")
        if status == 206:
            self.send_header("Content-Range", content_range)
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass

ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
