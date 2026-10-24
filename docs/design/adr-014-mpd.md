# ADR 014: MPD audio output

Date: 2026-10-24. Accepted at the owner's request to play on a Raspberry Pi
running Music Player Daemon. This extends ADR 012's output set with one more
single-receiver output; a general remote queue coordinator stays deferred.

MPD cannot accept a live PCM stream from a client, so Sonicfast does not send
processed audio. It reuses ADR 012's temporary media lease: the selected
original is served over an unguessable per-track URL that only the MPD host's IP
may fetch, and MPD's curl input fetches and decodes it. Music-server URLs and
credentials never reach MPD. Like Cast, EQ, limiting and visualization do not
apply, and codec support is MPD's own.

Sonicfast acts as an ordinary MPD protocol client over TCP (normally port 6600):
`clear`, `addid <lease>`, `addtagid` (title/artist/album, best effort),
`playid`, `seekcur`, `pause`, `setvol`, `stop`/`deleteid`, with `status`
polled every 500 ms for position, pause and end. Arguments are quoted and
control characters replaced, so track text cannot inject commands. Replies are
bounded (line length and pair count). Playing replaces MPD's current queue; this
is stated in Settings. Another client's song (`songid` not ours) or an MPD
`error` is a takeover/failure, never a natural song end; `stop` after our song
started is the end and advances Sonicfast's queue.

The remote player loop that ADR 012 wrote for Cast is now shared through a small
`Device` trait (connect, load, pause, resume, seek, status, volume, stop), keeping
its command epochs, stale-reply guards, lease revocation and error reporting.
Cast behavior is unchanged and retains its own Docker proof.

Seeking needs a byte-seekable source. When the music server offers no byte
ranges, MPD refuses `seekcur`; Sonicfast keeps playing and follows MPD's
reported position instead of failing. A volume refusal (no mixer) is ignored.

Discovery adds passive `_mpd._tcp` browsing; manual host/port works without it.
LAN casting consent is required as for AirPlay/Cast. MPD passwords travel in
clear text and are not supported in this first version: a permission refusal is
reported, never retried. Adding them needs the protected credential store.
No new Rust dependency is required.

Evidence is in casting.md. Sources: [MPD protocol](https://mpd.readthedocs.io/en/latest/protocol.html),
[MPD user manual](https://mpd.readthedocs.io/en/latest/user.html).
