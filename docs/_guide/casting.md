---
title: AirPlay and Chromecast
description: Choose a LAN audio output and control music from Sonicfast.
nav_order: 9
---

Open the speaker button in the bottom player bar to choose **Local**, a nearby
**AirPlay** or **Chromecast** receiver, or the configured **Snapcast** sender.
Choose **Find receivers** to scan again. Discovery does not send audio.

Enable **Allow LAN casting** before selecting a network receiver. Choosing an
output reconnects automatically at the current song position: playing music
continues and paused music stays paused. A song still loading is preserved too.
Use one output at a time. Choosing Local turns off Snapcast Receive so your own
queue can play. AirPlay/Cast and the independent Snapcast receiver can coexist;
the output picker and settings show a notice when receiving is also enabled.

## AirPlay Classic

Select a password-free Classic receiver, such as Shairport Sync. Sonicfast sends
its processed audio directly; EQ and visualization remain available. A receiver
that requires pairing, encryption or a password is currently unsupported.
AirPlay 2 grouping is not implemented. Playback has about two seconds of receiver
buffering, including after transport changes. Pause/resume retains the heard
song position. An output error requires pressing Play to reconnect.

Use the discovered receiver's advertised port when configuring it manually;
some stereos use a port other than 5000. Current builds answer clock requests
during the initial handshake, which receivers such as the Pioneer VSX-1123
require before accepting playback. If a stereo wakes and selects AirPlay but
playback immediately fails, update/rebuild Sonicfast before retrying.

## Chromecast

Select a Google Cast receiver and play a supported original file. Sonicfast
remains responsible for the queue; pause, seek, volume and next/previous use the
normal player controls. Receiver status supplies progress and natural song end.
Local downloads are preferred before fetching the server original.

The receiver fetches a temporary audio URL from this computer. Allow incoming
LAN connections to Sonicfast in your firewall and keep both devices reachable
on the same trusted network. The app serves just the selected track to that
receiver's IP. The link expires and is replaced for each song; music-server
credentials stay inside Sonicfast. Closing the app stops this playback path.

The receiver decodes the original file, so its codec support applies. EQ and
visualization do not apply to Chromecast. Seeking requires a byte-seekable
original/source. Cast speaker groups and seamless independent handoff are not
currently guaranteed. A disconnected or failing receiver stops playback; Sonicfast
does not silently switch to local speakers.

## Manual receiver and self-hosted Cast targets

Use **Settings → Casting** to enter an IP/hostname and port manually. AirPlay
normally uses 5000; Cast normally uses 8009. Snapcast's server/source settings
remain in its own section. Changes apply automatically: switches and selections
apply immediately, while text/number edits apply after a short typing pause.
Incomplete or invalid fields show **Not applied** and keep the last working
configuration. You can leave Settings while a valid edit is waiting to apply.
Use **Reconnect** only to retry an existing configuration.

**Receiver certificate** is blank for genuine Google Cast devices, which use
Google's device-authentication chain. For an owner-operated software receiver,
copy its complete SHA-256 TLS certificate fingerprint into this field. The valid
fingerprint takes effect automatically after the typing pause.
Obtain it directly from the receiver host, not an untrusted discovery message.
The pin verifies that receiver's TLS key; it does not turn it into a certified
Chromecast. Other casting apps may reject such software receivers.

See the [Snapcast guide]({{ '/snapcast/' | relative_url }}) for synchronized house-wide audio and
the repository's [casting integration fixture](https://github.com/1800alex/sonicfast/tree/master/integration/casting)
for the synthetic Docker proof. Physical receiver and platform compatibility
must be validated separately.
