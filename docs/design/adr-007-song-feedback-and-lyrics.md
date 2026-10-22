# ADR 007: song hearts, ratings and LRCLIB lyrics

Date: 2026-10-21. Status: accepted for this owner-requested Phase 4 slice.

Song hearts and personal ratings are independent server annotations. Libresonic
exposes favorite-song reads/writes and personal ratings (1–5; zero clears), with
neutral server/kind-scoped IDs. Missing/invalid `userRating` is unrated/unknown;
old serialized queue snapshots default the new optional rating field. Use
[getStarred2](https://opensubsonic.netlify.app/docs/endpoints/getstarred2/),
[star](https://opensubsonic.netlify.app/docs/endpoints/star/), unstar and
[setRating](https://opensubsonic.netlify.app/docs/endpoints/setrating/).

Writes share the existing bounded FIFO with playlist edits, do not retry, and
retain authenticated transport safeguards. Song-only scope/kind validation,
2,000-song batches, 1 MiB encoded forms and 8 KiB legacy GET limits apply. Use
form POST only when advertised. `getStarred2` is an unpaginated server response
subject to the existing 4 MiB JSON cap; do not pretend that local slicing is
server pagination.

Sonicfast reuses row/player hearts, the Liked Songs page and existing optimistic
heart rollback. Its server library still lists all albums/artists. Favorite-song
metadata is fetched from the server per session; inherited Spotify liked-cache
commands are bypassed, rather than sharing that account cache. Liked Songs plays
its displayed local track list, without inventing a Spotify collection context.
Song menus expose personal 1–5 ratings and Clear rating. Ratings wait for server
confirmation, preserve the previous value on error, and clear session overrides
on sign-out. Album/artist hearts and ratings are outside this slice.

For lyrics, reuse `src/lyrics.rs` and the existing panel/fullscreen/timing/seek
controls. Sonicfast uses [LRCLIB](https://lrclib.net/docs) directly with a separate
unauthenticated HTTP client; server credentials/URLs never enter lyric requests.
Lyrics retrieval sends artist, title, album and duration to LRCLIB. Server lyric
extensions and embedded lyric extraction remain separate future work.

The LRCLIB lookup cascade uses: exact, split artist, ±1 second duration, omit album, then
search. This adaptation reserves search and has at most six logical attempts;
the retained bounded 429 retries are inside each attempt. The whole lookup has
a 30-second limit, with one active lookup canceled when superseded. Each body is
capped at 1 MiB; timed-text expansion is also bounded. Search matches normalized
title/artist and known duration within two seconds, preferring synced lyrics.
Instrumental and plain-text results remain supported. Transient failures are
errors, not cached misses. Positive metadata-keyed caches last 30 days, negative
answers seven days. The cache version changes to invalidate older loose matches.

Playlist duplicate confirmation already exists. Preserve the shared picker,
server check, Add anyway and Cancel flow for single and multiple songs. A cancel
writes nothing. Another client may still change a playlist after the check;
this is an intentional confirmation, not an atomic uniqueness constraint.

No dependencies or audio-device changes. [Execution evidence](song-feedback-and-lyrics.md).

The owner's [visual alignment follow-up](settings-visuals.md) uses the existing
menu-item painter for rating choices and Clear rating, with no selected-label
border. The parent menu shows the current rating; clearing an unrated song is
disabled. No protocol or feedback state-machine changes.

## Player bar follow-up (2026-10-22)

The visible bottom-player ellipsis reuses the track-row menu and existing rating,
playlist and duplicate-confirmation actions. See the [native review and evidence](search-and-branding.md).

## Saved-heart follow-up (2026-10-22)

The owner supersedes the visible player ellipsis with the existing saved-heart
playlist picker. An unsaved song shows CirclePlus; its first click saves to Liked
Songs. A saved song shows the filled heart, and subsequent clicks open the shared
playlist picker with Remove from Liked Songs. Rating stays in track menus and
the player's existing title/cover right-click menu. See the
[follow-up evidence](search-and-branding.md#saved-heart-follow-up).
