"""Small audio-only CastV2 target for explicit certificate-pinning senders.

Implements Google's public Default Media Receiver message shapes. It has no
Google device-auth certificate and is not a Chromecast replacement for SDK apps.
"""
import hashlib
import html
import json
import os
from pathlib import Path
import pwd
import grp
import re
import shutil
import socket
import ssl
import struct
import subprocess
import sys
import threading
import time
import urllib.parse

STATE = Path(os.environ.get("CAST_STATE", "/state"))
PORT = int(os.environ.get("CAST_RECEIVER_PORT", "8009"))
NAME = os.environ.get("CAST_RECEIVER_NAME", "Garage Cast")
APP = "CC1AD845"
CONNECTION = "urn:x-cast:com.google.cast.tp.connection"
HEARTBEAT = "urn:x-cast:com.google.cast.tp.heartbeat"
RECEIVER = "urn:x-cast:com.google.cast.receiver"
MEDIA = "urn:x-cast:com.google.cast.media"

def varint(value):
    data = bytearray()
    while value > 127:
        data.append((value & 127) | 128)
        value >>= 7
    data.append(value)
    return bytes(data)

def integer(data, pos):
    value = 0
    for shift in range(0, 70, 7):
        if pos >= len(data):
            raise ValueError("Truncated protobuf")
        byte = data[pos]
        pos += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, pos
    raise ValueError("Oversized protobuf")

def decode(data):
    fields = {}
    pos = 0
    while pos < len(data):
        tag, pos = integer(data, pos)
        if tag & 7 == 0:
            fields[tag >> 3], pos = integer(data, pos)
        elif tag & 7 == 2:
            length, pos = integer(data, pos)
            if pos + length > len(data):
                raise ValueError("Invalid protobuf length")
            fields[tag >> 3] = data[pos:pos+length]
            pos += length
        else:
            raise ValueError("Unsupported protobuf wire type")
    return fields

def encode(destination, namespace, payload):
    data = b"\x08\x00"
    for field, value in ((2, "receiver-0"), (3, destination), (4, namespace), (6, json.dumps(payload))):
        value = value.encode()
        data += varint((field << 3) | 2) + varint(len(value)) + value
    data += b"\x28\x00"
    return struct.pack(">I", len(data)) + data

def exact(sock, length):
    data = bytearray()
    while len(data) < length:
        chunk = sock.recv(length - len(data))
        if not chunk:
            raise EOFError()
        data.extend(chunk)
    return bytes(data)

