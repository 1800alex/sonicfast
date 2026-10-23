# AirPlay PIN validation

Date: 2026-10-23. See [ADR 013](adr-013-airplay-pairing.md) for scope and security
decisions. The owner reported invisible Roku receivers that require a PIN.

## Diagnosis and physical receiver

Sonicfast originally browsed only `_raop._tcp`. The selected receiver advertises
only `_airplay._tcp`, on port 7000, and rejects unauthenticated playback requests.
It reports a TCL/Roku G137X model, RokuOS firmware 15.34.2402 and AirTunes
377.40.00, with audio and authenticated-transport capability flags. Its local
address and friendly name are intentionally omitted from tracked evidence.

The owner selected the test receiver and supplied its displayed code. The
native `airplay_pair_probe` completed SRP Pair-Setup, verified the saved receiver
identity, negotiated encrypted control/events/PCM, sent ten seconds of silence,
drained and closed successfully. Credentials remained only in that process's
memory. A later native discovery scan listed the receiver alongside the existing
Classic stereo, garage AirPlay and Cast outputs.

This confirms physical pairing and transport progress, not audible sound or
speaker synchronization. Normal music playback, native credential persistence
on the owner's desktop and other physical receiver models need separate checks.
The first request while the television was waking timed out; the pairing-start
deadline now permits ten seconds while retaining short cancellation polls.

## Independent Docker proof

`make casting-test CARGO='cargo +stable'` passed with a Python HAP receiver using
independent arbitrary-precision arithmetic and `cryptography` implementations.
The synthetic PIN starts with zero. One pairing established two verified audio
sessions across pause/reconnect, without a second PIN. The receiver checked:

- Both controller signatures, encrypted control records and two NTP exchanges.
- Two encrypted event acknowledgements, feedback and clean TEARDOWNs.
- 1,254 authenticated audio packets, two byte-identical retransmissions, and no
  invalid audio packets or fixture errors.
- Decoded PCM at about 659 Hz for the 660 Hz source, RMS 0.0353.
- An incorrect PIN rejected; forged SRP, accessory setup and receiver session
  signatures each rejected before sending audio.

The retained Shairport Sync 5.6.1 proof also decoded the native Classic ALAC tone
at 660 Hz (RMS 0.0906 on the final run). Actual-app Classic/Cast probes passed their existing
load/output-change, pause/seek/resume, certificate-error recovery and replay
checks. Containers own their audio sockets, reports and transient state and are
removed after the run; no host speakers, private music server or real credentials
are mounted. Reports/logs stay in ignored `.cache/casting/`.

## Regression and build checks

Rust tests cover the observed advertisement, duplicate services, unsupported
password/Home/video-only targets, HAP record fragmentation/replay, proof and
signature rejection, cancellation, endpoint-scoped remembered pairing and UI
attempt generations/cancellation. A synthetic SRP vector was generated with
independent `srptools` 1.0.1 and is checked against the native SRP implementation.

`make verify` passed: formatting, source/evidence/packaging policies, Clippy with
warnings denied, 1,409 default-feature and 1,449 all-feature library tests, binary
and integration tests, doctests and Rust docs. The existing opt-in tests stayed
ignored. The default native `target/debug/sonicfast` build passed. Dark/light PIN
dialogs, the retry/error view and the Roku picker entry were captured under Xvfb
and visually inspected; images remain outside tracked files. The production
Jekyll build and generated site link/base-path checks passed. Rust commands use
the installed stable 1.99.1
override from Phase 0; the pinned MSRV and other OSes are not verified here.
The new plist dependency changes Cargo.lock. Nix is unavailable in this
environment, so the existing unvalidated vendor hash remains a prerequisite for
Nix publication, as documented in PACKAGING.md; it is not replaced by a guessed
value. Native credential-store calls retain the existing bounded worker, but
an unlocked desktop round trip was not performed in this test environment.
