---
title: How it connects
description: Server requests, authentication and local playback.
nav_order: 2
---

Sonicfast uses [Libresonic](https://github.com/1800alex/libresonic) to communicate
with your OpenSubsonic/Subsonic server. Library browsing, search, playlists, hearts,
ratings, stream/download requests and optional scrobbles go to that server.
The server remains the source of your music and collection metadata.

Password authentication uses the Subsonic token/salt mechanism; supported API keys
are sent through the server's API-key extension. Use HTTPS to protect requests.
Authenticated requests do not follow redirects. A deployment that redirects API
requests should be configured with its final server URL.

The native sign-in form stores a non-secret profile separately from its secret.
Remembered secrets use platform protected storage, with a Sonicfast namespace.
Session-only sign-in keeps the secret in memory. URLs containing authentication
material must be treated as secrets when reporting problems.

Playback decodes audio on a background worker and sends PCM to Sonicfast's native
sink, preserving output-device selection, fades, EQ and visualization taps.
A configured local original is preferred before streaming. Download jobs also run
in the background and publish progress to the UI.

LRCLIB lyrics and optional visualizer preset downloads contact external services.
See [privacy]({{ '/privacy/' | relative_url }}) for the data involved.
The default application has no Spotify authentication, Premium requirement,
Connect session or browser login.
