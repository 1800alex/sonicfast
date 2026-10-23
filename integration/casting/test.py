#!/usr/bin/env python3
"""Exercise Sonicfast AirPlay and Cast playback against isolated Docker receivers."""
import array
import datetime
import json
import math
import os
from pathlib import Path
import shutil
import socket
import subprocess

ROOT = Path(__file__).resolve().parents[2]
IMAGE = "sonicfast-casting-fixture:5.6.1"

def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, **kwargs).stdout

def main():
    compose = ["docker", "compose"]
    if subprocess.run(compose + ["version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        if not shutil.which("docker-compose"):
            raise SystemExit("Docker Compose v2 is required")
        compose = ["docker-compose"]
    project = f"sonicfast-casting-it-{os.getpid()}"
    folder = ROOT / ".cache/casting" / datetime.datetime.now(datetime.timezone.utc).strftime("integration-%Y%m%d-%H%M%S")
    folder.mkdir(parents=True)
    with (folder / "build.log").open("wb") as log:
        subprocess.run(["docker", "build", "-f", str(ROOT / "integration/casting/Dockerfile"), "-t", IMAGE, str(ROOT / "integration")], stdout=log, stderr=subprocess.STDOUT, check=True)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1",0))
        cast_port=reservation.getsockname()[1]
    name = f"Sonicfast fixture {os.getpid()}"
    services = {
        "pulse": {"image": IMAGE, "command": ["pulse"], "volumes": ["pulse-runtime:/run/pulse"],
                  "healthcheck": {"test": ["CMD", "pactl", "--server=unix:/run/pulse/native", "info"], "interval": "1s", "timeout": "2s", "retries": 20},
                  "user": "0:0"},
        "airplay": {"image": IMAGE, "command": ["airplay"], "network_mode": "host", "init": True,
                    "environment": {"FIXTURE_NAME": name, "FIXTURE_PORT": str(port)},
                    "volumes": ["pulse-runtime:/run/pulse:ro"],
                    "depends_on": {"pulse": {"condition": "service_healthy"}},
                    "healthcheck": {"test": ["CMD-SHELL", "pidof shairport-sync >/dev/null"], "interval": "1s", "timeout": "2s", "retries": 20}},
    }
    # Health checks authenticate to Pulse as the same uid; entrypoint starts as
    # root only to prepare its private socket directory before dropping uid.
    services["pulse"]["healthcheck"]["test"] = ["CMD", "su-exec", "1000:1000", "pactl", "--server=unix:/run/pulse/native", "info"]
    services["cast"]={"image":IMAGE,"entrypoint":["python3","/usr/local/bin/cast_receiver.py"],"network_mode":"host","init":True,
        "environment":{"CAST_RECEIVER_PORT":str(cast_port),"CAST_RECEIVER_NAME":name+" Cast","PULSE_SERVER":"unix:/run/pulse/native"},
        "volumes":["pulse-runtime:/run/pulse:ro","cast-state:/state"],"depends_on":{"pulse":{"condition":"service_healthy"}},
        "healthcheck":{"test":["CMD","python3","/usr/local/bin/cast_receiver.py","--health"],"interval":"1s","timeout":"2s","retries":20}}
    services["music"]={"image":IMAGE,"entrypoint":["python3","/usr/local/bin/music-fixture.py"],"ports":["127.0.0.1::8080"]}
    fixture = folder / "compose.json"
    fixture.write_text(json.dumps({"services": services, "volumes": {"pulse-runtime": {},"cast-state":{}}}))
    command = compose + ["-p", project, "-f", str(fixture)]
    recorder = None
    try:
        run(*command, "up", "-d", "--wait")
        capture = folder / "output.f32"
        with capture.open("wb") as audio:
            recorder = subprocess.Popen(command + ["exec", "-T", "--user", "1000:1000", "pulse", "timeout", "14", "parec",
                                                   "--server=unix:/run/pulse/native", "--device=casting_test.monitor", "--format=float32le", "--rate=44100", "--channels=2"],
                                        stdout=audio, stderr=subprocess.PIPE)
            probe = run(str(ROOT / "target/debug/examples/casting_probe"), str(port), timeout=25)
            (folder / "probe.log").write_bytes(probe)
            _, errors = recorder.communicate(timeout=20)
            if recorder.returncode not in (0, 124):
                raise AssertionError(errors.decode())
        pcm = array.array("f")
        pcm.frombytes(capture.read_bytes())
        assert len(pcm) > 44100 * 2 * 8, "Capture too short"
        assert all(math.isfinite(value) for value in pcm), "Nonfinite samples"
        rms = math.sqrt(sum(value * value for value in pcm) / len(pcm))
        assert rms > 0.02, f"No useful audio: {rms}"
        mono = pcm[::2]
        active = [i for i in range(0, len(mono)-4410, 4410) if sum(x*x for x in mono[i:i+4410])/4410 > .0004]
        assert len(active) > 50, "Insufficient sustained audio"
        start = active[len(active)//2]
        segment = mono[start:start+4410]
        frequency = sum(a <= 0 < b for a,b in zip(segment, segment[1:])) * 10
        assert abs(frequency - 660) <= 20, f"Incorrect decoded signal: {frequency}"
        report = {"receiver": "Shairport Sync 5.6.1", "source": "Sonicfast Rust ALAC/RTP sender", "frequency_hz": frequency,
                  "rms": rms, "capture_samples": len(pcm), "cancelled_cleanly": True, "host_audio_access": False}
        (folder / "report.json").write_text(json.dumps(report, indent=2)+"\n")
        print(json.dumps(report, indent=2))
        fingerprint=run(*command,"exec","-T","cast","python3","/usr/local/bin/cast_receiver.py","--fingerprint").decode().strip()
        music_port=run(*command,"port","music","8080").decode().strip().rsplit(':',1)[1]
        cast_probe=[str(ROOT/"target/debug/examples/snapcast_probe"),"app-cast","--music-url",f"http://127.0.0.1:{music_port}","--port",str(cast_port),"--fingerprint"]
        capture=folder/"app-airplay-output.f32"
        with capture.open("wb") as audio:
            recorder=subprocess.Popen(command+["exec","-T","--user","1000:1000","pulse","timeout","10","parec","--server=unix:/run/pulse/native","--device=casting_test.monitor","--format=float32le","--rate=44100","--channels=2"],stdout=audio,stderr=subprocess.PIPE)
            proof=run(str(ROOT/"target/debug/examples/snapcast_probe"),"app-airplay","--music-url",f"http://127.0.0.1:{music_port}","--port",str(port),timeout=25)
            (folder/"app-airplay-probe.log").write_bytes(proof)
            assert b'"cast_controls_exercised":true' in proof,"AirPlay transport proof did not run"
            _,errors=recorder.communicate(timeout=15)
            assert recorder.returncode in (0,124),errors
        pcm=array.array("f");pcm.frombytes(capture.read_bytes())
        assert pcm and all(math.isfinite(v) for v in pcm)
        rms=math.sqrt(sum(v*v for v in pcm)/len(pcm))
        assert rms>.005,"App AirPlay receiver produced no audio"
        (folder/"app-airplay-report.json").write_text(json.dumps({"source":"Actual App with synthetic Subsonic FLAC","pause_seek_resume":True,"rms":rms},indent=2)+"\n")
        rejected=subprocess.run(cast_probe+["0"*64],capture_output=True,timeout=25)
        assert rejected.returncode!=0 and b"app_sent" not in rejected.stdout,"Untrusted receiver was accepted"
        capture=folder/"cast-output.f32"
        with capture.open("wb") as audio:
            recorder=subprocess.Popen(command+["exec","-T","--user","1000:1000","pulse","timeout","10","parec","--server=unix:/run/pulse/native","--device=casting_test.monitor","--format=float32le","--rate=44100","--channels=2"],stdout=audio,stderr=subprocess.PIPE)
            proof=run(*cast_probe,fingerprint,timeout=25)
            (folder/"cast-probe.log").write_bytes(proof)
            assert b'"cast_controls_exercised":true' in proof,"Cast pause/seek/resume proof did not run"
            assert b'"cast_replayed":true' in proof,"Finished Cast song did not restart"
            _,errors=recorder.communicate(timeout=15)
            assert recorder.returncode in (0,124),errors
        pcm=array.array("f");pcm.frombytes(capture.read_bytes())
        assert pcm and all(math.isfinite(v) for v in pcm)
        rms=math.sqrt(sum(v*v for v in pcm)/len(pcm))
        assert rms>.005,"Cast receiver produced no audio"
        report={"source":"Actual Sonicfast App with synthetic Subsonic FLAC","receiver":"Certificate-pinned software CastV2 target + mpv","rms":rms,"wrong_pin_rejected":True,"pause_seek_resume":True,"replay_after_end":True,"hardware_chromecast":False}
        (folder/"cast-report.json").write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps(report,indent=2))
    finally:
        if recorder is not None and recorder.poll() is None:
            recorder.kill()
            recorder.communicate()
        try:
            (folder / "containers.log").write_bytes(run(*command, "logs", "--no-color"))
        finally:
            run(*command, "down", "--volumes", "--remove-orphans")

if __name__ == "__main__":
    main()
