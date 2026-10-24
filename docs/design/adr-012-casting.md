# ADR 012: AirPlay Classic and Cast audio outputs

The subsequent [ADR 013](adr-013-airplay-pairing.md) extends the original
password-free Classic-only scope below with native PIN pairing and encrypted
single-receiver AirPlay audio. Group coordination stays deferred.

Date: 2026-10-23. The owner authorized the shared output picker and both senders,
then requested a software Cast target in the garage Docker stack.

Sonicfast keeps its queue, profile, downloads-first lookup and transport controls.
Output changes retire the previous player and restore the selected song/position
with its play/pause intent. An in-flight load is restarted on the new route.
This supersedes the initial restore-paused behavior at the owner's request.
Switches/selections apply immediately; endpoint/fingerprint edits apply after
500 ms without typing, with invalid drafts retained separately from the applied
configuration. Drafts live at app scope so filtering/navigation cannot strand
an edit and a newer picker choice cannot be undone by an old settings draft.
Receive-only Local mode has an explicit warning and pauses the queue. Choosing
Local explicitly disables Receive; network outputs can coexist with receiving. This extends the original casting deferral; a
general remote queue coordinator and AirPlay 2 grouping remain deferred.

AirPlay Classic uses the existing PCM normalization/EQ/limiter/tap, then 44.1 kHz
stereo signed 16-bit samples in lossless, verbatim ALAC frames. A native RTSP/RTP
sender supplies timing replies, synchronization packets, bounded retransmission
history, keepalive and cancellation. Only unencrypted, password-free Classic
receivers are supported. AirPlay 2, RSA/FairPlay and pairing are not implemented.
Pause reopens the decoder at the heard position, avoiding skipping the buffered
audio on resume. Receiver buffering still delays audible transport changes.
The sender services UDP timing/control while waiting for RTSP headers and bodies,
including SETUP and RECORD. Hardware can require its initial clock exchange before
acknowledging RECORD. Short read polls retain cancellation and an absolute request
deadline; TEARDOWN has a separate short deadline. The first sync packet carries
the protocol's initial extension marker.

Chromecast uses the Default Media Receiver rather than the legacy Spotify app.
The inherited Cast nonce/certificate authentication runs before sharing a media
URL. An optional explicit SHA-256 certificate pin supports an owner-operated
software receiver: TLS proves possession of that pinned key, and Google device
authentication is not claimed. Pins are never learned automatically from mDNS.
Amended 2026-10-24 at the owner's request: a pin may come from the certificate a
receiver presents over TLS when the owner trusts it in the picker's dialog, or
automatically on first contact with the opt-in **Trust self-hosted Cast
receivers** setting (trust on first use). A changed certificate always asks.
The software target is not a certified Chromecast and SDK senders may reject it.

A temporary HTTP listener binds the interface used to reach the receiver. It
serves only the selected original, through Libresonic's StreamProvider, using
an unguessable per-track path and allowing only the selected receiver IP.
There is no filesystem browsing, music-server URL forwarding, redirect handling
or exposed server credential. Four concurrent requests, bounded headers/chunks
and read/write timeouts bound resource usage. GET/HEAD, CORS and validated byte
ranges support compatible receiver decoders. Replacing the song, disconnecting,
changing route or shutting down revokes the old lease and active transfers.
Single receivers are supported; Cast groups may fetch from another IP and are
not promised. Original codec support and byte seeking depend on the receiver
and source. EQ/visualization are unavailable when the receiver decodes originals.

Remote state is driven by correlated MEDIA_STATUS replies. The existing queue
advances only after FINISHED; errors, takeovers, buffering and disconnects cannot
be interpreted as a completed song. Control intent supersedes stale replies.
Music-server scrobbling follows the receiver-reported playback position.

No new Rust dependency is required. The native encoders/framing are implemented
from public formats; no GPL receiver/client crate is linked. Synthetic Docker
captures prove decoded signal and lifecycle, not physical speaker synchronization
or compatibility with untested hardware. See casting.md for executed evidence.

Sources: [AirPlay protocol](https://nto.github.io/AirPlay.html),
[Shairport Sync](https://github.com/mikebrady/shairport-sync),
[Google receiver overview](https://developers.google.com/cast/docs/web_receiver),
[media messages](https://developers.google.com/cast/docs/media/messages),
[software receiver certificate requirements](https://github.com/emilsvennesson/vibecast#certificates).
