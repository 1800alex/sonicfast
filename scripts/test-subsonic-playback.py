#!/usr/bin/env python3
"""Opt-in Linux native proof against an already-running synthetic Navidrome.

Requires the built subsonic_playback example, pulseaudio/pactl/parec and connection
variables described in docs/design/phase-2.md. Owns a private null sink/server;
never changes the host defaults. Captures/evidence stay under ignored .cache/.
"""
import array
import json
import math
import os
from pathlib import Path
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / ".cache/audio/native-proof"


def rms(values):
    return math.sqrt(sum(v * v for v in values) / len(values)) if values else 0.0


def capture(pipe, path, blocks):
    with path.open("wb") as output:
        while data := pipe.read(3840):  # 10 ms, stereo 48 kHz float32
            at = time.time() * 1000
            output.write(data)
            values = array.array("f")
            values.frombytes(data)
            blocks.append((at, rms(values), max(map(abs, values), default=0)))


def check_window(blocks, start, end, silent):
    samples = [b for b in blocks if start < b[0] < end]
    assert len(samples) >= 5, "Insufficient captured samples in transport window"
    peak = max(b[2] for b in samples)
    level = rms([b[1] for b in samples])
    assert math.isfinite(peak) and peak <= 1.01, "Invalid captured PCM"
    if silent:
        assert level < 0.0001, f"Expected silence, captured RMS {level}"
    else:
        assert level > 0.001, f"Expected audio, captured RMS {level}"
    return {"blocks": len(samples), "rms": level, "peak": peak}


def prove(codec, env, device):
    blocks = []
    with (OUT / f"{codec}-capture-stderr.log").open("w") as errors:
        recorder = subprocess.Popen([
            "parec", "--device=libresonic_proof.monitor", "--format=float32le",
            "--rate=48000", "--channels=2", "--latency-msec=20", "--process-time-msec=10",
        ], env=env, stdout=subprocess.PIPE, stderr=errors)
        reader = threading.Thread(target=capture, args=(recorder.stdout, OUT / f"{codec}.f32le", blocks))
        reader.start()
        try:
            time.sleep(0.3)
            with (OUT / f"{codec}-stderr.log").open("w") as stderr:
                result = subprocess.run([
                    str(ROOT / "target/debug/examples/subsonic_playback"),
                    "--allow-http", "--device", device, "--format", codec,
                    "--scripted", "--check-missing-device",
                ], env=env, stdout=subprocess.PIPE, stderr=stderr, text=True, timeout=40)
            (OUT / f"{codec}-events.jsonl").write_text(result.stdout)
            assert result.returncode == 0, f"{codec} harness failed; see ignored logs in {OUT}"
            time.sleep(0.2)
        finally:
            recorder.terminate()
            recorder.wait(timeout=5)
            reader.join(timeout=5)
            recorder.stdout.close()
        events = {event["action"]: event for event in map(json.loads, result.stdout.splitlines())}
        assert "missing_device_recoverable" in events
        for action in ["playing", "paused", "resume", "muted", "unmute", "seeked", "stopped", "ended"]:
            assert action in events, f"Missing {action} evidence"
        assert events["seeked"]["position_ms"] >= 1500
        assert events["stopped"]["position_ms"] == 0
        assert events["muted"]["tap_rms"] > 0.001, "Pre-volume tap must survive mute"
        assert events["ended"]["duration_ms"] == 3000, "This proof requires the 3s fixture"
        assert events["ended"]["peak_input_bytes"] <= 18 * 16384
        t = lambda action: events[action]["unix_ms"]
        report = {
            "codec": codec,
            "startup_ms": events["playing"]["startup_ms"],
            "seek_startup_ms": events["seeked"]["startup_ms"],
            "peak_input_bytes": events["ended"]["peak_input_bytes"],
            "paused": check_window(blocks, t("paused") + 80, t("resume") - 40, True),
            "muted": check_window(blocks, t("muted") + 80, t("unmute") - 40, True),
            "stopped": check_window(blocks, t("stopped") + 80, t("stopped") + 280, True),
            "resumed_audio": check_window(blocks, t("resume") + 180, t("resume") + 380, False),
            "seeked_audio": check_window(blocks, t("seeked") + 150, t("seeked") + 350, False),
        }
        (OUT / f"{codec}-report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report), flush=True)


def main():
    for name in ["LIBRESONIC_URL", "LIBRESONIC_USERNAME", "LIBRESONIC_PASSWORD"]:
        assert os.environ.get(name), f"Set {name} for the isolated fixture"
    OUT.mkdir(parents=True, exist_ok=True)
    for folder in [OUT, OUT / "runtime", OUT / "config", OUT / "state"]:
        folder.mkdir(exist_ok=True)
        folder.chmod(0o700)
    cookie = OUT / "cookie"
    cookie.write_bytes(bytes(256))
    cookie.chmod(0o600)
    env = dict(os.environ, XDG_RUNTIME_DIR=str(OUT / "runtime"),
               XDG_CONFIG_HOME=str(OUT / "config"), PULSE_STATE_PATH=str(OUT / "state"),
               PULSE_RUNTIME_PATH=str(OUT / "runtime"), PULSE_COOKIE=str(cookie),
               PULSE_SERVER=f"unix:{OUT}/pulse.sock", PULSE_SINK="libresonic_proof")
    with (OUT / "pulse.log").open("w") as log:
        server = subprocess.Popen([
            "pulseaudio", "--daemonize=no", "--exit-idle-time=-1", "--use-pid-file=no", "-n",
            f"--load=module-native-protocol-unix socket={OUT}/pulse.sock auth-anonymous=1",
            "--load=module-null-sink sink_name=libresonic_proof rate=48000 channels=2",
        ], env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 5
            while subprocess.run(["pactl", "info"], env=env, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL).returncode:
                assert server.poll() is None and time.monotonic() < deadline, "Private PulseAudio failed"
                time.sleep(0.05)
            devices = subprocess.run([str(ROOT / "target/debug/examples/subsonic_playback"),
                                      "--list-devices"], env=env, capture_output=True,
                                     text=True, check=True).stdout
            (OUT / "devices.txt").write_text(devices)
            choices = [line.split("\t", 1)[0] for line in devices.splitlines()]
            assert choices, "No output in private audio server"
            device = "pulse" if "pulse" in choices else choices[0]
            for codec in ["mp3", "flac"]:
                prove(codec, env, device)
        finally:
            server.terminate()
            server.wait(timeout=5)


if __name__ == "__main__":
    main()
