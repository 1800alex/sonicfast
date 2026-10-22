---
title: What is Sonicfast?
description: A native player for your own music server.
nav_order: 1
---

Sonicfast connects to OpenSubsonic and compatible Subsonic music servers.
Your music and playlists stay on your server; Sonicfast provides a native desktop
interface, local audio output, downloads, an equalizer and visualizers.

Sonicfast is a fork of [Spotifast](https://github.com/crmne/spotifast) by Carmine
Paolino. The fork retains the original interface and native audio stack, with
[Libresonic](https://github.com/1800alex/libresonic) handling server access.
It preserves the upstream MIT license and attribution.

A server URL and server account or supported API key are required. See
[getting started]({{ '/getting-started/' | relative_url }}) and
[compatibility]({{ '/server-compatibility/' | relative_url }}).
Remote control, casting, multiroom, podcasts and radio are outside the current scope.

The source tree retains a separate `sonicfast-legacy` executable for Spotify/demo
regression comparisons. Public Sonicfast guides and release packages describe
the default OpenSubsonic application. Historical decisions and comparison results
remain in the repository's
[design archive](https://github.com/1800alex/sonicfast/tree/main/docs/design).
