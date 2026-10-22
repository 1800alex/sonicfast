---
title: Getting started
description: Connect your server and start playing.
nav_order: 3
---

[Install or build Sonicfast]({{ '/download/' | relative_url }}), then open it.

## Connect to your server

Choose **Add connection** if you already have saved servers. Enter a connection
name, server URL and username in the sign-in form. Choose password
or API-key authentication to match your server and enter the secret. API keys
require server support. Use the URL you normally use to reach your music server,
including any deployment subpath.

Use HTTPS when available. To connect to a self-hosted HTTP endpoint, explicitly
allow insecure HTTP in the form. Sonicfast validates HTTPS certificates.
**Remember** stores the secret in your operating system's protected credential
store. An unlocked credential store is required for remembered login; without it,
use a session-only connection. Server credentials are separate from legacy profiles.

Saved servers appear in the connection list. Open **Settings → Server → Manage
connections** to add or switch servers while connected. Only one server is active;
switching stops playback and preserves each connection's own queue. **Disconnect**
keeps remembered credentials; **Remove** forgets the connection after confirmation.
See [server connections]({{ '/connections/' | relative_url }}) for details.

## Play music

Browse artists or albums, open a server playlist, or search from the top bar.
Select a song to play it on this computer. Sonicfast supports MP3, FLAC and
M4A containing AAC-LC or ALAC. Your server's original file must use a supported codec.
The output device and playback options are in **Settings → Playback**.

Use the bottom bar for play/pause, seeking, shuffle, repeat and volume. Open Queue
to inspect upcoming songs. See [everyday use]({{ '/using-sonicfast/' | relative_url }}).

## Save and organize

The outlined plus in the bottom player bar saves the current song to Liked Songs.
Once saved, the filled heart opens the existing playlist picker, which also lets
you remove the song from Liked Songs. Right-click a song or the player title/cover
for its track menu, including ratings.

Enable optional [scrobbling and downloads]({{ '/settings-and-files/' | relative_url }})
in Settings. The download directory must be writable. Downloaded files remain on
your machine when you sign out.

## Appearance

**Settings → Appearance** controls light/dark/system themes and language.
The search field on Settings filters available rows. The application also retains
its [Winamp mini-player]({{ '/winamp/' | relative_url }}) and
[MilkDrop visualizer]({{ '/milkdrop/' | relative_url }}).
