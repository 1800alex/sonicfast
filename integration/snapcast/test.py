#!/usr/bin/env python3
"""Docker interoperability proof for the real Sonicfast sender/native receiver.

Requires the built snapcast_probe example. Uses fresh named volumes, ephemeral
loopback ports, real hash-pinned Snapcast packages and original synthetic audio.
Leaves captures/reports in .cache, and always tears down its own Docker project.
"""
import argparse
import array
import json
import math
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import time
from smoke import compose
ROOT = Path(__file__).resolve().parents[2]


def wait_for(check, seconds=15):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        try:
            if result := check():
                return result
        except (OSError, json.JSONDecodeError):
            pass
        time.sleep(.1)
    raise AssertionError("Timed out waiting for " + str(check))


def rpc(port, method="Server.GetStatus", params=None):
    with socket.create_connection(("127.0.0.1", port), timeout=2) as client:
        client.settimeout(2)
        request = {"jsonrpc": "2.0", "id": 1, "method": method}
        if params is not None:
            request["params"] = params
        client.sendall(json.dumps(request).encode() + b"\r\n")
        replies = client.makefile("rb")
        for _ in range(64):
            line = replies.readline(1024 * 1024)
            assert line, "Snapcast control connection closed"
            response = json.loads(line)
            # Snapserver broadcasts notifications on control sockets, including
            # while a fresh connection's request is still being handled.
            if response.get("id") != 1:
                continue
            assert "error" not in response, response
            return response["result"]
        raise AssertionError("Snapcast control response was lost among notifications")


def captures(path):
    """Read blocks from the same timestamp renderer used by the native device."""
    result = []
    with path.open("rb") as file:
        assert file.read(8) == b"SFCAP001", "Invalid diagnostic capture"
        while header := file.read(12):
            if len(header) != 12:
                break  # cancellation can interrupt the final diagnostic record
            at, count = struct.unpack("<qI", header)
            assert count == 882
            audio = file.read(count * 4)
            if len(audio) != count * 4:
                break
            samples = array.array("f")
            samples.frombytes(audio)
            if sys.byteorder != "little":
                samples.byteswap()
            assert all(math.isfinite(v) and abs(v) <= 1.001 for v in samples)
            rms = math.sqrt(sum(v * v for v in samples) / count)
            result.append((at / 1_000_000, rms, samples))
    assert result, "No receiver playout was captured"
    return result


def signal(blocks, right_frequency=880, left_frequency=440):
    active = [b for b in blocks if b[1] > .004]
    assert len(active) >= 180, f"Too little received audio: {len(active)} blocks"
    # Tone is stereo: 440 Hz left, 880 Hz right. Count crossings independently.
    rates = []
    for channel, expected in [(0, left_frequency), (1, right_frequency)]:
        estimates = []
        for _, _, audio in active:
            samples = audio[channel::2]
            crossings = sum(a <= 0 < b for a, b in zip(samples, samples[1:]))
            estimates.append(crossings * 100)
        rate = sum(estimates) / len(estimates)
        assert abs(rate - expected) < 35, (channel, rate)
        rates.append(rate)
    return {"active_blocks": len(active), "frequency_hz": rates,
            "first_audio_time_s": active[0][0], "peak_rms": max(b[1] for b in active)}


def quiet_gaps(blocks):
    active = [b for b in blocks if b[1] > .004]
    start, end = active[0][0], active[-1][0]
    gaps, current = [], None
    for at, rms, _ in blocks:
        if at < start or at > end:
            continue
        if rms < .0001:
            if current is None:
                current = at
        elif current is not None:
            if at - current > .3:
                gaps.append(at - current)
            current = None
    assert any(1.5 < gap < 2.8 for gap in gaps), f"Pause not reflected in output: {gaps}"
    assert any(.65 < gap < 1.3 for gap in gaps), f"Source mute not reflected in output: {gaps}"
    return gaps