class Player:
    def __init__(self):
        self.lock = threading.RLock()
        self.session = 0
        self.media = None
        self.seen_playing = False
        self.volume = 1.0
        self.muted = False
        self.request = 0
        self.last_position = 0
        self.loaded_at = time.monotonic()
        self.process = subprocess.Popen([
            "mpv", "--no-config", "--no-video", "--load-scripts=no", "--ytdl=no", "--idle=yes",
            "--ao=pulse", "--input-ipc-server=/tmp/garage-cast-mpv.sock", "--volume=100", "--msg-level=all=no",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + 5
        while not Path("/tmp/garage-cast-mpv.sock").exists():
            if self.process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("Audio player failed to start")
            time.sleep(.02)
        version = str(self.rpc("get_property", "mpv-version"))
        match = re.search(r"(\d+)\.(\d+)", version)
        self.modern_mpv = bool(match and tuple(map(int, match.groups())) >= (0, 38))

    def rpc(self, *command):
        self.request += 1
        request = self.request
        with socket.socket(socket.AF_UNIX) as sock:
            sock.settimeout(2)
            sock.connect("/tmp/garage-cast-mpv.sock")
            sock.sendall(json.dumps({"command": command, "request_id": request}).encode() + b"\n")
            response = sock.makefile("rb")
            for _ in range(32):
                data = json.loads(response.readline(65536))
                if data.get("request_id") == request:
                    if data.get("error") != "success":
                        if command[0] == "get_property":
                            return None  # Position/duration are unavailable while loading or idle.
                        print("Audio player rejected command: " + str(command[0]), flush=True)
                        raise RuntimeError("Audio player command failed")
                    return data.get("data")
        raise RuntimeError("Audio player did not respond")

    def status(self):
        if self.media is None:
            return []
        position = self.rpc("get_property", "time-pos")
        pause = self.rpc("get_property", "pause")
        eof = self.rpc("get_property", "eof-reached")
        idle = self.rpc("get_property", "idle-active")
        duration = self.rpc("get_property", "duration")
        phase = "BUFFERING"
        if position is not None:
            self.last_position = position
            self.seen_playing = True
            phase = "PAUSED" if pause else "PLAYING"
        if self.seen_playing and (eof or idle):
            phase = "IDLE"
        failed = bool(idle and not self.seen_playing and time.monotonic() - self.loaded_at > 10)
        if failed:
            phase = "IDLE"
        media = dict(self.media)
        if isinstance(duration, (int, float)):
            media["duration"] = duration
        status = {"mediaSessionId": self.session, "playerState": phase, "currentTime": position if position is not None else self.last_position,
                  "playbackRate": 1, "supportedMediaCommands": 15, "media": media}
        if phase == "IDLE":
            status["idleReason"] = "ERROR" if failed else "FINISHED"
        return [status]

    def receiver_status(self):
        return {"applications": [{"appId": APP, "displayName": "Garage audio", "sessionId": "garage",
                                  "transportId": "garage-media", "namespaces": [{"name": MEDIA}]}],
                "volume": {"level": self.volume, "muted": self.muted}}

    def handle(self, namespace, payload):
        with self.lock:
            kind = payload.get("type")
            request = payload.get("requestId", 0)
            if namespace == HEARTBEAT:
                return {"type": "PONG"} if kind == "PING" else None
            if namespace == CONNECTION:
                return None
            if namespace == RECEIVER:
                if kind == "LAUNCH" and payload.get("appId") != APP:
                    return {"type": "LAUNCH_ERROR", "requestId": request, "reason": "APP_NOT_FOUND"}
                if kind == "SET_VOLUME":
                    volume = payload.get("volume", {})
                    self.volume = max(0.0, min(1.0, float(volume.get("level", self.volume))))
                    self.muted = bool(volume.get("muted", self.muted))
                    self.rpc("set_property", "volume", self.volume * 100)
                    self.rpc("set_property", "mute", self.muted)
                return {"type": "RECEIVER_STATUS", "requestId": request, "status": self.receiver_status()}
            if namespace != MEDIA:
                return None
            if kind == "LOAD":
                media = payload.get("media", {})
                url = media.get("contentId", "")
                parsed = urllib.parse.urlsplit(url)
                if parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.username or parsed.password:
                    return {"type": "LOAD_FAILED", "requestId": request}
                self.session += 1
                self.media = {"contentId": url, "contentType": media.get("contentType", "application/octet-stream"), "streamType": "BUFFERED"}
                self.seen_playing = False
                self.last_position = 0
                self.loaded_at = time.monotonic()
                self.rpc("set_property", "pause", not payload.get("autoplay", True))
                options = {"start": str(max(0, float(payload.get("currentTime", 0))))}
                if self.modern_mpv:
                    self.rpc("loadfile", url, "replace", -1, options)
                else:
                    self.rpc("loadfile", url, "replace", options)
            elif kind != "GET_STATUS" and payload.get("mediaSessionId") != self.session:
                return {"type": "INVALID_REQUEST", "requestId": request}
            elif kind == "STOP":
                self.rpc("stop")
                self.media = None
            elif kind in ("PLAY", "PAUSE"):
                self.rpc("set_property", "pause", kind == "PAUSE")
            elif kind == "SEEK":
                self.rpc("seek", max(0, float(payload.get("currentTime", 0))), "absolute+exact")
                self.rpc("set_property", "pause", payload.get("resumeState") == "PLAYBACK_PAUSE")
            return {"type": "MEDIA_STATUS", "requestId": request, "status": self.status()}

def prepare():
    if not os.environ.get("PULSE_SINK"):
        os.environ.pop("PULSE_SINK", None)
    STATE.mkdir(exist_ok=True)
    uid, gid = int(os.environ.get("MUSICD_UID", "1000")), int(os.environ.get("MUSICD_GID", "1000"))
    try:
        grp.getgrgid(gid)
    except KeyError:
        subprocess.run(["groupadd" if shutil.which("groupadd") else "addgroup", "-g", str(gid), "cast-output"], check=True)
    try:
        pwd.getpwuid(uid)
    except KeyError:
        if shutil.which("useradd"):
            subprocess.run(["useradd", "-u", str(uid), "-g", str(gid), "-M", "cast-output"], check=True)
        else:
            subprocess.run(["adduser", "-D", "-u", str(uid), "-G", grp.getgrgid(gid).gr_name, "cast-output"], check=True)
    certificate, key = STATE / "receiver.crt", STATE / "receiver.key"
    if not certificate.exists() or not key.exists():
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "3650",
                        "-subj", "/CN=Garage Cast", "-keyout", str(key), "-out", str(certificate)],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    fingerprint = hashlib.sha256(ssl.PEM_cert_to_DER_cert(certificate.read_text())).hexdigest()
    for file in (STATE, certificate, key):
        os.chown(file, uid, gid)
    os.chmod(key, 0o600)
    print("Self-hosted receiver certificate SHA-256: " + fingerprint, flush=True)
    print("This is not a Google-authenticated Chromecast; configure the fingerprint in Sonicfast.", flush=True)
    service = f'''<?xml version="1.0"?><!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group><name>{html.escape(NAME)}</name><service><type>_googlecast._tcp</type><port>{PORT}</port>
<txt-record>id={fingerprint[:32]}</txt-record><txt-record>fn={html.escape(NAME)}</txt-record>
<txt-record>md=Sonicfast self-hosted receiver</txt-record><txt-record>ca=5</txt-record><txt-record>st=0</txt-record>
</service></service-group>'''
    Path("/etc/avahi/services/garage-cast.service").write_text(service)
    Path("/run/dbus").mkdir(exist_ok=True)
    for pid in ("/run/dbus/pid", "/run/avahi-daemon/pid", "/tmp/garage-cast-mpv.sock"):
        Path(pid).unlink(missing_ok=True)
    subprocess.run(["dbus-uuidgen", "--ensure"], check=True)
    subprocess.run(["dbus-daemon", "--system"], check=True)
    subprocess.run(["avahi-daemon", "--daemonize", "--no-chroot"], check=True)
    os.setgroups([])
    os.setgid(gid)
    os.setuid(uid)
    os.environ["HOME"] = "/tmp"

def main():
    if "--fingerprint" in sys.argv:
        print(hashlib.sha256(ssl.PEM_cert_to_DER_cert((STATE / "receiver.crt").read_text())).hexdigest())
        return
    if "--health" in sys.argv:
        with socket.create_connection(("127.0.0.1", PORT), timeout=2):
            return
    prepare()
    player = Player()
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(str(STATE / "receiver.crt"), str(STATE / "receiver.key"))
    limit = threading.Semaphore(4)
    def serve(raw):
        try:
            raw.settimeout(5)
            with context.wrap_socket(raw, server_side=True) as sock:
                sock.settimeout(30)
                while True:
                    length = struct.unpack(">I", exact(sock, 4))[0]
                    if not 0 < length <= 65536:
                        return
                    fields = decode(exact(sock, length))
                    if fields.get(5, 0) != 0:
                        return  # No Google device-auth certificate is available.
                    namespace = fields.get(4, b"").decode()
                    payload = json.loads(fields.get(6, b"{}"))
                    if not isinstance(payload, dict):
                        return
                    result = player.handle(namespace, payload)
                    if result is not None:
                        sock.sendall(encode(fields.get(2, b"sender-0").decode(), namespace, result))
        except (OSError, EOFError, ValueError, RuntimeError):
            pass  # Never log media URLs, request bodies or protocol exceptions.
        finally:
            raw.close()
            limit.release()
    with socket.socket() as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("0.0.0.0", PORT))
        listener.listen(8)
        while True:
            raw, _ = listener.accept()
            if limit.acquire(blocking=False):
                threading.Thread(target=serve, args=(raw,), daemon=True).start()
            else:
                raw.close()

if __name__ == "__main__":
    main()
