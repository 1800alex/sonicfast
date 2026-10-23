---
title: Snapcast audio
description: Send music to Snapserver and receive synchronized audio in Sonicfast.
nav_order: 8
---

Sonicfast 0.15.0 can send music to a Snapcast server and act as a **built-in
Snapcast receiver**. No `snapclient` program is required on your computer.
Your OpenSubsonic server still supplies the library; Snapserver distributes audio.

## Configure Snapserver

Add a TCP source to your Snapserver configuration:

```ini
[stream]
source = tcp://0.0.0.0:4953?name=Sonicfast&mode=server&sampleformat=44100:16:2&codec=flac&chunk_ms=20
buffer = 1000
```

Allow that source port through your trusted network's firewall. Use a dedicated
source port for each sender. The format **44100:16:2** is required for Sonicfast's
sender. `codec=flac` or `codec=pcm` works with the built-in receiver; this is the
network codec, independent of the MP3/FLAC/M4A original music file.

## Send music

1. Open **Settings → Snapcast**.
2. Enter the Snapcast server's hostname/IP address and source port (normally 4953).
3. Enable **Trusted network** and **Send music**, then **Apply / reconnect**.
4. Press Play on a song from your music library.
5. In Snapweb or your existing Snapcast controller, assign the desired receiver
   group to the **Sonicfast** stream.

Sending replaces direct local output. EQ and limiting remain active; Sonicfast's
player volume affects the source sent to every receiver. The visualizer keeps
showing that processed source. Other Snapcast clients can use their own volumes.
A disconnected sender reports an error; retry Play after the server is available.
It does not silently switch to local speakers or replay an old network backlog.

## Receive audio

Enable **Receive audio**, enter the receiver port (normally **1704**) and apply.
Receiving also works before signing in: choose **Snapcast audio…** on the sign-in
screen. It is independent of the selected OpenSubsonic connection.

The receiver supports mono/stereo PCM and FLAC streams at 8–192 kHz, with
16/24/32-bit integer PCM as supported by the stream. It negotiates the codec from
the server, estimates its clock offset and schedules audio by the server's
chunk timestamps. The native output backend supplies device latency and handles
ordinary device recovery. An unavailable device or lost connection produces a
visible status and reconnect attempts. Unsupported codecs produce a message
asking for PCM or FLAC; Opus and Vorbis reception are not implemented.

Choose **Default output** under **Receiver device** to follow the system output,
or select another audio device. **Latency correction** is normally 0; a positive
value advances playback to compensate for additional speaker/device latency.
It supplements the latency reported by the native backend.

Leave **Receiver identity** blank to generate and save an identity, or enter a
unique name for this room. Use a different identity on each computer. The server
can remember that client's group and volume. Use Snapweb to choose its stream,
change receiver volume or mute it. Received audio is passed through without
applying the sender EQ again. It does not populate the local song queue, create
song metadata or submit local scrobbles.

Enable **both Send and Receive** to hear your own source in sync with other
Snapcast clients. Receive-only mode suspends local music playback. Applying any
routing change preserves the selected local song/position but restores it paused.
The bottom song transport controls the local source, not other Snapcast streams.

## Latency and security

Audio can continue for the configured server buffer after pause, seek or stop.
A 1000 ms server buffer adds about one second of room-playback latency. Existing
clients cannot retract audio that has already been delivered. Smaller buffers
need a more reliable network and accurate device-latency reporting.

Snapcast TCP audio and the raw source are **unencrypted** in this version.
Explicit Trusted network consent is required, independently of your OpenSubsonic
HTTP setting. Do not expose these ports to the public internet; use a VPN.
Sonicfast does not send music-server credentials to Snapserver and does not
support Snapcast authenticated/TLS transports yet.

## Try the Docker playground

From a source checkout:

```sh
make snapcast-smoke-up
make snapcast-smoke-tone
```

Run both commands: the first starts the server, and the second feeds it audio.
**Connected; clock synchronized** confirms the receiver connection and clock;
it can appear while the server has no audio source playing.

In Sonicfast choose host `127.0.0.1`, **receiver port 1714**, enable Trusted network
and Receive, and apply. The tone is original synthetic audio. The fixture includes
Snapweb at [http://127.0.0.1:1781](http://127.0.0.1:1781).

To send your own music instead, first run `make snapcast-smoke-stop-tone`, then
use **source port 4993** and enable Send. The same machine can enable Receive too.
Only one source should feed a TCP input at a time.

`make snapcast-smoke-down` stops this separate environment and retains its server
state; `make snapcast-smoke-reset` removes that fixture's state. Automated tests
use separate projects/volumes and their own loopback ports. See the repository's
[integration instructions](https://github.com/1800alex/sonicfast/tree/master/integration/snapcast)
for LAN access, isolated app profiles, diagnostic playback and the test coverage.
