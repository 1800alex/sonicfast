# Filesystem-derived download availability

Date: 2026-10-22. Implements [ADR 009](adr-009-filesystem-availability.md).
Baseline Sonicfast `83a29cd4735fc4fd49bbcef7ff12b3d127d6d52f`; separate Libresonic
`edf8088cf50268e9829ff9959dbcf29f5d089a2d` is unchanged by this follow-up.

The owner wanted to avoid a stale JSON catalog. Sonicfast now creates no download
index. The filesystem supplies local availability, with bounded exact-path checks
for visible tracks approximately every five seconds plus I/O time. The original
local file is checked again before playback. No full library scan is performed.

The existing playback resume JSON gains unfinished download queue state, so a
paused queue, ordering and retries still survive restart. Completed-file records
are excluded. Old `.sonicfast/index.json` files are optional read-only hints and
are never rewritten; corrupt or stale files cannot decide an icon is downloaded. Legacy checksums
and unverified filename claims are discarded before file discovery.
Existing music is kept. Natural paths and deterministic legacy/collision names
work without the old index. Arbitrary renamed/moved files outside the candidate
paths and exceptional old random-suffix names require a matching path or a legacy
hint; there is no hidden full-directory scan.

Regressions cover externally copying a file into place, deleting, restoring,
moving between candidate paths, size changes, checksum changes, restart without
an index, read-only legacy migration, ignored corrupt legacy JSON, deterministic
collision rediscovery, and unfinished queue serialization excluding availability.
The cancellation, retry, containment, original-file transport and local-stream
fallback regressions remain in place.

The actual App/Navidrome flow now opens the album, removes its downloaded original
outside the worker and waits for Remote, restores it and waits for Complete. It
plays that original with the streaming endpoint deliberately blocked, queues a
second song while paused, restarts the App, verifies that unfinished queue and
pause state, rediscovers the original and plays it locally again. Assertions
confirm no index.json is created. The other four actual App flows and native
credential-store test remain in the same runner. No personal server writes are
part of this fixture verification. The separate owner-authorized custom server test
uses only one original download in an isolated temporary cache under `.cache/`.

The native UI walkthrough uses a fresh per-run download folder, allowing it to
show active progress even when earlier generated originals still exist elsewhere.
Captures remain outside tracked files in `.cache/downloads-review/native/`.
Logs use `.cache/validation/filesystem-downloads-*`; fixture logs use
the Libresonic checkout’s `.cache/filesystem-downloads-fixture.log`. The disposable Navidrome
stack/volume is removed after verification. There are no new dependencies or
lockfile changes. Work remains local and uncommitted.

Validation uses Linux x86_64 and Rust/Cargo 1.99.1 (`+stable`), with full default
and all-feature checks using four test threads. Platform/MSRV claims remain
limited: Rust 1.98, other OSes, physical devices and release packaging are not
validated. Filename/size matching is heuristic without a server content hash;
checksums are now a session cache, and unknown-size external files are not adopted.

| Check | Actual result |
|---|---|
| App default / all-feature suites | PASS: 1,404 / 1,447 top-level tests, plus isolated child test in each run; 10 ordinary-run ignores; four test threads |
| Filesystem regressions | PASS: new copy/delete/restore/move/restart checks, read-only corrupt/stale legacy recovery, deterministic collision rediscovery, unfinished snapshots and immediate cancel/close checkpoint |
| Native Navidrome App | PASS: all five flows and credential-store round trip; the expanded flow verifies icon changes, no index creation, paused queue restart and local playback with streaming blocked |
| Owner-authorized live custom server | PASS: one original M4A download, fresh manager lookup from files without an index, byte-size checks and isolated-file cleanup; no scrobble/playlist/feedback writes |
| Native UI | PASS: 19 captures with fresh per-run download folders; queue-panel capture reviewed visually, library arrow retained and compact/queue arrows hidden |
| App checks/build | PASS: default/all-feature warning-denied Clippy, fmt, doctests, warning-denied docs, default sonicfast and both native examples |
| Policy/launchers | PASS: source/review checks, 16 policy tests and four launcher tests |
| Libresonic/custom server | Unchanged by this follow-up; no protocol or backend edits |

Implementation checks also caught a Rust borrow conflict in the download-page
refresh scheduling; visible probe requests are now collected during layout and
emitted afterwards. Legacy migration explicitly discards stale checksum evidence
and unverified filename claims; the regression covers both. The final suites pass
with these corrections. The disposable Navidrome server and its volume were
removed after verification.
