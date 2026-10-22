# Server playlist integration

Date: 2026-10-22. Implements [ADR 006](adr-006-server-playlists.md). This is the
playlist step of Phase 4; favorites/scrobbling and the remaining parity work
remain separate.

The sidebar now lists readable server playlists and exposes the existing create
button. Open a playlist to browse/play it. Song menus offer Add to playlist with
the retained picker and duplicate confirmation. Editable rows support removal,
insertion and reordering; drag/drop uses the existing table actions. Owned
playlist menus open the retained details and permanent-delete dialogs. Details
support rename, comment (including clearing it) and visibility. Read-only/shared
ownership is distinguished. Queue saving uses the inherited playlist flow and
its existing duplicate-elimination policy.

Creation sends initial tracks in one create request. Visibility/comment are
updated separately; a failure in that second step reports a warning and keeps
the already-created playlist without retrying or adding the songs twice.

The server stores playlist definitions. Local playback takes a snapshot of
contents and uses the existing queue/transport. Editing a playlist does not
change the queue already playing. Playback contexts and individual write batches
retain the 2,000-song bound. Reading complete lists/details is subject to the
existing 4 MiB JSON response cap. Metadata HTTP errors identify the endpoint and
limit. There is no whole-song download or new audio device implementation.

Mutations are serialized and never automatically retried. Rows update
optimistically; success/failure triggers authoritative reconciliation. Selected
duplicate occurrences are removed by position and verified identity. A stale
fingerprint fails safely and reloads the current server contents. This is a
best-effort stale-selection check, not an atomic server transaction. An external
writer can still race between a check and a write. Replacement refuses to drop
unavailable entries and is capped rather than silently truncating a large list.

Protocol tests cover sparse metadata, repeated/null entry slots, ownership versus
editability, advertised API-key identity, form-POST parameter repetitions and
legacy GET fallback, exact removal indices, scope/kind validation, request/body
limits, permissions, redirects and no automatic write retries. Existing audio,
decoder, queue and redaction tests remain part of the checks.

The opt-in actual App/Navidrome flow creates a synthetic playlist, adds duplicate
songs, plays the second occurrence, advances, removes only one duplicate,
reorders, inserts with duplicate confirmation, renames, changes visibility,
clears the comment, rolls back a foreign-server addition, rejects a stale removal
after another client appends, and deletes its test playlist. Only this disposable
server and generated music are used.

Early live runs exposed two metadata problems: Navidrome omits false `public`
values, and inherited missing-field merging could reintroduce the previous
public flag after changing visibility. Fresh Subsonic responses now bypass that
merge, with an application regression. Optional server `readonly` evidence is
kept distinct from ownership. No personal server or music data was modified.

Native captures use the actual egui App/backend with Xvfb, private PulseAudio and
a private unlocked keyring. Create, playlist page, edit and permanent-delete
dialogs are captured in ignored `.cache/playlists-review/native/`, alongside the
retained library/player/settings views. The earlier native baseline remains in
`.cache/phase-3-review/`. Playlist and edit-dialog captures were visually reviewed;
the retained layout and controls remain intact. No review media is tracked.

Logs: Libresonic `.cache/playlists-*`; Sonicfast
`.cache/validation/playlists-*`. Baselines: Sonicfast
`9b088bdf0e6754439246e690937c079ff2d31690`, Libresonic
`dd517c977d34f749b429e2c3134a5960d088d7d1`; the existing M4A follow-up remains
in the working trees. Validation uses Linux x86_64 and Rust/Cargo
1.99.1 (`+stable`) with separate per-checkout caches/targets. Actual API-key
server editing, other servers/OSes, physical listening/hotplug, Rust 1.98 and Nix
packaging are not validated. API-key ownership can remain unknown when identity
discovery is absent/unavailable. Playlist cover uploads and song-link imports
are unavailable; server-generated covers are displayed normally.

| Check | Actual result |
|---|---|
| Libresonic workspace tests | PASS: 53 passed, including 7 playlist protocol tests; 1 opt-in browse test ignored |
| App default / all-feature tests | PASS: 1,385 / 1,428 top-level tests; 7 ignored in each ordinary run; each run also passes its isolated child-process test |
| Native isolated tests | PASS: credential-store round trip and all 3 actual App flows, including the full playlist lifecycle and conflict/rollback cases |
| Native captures | PASS: 12 actual App captures; playlist, edit and permanent-delete views reviewed |
| Both repositories | PASS: formatting, Clippy with warnings denied, doctests and warning-denied documentation; app checks include all features |
| Policy / packaging regression | PASS: dependency-source and review-evidence checks, 16 policy tests and 4 launcher tests |
| Native build | PASS: `sonicfast`, `subsonic_ui` and `subsonic_playback` with default native features |

The container's desktop portal logs unavailable FUSE/GTK services during native
tests. The isolated keyring/audio and actual egui flows complete successfully;
file-picker/portal integration is not validated by these playlist tests.

The playlist feature gate passes for Linux and this Navidrome fixture. Phase 4
as a whole remains incomplete. Next smallest feature: server favorites, followed
by a separately defined scrobbling policy.

Reproduce after building the two native examples:

Set `LIBRESONIC_CHECKOUT` to the absolute path of a checkout of
[Libresonic](https://github.com/1800alex/libresonic) for fixture commands.

```sh
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" up
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" smoke
LIBRESONIC_TEST_URL=http://127.0.0.1:4535 python3 scripts/test-subsonic-app.py --captures
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" down
```

The library and app checks in CONTRIBUTING/README must also pass independently.
