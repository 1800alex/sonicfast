# Phase 2: streamed single-track playback through the native sink

Date: 2026-10-21. Starting commits, clean before this phase:
Sonicfast `cbe8147fb0caf231bd970c64b63bf40fc7327016`,
Libresonic `2b71d8ca0f9c2fd47d86802381622d6ce520f187`.
No commit or push commands were issued by this implementation run. During
validation, HEADs advanced to WIP checkpoints `642c30e` / `e4ecc65`; those
commits are preserved. [ADR 003](adr-003-streamed-playback.md)
records the implemented boundary. Phase 3 desktop integration has not started.

## Implementation

Libresonic adds `libresonic-playback` and reexports it as `playback`. It owns
single-track load/play/pause/stop/seek/volume intent and state, a dedicated
feeder thread, Symphonia MP3/FLAC decoding, cancellation and generation checks.
The app supplies an `AudioOutput`; the library has no audio-device, GUI,
Spotify or librespot dependency. Core exposes `StreamProvider`/`AudioStream`;
the protocol adapter owns authenticated requests and never exports their URLs.

The HTTP producer feeds a bounded channel. The feeder owns the decoder and
output, so HTTP, decoding, output enumeration and device opening stay off the
calling/UI thread. Commands update shared intent without waiting for those
operations. Latest-state watch updates replace older snapshots. Superseded
loads/seeks and stop interrupt both pending HTTP and output backpressure;
`shutdown().await` also joins the worker. Dropping a player signals cancellation.

| Bound | Implementation |
|---|---|
| Encoded reads | At most 16 KiB per public chunk; at most 256 KiB retained HTTP chunk |
| Encoded queue | 16 chunks; queue/current reader/producer accounting budget 288 KiB |
| Decoder input/lookahead | Symphonia's 64 KiB MediaSourceStream buffer; metadata/visual budgets 1 MiB each |
| PCM | Finite mono/stereo, 8..192 kHz; decoder capacity capped at 65,536 frames; writes at most 1,024 frames |
| Native output | Retained 200 ms queue threshold plus one converted block; device buffering and sinc/limiter history are additional bounded storage |
| Network waits | 5s connect, 20s headers/individual reads by default; consumer backpressure does not spend a read deadline |
| Output waits | Retained queue waits/recovery; cancellation checked between waits; EOF drain has a 5s active-time limit |

