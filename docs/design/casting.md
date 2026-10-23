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

## Hardware handshake correction (2026-10-23)

A Pioneer VSX-1123 advertised password-free, unencrypted ALAC over UDP on port
1024. Its ANNOUNCE and SETUP succeeded, but RECORD waited for the initial clock
exchange. The sender only serviced UDP after RECORD completed, so the stereo
woke and selected AirPlay while Sonicfast timed out without sending audio.
An independent handshake reproduced the timeout; answering the three timing
requests let RECORD return 200 immediately. No authentication or codec change
was needed.

The sender now services timing/control while reading RTSP headers and bodies.
Ten-millisecond read polls retain cancellation, a two-second absolute request
deadline and a separate 50-millisecond teardown deadline. The first sync packet
also has its required initial extension marker. Diagnostics log only our method,
typed error and numeric RTSP status, never arbitrary receiver headers or bodies.

Three regression tests cover clock-gated SETUP/RECORD/body completion and initial
sync/audio markers, a missing reply, and cancellation before RECORD completes.
`make verify` passed (1393 default-feature and 1433 all-feature library tests;
existing opt-in tests stayed ignored). `make casting-test` passed again with
decoded Shairport audio and the actual app's AirPlay/Cast transport controls.
The production documentation build/link check and default native app build also
passed, using the previously documented Rust stable override.

The patched native sender separately completed ten seconds of **silent** RTP
streaming to the Pioneer, including keepalive, drain, cancellation and teardown.
This confirms hardware handshake/transport progress, not audible speaker output.
The user's normal music playback still needs a listening retry. This extends the
earlier software-only evidence without adding Google Cast hardware or other OS
claims. Diagnostic logs remain in ignored `.cache/airplay-receiver/`.

## Automatic settings and playback handoff (2026-10-23)

The owner reported an extra pause/play step after changing outputs, hidden
receive-only conflicts and settings that silently needed Apply. The original
restore-paused policy and independent egui Snapcast draft caused those UX gaps.

Casting and Snapcast now share application-scoped pending edits. Switches and
selections apply immediately; text/number edits apply after 500 ms of inactivity.
Incomplete fingerprints/endpoints stay pending with a visible warning and retain
the applied configuration. Filtering, navigation and picker changes cannot leave
an old draft that later re-enables sending. Reconnect remains an explicit retry,
not a prerequisite for applying settings. Notices wrap and appear near the
receive/output controls so narrow layouts do not hide the conflict explanation.

The backend retires the old output, restores its heard position and resumes only
when the user's play intent is active. Paused songs stay paused. An in-flight
metadata load is restarted on the new route instead of discarded. Output errors
retain play intent, allowing a corrected Cast fingerprint to reconnect and play
automatically. Explicit pause, natural completion and receive-only takeover clear
that intent. Local song loads are guarded at the backend as well as the UI.

Receive-only Local mode has a persistent warning. Explicitly choosing Local
disables Receive and shows a notice. AirPlay/Cast can still coexist with the
independent receiver, with a notice explaining the additional local audio.
This preserves synchronized Send+Receive use without an unexplained NoDevice.

Regression coverage includes incomplete/valid fingerprint edits, debounce across
navigation, newer picker choices superseding drafts, immediate switches and
receive-only warnings. The actual Docker app proofs exercise loading/playing/
paused output changes for AirPlay, Cast and Snapcast. The Cast proof additionally
starts with a wrong pin and corrects it through the automatic settings path,
without a Play action, before testing transport controls and replay. Synthetic
audio/certificates are used; this follow-up sends no audio to physical receivers.
Native dark/light and narrow-window settings captures stay in ignored cache.

Validation passed on Linux: `make verify` (1396 default-feature and 1436
all-feature library tests, with existing opt-in tests ignored),
`make casting-test`, PCM/FLAC `make snapcast-test`, the default native app build,
and the production documentation build/link check. The installed stable Rust
override was used. The first extended Cast harness relied on a per-player
generation counter across player replacement; its check now observes resumed
playback progress, which also works after fingerprint-error recovery.
