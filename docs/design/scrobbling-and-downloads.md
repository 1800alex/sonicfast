# Scrobbling and downloads

Later follow-up: [filesystem availability](filesystem-availability.md) removes the
download catalog and supersedes the index behavior recorded in this report.

Date: 2026-10-22. Implements [ADR 008](adr-008-scrobbling-and-downloads.md).
Baselines: Sonicfast `0fc4eacd91eebf878d2bf58d1d53878a5d4b644a`, Libresonic
`7677c4d0d967f984088d0be2b71248b8dcd5dc48`.

In Settings, enable Scrobbling and choose the percentage and optional seconds
threshold. The first threshold reached by actual advancing playback records a
listen. Pauses and seek jumps do not count. Reporting defaults off.

Enable Downloads, enter an absolute folder path and choose Apply folder. Download
from a song/album/playlist menu or selected songs' menu, or click a track's arrow.
Settings → View downloads opens the persistent queue with byte progress, pause/
resume, cancel, retry and move-first controls. An active track has a ring around
its arrow; an available local track uses the accent color. Clicking those arrows
opens the queue. Downloads preserve originals and safe server-relative paths; without those
hints, they use artist/album folders. Natural filenames get suffixes only for
collisions. Compact/queue rows hide the download arrow. They survive sign-out and restart. Disabling stops downloads
and local preference without deleting completed files.

Playback checks the index/exact filenames in the background and uses local audio
before the server stream. Existing files can be adopted when their original size
matches; no library scan is needed. Original server filename/path and size are
optional Subsonic metadata; there is no standard server content hash. See the ADR
for matching confidence, file integrity, queue limits and filesystem requirements.

Initial verification uses Linux x86_64, Rust/Cargo 1.99.1 (`+stable`) and the pinned,
disposable Navidrome 0.64.0 fixture with generated audio. That initial proof uses
no personal music/server; the naming correction below separately records the
owner-authorized live custom server check. The native runner has a loopback proxy that blocks only streaming for
the local-playback test and throttles original downloads for active UI captures.
The proxy forwards both GET and advertised form POST; an initial test failure
revealed its missing POST forwarding, which was corrected before rerunning.

Protocol tests cover original bytes with no transcode parameters, bounded chunks,
filename/size/play-count mapping, redirects/non-audio/partial responses, scoped IDs,
now-playing vs submission timestamps, advertised POST and no mutation retry.
App regressions cover persistence/lock, pause/deduplication, cancel during stalled
reads, cleanup, backoff, length mismatch, missing-file stream fallback, seekable
local input, exact-name adoption, filename collisions, changed-file hashing,
changed-file preservation on retry, corrupt index preservation and symlink/path
rejection. Receipt tests cover restart deduplication and ambiguous failures.
Neutral listening tests cover thresholds, pause, seeks, disable, unknown durations
and new occurrences.

The opt-in actual App flow checks disabled reporting, enabling at a configured
threshold, Navidrome's persisted play count, no duplicate from seek/replay,
completed-file restart and sign-out preservation. Streaming is blocked throughout
that flow, so successful playback exercises the local provider. The other four
App flows and protected native credential-store test remain in the same runner.

| Check | Actual result |
|---|---|
| Libresonic workspace tests | PASS: 62 tests; 1 opt-in browse test ignored |
| App default / all-feature tests | PASS: 1,399 / 1,442 top-level tests, plus the isolated child test in each run; 9 ordinary-run ignores |
| Both repositories | PASS: formatting, warning-denied Clippy, doctests and warning-denied documentation; app Clippy/tests include all features |
| Policy/launchers | PASS: source/review checks, 16 policy tests and 4 launcher tests |
| Native App | PASS: all 5 actual App flows and protected credential-store round trip |
| Native UI | PASS: 18 captures; settings, active arrow/ring, queue/progress and completed arrow reviewed visually |
| Native build | PASS: default-feature sonicfast and both native examples |

A repeat of the full parallel suite exceeded the new filesystem tests' original
3–5 second waits under host load, although focused runs passed. Their test-only
wait budget now exceeds the protocol deadline; final full runs limit test
concurrency to four. Production transport timeouts are unchanged.

Visual review also caught small originals rounding to “0.0 MiB”; byte progress
now selects B/KiB/MiB/GiB. Queue rendering borrows records and virtualizes rows.
No directory scan or file hashing runs during UI drawing.

