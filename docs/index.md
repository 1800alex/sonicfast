---
layout: home
title: Sonicfast
description: Your music server, in a native desktop player.
hero:
  name: Sonicfast
  text: Your music server. A native player.
  tagline: Browse, play and download music from OpenSubsonic and compatible Subsonic servers.
  actions:
    - theme: brand
      text: Get started
      link: /getting-started/
    - theme: alt
      text: Source on GitHub
      link: https://github.com/1800alex/sonicfast
---

Sonicfast is a Rust desktop music player built with egui. Connect your own
server, play your library and manage server playlists without a browser engine.

- Browse artists, albums, songs and playlists; search your server library.
- Play MP3, FLAC and M4A (AAC-LC or ALAC).
- Heart songs, rate them from one to five stars, and find lyrics through LRCLIB.
- Enable scrobbling and choose the listening threshold.
- Download originals in the background and play local files first.
- Keep the native equalizer, visualizers, themes and classic Winamp mini-player.

Start with the [setup guide]({{ '/getting-started/' | relative_url }}) and
[server compatibility]({{ '/server-compatibility/' | relative_url }}).
Linux with Navidrome has native integration coverage. Other platforms and servers
need further desktop validation; available build jobs do not establish hardware compatibility.

Sonicfast is a fork of [Spotifast](https://github.com/crmne/spotifast) by
Carmine Paolino and its contributors. It uses
[Libresonic](https://github.com/1800alex/libresonic) for the server protocol and
playback state. The MIT license and upstream notices are preserved.
