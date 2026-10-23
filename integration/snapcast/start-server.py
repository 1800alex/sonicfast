#!/usr/bin/env python3
"""Select only a standard transport codec; keep the reviewed server config."""
import os
from pathlib import Path
codec = os.environ.get("SNAPCAST_CODEC", "flac")
if codec not in ("pcm", "flac"):
    raise SystemExit("SNAPCAST_CODEC must be pcm or flac")
extra = os.environ.get("SNAPCAST_EXTRA_FORMAT", "")
if extra not in ("", "48000:24:2", "48000:16:1"):
    raise SystemExit("Unsupported extra test format")
config = Path("/etc/sonicfast-snapserver.conf").read_text()
if extra:
    source = f"source = tcp://0.0.0.0:4954?name=FormatTest&mode=server&sampleformat={extra}&codec={codec}&chunk_ms=20\n"
    config = config.replace("buffer = 1000", source + "buffer = 1000")
Path("/state/fixture.conf").write_text(config.replace("codec=flac", "codec=" + codec))
os.execvp("snapserver", ["snapserver", "--config", "/state/fixture.conf"])
