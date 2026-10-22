# ADR 003: single-track streamed playback and retained PCM output

Date: 2026-10-21. Status: accepted; Phase 2 Linux virtual-device proof passed.
Baselines: Sonicfast cbe8147, Libresonic 2b71d8c (full SHAs in phase report).

- Add libresonic-playback, independent of GUI/librespot. Core exposes a small
  authenticated byte-stream/provider contract; protocol owns all request URLs.
- Preserve source sample rate and mono/stereo channels through Symphonia MP3/
  FLAC decoding. Bound encoded chunks/queue, decoder frames and metadata.
  Sonicfast converts to its existing stereo 44.1 kHz processing format using
  the retained stateful resampler before shared EQ/tap/limiter processing.
- Extract common PCM writes from RodioSink and common DSP from vis::Tapped.
  Existing librespot wrappers delegate to those helpers. The new output adapter
  uses neutral commands/gain/PCM without constructing a librespot session.
- A single feeder thread owns decoder/output. A bounded async HTTP producer
  supplies bytes; no network/decoder/device work runs on a GUI thread. Shared
  intent/generation tokens cancel superseded loads and seeks. State uses watch
  snapshots; no unbounded event queue or detached retry work.
- Seek cancels/reopens the original authenticated stream and decodes/discards
  to the requested sample. This is linear work proportional to target time,
  not HTTP Range/timeOffset seeking. Precision and cancellation are tested;
  startup/seek latency is reported honestly. Range optimization is Phase 4.
- Audio requests have connect/header/read deadlines, rather than the metadata
  client's whole-body deadline, so songs longer than 20 seconds are valid.
  Backpressure waits do not consume network-read deadlines. Redirects/retries
  remain disabled, and all errors omit URLs/server bodies.
- Only single-track transport is included. Queue, gapless, transcoding,
  scrobbling, desktop login and remote playback remain later phases.
- Validate real MP3/FLAC from the generated Navidrome library against the native
  fastframe/rodio path. This container exposes no physical audio endpoint, so
  use an isolated PulseAudio null sink/monitor for captured PCM evidence and
  report physical listening as untested. Never change the user's host defaults.
- A strict explicit device choice in the debug harness must fail recoverably
  if absent. Existing saved-device fallback behavior stays unchanged.

Evidence and limits: [Phase 2 report](phase-2.md). Physical hardware and other
operating systems remain unverified. This records the Phase 2 boundary. Later
desktop integration is in ADR 004, and M4A/container byte seeks are in
[ADR 005](adr-005-m4a-playback.md).

## Search row correction (2026-10-22)

The [search follow-up](search-and-branding.md) maps the selected URI into the
actual playback list before sending its offset. Search rows may hold a one-song
list despite having a larger screen index; this fixes the misleading unsupported
codec error without changing decoders.
