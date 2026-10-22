# Search, player actions, playing indicator and Sonicfast identity

Date: 2026-10-22. Owner-requested follow-up covering the six reported issues.
The default executable and crate/library are now `sonicfast`. The inherited
Spotify/demo executable is `sonicfast-legacy`; it remains a comparison tool.
No Libresonic or custom server source changes are required.

## Search playback

Search rows each carry a one-song URI list, but the OpenSubsonic action adapter
sent the index of that row in the full results. The worker then looked beyond
the one-element list and reported an unsupported codec. Album contexts had valid
indices, explaining why the same song worked there. The adapter now maps the
chosen URI into the list it actually sends, preserving positional-only requests
and a matching duplicate occurrence, and rejecting an absent song instead of
playing something else.
Codec support/decoders have not changed.

A focused regression checks a search row at index 17 resolving to index zero,
filtered positions, duplicates and missing selections. The native Navidrome flow
now searches a broad query, plays a supported result beyond the first row using
its one-song context, and checks that exact URI in real playback. A read-only
custom server probe confirmed matching M4A/audio-mp4 hints between search and getSong for
the previously reported album; no personal-server writes or downloads were made.

## Search and player controls

Top search now requests server playlists alongside the song/album/artist catalog
and filters titles without case sensitivity. Playlist and catalog errors retain
separate completion results under the existing query serial/generation guards.
Mock coverage includes catalog failure with successful playlist matches; native
coverage checks the Playlists filter and the generated mixtape. Podcasts and
Episodes are omitted from Sonicfast's search filters, and its empty-search copy
refers to the server library. Legacy Spotify search remains available separately.

The bottom player bar has a visible ellipsis button using the exact same song
menu as track rows, including ratings and Add to playlist/duplicate confirmation.
The existing right-click menu, heart control and actions are retained. Layout
reserves space for both heart and menu even with long titles. Sonicfast menus now
use the same bounded width as the inherited menu widgets.

A visible playing row renders five smooth, staggered vertical bars. A 16 ms timed
repaint animates them; clipped rows schedule no animation. Paused/stopped rows
return to their ordinary number/cover and hovering retains the play/pause target.
No audio-output, decoder, DSP or visualization-tap changes are involved.

## Branding and compatibility

Application strings, crate imports, translations/catalog name, desktop/package
identities, icons/launcher filenames, scripts and workflow artifact names use
Sonicfast. README is rewritten around OpenSubsonic and explicitly credits the
Spotifast fork and upstream MIT notices. The built-in Winamp archive is regenerated
from its Rust sprite generator with SONICFAST in the title bitmaps, not only the
window caption/marquee. Skin filenames and fallback name are updated together.

Existing Sonicfast configuration/state/cache locations and the protected server
credential namespace remain unchanged. The legacy comparison command has separate
folders, IPC and MPRIS names. Production command detection matches Sonicfast's
basename case-insensitively (including packaged capitalization), rather than
mistaking the renamed library's hashed test executable for the live client.
Legacy stored-grant identifiers remain for compatibility. No Spotify grants are
copied into Sonicfast server profiles. The Linux desktop launcher no longer registers
Sonicfast as a handler for Spotify links. macOS/Flatpak application IDs and Windows
installer GUIDs are distinct from upstream; full installer/platform validation
and the inherited Windows link registrations remain part of the release audit.

Real upstream URLs, pinned dependency branch names, attribution, original release
notes and historical design evidence retain their original names. No invented
release hosts or public fork URLs are introduced. Packaging/publication, signing
and release feeds still need the separate release audit; Sonicfast's updater
remains disabled. This work does not publish or install packages, replace old
marketing screenshots/videos or claim cross-platform release validation.

Validation uses Linux and Cargo/Rust 1.99.1 (`+stable`). Logs and native captures
are kept in `.cache/validation/search-branding-*` and
`.cache/search-branding-review/native/`. The crate rename initially left obsolete
root-crate build artifacts; only those old Spotifast root artifacts were removed,
retaining all dependency caches. No `cargo clean` was used.

Final results:

| Check | Result |
|---|---|
| Default all-target tests | 1,409 passed, 10 opt-in tests ignored |
| All-feature all-target tests | 1,452 passed, 10 opt-in tests ignored |
| Default and all-feature Clippy | Passed with warnings denied |
| Formatting, all-feature doctests and documentation | Passed; no doctest examples; documentation warnings denied |
| Dependency/review policy and policy unit tests | Passed, including 16 policy tests |
| Launcher, release-name and Flatpak metainfo checks | 4 + 8 + 4 tests passed |
| Native private credential store and five Navidrome App flows | 6 checks passed |
| Native UI walkthrough | 31 captures; reviewed player menu/rating submenu, playlist filter, two animation frames and light/narrow player layouts |
| Winamp asset generator/preview | Regenerated the 17-file archive and visually verified SONICFAST title pixels |

An early native search probe used the stop word `a`; Navidrome returned no
songs. The proof now derives a real title token and requires a supported result
beyond the first row. Native feedback coverage also caught the existing
positional-only empty-URI action, which the index helper now preserves and tests.
One playlist-removal run timed out; the two subsequent full native runs passed
without changing playlist mutation behavior. The final logs retain the successful
run. Initial path/MPRIS test expectations were corrected to the separate legacy
identity. These failures are not counted as successful validation runs.

Windows MSI/uninstall tests skipped 19 cases on Linux. Other OSes, real installers,
release publication, Nix vendor hashing and physical audio devices remain
unvalidated. The disposable server/volume and private native services were stopped
after review. Neither separate repository has source modifications for this work.

## Saved-heart follow-up

The owner requested the existing saved-heart playlist menu in place of the new
player ellipsis. The ellipsis and its reserved space are removed. Unsaved songs
show the existing CirclePlus icon; the first click still emits ToggleSaved.
Saved songs retain the filled accent heart and the existing playlist picker,
including filtering, New playlist, duplicate confirmation and Remove from Liked
Songs. Ratings remain in track menus, including the player title/cover context
menu; they are not added to the heart's playlist picker.

The native walkthrough now clicks the bottom player's outlined plus, checks
the song becomes saved without opening a popup, then clicks the filled heart
and checks that the playlist picker opens. Captures are kept separately in
`.cache/saved-heart-review/native/`, with logs in
`.cache/validation/saved-heart-*`.

Validation on Linux with Rust/Cargo 1.99.1 (`+stable`): the Sonicfast binary and
native examples build; the fixture smoke and native secret-store/five App flows
pass. All 31 UI captures completed. The unsaved, saved and playlist-picker shots
show the actual two clicks; wide and narrow/light player layouts were reviewed.
The initial native run hit the previously recorded intermittent duplicate-playlist
removal timeout; the complete rerun passed without playlist changes. Both logs
are retained (`saved-heart-native-initial.log` and `saved-heart-native.log`).
The disposable fixture container/volume and private native services were stopped.
`make verify CARGO='cargo +stable'` passed: formatting, dependency/review policy,
16 policy tests, 4 launcher tests, default/all-feature warning-denied Clippy,
1,409 default and 1,452 all-feature tests, doctests and warning-denied documentation.
The ordinary suites retain their 10 opt-in ignores; the native checks above are
separate. No new server/OS or physical audio coverage is claimed.
