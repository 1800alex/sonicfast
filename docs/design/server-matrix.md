# Server and feature evidence

Date: 2026-10-22. The `sonicfast` executable uses Libresonic/OpenSubsonic.
The legacy executable remains available for comparison; this matrix records
actual server/native evidence, not cross-platform release support.

The [MPRIS artwork correction](mpris-artwork.md) verifies authenticated artwork
through the app's loader into readable file URIs on a private Linux D-Bus session,
including arrival, replacement and clearing. It adds no real-server or rendered
GNOME Shell coverage.

The [casting follow-up](adr-012-casting.md) adds native AirPlay Classic and Cast
outputs. A Linux Docker Shairport Sync 5.6.1 receiver independently decodes the
native Rust ALAC/RTP source into a private PulseAudio null sink. A certificate-pinned
software Cast target decodes the actual App's synthetic Subsonic FLAC through
the protected local media listener and mpv. Wrong-pin rejection and Cast
pause/seek/resume are exercised. This is not Google SDK or real Chromecast
hardware compatibility evidence. See [executed casting evidence](casting.md).
The subsequent [Pioneer handshake correction](casting.md#hardware-handshake-correction-2026-10-23)
adds a VSX-1123 RTSP/clock exchange and ten-second silent native sender test.
Audible output on that stereo still needs a listening retry.
The [automatic settings follow-up](casting.md#automatic-settings-and-playback-handoff-2026-10-23)
adds app-level loading/playing/paused output changes and automatic recovery after
correcting a Cast certificate pin, with receive-only warnings and draft regression
coverage. It adds no physical hardware or independent music-server coverage.

The [M4A follow-up](m4a-playback.md) adds AAC-LC/ALAC decoder coverage and an
actual App AAC/M4A playback path. Tail metadata uses advertised, validated raw
byte ranges; fast-start files can stream without them. This does not claim
HE-AAC, DRM or all MP4 codecs.

The [playlist follow-up](playlists.md) adds server playlist browsing/playback and
create, rename, comment, visibility, add/remove/reorder and delete. Actual App
tests on the pinned Navidrome verify duplicate occurrences, rejected-write
rollback and stale-selection reconciliation. Legacy GET and advertised form
POST/API-key identity paths have mock coverage.

| Server | Evidence | Libresonic/app status |
|---|---|---|
| Navidrome 0.64.0 (1072e9f7), API 1.16.1 | Python fixture checks plus Rust token login, discovery (7 extensions), 8 artists, artist albums, 24 albums over pages, album/song/search/artwork and rejected password; CLI browse demonstrated | Rust connection/browsing and streamed MP3/FLAC native harness implemented; Linux PulseAudio monitor capture validates pause/resume/mute/seek/stop/end and missing-device recovery; Phase 3 actual App login/browse/search/artwork, MP3/FLAC playback, local queue, native remembered login, paused restart and sign-out tests pass; native GUI captures reviewed |
| Additional compatible server | Historical API-key metadata and original-download checks confirmed optional original paths/sizes and local-file lookup | Narrow metadata/download evidence; not a second full browse/play/playlist/scrobble validation |
| Gonic/another distinct server | Required before release; not tested | Not implemented |
| Mock legacy Subsonic without extensions | HTTP 404/501, protocol 70 and explicit unsupported discovery; successful ping and later artists remain usable | Rust baseline fallback tested |
| Mock OpenSubsonic | Token/API-key signing, sparse metadata, errors, paging, redirects, endpoint-specific byte-limit diagnostics, 20,000-artist index, 8 MiB stream with consumer backpressure, stream chunk bounds, per-read deadlines and cancellation | Automated facade coverage; not a second real-server claim |

The [song feedback/lyrics follow-up](song-feedback-and-lyrics.md) records song
hearts, personal 1–5 ratings, Liked Songs and LRCLIB integration evidence.
Actual App tests persist hearts and every rating on Navidrome, independently
clear them, play the Liked Songs view, and verify duplicate Cancel/Add anyway.
LRCLIB HTTP behavior is mocked; native synced lyrics use generated cached text.
Synthetic public LRCLIB probes returned HTTP 503, so no real lyric match is
claimed. The native menu/heart/lyrics captures were reviewed.

The self-contained fixture pins `deluan/navidrome` digest
`sha256:a384948b81bd1529986c5960169e7fc4fa00f46bde6bd517971a4c36671db2af`
and generates its own synthetic tones and artwork. It uses its own
data volume, on loopback port 4535. See
[fixture instructions](https://github.com/1800alex/libresonic/blob/main/integration/navidrome/README.md).

Native playback evidence is a Linux virtual-device capture through the retained
fastframe/rodio sink, using real Navidrome MP3/FLAC streams; it is not physical
listening or hardware hotplug coverage. Replay seeking is tested against decoded
samples, with no Range/timeOffset optimization. Mock/fake-output tests cover
bounded backpressure, stalled reads, cancellation and recoverable decode/output
errors. Beyond the narrow custom server API-key metadata/download check, real API-key
server flows, playback network recovery, physical device
loss and other OSes remain unverified. Playlist writes
have no atomic cross-client conflict guarantee, and other real playlist servers
remain untested. See
[Phase 1](phase-1.md) and [Phase 2](phase-2.md) and [Phase 3](phase-3.md) for scope and evidence.

The [scrobbling/download follow-up](scrobbling-and-downloads.md) records original
file downloads, local-first playback and configurable listening reports. Mock
coverage includes transport/size/integrity failures and durable at-most-once
reporting; native Navidrome evidence and limits are in that report. Original
filename/path and size are optional; standard Child metadata has no content hash.

The owner's [download naming correction](scrobbling-and-downloads.md#download-naming-correction-2026-10-22)
prefers validated server-relative paths, keeps legal Linux punctuation and adds
identity suffixes only on collisions. The [original-path correction](original-file-paths.md)
records how virtual tag-based paths differ from original filenames and how
the client accepts corrected metadata. Sonicfast keeps prior title-based and
hashed downloads discoverable after the metadata correction.

The [filesystem availability follow-up](filesystem-availability.md) removes
persisted download availability. Exact paths are rechecked for visible tracks;
originals are validated again before local playback. Unfinished downloads survive
in existing app resume state. The Navidrome App proof includes externally deleting
and restoring a visible song and restarting a paused queue without an index.

The [settings/artist/menu visual follow-up](settings-visuals.md) aligns the new
settings with Appearance and removes Sonicfast's artist following control. Its
native Navidrome walkthrough reviews the rating submenu, artist page and settings
in wide, narrow and light-theme captures; this adds no server capability claims.

The [search/player/branding follow-up](search-and-branding.md) verifies a
non-first search result with its one-song playback context and top-search
playlist matches against the disposable Navidrome. Native captures review the
player-bar rating menu, animated playing bars, server-only search filters and
wide/narrow/light layouts. The executable/crate is Sonicfast; retained upstream
URLs and historical reports continue to refer to Spotifast.

The later [saved-heart follow-up](search-and-branding.md#saved-heart-follow-up)
removes the visible player ellipsis and reuses the existing heart playlist picker.
This changes the player affordance without adding server operations.

The [public source preparation](public-source-preparation.md) replaces the
external fixture-copy step with a self-contained generator and removes external
checkout assumptions. Fresh Navidrome smoke, Rust service and native App flows
exercise that generated library; existing history is preserved by request.

The [public documentation/workflow follow-up](public-docs-and-workflows.md)
consumes published Libresonic 0.1.0 and repeats Navidrome smoke, native credential
storage and the five native App flows. A 32-capture walkthrough additionally
reviews the library sort menu with an old Spotify sort preference. Packaging
and site checks in that report do not add server, physical audio or OS coverage.

The [connection-list follow-up](connections-and-sync.md) exercises two saved
logical profiles against the disposable Navidrome, isolated resume state,
credential-preserving disconnect/reconnect and metadata refresh during playback.
It adds no physical audio, independent-server or operating-system coverage.

## Snapcast audio transport (v0.15.0)

Snapcast 0.35.0 is an audio-distribution server, separate from the music-server
compatibility entries above. Linux Docker coverage includes Sonicfast's sender,
built-in PCM/FLAC receiver, real-App decode/routing, official reference clients,
isolated native output, stream/format changes, mute and outage/recovery.
See [ADR 011](adr-011-snapcast.md) and the [implementation evidence](snapcast.md).
Physical-speaker and other-platform synchronization remain unverified.
