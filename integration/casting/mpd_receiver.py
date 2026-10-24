"""Unmodified Music Player Daemon playing into the fixture's private Pulse sink.

Configured like a typical LAN MPD (no password, curl input for HTTP URLs) but
bound to loopback on a per-run port. `--health` checks the protocol greeting.
"""
import os
from pathlib import Path
import socket
import subprocess
import sys

PORT = int(os.environ.get("MPD_PORT", "6600"))

if sys.argv[1:] == ["--health"]:
    with socket.create_connection(("127.0.0.1", PORT), timeout=2) as connection:
        sys.exit(0 if connection.recv(64).startswith(b"OK MPD ") else 1)

root = Path("/tmp/mpd")
(root / "music").mkdir(parents=True, exist_ok=True)
(root / "mpd.conf").write_text(f'''music_directory "{root}/music"
db_file "{root}/database"
bind_to_address "127.0.0.1"
port "{PORT}"
log_file "/dev/stderr"
zeroconf_enabled "no"
audio_output {{
    type "pulse"
    name "Fixture null sink"
    server "unix:/run/pulse/native"
    sink "casting_test"
}}
''')
os.environ.update(HOME="/tmp", XDG_RUNTIME_DIR="/run/pulse", PULSE_SERVER="unix:/run/pulse/native")
# Pulse only accepts its own uid, as for the other fixture receivers.
if subprocess.run(["getent", "passwd", "1000"], stdout=subprocess.DEVNULL).returncode:
    subprocess.run(["addgroup", "-g", "1000", "fixture"], check=True)
    subprocess.run(["adduser", "-D", "-u", "1000", "-G", "fixture", "fixture"], check=True)
for path in [root, *root.rglob("*")]:
    os.chown(path, 1000, 1000)
os.setgroups([])
os.setgid(1000)
os.setuid(1000)
command = ["mpd", "--no-daemon", "--stderr", str(root / "mpd.conf")]
os.execvp(command[0], command)
