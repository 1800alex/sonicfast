---
title: Queue behavior
description: The local playback queue and album or playlist context.
nav_order: 4
---

Sonicfast maintains playback on this computer. Starting an album or playlist
provides the current context; songs explicitly queued play before the remaining
context. Inspect **Now playing** and **Next up** in the Queue panel.

Use a song's track menu to queue it, or start playback directly to choose a new
context. Queue actions and playlist editing serve different purposes: removing
an upcoming song does not delete it from a server playlist or the music library.
Shuffle and repeat affect local playback. Remote transfer is unavailable.

The recent-history view shows local playback activity. Resume state belongs to the
Sonicfast server profile. The compact Queue panel hides download controls so track
titles and durations fit.

Implementation decisions and regression evidence are recorded in
[ADR 004](https://github.com/1800alex/sonicfast/blob/main/docs/design/adr-004-desktop-integration.md)
and the [Phase 3 report](https://github.com/1800alex/sonicfast/blob/main/docs/design/phase-3.md).
The retained legacy comparison executable has separate Spotify Connect behavior.
