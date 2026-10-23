#!/usr/bin/env python3
"""Independent, isolated HAP/PIN audio fixture. Not an AirPlay product/server.

Uses Python integers/hashlib and distribution cryptography, independently of the
Rust client's ring/num-bigint. Never exposes pairing keys or mounts host audio.
"""
import array
import hashlib
import hmac
import json
import math
import os
from pathlib import Path
import plistlib
import socket
import socketserver
import struct
import threading
import time

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.hashes import SHA512
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

# RFC 3526 group 15 / RFC 5054 3072-bit group, generator 5 for SRP.
N = int("""
FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E08
8A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD
3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625
E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE386BFB5A8
99FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163
BF0598DA48361C55D39A69163FA8FD24CF5F83655D23DCA3A
D961C62F356208552BB9ED529077096966D670C354E4ABC9804
F1746C08CA18217C32905E462E36CE3BE39E772C180E86039B
2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF695581718399
5497CEA956AE515D2261898FA051015728E5A8AAAC42DAD331
70D04507A33A85521ABDF1CBA64ECFB850458DBEF0A8AEA715
75D060C7DB3970F85A6E1E4C7ABF5AE8CDB0933D71E8C94E
04A25619DCEE3D2261AD2EE6BF12FFA06D98A0864D8760273
3EC86A64521F2B18177B200CBBE117577A615D6C770988C0BA
D946E208E24FA074E5AB3143DB5BFCE0FD108E4B82D120A93
AD2CAFFFFFFFFFFFFFFFF
""".replace("\n", ""), 16)
PIN = "0427"  # Synthetic fixture only; leading zero is intentional.
IDENTITY = b"Sonicfast HAP integration receiver"
SIGNING = Ed25519PrivateKey.generate()
PAIRED = {}
LOCK = threading.Lock()
REPORT = {"paired": 0, "verified": 0, "wrong_pin_rejected": 0, "clock_replies": 0,
          "events_acknowledged": 0, "audio_packets": 0, "feedback": 0, "teardown": 0,
          "invalid_audio": 0, "resends": 0, "errors": []}
ROOT = Path(os.environ.get("HAP_REPORT_DIR", "/report"))

def save():
    with LOCK:
        ROOT.mkdir(parents=True, exist_ok=True)
        (ROOT / "report.json").write_text(json.dumps(REPORT))

def inc(key):
    with LOCK:
        REPORT[key] += 1

