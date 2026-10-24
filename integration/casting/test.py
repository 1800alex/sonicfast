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
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        mpd_port = reservation.getsockname()[1]
    services["mpd"] = {"image": IMAGE, "entrypoint": ["python3", "/usr/local/bin/mpd_receiver.py"],
        "network_mode": "host", "init": True, "environment": {"MPD_PORT": str(mpd_port)},
        "volumes": ["pulse-runtime:/run/pulse:ro"], "depends_on": {"pulse": {"condition": "service_healthy"}},
        "healthcheck": {"test": ["CMD", "python3", "/usr/local/bin/mpd_receiver.py", "--health"],
                        "interval": "1s", "timeout": "2s", "retries": 20}}
    services["music"]={"image":IMAGE,"entrypoint":["python3","/usr/local/bin/music-fixture.py"],"ports":["127.0.0.1::8080"]}
    hap_ports = {}
    for service, fault in [("hap", None), ("hap-bad-srp", "HAP_BAD_SRP"),
                           ("hap-bad-setup", "HAP_BAD_SETUP_SIGNATURE"),
                           ("hap-bad-verify", "HAP_BAD_VERIFY_SIGNATURE")]:
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            hap_ports[service] = reservation.getsockname()[1]
        environment = {"HAP_PORT": str(hap_ports[service])}
        if fault:
            environment[fault] = "1"
        services[service] = {"image": IMAGE, "entrypoint": ["python3", "/usr/local/bin/hap_receiver.py"],
            "network_mode": "host", "init": True, "environment": environment,
            "healthcheck": {"test": ["CMD", "python3", "/usr/local/bin/hap_receiver.py", "--health"],
                            "interval": "1s", "timeout": "2s", "retries": 20}}
    fixture = folder / "compose.json"
    fixture.write_text(json.dumps({"services": services, "volumes": {"pulse-runtime": {},"cast-state":{}}}))
    command = compose + ["-p", project, "-f", str(fixture)]
    def run_probe(tag, *args, **kwargs):
        result = subprocess.run(args, capture_output=True, **kwargs)
        (folder / (tag + "-process.log")).write_bytes(result.stdout + result.stderr)
        result.check_returncode()
        return result.stdout
    recorder = None
    try:
        run(*command, "up", "-d", "--wait")
        pair_probe = str(ROOT / "target/debug/examples/airplay_pair_probe")
        def hap_report(service):
            report = json.loads(run(*command, "exec", "-T", service, "cat", "/report/report.json"))
            (folder / (service + "-report.json")).write_text(json.dumps(report, indent=2) + "\n")
            return report
        # The synthetic PIN has a leading zero. No personal receiver is contacted.
        wrong_pin = subprocess.run([pair_probe, "127.0.0.1", str(hap_ports["hap"])], input=b"0000\n", capture_output=True, timeout=15)
        (folder / "hap-wrong-pin-process.log").write_bytes(wrong_pin.stdout + wrong_pin.stderr)
        assert wrong_pin.returncode != 0 and b"Pairing complete" not in wrong_pin.stdout
        for service in ("hap-bad-srp", "hap-bad-setup", "hap-bad-verify"):
            rejected = subprocess.run([pair_probe, "127.0.0.1", str(hap_ports[service])], input=b"0427\n", capture_output=True, timeout=15)
            (folder / (service + "-process.log")).write_bytes(rejected.stdout + rejected.stderr)
            assert rejected.returncode != 0, f"Forged {service} authentication was accepted"
            report = hap_report(service)
            assert report["audio_packets"] == 0, "Audio leaked before receiver authentication"
            assert not report["errors"], report
        run_probe("hap", pair_probe, "127.0.0.1", str(hap_ports["hap"]), "--reconnect", input=b"0427\n", timeout=30)
        # Allow receiver capture threads to flush their report after TEARDOWN.
        import time
        deadline = time.monotonic() + 3
        while True:
            report = hap_report("hap")
            if report["teardown"] >= 2 and "frequency_hz" in report:
                break
            assert time.monotonic() < deadline, "Paired receiver did not close cleanly"
            time.sleep(.05)
        assert report["wrong_pin_rejected"] == 1 and report["paired"] == 1
        assert report["verified"] == 2, "Reconnect must verify saved pairing without another PIN"
        assert report["clock_replies"] == 2 and report["events_acknowledged"] == 2
        assert report["resends"] == 2 and report["feedback"] >= 2
        assert report["audio_packets"] > 1000 and report["invalid_audio"] == 0
        assert report["rms"] > .02 and abs(report["frequency_hz"] - 660) < 5
        assert not report["errors"], report
        print(json.dumps({"paired_airplay": report}, indent=2))
        capture = folder / "output.f32"
        with capture.open("wb") as audio:
            recorder = subprocess.Popen(command + ["exec", "-T", "--user", "1000:1000", "pulse", "timeout", "14", "parec",
                                                   "--server=unix:/run/pulse/native", "--device=casting_test.monitor", "--format=float32le", "--rate=44100", "--channels=2"],
                                        stdout=audio, stderr=subprocess.PIPE)
            probe = run_probe("native", str(ROOT / "target/debug/examples/casting_probe"), str(port), timeout=25)
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
            proof=run_probe("app-airplay", str(ROOT/"target/debug/examples/snapcast_probe"),"app-airplay","--music-url",f"http://127.0.0.1:{music_port}","--port",str(port),timeout=25)
            (folder/"app-airplay-probe.log").write_bytes(proof)
            assert b'"cast_controls_exercised":true' in proof,"AirPlay transport proof did not run"
            assert b'"output_auto_resumed":true' in proof and b'"paused_output_stayed_paused":true' in proof and b'"load_survived_output_change":true' in proof,"AirPlay output transition proof did not run"
            _,errors=recorder.communicate(timeout=15)
            assert recorder.returncode in (0,124),errors
        pcm=array.array("f");pcm.frombytes(capture.read_bytes())
        assert pcm and all(math.isfinite(v) for v in pcm)
        rms=math.sqrt(sum(v*v for v in pcm)/len(pcm))
        assert rms>.005,"App AirPlay receiver produced no audio"
        (folder/"app-airplay-report.json").write_text(json.dumps({"source":"Actual App with synthetic Subsonic FLAC","pause_seek_resume":True,"rms":rms},indent=2)+"\n")
        # The picker's check: unpinned, the software receiver is untrusted and
        # presents exactly its own certificate fingerprint; pinned, it verifies.
        cast_check=str(ROOT/"target/debug/examples/cast_check")
        first=json.loads(run(cast_check,"127.0.0.1",str(cast_port),timeout=15))
        assert first=={"trust":"untrusted","pinned":False,"presented":fingerprint.lower()},first
        assert json.loads(run(cast_check,"127.0.0.1",str(cast_port),fingerprint,timeout=15))=={"trust":"verified"}
        changed=json.loads(run(cast_check,"127.0.0.1",str(cast_port),"0"*64,timeout=15))
        assert changed["trust"]=="untrusted" and changed["pinned"] and changed["presented"]==fingerprint.lower(),changed
        (folder/"cast-check-report.json").write_text(json.dumps({"unpinned":first,"changed":changed},indent=2)+"\n")
        rejected=subprocess.run(cast_probe+["0"*64],capture_output=True,timeout=25)
        assert rejected.returncode!=0 and b"app_sent" not in rejected.stdout,"Untrusted receiver was accepted"
        capture=folder/"cast-output.f32"
        with capture.open("wb") as audio:
            recorder=subprocess.Popen(command+["exec","-T","--user","1000:1000","pulse","timeout","10","parec","--server=unix:/run/pulse/native","--device=casting_test.monitor","--format=float32le","--rate=44100","--channels=2"],stdout=audio,stderr=subprocess.PIPE)
            proof=run_probe("app-cast", *cast_probe,fingerprint,timeout=25)
            (folder/"cast-probe.log").write_bytes(proof)
            assert b'"cast_controls_exercised":true' in proof,"Cast pause/seek/resume proof did not run"
            assert b'"cast_replayed":true' in proof,"Finished Cast song did not restart"
            assert b'"fingerprint_auto_recovered":true' in proof,"Corrected fingerprint did not auto-apply and restart playback"
            assert b'"output_auto_resumed":true' in proof and b'"paused_output_stayed_paused":true' in proof and b'"load_survived_output_change":true' in proof,"Cast output transition proof did not run"
            _,errors=recorder.communicate(timeout=15)
            assert recorder.returncode in (0,124),errors
        pcm=array.array("f");pcm.frombytes(capture.read_bytes())
        assert pcm and all(math.isfinite(v) for v in pcm)
        rms=math.sqrt(sum(v*v for v in pcm)/len(pcm))
        assert rms>.005,"Cast receiver produced no audio"
        report={"source":"Actual Sonicfast App with synthetic Subsonic FLAC","receiver":"Certificate-pinned software CastV2 target + mpv","rms":rms,"wrong_pin_rejected":True,"pause_seek_resume":True,"replay_after_end":True,"hardware_chromecast":False}
        (folder/"cast-report.json").write_text(json.dumps(report,indent=2)+"\n")
        print(json.dumps(report,indent=2))
        # MPD fetches the app's protected lease with its curl input and decodes it.
        mpd_probe=[str(ROOT/"target/debug/examples/snapcast_probe"),"app-mpd","--music-url",f"http://127.0.0.1:{music_port}","--port",str(mpd_port)]
        capture=folder/"mpd-output.f32"
        with capture.open("wb") as audio:
            recorder=subprocess.Popen(command+["exec","-T","--user","1000:1000","pulse","timeout","10","parec","--server=unix:/run/pulse/native","--device=casting_test.monitor","--format=float32le","--rate=44100","--channels=2"],stdout=audio,stderr=subprocess.PIPE)
            proof=run_probe("app-mpd", *mpd_probe, timeout=25)
            (folder/"mpd-probe.log").write_bytes(proof)
            assert b'"cast_controls_exercised":true' in proof,"MPD pause/seek/resume proof did not run"
            assert b'"cast_replayed":true' in proof,"Finished MPD song did not restart"
            assert b'"output_auto_resumed":true' in proof and b'"paused_output_stayed_paused":true' in proof and b'"load_survived_output_change":true' in proof,"MPD output transition proof did not run"
            _,errors=recorder.communicate(timeout=15)
            assert recorder.returncode in (0,124),errors
        pcm=array.array("f");pcm.frombytes(capture.read_bytes())
        assert pcm and all(math.isfinite(v) for v in pcm)
        rms=math.sqrt(sum(v*v for v in pcm)/len(pcm))
        assert rms>.005,"MPD produced no audio"
        version=run(*command,"exec","-T","mpd","mpd","--version").decode().splitlines()[0]
        report={"source":"Actual Sonicfast App with synthetic Subsonic FLAC","receiver":version+" (curl input, pulse output)","rms":rms,"pause_seek_resume":True,"replay_after_end":True,"password_protected":False}
        (folder/"mpd-report.json").write_text(json.dumps(report,indent=2)+"\n")
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
