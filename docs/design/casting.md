# AirPlay and Cast implementation evidence

The owner authorized both senders and the shared output picker, then requested
a software receiver for their Docker audio stack. [ADR 012](adr-012-casting.md)
records the scope, protocol boundaries and explicit identity-pin exception.

On Linux, the native default-feature build retains MilkDrop, PipeWire and the
existing device stack. No Cargo dependency or lockfile changes were necessary.
Local and AirPlay/Snapcast playback keep Libresonic's decoder and the existing
PCM normalization, EQ, limiter and visualization tap. Chromecast uses the
receiver's decoder and the downloads-first StreamProvider for originals.

`make casting-test` builds checksum-verified Shairport Sync 5.6.1 with full
FFmpeg decoding and a separate software Cast target. A private PulseAudio Unix
socket and null sink record decoded output without access to host speakers.
The test exercises:

- The native ALAC/RTP sender with a ten-second 660 Hz tone, timing replies,
  finite samples, sustained signal, frequency/RMS checks and cancellation.
- Actual app sign-in against a synthetic Subsonic fixture, FLAC decoding and
  AirPlay output, including pause, seek, resume and natural completion.
- Actual app sign-in and Cast original serving, followed by mpv decoding and
  captured audio, including pause, seek, resume, natural completion and replay
  after the song has finished.
- Rejection of the same software target with the wrong TLS certificate pin.

The completed Docker run on 2026-10-23 captured a 660 Hz native AirPlay signal
with RMS approximately 0.092, app-to-AirPlay audio with RMS approximately 0.024,
and app-to-Cast audio with RMS approximately 0.019. The app-to-Cast replay test
also passed. Reports and raw captures are kept outside
git under `.cache/casting/integration-*`; the reproducible fixture is in
`integration/casting/`. The receiver's lack of Google device credentials is
intentional and never treated as successful Google authentication.

Rust regression tests retain Cast nonce/certificate validation and check generic
media launch, request correlation and controls without Spotify messages. HTTP
tests cover GET/HEAD, CORS, valid and invalid ranges, wrong peer IP, and forged
or revoked leases. Receiver status tests distinguish playing, paused, natural
end, errors and takeover. Existing `make snapcast-test` passed again with both
PCM and FLAC, reference receivers, decoded app output, mute/volume, format
changes and outage recovery.

Native captures reviewed the output picker and settings in dark/light themes,
filtered settings and a 680-pixel window. Review found and corrected an unlabeled
LAN switch and an inconsistent certificate input. Captures stay in ignored cache.

Validation uses installed Rust stable 1.99.1 because the repository's pinned
1.98.0 toolchain was unavailable. Default/all-feature Clippy, tests, Rustdoc, policy checks, workflow lint and
the production documentation build/link check passed alongside the Docker suite.
The library suites passed 1390 tests with default features and 1430 with all
features, with their existing opt-in tests ignored. The live original-download,
Navidrome/credential-store and platform display opt-in tests were not rerun.
The garage Debian image was also built and separately exercised through actual
app playback and captured mpv output, covering its older mpv command interface. This evidence covers Linux software playout, not physical speaker
timing, real Google Cast hardware, Android SDK senders, Windows or macOS.

During fixture development, an ALAC-only Shairport image could not decode this
sender's uncompressed ALAC frames; building against the complete FFmpeg library
fixed the decoder boundary. Actual app tests also exposed mpv's version-dependent
`loadfile` arguments and an idle-property error; the software receiver handles
both older distribution mpv and current versions. These fixes were verified with
decoded output rather than connection status alone.
