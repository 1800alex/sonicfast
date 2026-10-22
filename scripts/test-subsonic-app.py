#!/usr/bin/env python3
"""Opt-in Phase 3 application tests with private audio/keyring/DBus state.

Start the disposable Navidrome first and set LIBRESONIC_TEST_URL explicitly.
Requires pulseaudio/pactl, GNOME Keyring, dbus-run-session and Cargo. --captures
also requires xvfb-run and the subsonic_ui example. No host defaults are changed.
--m4a also runs the native AAC/M4A PCM/tap/transport proof.
"""
import os
import contextlib
import http.server
import threading
import urllib.request
import urllib.error
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


@contextlib.contextmanager
def fixture_proxy(base, *, block_stream=False, slow_download=False):
    """Loopback fixture only. Never print authenticated request targets."""
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            self.do_GET()

        def do_GET(self):
            if block_stream and self.path.startswith("/rest/stream.view"):
                self.send_error(503)
                return
            body = self.rfile.read(int(self.headers.get("Content-Length", "0"))) if self.command == "POST" else None
            request = urllib.request.Request(base.rstrip("/") + self.path, data=body, method=self.command,
                                             headers={key: self.headers[key] for key in ["Range", "Content-Type"] if key in self.headers})
            try:
                response = urllib.request.urlopen(request, timeout=20)
            except urllib.error.HTTPError as error:
                response = error
            except Exception:
                self.send_error(502)
                return
            with response:
                self.send_response(response.status)
                for key in ["Content-Type", "Content-Length", "Content-Range", "Accept-Ranges"]:
                    if key in response.headers:
                        self.send_header(key, response.headers[key])
                self.end_headers()
                try:
                    slow = slow_download and self.path.startswith("/rest/download.view")
                    while chunk := response.read(2048 if slow else 65536):
                        self.wfile.write(chunk)
                        self.wfile.flush()
                        if slow:
                            time.sleep(0.12)
                except (BrokenPipeError, ConnectionResetError):
                    pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def session(captures, m4a):
    out = Path(os.environ["SONICFAST_TEST_STATE"])
    env = os.environ.copy()
    keyring = pulse = None
    with (out / "services.log").open("w") as log:
        try:
            pulse = subprocess.Popen([
                "pulseaudio", "--daemonize=no", "--exit-idle-time=-1", "--use-pid-file=no", "-n",
                f"--load=module-native-protocol-unix socket={out}/pulse.sock auth-anonymous=1",
                "--load=module-null-sink sink_name=sonicfast_phase3 rate=48000 channels=2",
            ], env=env, stdout=log, stderr=log)
            keyring = subprocess.Popen(["gnome-keyring-daemon", "--foreground", "--unlock", "--components=secrets"],
                                       env=env, stdin=subprocess.PIPE, stdout=log, stderr=log)
            keyring.stdin.write(b"fixture-only-keyring\n")
            keyring.stdin.close()
            deadline = time.monotonic() + 5
            while subprocess.run(["pactl", "info"], env=env, stdout=subprocess.DEVNULL, stderr=log).returncode:
                assert pulse.poll() is None and time.monotonic() < deadline, "Private PulseAudio failed"
                time.sleep(0.05)
            listing = subprocess.check_output([str(ROOT / "target/debug/examples/subsonic_playback"), "--list-devices"], env=env, text=True)
            choices = [line.split("\t", 1)[0] for line in listing.splitlines()]
            env["LIBRESONIC_TEST_DEVICE"] = "pulse" if "pulse" in choices else choices[0]
            cargo = ["cargo", "+stable", "test", "--locked"]
            subprocess.run(cargo + ["--lib", "subsonic::profile::tests::native_server_secret_round_trip_and_revocation", "--", "--ignored", "--nocapture"], cwd=ROOT, env=env, check=True)
            with fixture_proxy(env["LIBRESONIC_TEST_URL"], block_stream=True) as local_url:
                test_env = dict(env, LIBRESONIC_TEST_LOCAL_URL=local_url)
                subprocess.run(cargo + ["--test", "subsonic", "--", "--ignored", "--nocapture", "--test-threads=1"], cwd=ROOT, env=test_env, check=True)
            if m4a:
                probe_env = dict(env, LIBRESONIC_URL=env["LIBRESONIC_TEST_URL"],
                                 LIBRESONIC_USERNAME="admin", LIBRESONIC_PASSWORD="admin")
                probe_env.pop("LIBRESONIC_API_KEY", None)
                subprocess.run([str(ROOT / "target/debug/examples/subsonic_playback"),
                                "--allow-http", "--format", "m4a", "--scripted", "--check-missing-device",
                                "--device", env["LIBRESONIC_TEST_DEVICE"]],
                               cwd=ROOT, env=probe_env, check=True, timeout=60)
            if captures:
                env.pop("WAYLAND_DISPLAY", None)
                env.pop("WAYLAND_SOCKET", None)
                with fixture_proxy(env["LIBRESONIC_TEST_URL"], slow_download=True) as slow_url:
                    subprocess.run([str(ROOT / "target/debug/examples/subsonic_ui")], cwd=ROOT,
                                   env=dict(env, LIBRESONIC_TEST_URL=slow_url), check=True, timeout=130)
        finally:
            for process in [keyring, pulse]:
                if process is not None and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)


def main():
    assert os.environ.get("LIBRESONIC_TEST_URL"), "Set the disposable LIBRESONIC_TEST_URL explicitly"
    if "--session" in sys.argv:
        session("--captures" in sys.argv, "--m4a" in sys.argv)
        return
    out = ROOT / ".cache" / f"phase-3-services-{os.getpid()}"
    for folder in [out, out / "runtime", out / "config", out / "data", out / "state", out / "cache"]:
        folder.mkdir(parents=True, exist_ok=True)
        folder.chmod(0o700)
    cookie = out / "cookie"
    cookie.write_bytes(bytes(256))
    cookie.chmod(0o600)
    env = dict(os.environ, SONICFAST_TEST_STATE=str(out), XDG_RUNTIME_DIR=str(out / "runtime"),
               XDG_CONFIG_HOME=str(out / "config"), XDG_DATA_HOME=str(out / "data"),
               XDG_CACHE_HOME=str(out / "cache"), PULSE_STATE_PATH=str(out / "state"),
               PULSE_RUNTIME_PATH=str(out / "runtime"), PULSE_COOKIE=str(cookie),
               PULSE_SERVER=f"unix:{out}/pulse.sock", CARGO_HOME=str(ROOT / ".cache/cargo"))
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("WAYLAND_SOCKET", None)
    subprocess.run(["xvfb-run", "-a", "-s", "-screen 0 1400x1000x24", "dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "--session"]
                   + [flag for flag in ["--captures", "--m4a"] if flag in sys.argv], env=env, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
