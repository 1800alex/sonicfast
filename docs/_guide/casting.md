---
title: AirPlay and Chromecast
description: Choose a LAN audio output and control music from Sonicfast.
nav_order: 9
---

Open the speaker button in the bottom player bar to choose **Local**, a nearby
**AirPlay** or **Chromecast** receiver, or the configured **Snapcast** sender.
Choose **Find receivers** to scan again. Discovery does not send audio.

Enable **Allow LAN casting** before selecting a network receiver. Choosing an
output preserves the song and position paused; press Play to continue. Use one
output at a time. The independent Snapcast receiver can still receive audio.
If Snapcast Receive is the only local route, enable Send too or choose another
output before playing your own queue.

## AirPlay Classic

Select a password-free Classic receiver, such as Shairport Sync. Sonicfast sends
its processed audio directly; EQ and visualization remain available. A receiver
that requires pairing, encryption or a password is currently unsupported.
AirPlay 2 grouping is not implemented. Playback has about two seconds of receiver
buffering, including after transport changes. Pause/resume retains the heard
song position. An output error requires pressing Play to reconnect.

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
remain in its own section.

**Receiver certificate** is blank for genuine Google Cast devices, which use
Google's device-authentication chain. For an owner-operated software receiver,
copy its SHA-256 TLS certificate fingerprint into this field before applying.
Obtain it directly from the receiver host, not an untrusted discovery message.
The pin verifies that receiver's TLS key; it does not turn it into a certified
Chromecast. Other casting apps may reject such software receivers.

See the [Snapcast guide]({{ '/snapcast/' | relative_url }}) for synchronized house-wide audio and
the repository's [casting integration fixture](https://github.com/1800alex/sonicfast/tree/master/integration/casting)
for the synthetic Docker proof. Physical receiver and platform compatibility
must be validated separately.
