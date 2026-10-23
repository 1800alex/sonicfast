#!/usr/bin/env python3
"""Original synthetic stereo tone for the opt-in smoke profile."""
import math
import socket
import struct
import time
while True:
    try:
        with socket.create_connection(("server", 4953), timeout=3) as client:
            client.settimeout(1)
            frame = 0
            deadline = time.monotonic()
            while True:
                audio = bytearray()
                for n in range(frame, frame + 882):
                    audio.extend(struct.pack("<hh", int(3000 * math.sin(math.tau * 440 * n / 44100)),
                                             int(3000 * math.sin(math.tau * 880 * n / 44100))))
                client.sendall(audio)
                frame += 882
                deadline += 0.02
                time.sleep(max(0, deadline - time.monotonic()))
    except OSError:
        time.sleep(1)