def run_codec(binary, codec, out):
    project = f"sonicfast-snapcast-it-{os.getpid()}-{codec}"
    extra_format = "48000:24:2" if codec == "flac" else "48000:16:1"
    env = dict(os.environ, SNAPCAST_CODEC=codec, SNAPCAST_EXTRA_FORMAT=extra_format)
    for key in ("SNAPCAST_SOURCE_PORT", "SNAPCAST_STREAM_PORT", "SNAPCAST_CONTROL_PORT", "SNAPCAST_HTTP_PORT", "SNAPCAST_FORMAT_PORT", "SNAPCAST_PULSE_PORT", "SNAPCAST_MUSIC_PORT"):
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            env[key] = str(reservation.getsockname()[1])
    command = compose() + ["--profile", "native-test", "-p", project, "-f", str(ROOT / "integration/snapcast/compose.yml")]
    children = []
    handles = []

    def dc(*args, **kwargs):
        return subprocess.run(command + list(args), cwd=ROOT, env=env, check=True, **kwargs)

    def launch(name, *args, extra_env=None):
        handle = (out / f"{codec}-{name}.log").open("w")
        handles.append(handle)
        child = subprocess.Popen([str(binary), *args], cwd=ROOT, env=extra_env, stdout=handle, stderr=subprocess.STDOUT)
        children.append(child)
        return child

    def port(number):
        return int(dc("port", "server", str(number), capture_output=True, text=True).stdout.strip().rsplit(":", 1)[1])

    def clients():
        return [c for g in rpc(control)["server"]["groups"] for c in g["clients"] if c["connected"]]

    def wait_connected(ids):
        return wait_for(lambda: all(any(c["id"].startswith(name) for c in clients()) for name in ids))

    report = {"codec": codec}
    try:
        with (out / f"{codec}-docker-build.log").open("w") as log:
            dc("up", "--build", "-d", "--wait", stdout=log, stderr=subprocess.STDOUT)
        source, stream, control = port(4953), port(1704), port(1705)
        wait_connected(["reference-a", "reference-b", "reference-pulse"])
        assert len(rpc(control)["server"]["streams"]) == 2
        # Explicitly select the normal source; the extra format stream is idle.
        for group in rpc(control)["server"]["groups"]:
            rpc(control, "Group.SetStream", {"id": group["id"], "stream_id": "Sonicfast"})
        for suffix in ("a", "b"):
            launch("native-" + suffix, "receive", "--port", str(stream), "--seconds", "18",
                   "--id", "sonicfast-native-" + suffix, "--capture", str(out / f"{codec}-native-{suffix}.bin"))
        pulse_port = int(dc("port", "pulse", "4713", capture_output=True, text=True).stdout.strip().rsplit(":", 1)[1])
        runtime = out / "runtime"
        runtime.mkdir(exist_ok=True, mode=0o700)
        device_env = dict(os.environ, PULSE_SERVER=f"tcp:127.0.0.1:{pulse_port}", XDG_RUNTIME_DIR=str(runtime),
                          PIPEWIRE_REMOTE="sonicfast-test-no-pipewire", PULSE_COOKIE=str(runtime / "no-cookie"))
        dc("exec", "-d", "pulse", "sh", "-c", "exec parec --server=tcp:127.0.0.1:4713 --device=sonicfast_snapcast.monitor --format=float32le --rate=48000 --channels=2 --latency-msec=20 > /captures/native-device.f32")
        dc("exec", "-d", "pulse", "python3", "/usr/local/bin/record-pulse.py")
        launch("native-device", "receive", "--port", str(stream), "--seconds", "18", "--id", "sonicfast-native-device", "--device", "sonicfast_snapcast", extra_env=device_env)
        wait_connected(["sonicfast-native-a", "sonicfast-native-b", "sonicfast-native-device"])
        for group in rpc(control)["server"]["groups"]:
            rpc(control, "Group.SetStream", {"id": group["id"], "stream_id": "Sonicfast"})
        time.sleep(1)
        sent = subprocess.run([str(binary), "send", "--port", str(source), "--seconds", "8", "--scripted"],
                              cwd=ROOT, capture_output=True, text=True, timeout=20)
        (out / f"{codec}-send.log").write_text(sent.stdout + sent.stderr)
        assert sent.returncode == 0, sent.stderr
        evidence = [json.loads(line) for line in sent.stdout.splitlines()]
        end = evidence[-1]
        assert end["frames"] == 8 * 44100 and end["cancelled_write_rejected"]
        assert 9800 <= end["elapsed_ms"] < 16000, end
        report["sender"] = end
        for child in children:
            assert child.wait(timeout=22) == 0
        records = [captures(out / f"{codec}-native-{suffix}.bin") for suffix in ("a", "b")]
        report["native_audio"] = [signal(record) for record in records]
        report["pause_and_mute_gaps_s"] = [quiet_gaps(record) for record in records]
        first = [r["first_audio_time_s"] for r in report["native_audio"]]
        assert abs(first[0] - first[1]) < .035, f"Native clients disagree on playout: {first}"
        report["native_alignment_ms"] = abs(first[0] - first[1]) * 1000
        for suffix in ("a", "b"):
            dc("cp", f"reference-{suffix}:/captures/audio.pcm", str(out / f"{codec}-reference-{suffix}.pcm"), stdout=subprocess.DEVNULL)
            pcm = array.array("h")
            pcm.frombytes((out / f"{codec}-reference-{suffix}.pcm").read_bytes())
            if sys.byteorder != "little":
                pcm.byteswap()
            assert sum(abs(v) > 100 for v in pcm) > 5 * 44100, "Official client received insufficient audio"
            assert max(map(abs, pcm)) < 32767, "Official client clipped"
        dc("cp", "pulse:/captures/native-device.f32", str(out / f"{codec}-native-device.f32"), stdout=subprocess.DEVNULL)
        device_samples = array.array("f")
        device_samples.frombytes((out / f"{codec}-native-device.f32").read_bytes())
        if sys.byteorder != "little":
            device_samples.byteswap()
        device_blocks = []
        for i in range(0, len(device_samples) - 960, 960):
            audio = device_samples[i:i + 960]
            assert all(math.isfinite(v) and abs(v) <= 1.001 for v in audio)
            device_blocks.append((i / 96000, math.sqrt(sum(v * v for v in audio) / 960), audio))
        report["native_device_output"] = signal(device_blocks)
        report["native_device_transport_gaps_s"] = quiet_gaps(device_blocks)
        dc("cp", "pulse:/captures/timing.jsonl", str(out / f"{codec}-device-timing.jsonl"), stdout=subprocess.DEVNULL)
        timing = [json.loads(line) for line in (out / f"{codec}-device-timing.jsonl").read_text().splitlines() if line.endswith("}")]
        first = {name: next(row["at"] for row in timing if row["room"] == name and row["rms"] > .004) for name in ("native", "reference")}
        delta = abs(first["native"] - first["reference"]) * 1000
        assert delta < 40, f"Native output disagrees with official client on the same audio server: {delta:.1f} ms"
        report["native_vs_official_output_alignment_ms"] = delta
        report["official_receivers"] = 3
        # Drive the actual App, OpenSubsonic client, decoder and routed output.
        # No audio device is requested by the sender, even in a signed-in app.
        children.clear()
        app_receiver = launch("app-receiver", "receive", "--port", str(stream), "--seconds", "7",
                              "--id", "sonicfast-app-receiver", "--capture", str(out / f"{codec}-app.bin"))
        wait_connected(["sonicfast-app-receiver"])
        time.sleep(.4)
        music_port = int(dc("port", "music", "8080", capture_output=True, text=True).stdout.strip().rsplit(":", 1)[1])
        app_source = launch("app-source", "app-send", "--port", str(source), "--music-url", f"http://127.0.0.1:{music_port}")
        assert app_source.wait(timeout=18) == 0, (out / f"{codec}-app-source.log").read_text()
        assert app_receiver.wait(timeout=12) == 0
        report["app_login_decode_route"] = signal(captures(out / f"{codec}-app.bin"), right_frequency=660, left_frequency=660)
        # The server's per-client mute must affect only that receiver.
        children.clear()
        volume_probe = launch("server-mute", "receive", "--port", str(stream), "--seconds", "9",
                              "--id", "sonicfast-server-mute", "--capture", str(out / f"{codec}-server-mute.bin"))
        wait_connected(["sonicfast-server-mute"])
        time.sleep(.6)
        tone = launch("server-mute-source", "send", "--port", str(source), "--seconds", "6")
        time.sleep(1.8)
        client_id = next(c["id"] for c in clients() if c["id"].startswith("sonicfast-server-mute"))
        rpc(control, "Client.SetVolume", {"id": client_id, "volume": {"percent": 100, "muted": True}})
        time.sleep(1.5)
        rpc(control, "Client.SetVolume", {"id": client_id, "volume": {"percent": 100, "muted": False}})
        assert tone.wait(timeout=10) == 0
        assert volume_probe.wait(timeout=12) == 0
        muted_audio = captures(out / f"{codec}-server-mute.bin")
        active = [b[0] for b in muted_audio if b[1] > .004]
        assert len(active) >= 200
        assert any(1.3 < b - a < 1.9 for a, b in zip(active, active[1:])), "Server mute did not change native playout"
        report["server_volume_mute"] = "passed"
        # Switch an already-connected native receiver to a different format.
        # This proves codec-header changes, 48k resampling, 24-bit FLAC and mono PCM.
        children.clear()
        format_probe = launch("format-switch", "receive", "--port", str(stream), "--seconds", "9",
                              "--id", "sonicfast-format", "--capture", str(out / f"{codec}-format.bin"))
        wait_connected(["sonicfast-format"])
        group = next(g for g in rpc(control)["server"]["groups"] if any(c["id"].startswith("sonicfast-format") for c in g["clients"]))
        rpc(control, "Group.SetStream", {"id": group["id"], "stream_id": "FormatTest"})
        time.sleep(.5)
        channels, bits = (2, 24) if codec == "flac" else (1, 16)
        with socket.create_connection(("127.0.0.1", port(4954)), timeout=3) as input_socket:
            deadline = time.monotonic()
            for block in range(250):
                audio = bytearray()
                for frame in range(block * 960, (block + 1) * 960):
                    for channel in range(channels):
                        frequency = 440 if channel == 0 else 880
                        sample = int(.1 * (2 ** (bits - 1) - 1) * math.sin(math.tau * frequency * frame / 48000))
                        audio.extend(struct.pack("<i" if bits == 24 else "<h", sample))
                input_socket.sendall(audio)
                deadline += .02
                time.sleep(max(0, deadline - time.monotonic()))
        assert format_probe.wait(timeout=12) == 0
        report["format_switch"] = {"format": extra_format, "signal": signal(captures(out / f"{codec}-format.bin"), 880 if channels == 2 else 440)}
        # Outage must fail a live source promptly, and both receiver implementations
        # must reconnect without replaying old buffered data.
        children.clear()
        recovery = launch("recovery", "receive", "--port", str(stream), "--seconds", "22", "--id", "sonicfast-recovery",
                          "--capture", str(out / f"{codec}-recovery.bin"))
        wait_connected(["sonicfast-recovery"])
        for group in rpc(control)["server"]["groups"]:
            if any(c["id"].startswith("sonicfast-recovery") for c in group["clients"]):
                rpc(control, "Group.SetStream", {"id": group["id"], "stream_id": "Sonicfast"})
        outage = launch("outage-source", "send", "--port", str(source), "--seconds", "30")
        time.sleep(1)
        dc("stop", "server", stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        assert outage.wait(timeout=5) != 0, "Source ignored server shutdown"
        time.sleep(.5)
        dc("start", "server", stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        wait_connected(["sonicfast-recovery", "reference-a", "reference-b"])
        recovered = subprocess.run([str(binary), "send", "--port", str(source), "--seconds", "4"], cwd=ROOT,
                                   capture_output=True, text=True, timeout=12)
        assert recovered.returncode == 0, recovered.stderr
        assert recovery.wait(timeout=25) == 0
        status = (out / f"{codec}-recovery.log").read_text()
        assert "reconnecting" in status and status.count("clock synchronized") >= 2, status
        assert sum(b[1] > .004 for b in captures(out / f"{codec}-recovery.bin")) >= 250
        report["outage_and_recovery"] = "passed"
        (out / f"{codec}-report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report), flush=True)
    finally:
        for child in children:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=6)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
        for handle in handles:
            handle.close()
        with (out / f"{codec}-docker.log").open("w") as log:
            subprocess.run(command + ["logs", "--no-color"], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        original_error = sys.exc_info()[1]
        cleanup = subprocess.run(command + ["down", "--volumes", "--remove-orphans"], cwd=ROOT, env=env,
                                 capture_output=True, text=True)
        if cleanup.returncode:
            (out / f"{codec}-cleanup.log").write_text(cleanup.stdout + cleanup.stderr)
            if original_error is None:
                cleanup.check_returncode()
            else:
                print("Fixture cleanup also failed; see the cleanup log", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/examples/snapcast_probe")
    parser.add_argument("--codec", choices=["pcm", "flac", "both"], default="both")
    args = parser.parse_args()
    if not args.binary.is_file():
        raise SystemExit("Build the probe first: cargo build --locked --example snapcast_probe")
    out = ROOT / ".cache/snapcast" / time.strftime("integration-%Y%m%d-%H%M%S", time.gmtime())
    out.mkdir(parents=True)
    print("Evidence:", out, flush=True)
    for codec in (["flac", "pcm"] if args.codec == "both" else [args.codec]):
        run_codec(args.binary.resolve(), codec, out)
    print("Snapcast Docker interoperability checks passed", flush=True)

if __name__ == "__main__":
    main()
