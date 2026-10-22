---
title: Settings and files
description: Server, scrobbling, downloads, playback and storage.
nav_order: 1
---

Open Settings using the gear button. Search filters the settings rows.

| Section | Purpose |
| --- | --- |
| Server | Review the connected server and sign out |
| Scrobbling | Enable reporting and set the percentage/time thresholds |
| Downloads | Enable downloads, choose a folder and open the queue |
| Playback | Choose the output device and native playback options |
| Appearance | Themes, light/dark/system appearance and language |
| Equalizer | Ten-band EQ, preamp and saved presets |
| Visualizers / mini-player | Display options, MilkDrop presets and Winamp skins |

## Scrobbling

Enable scrobbling to send now-playing and completed-listen reports to your music
server. Set the listening percentage and time threshold. A listen qualifies at
the earlier configured threshold; only time actually played counts, so seeking
forward does not count as listening. Paused or stalled time does not count.
Disabling scrobbling stops further reports. This reports to your server; any
Last.fm forwarding is configured on that server.

## Downloads and local playback

Enable Downloads and choose a writable destination. The Downloads view shows progress and queued/failed items; use its actions
to cancel or retry work.

The destination is the root for server-relative paths. For example, a server path
`Artist/Album/Artist - Song.m4a` is saved beneath the chosen directory with those
components. If the server supplies no usable path, Sonicfast derives a filename
from the track metadata. Existing safe server filenames are preserved, including
artist prefixes; filename sanitization depends on the local platform.

No persistent JSON download index is used. Background filesystem checks look for
candidate files and update the track-list arrow. A local file is preferred for
playback; missing or unusable files fall back to the server. Renaming files outside
the known candidate paths can prevent matching. Existing files are not evidence
of matching content merely because they have the same name.

Downloaded music belongs to the selected directory and survives sign-out. Changing
the directory changes where availability is checked; it does not move old downloads.
The app's existing state stores unfinished work separately from file availability.

## Stored data

Sonicfast uses platform application directories with the `sonicfast` identity.
On Linux, settings normally live in `~/.config/sonicfast`, state in
`~/.local/state/sonicfast`, and caches in `~/.cache/sonicfast`; XDG overrides apply.
On macOS and Windows, the native platform application-data directories are used.
Diagnostic logs include the selected locations.

- `settings.json`: non-secret preferences.
- `session.json`: local session and resume state.
- `history.json`: local playback history.
- `sonicfast.log` and `panic.log`: diagnostics.
- `skins/` and `milkdrop/` in configuration: installed skins and presets.
- Artwork, lyrics and other disposable caches: the application's cache directory.

Remembered server secrets use native protected storage, not these settings files.
The legacy comparison binary has a separate profile identity. The default
application currently uses the per-user directories even when launched from a ZIP;
placing it on a USB drive does not make server credentials portable.

Review logs and screenshots before sharing them: server names, library metadata
and local paths can be private. Do not share API keys or authenticated URLs.
