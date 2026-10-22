---
title: Using Sonicfast
description: Playback, playlists, hearts, ratings, lyrics and downloads.
nav_order: 4
---

## Browse and search

Use the library sidebar for artists, albums, Liked Songs and server playlists.
The top search finds artists, albums, songs and playlists. Playlist search matches
the server playlist list; podcasts and episodes are not offered in this client.
Library sorting includes name, recent activity and applicable library/local order;
Spotify custom order is unavailable in Sonicfast.

## Playlists

Create playlists from the library or Add to playlist picker. Open a playlist to
play it, edit its details, reorder or remove songs, or delete it when you have
permission. Server permissions determine which playlists you can edit.

Select one or more songs and choose **Add to playlist** in the track menu.
Search the picker by name or create a new playlist. If a song is already present,
Sonicfast asks whether to add another copy. Confirming adds the duplicate;
cancelling leaves the playlist unchanged. Server playlists and your local playback
queue are separate.

## Hearts and ratings

The outlined plus in the bottom bar saves the song to Liked Songs. A saved song
shows a filled heart; click it to open the playlist picker or remove the like.
Track menus also offer hearts and ratings from one to five stars, plus clear rating.
Right-click the player title or cover to open the current song's track menu.
The server stores song hearts and ratings.

## Queue and playback

Open Queue to inspect the current song and upcoming items. Queued songs are played
before the remaining album or playlist context. Repeat and shuffle apply to local
playback. Playback actions are handled on this computer; there is no remote device
transfer. See [queue behavior]({{ '/queue/' | relative_url }}).

The current song has five smoothly animated bars while playing. Pausing stops
the animation. Volume, EQ and visualizers use the existing native audio pipeline.

## Lyrics

Open Lyrics from the player to look up plain or synchronized lyrics through
[LRCLIB](https://lrclib.net). Synchronized lyrics follow playback position.
Availability depends on the service and track metadata. LRCLIB requests include
the artist, title, album and duration, not your server credentials.

## Downloads

Enable Downloads and select a destination in Settings. Download a song or album
from its menu or the track-list download control. The Downloads view shows queued,
active, complete and failed items with progress and cancellation/retry actions.
A ring around the down arrow indicates an active download; the colored arrow
indicates an original file found locally. The compact Queue panel omits the icon.

Sonicfast checks candidate paths directly rather than scanning the library.
Original server paths take precedence when supplied; otherwise it derives a safe
artist/album/track path. Local availability is refreshed in the background, so
external file changes can take a short time to appear. Playback tries the local
original first and falls back to streaming when unavailable. See
[download settings]({{ '/settings-and-files/' | relative_url }}).
