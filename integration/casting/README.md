# Casting audio proof

Run `make casting-test` with Docker Engine, Compose v2, Python 3.11+ and the
normal native Rust build prerequisites. Both `docker compose` and standalone
`docker-compose` work. `CARGO='cargo +stable'` can override the toolchain.

The fixture builds checksum-verified Shairport Sync 5.6.1 against complete FFmpeg
libraries, so ALAC and PCM are available. Sonicfast's actual Rust sender sends a
synthetic 660 Hz tone over RTSP/RTP. The receiver plays through a private
PulseAudio Unix socket into a Docker null sink; a separate monitor capture
checks finite samples, sustained signal, frequency and useful RMS. The source
also checks finish/drain and cancellation. No host sound socket/device or music
account is mounted. Host networking is used for the transient receiver's network
protocol, so this Linux test is not a cross-platform or hardware-sync claim.

The suite also signs the actual Sonicfast app into a synthetic Subsonic fixture,
loads its FLAC track and tests both output paths. AirPlay reaches Shairport's
decoder; Cast reaches a certificate-pinned CastV2 receiver, which fetches the
app's protected HTTP original and decodes it with mpv. Both play through the
isolated null sink. Captures must contain useful, finite audio, and app controls
must pause, seek, resume and reach natural track completion. Cast must also
restart a finished song when Play is pressed again. A wrong receiver
certificate pin must fail before playback. The receiver implementation is also
used by the garage deployment; the public fixture needs no private checkout.

The app proof also changes outputs during metadata loading, during playback and
while paused. It requires a continuing song/position without another Play action,
and requires paused music to stay paused. Cast first attempts a wrong certificate
pin, then corrects it through the normal debounced settings action; the app must
automatically apply the correction, reconnect and resume. Per-process output is
saved even when a probe fails.

Each run owns a separate Compose project and volume, cleans up in `finally`,
and stores captures/reports/logs under ignored `.cache/casting/integration-*`.
It does not modify the manual Snapcast environment or the garage deployment.

Rust tests exercise Cast device authentication, generic receiver launch,
correlated media controls, playback-state mapping and protected HTTP serving:
HEAD/ranges/CORS, wrong peer, forged/revoked URLs and invalid range requests.
AirPlay regression tests emulate a receiver that withholds SETUP/RECORD until
clock replies arrive, including a timing request during a response body. They
also check cancellation, a missing RTSP reply and the first sync/audio markers.
Shairport alone does not reproduce this hardware handshake ordering.
Genuine Chromecast hardware and Google SDK sender interoperability require a
separate device test. The garage's certificate-pinned software receiver is an
owner-operated Cast-compatible target, not a Google-authenticated Chromecast.

The suite also starts independent synthetic HAP receivers implemented with
Python integers/hashlib and distribution `cryptography`. A leading-zero PIN
completes SRP/Ed25519 pairing, and two native audio sessions verify the saved
receiver identity without pairing again. Captured ChaCha20-Poly1305 PCM must
contain the expected 660 Hz tone. The fixture checks encrypted event replies,
NTP, feedback, exact encrypted retransmissions and teardown. Incorrect PINs,
forged server SRP proofs, forged setup signatures and forged session signatures
must fail before any audio is sent. These targets do not implement AirPlay
grouping and are not substitutes for physical compatibility testing.

`airplay_pair_probe discover` lists the actual discovery results without
connecting. `airplay_pair_probe HOST PORT --silence` explicitly requests an
on-screen code, reads it from stdin, and sends ten seconds of silence to that
receiver. Pairing keys live only in this probe's memory and are never printed or
saved. Omitting `--silence` sends a quiet 660 Hz test tone. `--reconnect` closes
and reopens the audio session halfway through, reusing its verified pairing.

For an explicitly selected receiver, `casting_probe PORT HOST --silence` sends
ten seconds of silent audio through the actual sender. This checks handshake,
keepalive, sending and teardown without an audible tone. It does not establish
speaker output or decoder correctness; the Docker tone/capture test does that
for Shairport. Omitting `--silence` sends the synthetic tone.
