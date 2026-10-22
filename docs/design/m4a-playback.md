# M4A follow-up validation

Date: 2026-10-21. Implements [ADR 005](adr-005-m4a-playback.md) in the independent
Libresonic checkout and native Sonicfast app. The existing generated Navidrome
library already contains 66 AAC/M4A songs; it was not necessary to replace or
transcode user music or change the fixture's 264-track/24-album layout.

The new decoder fixtures are synthetic two-second stereo tones. Front-metadata
and padded tail-metadata variants exercise AAC-LC and ALAC independently. Tail
fixtures place `moov` more than 64 KiB beyond the encoded audio, forcing real
source seeks rather than relying on Symphonia's rewind buffer. Tests compare
decoded samples after a 500 ms replay seek with the original decode. AAC frame
padding is allowed in duration checks; no container edit-list/gapless precision
claim is made. Fast-start M4A without ranges works; tail metadata without ranges
returns a recoverable unsupported-media error. Tiny reads, cancellation during
range requests and safe service-error propagation have regressions.

HTTP regressions cover authenticated absolute ranges and EOF/rewind, ignoring
Range (HTTP 200), wrong start/total, truncated range lengths and a streaming-only
server. No authenticated URLs or arbitrary server text are included in errors.

The actual App fixture flow now selects an M4A song, verifies it is enabled,
plays, pauses, seeks while paused and resumes before checking FLAC and MP3.
Track identity is checked when waiting for playback so an earlier track's
Playing snapshot cannot satisfy the next selection's assertion.

| Executed check | Result |
|---|---|
| Library workspace tests | PASS: 46 ordinary tests; one opt-in browse test ignored |
| App default / all-feature tests | PASS: 1,383 / 1,426; 6 ignored in each ordinary run |
| Both checkouts fmt, Clippy, doctests, rustdoc | PASS; warnings denied; app Clippy covers default and all features |
| Dependency/evidence policy, policy/launcher tests | PASS: 16 policy and 4 launcher tests |
| Updated Navidrome smoke | PASS: MP3, FLAC and M4A raw streams |
| Actual App + protected store | PASS: M4A play/pause/paused seek/resume, then FLAC/MP3, queue and remembered paused restoration/sign-out |
| Native M4A scripted proof | PASS: missing-device recovery, play/pause/resume/mute/seek/stop/end; retained pre-volume tap stays active at mute |
| Source binary/examples | PASS: rebuilt sonicfast and native proof examples |

The native proof uses the isolated PulseAudio null sink, not physical hardware.
Its AAC-LC song reported 3,023 ms duration and reached the same end position.
The paused position stayed fixed; seek resumed at 1,500 ms; the muted pre-volume
tap RMS was approximately 0.062; the observed encoded input peak was 16 KiB.
The complete proof log is `.cache/validation/m4a-native-proof.log`. These are
one fixture run's measurements, not general performance guarantees.

Validation uses Linux x86_64 and Rust/Cargo 1.99.1 (`+stable`), with each
checkout's own Cargo home and target. Physical listening, other OSes, Rust 1.98,
HE-AAC/DRM and Nix packaging are not validated. ALAC has decoder/output-contract
coverage; the real Navidrome/native-output case uses AAC-LC. Logs are under
Libresonic `.cache/m4a-*` and Sonicfast `.cache/validation/m4a-*`.

Use the existing isolated fixture runner from the Phase 3 report, after building
`subsonic_playback` and `subsonic_ui`; add `--m4a` to also run the native
pause/resume/mute/seek/stop/end and pre-volume tap proof against an AAC/M4A song.
The CLI playback example also accepts
`--format m4a`. A server must advertise and honor raw HTTP byte ranges for files
with tail metadata. No whole-song download fallback is used. Time seeks retain
linear decode/discard cost.
