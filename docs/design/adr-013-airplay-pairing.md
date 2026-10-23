# ADR 013: native AirPlay PIN pairing and Roku discovery

Date: 2026-10-23. Accepted at the owner's request for Roku receivers that require
an on-screen AirPlay code. This supersedes ADR 012's pairing exclusion for
HomeKit-style pairing and a single encrypted audio receiver only.

Discovery browses `_raop._tcp`, `_airplay._tcp` and `_googlecast._tcp`. It remains
bounded and passive: it does not connect, start pairing, trust advertised public
keys or send audio. Dual AirPlay/RAOP announcements merge, preferring a usable
Classic route. Audio capability, password and access-control fields distinguish
PIN receivers from unsupported video-only/Home-only/password targets.

Selecting a PIN receiver first checks session/native protected credentials for
that host/port. Otherwise it requests `/pair-pin-start` and performs SRP-3072 /
SHA-512 Pair-Setup with the four-digit code. The code stays in transient memory;
the UI masks it, actions redact it, and settings/state do not serialize it.
Both the server SRP proof and final accessory Ed25519 signature must verify.
Only then is the receiver identity/key saved alongside the client signing seed.
Native store calls use a separate scoped worker and namespace from server
logins. An unavailable store permits session-only pairing with an explicit notice.

Every audio connection performs fresh X25519 Pair-Verify and validates the saved
receiver ID and Ed25519 signature. HKDF-SHA-512 derives independent control/event
keys. HAP ChaCha20-Poly1305 records have authenticated two-byte lengths, bounded
1024-byte payloads and separate monotonic nonces in each direction. Partial
records survive timed socket reads; malformed records and replay fail closed.
An authenticated binary-plist SETUP negotiates NTP, an event connection, and a
44.1 kHz stereo signed-16-bit PCM stream. A fresh random audio key protects RTP
payloads, authenticating the timestamp/SSRC and using monotonic packet nonces.
Retransmissions reuse identical authenticated bytes. The existing EQ, tap,
clock/retransmit handling, cancellation, pacing and playback handoff remain.

Network/crypto/disk/native-store work stays off the UI thread. Pairing has a
two-minute challenge lifetime, bounded requests and cancellation. Attempts are
generation-scoped: late replies, closing the dialog or selecting another output
cannot apply an old receiver. A successful current attempt selects that output
through the existing route action and preserves play/pause intent. The existing
Classic path remains separate; authentication failures never downgrade to it.

Registry dependencies: `num-bigint` and `subtle` were already transitive and now
have direct requirements for SRP arithmetic and constant-time proof comparison.
`plist` 1.10.3 adds maintained binary-plist parsing/writing (and its base64/XML
dependencies). Existing `ring` supplies X25519, Ed25519, HKDF and AEAD. No receiver
code or companion process is linked into the application. Nix's previously
unvalidated vendor hash still needs refresh before Nix publication.

Passwords, legacy AirPlay PIN authentication, FairPlay/RSA, transient anonymous
pairing, Apple Home membership policies, AirPlay groups and PTP/multiroom remain
outside this extension. Do not claim compatibility with all AirPlay 2 devices.

Evidence: [AirPlay PIN validation](airplay-pairing.md).
Protocol references: [HomeKit pairing/framing](https://openairplay.github.io/airplay-spec/pairing/hkp.html),
[pyatv's HAP implementation](https://github.com/postlund/pyatv/blob/master/pyatv/protocols/airplay/auth/hap.py),
[encrypted audio setup](https://github.com/postlund/pyatv/blob/master/pyatv/protocols/raop/protocols/airplayv2.py),
[SRP group parameters](https://www.rfc-editor.org/rfc/rfc5054#appendix-A).