Follow-up: the retained HTTP chunk cap above was raised to 512 KiB after a
large-stream/backpressure regression exposed valid HTTP/1 reads above 256 KiB.
See the [Phase 3 follow-up](phase-3.md#follow-up-source-builds-and-playback-stopping-2026-10-21).

The encoded peak counter excludes HTTP, decoder metadata/lookahead, decoded PCM
and app/output conversion buffers; it is not a total process-memory metric.
Audio has no whole-song deadline. Metadata/artwork retain the previous whole-
request deadlines and body caps. Authenticated redirects/retries remain disabled.
JSON errors returned by `stream.view` use the same bounded, sanitized mapping as
metadata. No raw request error, response body, URL or credential reaches state.

Seeking reopens the raw stream and decodes/discards to the requested sample.
Tests compare the decoded PCM with the original at exactly 500 ms for both
codecs. Cost grows with target position and network speed; there is no HTTP
Range/timeOffset optimization or server-side seek precision claim. Position is
an estimate of submitted source time minus native backlog, with decoder-reported
duration when present. It is not a DAC clock. Pause intent survives load/seek;
EOF cannot publish Ended while paused. Errors allow a new load/play to retry.

## Native adapter and regression coverage

`src/sink/pcm.rs::PcmOutput` accepts source PCM, duplicates mono and converts
source rates with the retained stateful sinc resampler to stereo 44.1 kHz.
`vis::PcmProcessor` holds the shared EQ -> pre-volume tap -> limiter processing.
The existing `Tapped` librespot wrapper delegates to it. `RodioSink::write_pcm`
is shared by both paths, retaining fastframe device lifecycle/recovery, queue
accounting, transitions, output resampling and output gain ramps. The new path
uses neutral gain closures and transport commands and constructs no session.

The harness's explicit absent device returns typed `OutputError::NoDevice`.
The legacy saved-device fallback remains in the shared path. Native pause fades
and retains queued samples; stop fades/discards and resets processing. Volume
uses the inherited cubic curve with exact zero mute; the EQ/visualizer tap stays
before volume. Finish flushes bounded sinc/limiter history before output drain.

New signal tests verify 48 kHz mono -> 44.1 kHz stereo duration, pitch, level and
channel equality across blocks, EQ before the muted tap, and processing reset.
Existing sink/device/fade, EQ, limiter, resampler and visualization tests continue
to run. Decoder tests include both codecs/durations, sample-accurate replay seek,
pause/volume/paused seek/resume, stale loads, stalled opens/reads, full-channel
cancellation, bounded backpressure, malformed/truncated audio, recoverable auth/
output failures, EOF pause and rejected invalid/nonfinite PCM.

## Reproduce the native proof

Requires Linux PulseAudio utilities (`pulseaudio`, `pactl`, `parec`), Python 3,
and the existing native build prerequisites. This container has no `/dev/snd`;
physical listening, unplugging hardware and host-default changes were not tested.
The runner opens its own PulseAudio server and null sink, records its monitor,
and terminates both capture and server. It never changes host defaults. Captures,
JSON command timelines and reports remain ignored under `.cache/audio/native-proof`.

```sh
# From sonicfast; use a separate writable Cargo home in each checkout.
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" up
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" smoke
CARGO_HOME="$PWD/.cache/cargo" cargo +stable build --locked --example subsonic_playback
# Disposable synthetic fixture credentials only:
LIBRESONIC_URL=http://127.0.0.1:4535 LIBRESONIC_USERNAME=admin LIBRESONIC_PASSWORD=admin \
  python3 scripts/test-subsonic-playback.py
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" down
```

For a real server/device, use the example directly with the connection environment
from Libresonic's `examples/library.rs`. API-key auth is also supported. Select
`--track <opaque-id>` or `--format mp3|flac`; enumeration is `--list-devices` and
selection is `--device <id-or-name>`. HTTP requires `--allow-http`. Without
`--scripted`, stdin accepts `play`, `pause`, `stop`, `seek <milliseconds>`,
`volume <0..65535>`, `status`, `quit`. It does not persist credentials. Safe JSON
instrumentation includes position, duration, generation, first-submission delay,
encoded byte/peak counts and tap RMS. Diagnostics enable only native sink/audio
log targets; network debug logging remains disabled.

The automated proof uses Navidrome 0.64.0 (1072e9f7), the pinned Phase 1 image,
and the copied 264-track generated library. It enumerated and explicitly selected
`libresonic_proof` (Null Output) through cpal's PulseAudio backend, with the retained
fastframe/rodio device configured at stereo 44.1 kHz/882-frame periods. Its monitor
was captured as stereo 48 kHz float32. For each 3s MP3 and FLAC, it checked pause,
resume, mute/unmute, seek to 1.5s, stop, restart and natural Ended at 3s. A separate
absent-device attempt returned NoDevice, then the valid output succeeded.

| Captured result | MP3 | FLAC |
|---|---:|---:|
| First completed PCM submission | 10 ms | 9 ms |
| Seek's first completed PCM submission | 11 ms | 8 ms |
| Peak encoded queue/current/in-flight | 29,823 B | 57,382 B |
| Resumed output RMS | 0.01038 | 0.01038 |
| Output RMS after seek/unmute | 0.06248 | 0.06250 |
| Paused / muted / stopped output RMS | 0 / 0 / 0 | 0 / 0 / 0 |

These timings are observations on short local fixtures, not latency guarantees.
Silence checks use settled 200–280 ms windows; capture timings have PulseAudio/
pipe scheduling delay. Tap RMS remained above 0.001 at mute. The native path could not obtain system-bus priority. An initial trial
reported a startup underrun; the final settled capture checks passed. Full recordings were not listened to or certified free of
all audible artifacts. An initial harness trial incorrectly counted asynchronous
sleep/printing in command latency; the final check measures the stop call itself.
Another trial hardcoded ALSA's `pulse` device; the final runner enumerates the
actual host/device ID before selection. Final native proof passes both codecs.

## Validation and remaining work

Installed Rust/Cargo 1.99.1 via `+stable` was used with per-checkout Cargo homes.
The declared/pinned 1.98 toolchain was not changed or verified. Symphonia 0.5
(MP3/FLAC) was already an app dependency; Tokio and bytes were already transitive.
The library adds their direct playback/transport uses. Sonicfast's external
lockfile packages/revisions remain unchanged; only local package entries change.
Libresonic's lock adds decoder dependencies. No device dependency enters the library.
Command logs use `.cache/phase-2-*` in Libresonic and
`.cache/validation/phase-2-*` in Sonicfast.

| Check | Actual result |
|---|---|
| Libresonic workspace tests | PASS: 34 tests (3 core, 22 HTTP protocol, 9 playback); live browse ignored in ordinary runs |
| Libresonic fmt/clippy/doctests/docs | PASS; warnings denied |
| Live Navidrome Python fixture + Rust browsing | PASS; explicit fixture URL required |
| Native MP3/FLAC capture and missing-device proof | PASS, Linux PulseAudio null sink |
| Sonicfast default/all-feature clippy | PASS; warnings denied |
| Sonicfast default/all-feature tests | PASS: 1,364 / 1,405 passed; 3 ignored in each run |
| Sonicfast fmt/doctests/docs | PASS; rustdoc warnings denied |
| Dependency/evidence policy, 16 policy tests, 4 launcher tests | PASS |
| Physical devices, Windows/macOS, Rust 1.98, Nix, Jekyll | NOT RUN |

No GUI login/backend integration, protected-store changes, queue, gapless,
transcoding, playlists/favorites, scrobbling or remote output is included. Long
track performance, live API-key servers, real device hotplug, a second distinct
server and network recovery during playback need later validation. Pause/volume
intent is consumed at feeder checkpoints; while blocked waiting for input, device
control waits for data/cancellation/read timeout, although the bounded PCM queue
drains. Stop interrupts that read. This is a playback proof, not completed daily-
use desktop migration. No UI layout changed, so there are no UI review captures.

```text
Phase: 2 / streamed MP3/FLAC single-track playback
Changes: bounded stream/provider APIs, standalone playback crate/controller,
         shared PCM sink and DSP, independent native harness and capture runner.
Manual evidence: Linux x86_64; real Navidrome streams through fastframe/rodio;
                 PulseAudio monitor captures prove audio/silence transport states.
Decision log: adr-003-streamed-playback.md.
Exit gate: PASS for the Linux native virtual-device proof. Physical hardware and
           a second OS remain unverified; no cross-platform completion claim.
Next smallest task: Phase 3 server profile/protected login and backend adapter,
                    then existing library/now-playing UI and local queue wiring.
```
