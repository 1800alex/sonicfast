# Snapcast v0.15.0 implementation and evidence

Date: 2026-10-23. Baseline: `4f1493e`. Decision: [ADR 011](adr-011-snapcast.md).
The owner's explicit request authorizes sending and built-in receiving for
v0.15.0; general remote queue control and other casting remain deferred.

## Implemented boundary

- `src/snapcast/output.rs`: Libresonic `AudioOutput` implementation sends raw
  44100:16:2 PCM to Snapserver's TCP source port. The app chooses this output
  when Send is enabled; otherwise the existing native PCM output is retained.
- `src/sink/pcm.rs`: network helpers reuse source normalization/resampling, EQ,
  visualization tap and limiting without opening a local device. The source
  applies current player volume, paces an anchored sample clock, bounds writes,
  checks cancellation and completes the last server chunk at natural end.
- `src/snapcast/protocol.rs`: bounded v2 framing, length/timestamp validation and
  NTP-style offset estimates using low-delay samples. No authenticated URLs or
  music-server secrets enter this protocol.
- `src/snapcast/native.rs`: built-in PCM/FLAC receiver using existing Symphonia
  and fastframe dependencies. Timestamped buffering is bounded to 256 chunks/
  two million samples; messages are capped at 1 MiB. The native callback never
  blocks waiting for the network, supplies silence on underruns, compensates
  reported device latency and corrects small clock drift by interpolation.
  Larger discontinuities discard stale audio. Source/device rate differences
  also use linear interpolation in this receiver path.
- `src/snapcast/receiver.rs`: owns receiver lifecycle and background source DNS.
  Configuration changes cancel the previous receiver; network/device failures
  report sanitized status and retry with bounded backoff. App shutdown cancels
  pending connections and receiver work. No companion process is launched.
- App/worker/settings adapter: independent Send/Receive toggles, explicit trusted
  network consent, separate source/receiver ports, output-device picker, latency
  correction, persistent generated receiver ID and live status. Existing
  Appearance cards/rows are reused. The sign-in screen can open these controls.
  Applying routing aborts stale pending playback loads, preserves the queue,
  position and current volume, and restores the local track paused.

The receiver runs independently of OpenSubsonic login and server switching.
Receive-only mode rejects local song playback; both toggles allow the sender to
listen to its own synchronized stream. Snapserver controls receiver groups,
stream selection, mute and volume. The app's song transport controls its source.
Received audio is passed through without applying source EQ twice, generating
song metadata or submitting local scrobbles.

No new Rust dependency or Libresonic release is required. The lockfile change is
only Sonicfast's version. The published Libresonic 0.1.0 boundary stays unchanged.
The owner's concurrent smaller dev/test build-profile settings are preserved;
the duplicate profile table was merged into a valid manifest.

## Validation

All Cargo checks used Linux x86-64, Rust/Cargo 1.99.1 through `+stable`, with a
checkout-local Cargo cache. The pinned 1.98 toolchain was not exercised.

| Check | Result |
| --- | --- |
| `make verify CARGO='cargo +stable'` | PASS: format, source/evidence policy, 48 Python regressions, default/all-feature Clippy with warnings denied, tests, doctests and warnings-denied Rustdoc |
| Default Rust tests | PASS: 1,428 passed, 11 opt-in ignores |
| All-feature Rust tests | PASS: 1,473 passed, 11 opt-in ignores |
| `make snapcast-test CARGO='cargo +stable'` | PASS for PCM and FLAC against official, SHA-256-pinned Snapcast 0.35.0 Docker packages |
| Default-feature app and probe/UI examples | Built, including MilkDrop |
| Native UI review | Six captures: signed-out controls, settings, receiver filtering, dark/light and narrow layout; saved outside git |
| Release validation / workflow lint | `v0.15.0` metadata agrees; new workflow passes actionlint |
| Documentation | Production Jekyll build and generated-link/base-path checks passed |
| Separate manual environment | Smoke project starts, serves Snapweb and accepts the original tone source; ports/state separate from tests |

