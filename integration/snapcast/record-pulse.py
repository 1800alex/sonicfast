#!/usr/bin/env python3
"""Compare independently scheduled native/reference outputs on one test clock.

Only runs inside the disposable PulseAudio container. Never connects to a host
sound server. File captures above independently verify frequency/format/level.
"""
import array
import json
import math
import subprocess
import threading
import time
lock = threading.Lock()
output = open("/captures/timing.jsonl", "w", buffering=1)


def record(room, sink):
    process = subprocess.Popen(["parec", "--server=tcp:127.0.0.1:4713", "--device=" + sink + ".monitor",
                                "--format=float32le", "--rate=48000", "--channels=2", "--latency-msec=10"],
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        while data := process.stdout.read(3840):
            at = time.monotonic()
            samples = array.array("f")
            samples.frombytes(data)
            rms = math.sqrt(sum(v * v for v in samples) / len(samples))
            with lock:
                output.write(json.dumps({"room": room, "at": at, "rms": rms}) + "\n")
    finally:
        process.terminate()
        process.wait()


threads = [threading.Thread(target=record, args=(room, sink)) for room, sink in
           [("native", "sonicfast_snapcast"), ("reference", "sonicfast_reference")]]
for thread in threads:
    thread.start()
for thread in threads:
    thread.join()
