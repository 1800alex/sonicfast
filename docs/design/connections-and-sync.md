# Connection list, Sync and README capture

Date: 2026-10-22. Implementation decisions are in
[ADR 010](adr-010-server-connections.md).

This follow-up adds saved server management and separates Disconnect from Sign
out/Remove, adds a metadata-only Sync now button, hides By You, and publishes an
explicitly requested product screenshot in the README. Other native review
captures stay ignored. Libresonic remains the published 0.1.0 dependency; no
library source, dependency or lockfile change is required.

The disposable Navidrome 0.64.0 fixture uses newly generated original tones,
artwork and metadata (264 tracks/24 albums). Native audio uses a private null
output and the native secret store runs in a private desktop service session.
Multiple saved connections are exercised as separate logical profiles against
that fixture. This does not claim two independent server implementations or
physical audio-device coverage.

## Regression coverage

- Stale startup profile events must not overwrite a typed connection draft.
- Adding/editing a connection leaves playback running until Connect; connection
  startup clears old views, navigation, track caches and selection.
- Sync preserves playback position, queued tracks and pending feedback, and does
  not navigate to an old search. Native testing creates a playlist externally
  and confirms Sync finds it while the current track continues playing.
- Stored By You preferences cannot hide server playlists.
- Cancelled native credential edits restore the previous stored value.
- The native multi-profile flow switches accounts, restores each queue paused,
  removes an inactive connection during playback, disconnects without revocation,
  restarts signed out, reconnects using the retained credential and removes the
  active connection.

## Failures found during implementation

An initial native run caught a new startup race: the incoming empty profile list
cleared a connection draft before Connect. Three existing flows failed with empty
credentials. Preserve the draft unless explicitly selecting/removing a connection;
the new startup regression covers this case. Logs are retained as
`.cache/connections-native-initial.log`.

Native review found three authentication switches whose accessibility labels were
not drawn as visible text. The shared form now renders those labels alongside
the switches; dark and narrow/light captures verify the form layout.

Several intermediate overlapping runs hit timeouts in local HTTP/scrobble and
download waits, including two native browse/download waits. The focused scrobble
rerun passed. The final contributor checks, build and native walkthrough ran in
sequence and passed without changing those timeout limits. Failed logs remain in
`.cache/connections-verify*.log` and `.cache/connections-native-final.log`; final
serial logs are identified below.

## Final validation

Linux with Rust/Cargo 1.99.1 (`+stable`), overriding the committed 1.98.0 pin:

| Check | Result |
| --- | --- |
| Full `make verify CARGO='cargo +stable'` | Passed |
| Formatting, dependency/review policy and 45 Python regressions | Passed |
| Clippy, default/all features, all targets, warnings denied | Passed |
| Default tests | 1,413 passed, 11 opt-in ignores |
| All-feature tests | 1,458 passed, 11 opt-in ignores |
| Doctests and warning-denied Rustdoc | Passed |
| Sonicfast binary and native examples | Built with default features including MilkDrop |
| Native protected-store round trip | Passed |
| Native application integration | All six flows passed, including the new connection/Sync flow |
| Native UI walkthrough | All 36 captures completed |
| Visual review | Connection list, labeled form, narrow/light form, Server settings and README capture reviewed |
| Production Jekyll site and generated links | Passed |

Final logs: `.cache/connections-verify-serial.log`,
`.cache/connections-build-serial.log`, `.cache/connections-native-serial.log` and
`.cache/connections-docs-final.log`. Review captures are ignored under
`.cache/connections-review/native/`. The explicitly requested product capture is
`docs/assets/images/demo.png` (1200×850), referenced by README.md. It uses synthetic
music metadata and original fixture artwork, with no personal music or credentials.

No macOS/Windows/ARM64 or physical audio validation is claimed. No commit, push,
release or deployment was performed. The disposable fixture and private desktop
services were stopped after validation.