The Docker suite tests the actual sender plus two built-in virtual-output
receivers, the production native audio backend, two official file-output clients
and an official PulseAudio client. Its dedicated Docker audio server has separate
null sinks for native/reference output, with independent monitor timestamps on
one clock. The onset tolerance is 40 ms, including 10 ms capture blocks; this is
not a sub-millisecond hardware-synchronization assertion.

It verifies stereo frequency/level/finite samples, pacing, source cancellation,
pause/resume, source mute, server-side receiver mute, stream switching to 48 kHz/
24-bit FLAC and mono PCM, source failure on server shutdown, receiver reconnect
and a fresh source restoring audio. A synthetic Subsonic service also drives the
real `App` through login, a route apply/volume update, FLAC decode/resampling and
network playback; an independent receiver checks the 660 Hz signal. That service
is a test double, not another real-world backend compatibility claim.

Evidence stays under ignored `.cache/snapcast/`: `verify-final.log`,
`make-integration-final.log`, dated `integration-*` audio/timing captures and JSON
reports, `ui-review/`, documentation and smoke logs. Test projects always clean
up their own containers/volumes and retain failure evidence. The manual project
uses distinct ports/volume; its reference clients discard output rather than
accumulating indefinite recordings.

## Failures resolved during implementation

- The companion-client approach was replaced after the owner explicitly required
  a built-in receiver. No companion dependency remains in product code.
- Ephemeral Docker port mappings changed after restart. Tests reserve fresh
  loopback ports per run and retain them through the outage/recovery scenario.
- Snapserver can send JSON-RPC notifications before a response. Fixture control
  readers now match response IDs and skip notifications.
- The headless real-App probe initially failed egui's texture-delta accounting;
  it now explicitly clears unrendered texture deltas, like existing app tests.
- A temporary Compose edit produced an invalid nested service mapping; the
  corrected base and smoke configurations were validated before the final run.
- Current source volume could revert to startup volume after route apply. The
  worker retains live volume in engine configuration, and the App probe checks
  the value after applying routing.

The final review also added bounded fallback across resolved source addresses,
so an unreachable first DNS result does not mask another usable IPv4/IPv6 address.
A loopback regression verifies fallback without opening a local device.

## Remaining limits

Only trusted, unencrypted Snapcast TCP is implemented. Authenticated/TLS
Snapcast, Opus/Vorbis reception, remote queue control and automatic discovery are
not included. The sender requires its documented 44100:16:2 server input format.
Server buffering remains audible after source transport changes; the client
cannot retract audio already sent to other receivers.

Physical speakers, Bluetooth latency, network asymmetry/long-duration drift,
Windows/macOS audio, Rust 1.98 and installed release packages were not exercised.
Existing opt-in Navidrome/credential-store tests were not rerun for this change;
the new Docker suite needs no private server or native credential store. Nix
release packaging remains opt-in pending the previously documented vendor-hash
refresh. Software null/file outputs do not establish physical multiroom accuracy.

## Snapcast output switch and help (2026-10-24, v0.15.1)

The owner found the Snapcast option confusing: the output picker listed Snapcast
whenever the trusted-network switch was on, even with Send and Receive off, and
the Send switch both "enabled" Snapcast and routed music to it. The Send switch is
replaced by **Snapcast output** (`snapcast::Settings::offer`), which only lists
Snapcast in the picker and the Casting Output menu. Routing stays in `send`, set by
choosing Snapcast there. Turning the switch off while sending routes back to Local.
`can_send` requires `offer || send` (configs saved while sending keep it), a
trusted network and a valid server. A **How Snapcast works** dialog explains Send,
Receive and both together. Evidence: unit tests for `can_send` and a Settings
render test for the help button; the receive/send transport is unchanged.
