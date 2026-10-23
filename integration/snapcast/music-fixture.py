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
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass

ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
