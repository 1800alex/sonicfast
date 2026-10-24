---
title: AirPlay, Chromecast and MPD
description: Choose a LAN audio output and control music from Sonicfast.
nav_order: 9
---

Open the speaker button in the bottom player bar to choose **This device**, a
nearby **AirPlay**, **Chromecast** or **MPD** receiver, or **Snapcast** once it is
set up. Each receiver shows a small AirPlay, Cast or MPD tag; MPD servers using
MPD's default announced name are shown by their host name. The picker looks for receivers when it opens; its refresh button scans
again. Discovery does not send audio.

Enable **Allow LAN casting** before selecting a network receiver. Choosing an
output reconnects automatically at the current song position: playing music
continues and paused music stays paused. A song still loading is preserved too.
Use one output at a time. Choosing Local turns off Snapcast Receive so your own
queue can play. AirPlay/Cast and the independent Snapcast receiver can coexist;
the output picker and settings show a notice when receiving is also enabled.

## AirPlay

Select a password-free Classic receiver, such as Shairport Sync, or a newer
receiver with an on-screen AirPlay code, such as Roku. Sonicfast sends its
processed audio directly; EQ and visualization remain available.
Receivers with a display (Shairport Sync front ends, AVRs, TVs) show the song
title, artist, album, cover art and a progress bar; receivers without one
decline it and keep playing.
AirPlay grouping is not implemented. Playback has about two seconds of receiver
buffering, including after transport changes. Pause/resume retains the heard
song position. An output error requires pressing Play to reconnect.

Use the discovered receiver's advertised port when configuring it manually;
some stereos use a port other than 5000. Current builds answer clock requests
during the initial handshake, which receivers such as the Pioneer VSX-1123
require before accepting playback. If a stereo wakes and selects AirPlay but
playback immediately fails, update/rebuild Sonicfast before retrying.

### Roku and on-screen PIN pairing

Choose **Find receivers** in the output picker, then select your Roku. Sonicfast
discovers both Classic RAOP and newer AirPlay advertisements; the Roku appears
even when it advertises only the newer service, normally on port 7000.

On the first connection, enter the four-digit AirPlay code shown on the TV into
Sonicfast's pairing dialog. Leave the TV awake while pairing. Successful pairing
selects the receiver and reconnects your current song automatically, preserving
whether it was playing or paused. Cancel keeps your previous output.

Pairing keys are saved in the operating system's protected credential store,
separately from music-server sign-ins. A remembered receiver does not need a new
PIN for every song or pause/resume. If the store is locked/unavailable, pairing
works for this app session and Sonicfast tells you it could not remember it.
PINs are never saved in settings. Control and audio use authenticated encryption
on the paired transport; Classic receivers keep their existing transport.

For manual setup, enter the receiver's advertised host and port under
**Settings → Casting**, enable **Receiver uses AirPlay PIN pairing**, then choose
**Pair receiver…**. Use this button to pair again if the TV has forgotten this
computer. Incorrect/expired codes and receiver attempt limits have explicit
errors with a retry option. Pairing requests expire after two minutes.

Receiver passwords, older legacy PIN schemes, RSA/FairPlay-only receivers,
Apple Home-only access policies and synchronized AirPlay groups are unsupported.
PIN support uses HomeKit-style AirPlay pairing, not the Roku account PIN.

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

## MPD

Select an MPD server (Music Player Daemon, for example on a Raspberry Pi) to play
your queue through its speakers. It appears in the picker when MPD advertises
itself over Avahi/Bonjour; otherwise enter its host and port (normally 6600) in
**Settings → Casting** and choose **MPD** as the output.

Like Chromecast, MPD fetches a temporary link to the original file from this
computer and decodes it itself, so the firewall note above applies, MPD's codec
support applies, and EQ/visualization do not. MPD needs its curl input plugin to
play links; `mpd --version` lists `curl` under input plugins in standard Debian,
Ubuntu, Raspberry Pi OS and Alpine packages. Its `bind_to_address` must accept
connections from this computer (for example `"0.0.0.0"` on a trusted LAN).

Playing replaces whatever is in MPD's own queue. Sonicfast's controls pause,
seek, change MPD's volume (when its output has a mixer) and move to the next song
when MPD finishes one. If another MPD client starts something else, Sonicfast
stops rather than treating it as the end of your song. Seeking needs a music
server that offers byte ranges for originals; otherwise the song keeps playing
from where MPD is. Password-protected MPD servers are not supported yet.

## Manual receiver and self-hosted Cast targets

Use **Settings → Casting** to enter an IP/hostname and port manually. AirPlay
Classic normally uses 5000; newer AirPlay receivers commonly use 7000; Cast
normally uses 8009 and MPD 6600. Snapcast's server/source settings
remain in its own section. Changes apply automatically: switches and selections
apply immediately, while text/number edits apply after a short typing pause.
Incomplete or invalid fields show **Not applied** and keep the last working
configuration. You can leave Settings while a valid edit is waiting to apply.
Use **Reconnect** only to retry an existing configuration.

Choosing a Cast receiver checks it first. Genuine Google Cast devices connect
directly. A receiver that cannot prove it is one, such as a self-hosted receiver,
opens **Trust this Cast receiver?** with the SHA-256 fingerprint of the
certificate it presented. Compare it with the receiver if you like, then choose
**Trust receiver**, or **Paste another…** to enter one yourself. Sonicfast
remembers the fingerprint for that receiver; if the receiver later presents a
different certificate, it asks again (**Cast receiver changed**). Turn on **Trust
self-hosted Cast receivers** (in the dialog or **Settings → Casting**) to trust a
new receiver the first time without asking; a changed certificate still asks.

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