Logs use ignored `.cache/scrobble-downloads-*` in Libresonic and
`.cache/validation/scrobble-downloads-*` in Sonicfast. Captures use
`.cache/downloads-review/native/`, outside tracked files. Reproduce with the
existing native build prerequisites and:

Set `LIBRESONIC_CHECKOUT` to the absolute path of a checkout of
[Libresonic](https://github.com/1800alex/libresonic) for fixture commands.

```sh
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" up
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" smoke
CARGO_HOME="$PWD/.cache/cargo" cargo +stable build --locked --bin sonicfast --example subsonic_ui --example subsonic_playback
LIBRESONIC_TEST_URL=http://127.0.0.1:4535 CARGO_HOME="$PWD/.cache/cargo" python3 scripts/test-subsonic-app.py --captures
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" down
```

The folder index and receipts are local data, not encrypted credentials. Other
servers/OSes, physical audio hardware, Rust 1.98, power-loss durability and Nix/
release packaging remain unvalidated. Download transfers restart partials from
zero; there is no offline catalog/login, automatic disk quota/eviction, tag edit
or download-format conversion. Scrobbles are at-most-once attempts and may be
lost on ambiguous failure. No dependencies/lockfiles are changed; work remains
local and uncommitted.

## Download naming correction (2026-10-22)

The owner reported Windows-style colon replacement and unconditional ID suffixes
against their live custom server backend. Read-only source/metadata checks confirmed that
custom server supplies a virtual relative artist/album/title path, including this album's
colon, but no original disk filename or track number. The client now preserves
that relative layout and legal Linux punctuation, using suffixes only for actual
collisions. The root is exactly the configured folder: choose `/path/to/Music`
to produce `/path/to/Music/<artist>/<album>/<title>.m4a`. The owner's local
Music/configuration directories are not mounted in this workspace, so their root
setting and existing files were not edited. Existing indexed/legacy downloads
remain usable; this is a correction for new downloads, not an automatic rename.

Regression coverage adds server-relative nested paths/colon handling, traversal/
absolute/drive/UNC/internal-path rejection, natural names, atomic concurrent name
reservation, no overwrite and legacy hashed-file discovery without redownload.
A separate ignored test accepts URL/key/track ID only through the environment,
downloads one original into an isolated cache, checks size/indexed path/checksum,
and verifies local lookup after manager restart. It removes its isolated music
file afterwards and performs no scrobbles, playlist or feedback writes. Secrets
are not saved in source, logs or download indexes. Production custom server source stays
unchanged; this is narrow live API-key/download evidence, not full server support.

Queue widgets now suppress arrows for compact rows and queue contexts/pages,
including Now playing, Next up and recent rows. The native walkthrough adds a
queue-panel capture with downloads enabled. An initial concurrent-name test
incorrectly assumed which asynchronous transfer would win the natural name;
it now accepts either winner while requiring distinct names and no overwrite.
Final results are recorded below.


| Naming/queue follow-up check | Actual result |
|---|---|
| Focused download regressions | PASS: 10 tests; live test opt-in |
| Owner-authorized live custom server | PASS: API-key metadata plus one original M4A download at the exact relative colon-containing path; size/checksum and restored local lookup verified; isolated file removed |
| App default / all features | PASS: 1,402 / 1,445 top-level tests plus isolated child test; 10 ordinary-run ignores; concurrency limited to four |
| Libresonic | PASS: 62 workspace tests, formatting, warning-denied Clippy, doctests and warning-denied docs; only public path-field documentation changes |
| Native Navidrome | PASS: five actual App flows and native credential round trip; 19 captures, including queue panel with downloads enabled |
| Native visual review | PASS: library arrows remain; Now playing, Playing next and Next up durations have no overlapping download arrow |
| App checks/build | PASS: default/all-feature warning-denied Clippy, fmt, doctests, warning-denied docs, policy/launcher checks, default sonicfast and both native examples |

Logs use `.cache/validation/download-paths-*` in Sonicfast and
`.cache/download-paths-*` in Libresonic. Review captures stay ignored under
`.cache/downloads-review/native/`. Linux/Rust 1.99.1 only; other platforms and
Rust 1.98 remain unvalidated. The disposable Navidrome stack/volume is removed
after validation. Changes remain local and uncommitted.

The later [original disk-path correction](original-file-paths.md) changes custom server's
virtual path mapping to the library-relative physical filename and extends
Sonicfast's compatibility lookup. The naming-correction evidence above describes
the server before that change; deployment is required to advertise the new path.
