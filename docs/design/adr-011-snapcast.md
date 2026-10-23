# ADR 011: Snapcast audio transport

Date: 2026-10-23. The owner requested v0.15.0, sending audio to Snapserver,
receiving Snapcast audio, Docker integration tests and a separate manual environment.
This authorizes the Snapcast part of the original Phase 7 deferral.

## Boundaries

Sonicfast remains the OpenSubsonic player and queue owner. A selectable output
feeds processed, normalized 44100 Hz stereo signed 16-bit little-endian PCM to
Snapserver's TCP stream source. This is a source port (default 4953), distinct
from the receiver port (1704) and JSON-RPC port (1705). There is no custom
coordinator, proprietary control plane or library upload.

Sending replaces direct local output. To hear the same stream in sync on this
machine, also enable the receiver. The sender retains EQ, limiting and the
visualizer tap, applies the player volume to the distributed source, and uses
real-time pacing rather than allowing TCP buffering to consume a whole song.
Network writes are bounded, cancellable and performed on the existing feeder
thread; failed audio is not replayed after a connection failure.

The receiver is built into Sonicfast, as explicitly requested by the owner. It
implements the published Snapcast v2 TCP protocol, PCM and FLAC decoding,
low-delay clock-offset sampling, bounded timestamped playout and drift correction.
It uses the existing fastframe native output backend and its reported device
latency. There is no companion executable and no linked/copied GPL receiver code.
The `snapcast-client` Rust crate was evaluated but is GPL-3.0, so it is not added
to this MIT project. Symphonia already supplies the FLAC decoder. The receiver's
callback interpolates timestamps across source/device rate differences and small
clock drift, emits silence on underruns, and discards stale audio on resync.

Settings are disabled by default and require explicit consent to unencrypted
transport on a trusted network. Snapcast endpoints contain no credentials.
Settings changes apply automatically (500 ms idle for typed values). Receive-only
Local mode displays a warning and pauses the queue. Choosing Local disables
Receive; other output changes retain the song, position and play/pause intent.
Receive playback is independent of OpenSubsonic authentication and does not
report received songs as locally listened/scrobbled tracks. Sender transport
controls affect the source; receiver group/stream assignment and volume are
managed by Snapserver. Remote audio may continue for the server's configured
buffer duration after pause/seek/stop; this is not immediate room control.

## Proof requirements

Pin upstream packages and verify their hashes. Test the actual Rust sender and
built-in receiver against a Docker Snapserver and independent official
clients, including independent native/reference output on a Docker audio server.
Assert audio format/signal, pacing, two-client alignment, lifecycle,
pause/resume, cancellation, server outage and recovery. Use synthetic tones only.
An isolated smoke project must remain available for interactive use, without
modifying the user's server, audio daemon or normal Sonicfast profile.

Docker file output can prove transport and the software playout schedule; it
does not establish synchronization between physical speakers or platforms that
were not exercised. Record executed evidence and limitations in the phase report.

Sources: [Snapcast v0.35.0 configuration](https://github.com/snapcast/snapcast/blob/v0.35.0/doc/configuration.md),
[binary protocol](https://github.com/snapcast/snapcast/blob/v0.35.0/doc/binary_protocol.md),
[official receiver](https://github.com/snapcast/snapcast/tree/v0.35.0/client).
