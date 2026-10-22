# ADR 006: server playlists

Date: 2026-10-22. Status: accepted. Owner-requested next feature after M4A.
This implements the playlist portion of Phase 4, not the entire parity gate.

Libresonic owns playlist models, ordered entries, permissions evidence and
protocol requests. Sonicfast reuses the native sidebar, playlist table, create/
edit/delete dialogs, picker, duplicate confirmation and optimistic row edits.

- Core adds readable playlist summaries/details and create, update, replace and
  delete operations. Default trait methods return Unsupported for other service
  adapters. IDs stay server/kind scoped. Details retain unavailable/null slots;
  duplicate songs remain distinct occurrences with their original row positions.
- Use the standard [playlist endpoints](https://opensubsonic.netlify.app/docs/endpoints/getplaylists/).
  Append and exact-position removal use [updatePlaylist](https://opensubsonic.netlify.app/docs/endpoints/updateplaylist/).
  Insert/reorder use one complete-content replacement through
  [createPlaylist with playlistId](https://opensubsonic.netlify.app/docs/endpoints/createplaylist/).
  Refuse full replacement if unavailable entries would be lost.
- Creation sends initial songs once in the create request, preserving order and
  duplicates. Visibility/comment require a separate update. If that update fails,
  retain the created playlist and report the partial failure without retrying.
- Writes use [form POST](https://opensubsonic.netlify.app/docs/extensions/formpost/)
  only when version 1 is advertised. Legacy servers get bounded GET requests;
  requests beyond 8 KiB fail before sending instead of being split into partially
  applied writes. Form bodies and song-identity inputs are capped at 1 MiB.
  Each mutation accepts at most 2,000 added/replacement songs or removed rows.
- All authenticated requests retain TLS validation, explicit HTTP opt-in,
  deadlines, bounded collected responses, blocked redirects and disabled retries.
  Errors never contain raw request URLs, credentials or arbitrary server text.
- A bounded FIFO (64 pending batches, one active write) preserves edit order.
  Ordinary metadata requests remain concurrent. Session generations reject old
  results and cancel pending work on sign-out/switch. Already accepted server
  writes cannot be undone by canceling a future; they are never replayed on
  restart or automatically retried after an ambiguous response.
- A client-computed fingerprint covers ordered contents and metadata. Check it
  before positional removal/reordering, and also verify each selected row ID.
  This catches changes observed since selection; the API has no atomic
  compare-and-swap guarantee. A remote edit racing between validation and write
  can still occur, especially for insertion/full replacement.
- Reuse optimistic add/remove/reorder rendering and existing error handling.
  Re-read the authoritative playlist/list after every row mutation outcome,
  including failures, so rollback restores server counts/order. Metadata updates
  wait for confirmation. Successful deletion removes the playlist, rather than
  unfollowing it, and the confirmation describes permanent server deletion.
- Ownership and editability are separate. Token login verifies ownership by
  account name; advertised [tokenInfo](https://opensubsonic.netlify.app/docs/endpoints/tokeninfo/)
  can establish API-key account identity. Honor explicit readonly metadata when
  supplied. Unknown identity/permissions do not imply ownership; deletion UI
  requires verified ownership. Server permission checks remain authoritative.
- Fresh Subsonic metadata is authoritative. Do not apply inherited Spotify
  missing-field merge behavior: Navidrome omits false visibility flags, and
  merging an old public=true flag would undo a confirmed visibility change in
  the UI. Preserve absent flags as unknown rather than inventing protocol data.
- Playback loads a playlist as a local queue snapshot. Preserve original source
  positions through unsupported/unavailable filtering and paused restart.
  Later playlist edits do not rewrite the currently playing queue.

Playlist cover upload, link import, local M3U export, smart-playlist editing and
atomic cross-client conflict resolution are outside this feature. No new crate
dependencies are needed. [Execution evidence](playlists.md).

## Top search follow-up (2026-10-22)

[Top search](search-and-branding.md) now completes both catalogue and
case-insensitive server-playlist results through the existing split responses and
query serial guards. Mock coverage preserves playlist matches when catalogue
search fails; native Navidrome coverage exercises the Playlists filter.