def raw(n, padded=False):
    return n.to_bytes(384 if padded else max(1, (n.bit_length() + 7) // 8), "big")

def digest(*parts):
    return hashlib.sha512(b"".join(parts)).digest()

def derive(secret, salt, info):
    return HKDF(SHA512(), 32, salt.encode(), info.encode()).derive(secret)

def tlv(fields):
    out = bytearray()
    for tag, value in fields.items():
        for offset in range(0, len(value), 255):
            chunk = value[offset:offset + 255]
            out.extend(bytes([tag, len(chunk)]) + chunk)
    return bytes(out)

def untlv(data):
    fields = {}
    while data:
        tag, length = data[:2]
        assert len(data) >= length + 2
        fields[tag] = fields.get(tag, b"") + data[2:length + 2]
        data = data[length + 2:]
    return fields

def nonce(value):
    return b"\0" * 4 + (value.to_bytes(8, "little") if isinstance(value, int) else value)

def seal(key, n, body, aad=b""):
    return ChaCha20Poly1305(key).encrypt(nonce(n), body, aad)

def unseal(key, n, body, aad=b""):
    return ChaCha20Poly1305(key).decrypt(nonce(n), body, aad)

class Wire:
    def __init__(self, sock):
        self.sock, self.buffer = sock, b""
        self.write_key = self.read_key = None
        self.rx = self.tx = 0

    def exact(self, size):
        data = b""
        while len(data) < size:
            part = self.sock.recv(size - len(data))
            if not part:
                raise EOFError
            data += part
        return data

    def read(self, size):
        if self.read_key is None:
            return self.exact(size)
        while len(self.buffer) < size:
            header = self.exact(2)
            length = int.from_bytes(header, "little")
            assert 0 < length <= 1024
            self.buffer += unseal(self.read_key, self.rx, self.exact(length + 16), header)
            self.rx += 1
        result, self.buffer = self.buffer[:size], self.buffer[size:]
        return result

    def send(self, data):
        if self.write_key is None:
            self.sock.sendall(data)
            return
        for offset in range(0, len(data), 1024):
            chunk = data[offset:offset + 1024]
            header = len(chunk).to_bytes(2, "little")
            record = header + seal(self.write_key, self.tx, chunk, header)
            self.tx += 1
            # Deliberately fragment records, including the two-byte size.
            self.sock.sendall(record[:1])
            self.sock.sendall(record[1:7])
            self.sock.sendall(record[7:])

    def request(self):
        header = b""
        while not header.endswith(b"\r\n\r\n"):
            header += self.read(1)
            assert len(header) <= 8192
        lines = header.decode().split("\r\n")
        method, path, _ = lines[0].split()
        headers = dict(line.split(": ", 1) for line in lines[1:] if ": " in line)
        length = int(headers.get("Content-Length", "0"))
        assert 0 <= length <= 65536
        return method, path, headers, self.read(length)

    def reply(self, headers, body=b"", status=200, extra=""):
        self.send(f"RTSP/1.0 {status} Reply\r\nCSeq: {headers.get('CSeq', '0')}\r\nContent-Length: {len(body)}\r\n{extra}\r\n".encode() + body)

def event_server(listener, secret):
    try:
        sock, _ = listener.accept()
        with sock:
            sock.settimeout(30)
            wire = Wire(sock)
            wire.write_key = derive(secret, "Events-Salt", "Events-Write-Encryption-Key")
            wire.read_key = derive(secret, "Events-Salt", "Events-Read-Encryption-Key")
            wire.send(b"POST /event HTTP/1.1\r\nCSeq: 123\r\nContent-Length: 5\r\n\r\nevent")
            method, _, headers, body = wire.request()
            assert method in ("HTTP/1.1", "RTSP/1.0") and headers["CSeq"] == "123" and body == b""
            inc("events_acknowledged")
            while sock.recv(1024):
                pass
    except (EOFError, ConnectionResetError, socket.timeout):
        pass
    finally:
        listener.close()

def udp_socket():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", 0))
    sock.settimeout(1)
    return sock

class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        self.request.settimeout(130)
        wire = Wire(self.request)
        resources = []
        stopped = threading.Event()
        try:
            while True:
                method, path, headers, body = wire.request()
                if path == "/pair-pin-start":
                    wire.reply(headers)
                elif path == "/pair-setup":
                    fields = untlv(body)
                    seq = fields[6][0]
                    if seq == 1:
                        salt, private = os.urandom(16), int.from_bytes(os.urandom(32), "big")
                        verifier = pow(5, int.from_bytes(digest(salt, digest(b"Pair-Setup:", PIN.encode())), "big"), N)
                        multiplier = int.from_bytes(digest(raw(N), raw(5, True)), "big")
                        public = (multiplier * verifier + pow(5, private, N)) % N
                        wire.reply(headers, tlv({6: b"\2", 2: salt, 3: raw(public)}))
                    elif seq == 3:
                        client = int.from_bytes(fields[3], "big")
                        assert client % N
                        scrambling = int.from_bytes(digest(raw(client, True), raw(public, True)), "big")
                        secret = pow(client * pow(verifier, scrambling, N), private, N)
                        key = digest(raw(secret))
                        proof = digest(bytes(a ^ b for a, b in zip(digest(raw(N)), digest(raw(5)))), digest(b"Pair-Setup"), salt, fields[3], raw(public), key)
                        if not hmac.compare_digest(proof, fields[4]):
                            inc("wrong_pin_rejected")
                            wire.reply(headers, tlv({6: b"\4", 7: b"\2"}))
                            return
                        response = digest(fields[3], proof, key)
                        if os.environ.get("HAP_BAD_SRP") == "1":
                            response = b"\0" * 64
                        wire.reply(headers, tlv({6: b"\4", 4: response}))
                    elif seq == 5:
                        pair_key = derive(key, "Pair-Setup-Encrypt-Salt", "Pair-Setup-Encrypt-Info")
                        identity = untlv(unseal(pair_key, b"PS-Msg05", fields[5]))
                        controller_x = derive(key, "Pair-Setup-Controller-Sign-Salt", "Pair-Setup-Controller-Sign-Info")
                        Ed25519PublicKey.from_public_bytes(identity[3]).verify(identity[10], controller_x + identity[1] + identity[3])
                        PAIRED[identity[1]] = identity[3]
                        receiver_key = SIGNING.public_key().public_bytes_raw()
                        accessory_x = derive(key, "Pair-Setup-Accessory-Sign-Salt", "Pair-Setup-Accessory-Sign-Info")
                        sig = SIGNING.sign(accessory_x + IDENTITY + receiver_key)
                        if os.environ.get("HAP_BAD_SETUP_SIGNATURE") == "1":
                            sig = b"\0" * 64
                        encrypted = seal(pair_key, b"PS-Msg06", tlv({1: IDENTITY, 3: receiver_key, 10: sig}))
                        wire.reply(headers, tlv({6: b"\6", 5: encrypted}))
                        inc("paired")
                    else:
                        raise AssertionError("Invalid pairing sequence")
                elif path == "/pair-verify":
                    fields = untlv(body)
                    if fields[6] == b"\1":
                        private_key = X25519PrivateKey.generate()
                        client_public = fields[3]
                        public_key = private_key.public_key().public_bytes_raw()
                        shared = private_key.exchange(X25519PublicKey.from_public_bytes(client_public))
                        verify_key = derive(shared, "Pair-Verify-Encrypt-Salt", "Pair-Verify-Encrypt-Info")
                        signature = SIGNING.sign(public_key + IDENTITY + client_public)
                        if os.environ.get("HAP_BAD_VERIFY_SIGNATURE") == "1":
                            signature = b"\0" * 64
                        identity = tlv({1: IDENTITY, 10: signature})
                        wire.reply(headers, tlv({6: b"\2", 3: public_key, 5: seal(verify_key, b"PV-Msg02", identity)}))
                    else:
                        identity = untlv(unseal(verify_key, b"PV-Msg03", fields[5]))
                        Ed25519PublicKey.from_public_bytes(PAIRED[identity[1]]).verify(identity[10], client_public + identity[1] + public_key)
                        wire.reply(headers, tlv({6: b"\4"}))
                        wire.read_key = derive(shared, "Control-Salt", "Control-Write-Encryption-Key")
                        wire.write_key = derive(shared, "Control-Salt", "Control-Read-Encryption-Key")
                        inc("verified")
                elif method == "SETUP":
                    assert wire.read_key, "Unverified stream setup"
                    setup = plistlib.loads(body)
                    if "streams" not in setup:
                        event = socket.socket()
                        event.bind(("127.0.0.1", 0))
                        event.listen()
                        event.settimeout(5)
                        threading.Thread(target=event_server, args=(event, shared), daemon=True).start()
                        timing = udp_socket()
                        resources.append(timing)
                        timing.sendto(b"\x80\xd2\0\7" + b"\0" * 20 + struct.pack("!Q", 12345), (self.client_address[0], setup["timingPort"]))
                        clock, _ = timing.recvfrom(128)
                        assert clock[1] == 0xd3 and int.from_bytes(clock[8:16], "big") == 12345
                        inc("clock_replies")
                        wire.reply(headers, plistlib.dumps({"eventPort": event.getsockname()[1]}, fmt=plistlib.FMT_BINARY), extra="Session: 123\r\n")
                    else:
                        stream = setup["streams"][0]
                        assert stream["ct"] == 1 and stream["spf"] == 352 and stream["sr"] == 44100
                        audio, control = udp_socket(), udp_socket()
                        resources.extend([audio, control])
                        threading.Thread(target=self.audio, args=(audio, control, stream, stopped), daemon=True).start()
                        wire.reply(headers, plistlib.dumps({"streams": [{"dataPort": audio.getsockname()[1], "controlPort": control.getsockname()[1]}]}, fmt=plistlib.FMT_BINARY))
                elif method == "RECORD" or method == "SET_PARAMETER":
                    assert wire.read_key
                    wire.reply(headers)
                elif path == "/feedback":
                    assert wire.read_key
                    inc("feedback")
                    wire.reply(headers)
                elif method == "TEARDOWN":
                    inc("teardown")
                    wire.reply(headers)
                    return
                else:
                    wire.reply(headers, status=400)
        except (EOFError, ConnectionResetError, BrokenPipeError):
            pass
        except Exception as error:
            with LOCK:
                REPORT["errors"].append(type(error).__name__)
        finally:
            stopped.set()
            for resource in resources:
                resource.close()
            save()

    def audio(self, audio, control, stream, stopped):
        samples = array.array("h")
        expected = 0
        try:
            while not stopped.is_set():
                try:
                    packet, _ = audio.recvfrom(4096)
                except socket.timeout:
                    continue
                n = int.from_bytes(packet[-8:], "little")
                assert n == expected
                expected += 1
                pcm = unseal(stream["shk"], n, packet[12:-8], packet[4:12])
                assert len(pcm) == 352 * 2 * 2
                block = array.array("h")
                block.frombytes(pcm)
                if __import__("sys").byteorder == "little":
                    block.byteswap()
                samples.extend(block)
                inc("audio_packets")
                if expected == 10:
                    seq = int.from_bytes(packet[2:4], "big")
                    control.sendto(b"\x80\xd5\0\1" + struct.pack("!HH", seq, 1), (self.client_address[0], stream["controlPort"]))
                    while True:
                        reply, _ = control.recvfrom(4096)
                        if reply[1] == 0xd6:
                            assert reply[4:] == packet, "Retransmit must reuse identical authenticated bytes"
                            inc("resends")
                            break
        except OSError:
            pass
        except Exception as error:
            inc("invalid_audio")
            with LOCK:
                REPORT["errors"].append(type(error).__name__)
        finally:
            if samples:
                rms = math.sqrt(sum((v / 32768) ** 2 for v in samples) / len(samples))
                mono = samples[::2]
                frequency = sum(a <= 0 < b for a, b in zip(mono, mono[1:])) * 44100 / len(mono)
                with LOCK:
                    REPORT["rms"] = rms
                    REPORT["frequency_hz"] = frequency
                    REPORT["frames"] = len(mono)
                save()

class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

if __name__ == "__main__":
    if "--health" in __import__("sys").argv:
        raise SystemExit(0 if (ROOT / "ready").exists() else 1)
    ROOT.mkdir(parents=True, exist_ok=True)
    with Server(("127.0.0.1", int(os.environ["HAP_PORT"])), Handler) as server:
        (ROOT / "ready").write_text("ready")
        save()
        server.serve_forever()
