---
title: Server compatibility
description: Protocol features, codecs and known boundaries.
nav_order: 3
---

Sonicfast targets OpenSubsonic and compatible Subsonic servers. The disposable
Navidrome fixture has native integration coverage. Other implementations may
provide different optional fields, authentication methods and permissions.
The repository records actual checks in its
[server evidence matrix](https://github.com/1800alex/sonicfast/blob/main/docs/design/server-matrix.md).

| Feature | Requirement |
| --- | --- |
| Browse, search, stream | Compatible Subsonic REST API and a readable library |
| API-key login | Server support for the API-key extension |
| Playlist editing | Server endpoints and account permission to edit the playlist |
| Hearts and ratings | Server support for song starring and 1–5 ratings |
| Scrobbling | Server scrobble endpoint, enabled in Settings |
| Original downloads | Server download endpoint and a writable local directory |
| Exact local filenames | Server-provided song path when available |

Original MP3, FLAC and M4A (AAC-LC/ALAC) are supported. An `.m4a` extension alone
does not guarantee the container holds a supported codec. Unsupported originals
need conversion or server-side transcoding; this client does not promise automatic
transcoding negotiation.

Server paths are relative library paths. Unsafe absolute paths, traversal and
incompatible components are rejected or made safe for the local destination.
No universal file hash is required for local availability. Filesystem checks use
server metadata and candidate paths, without a recursive scan or JSON download index.

Casting, remote control, multiroom, podcasts and radio are outside current scope.
A working API endpoint or successful build is not evidence of desktop/audio-device
validation on every operating system.
