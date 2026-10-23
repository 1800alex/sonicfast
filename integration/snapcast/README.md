# Snapcast integration and manual playground

Sonicfast contains its own receiver. Only these test containers use the official
Snapclient as an independent reference. Snapcast 0.35.0's official amd64/arm64
Debian packages are downloaded over HTTPS and checked against pinned SHA-256s.
No private music, music-server account or host sound-device change is involved.

The automated proof currently runs on Linux. Requires Docker Engine, Docker Compose **2.24.4+**, Python 3.11+ and Sonicfast's
normal Rust/native build dependencies. Both `docker compose` and the standalone
`docker-compose` command are supported. The smoke override uses Compose's
`!override` tag. Image builds need network access; repeated tests reuse the image.

## Automated integration proof

```sh
make snapcast-test
# An installed toolchain override, if necessary:
make snapcast-test CARGO='cargo +stable'
```

This builds the actual Sonicfast `snapcast_probe` example and runs it on the host
against Docker Snapserver, two official file-output clients and an official
PulseAudio client. An isolated Docker PulseAudio server supplies two null outputs;
no host audio daemon or sound device is modified. Testing the host
binary avoids assuming its glibc matches a container or granting access to audio
hardware. The native receiver is tested with both timestamped virtual output and the
production fastframe audio backend against the Docker audio server. Timestamped
virtual captures and independent audio-monitor captures verify both paths.

The suite runs **FLAC and PCM**, checking:

- The real App signing into a synthetic Subsonic fixture, decoding a 48 kHz mono
  FLAC file and routing it to Snapserver; received audio confirms the 660 Hz signal.
- Real TCP source audio, fixed sample format, distinct stereo frequencies,
  finite samples, useful level, pacing and explicit source cancellation.
- Two built-in receivers' timestamped output, signal and software alignment.
- Three independent official receivers receiving the same source.
- The production native audio backend and an official PulseAudio receiver playing
  into separate Docker null sinks. Monitor timestamps compare audible signal onset
  on the same clock (40 ms tolerance, including 10 ms capture blocks).
- Source pause/resume, source-volume mute and flushing a partial final source chunk.
- Server-side per-client mute/unmute applied by the built-in receiver.
- Live stream switching to 48 kHz/24-bit stereo FLAC and 48 kHz mono PCM, including
  resampling to the receiver output rate.
- A live source failing promptly on server loss; automatic native and official
  receiver reconnect after restart; a new source restoring audible data.
- Receiver cancellation and clean process exit without a companion binary.

The test reserves unused loopback ports for each run and keeps those mappings
stable through restarts. Its own containers, network and volumes are removed in
`finally`, even on failure. It does not stop the separate smoke environment.
Captured audio, Docker logs and JSON reports stay in ignored
`.cache/snapcast/integration-*`. A failing assertion retains its evidence.

For a focused rerun after building:

```sh
python3 integration/snapcast/test.py --codec flac
python3 integration/snapcast/test.py --codec pcm
```

These are transport and software-scheduling checks, not a measurement of physical
speaker synchronization. Hardware latency, Bluetooth buffering and different
platforms need separate listening/latency tests. The native/PulseAudio check is still a software-output test. Other codecs, authenticated
Snapcast and remote queue control are outside this implementation.

## Separate smoke environment

```sh
make snapcast-smoke-up
make snapcast-smoke-tone
```

Both commands are needed for the listening test: `snapcast-smoke-up` starts the
server and silent reference clients; `snapcast-smoke-tone` starts the audio
source. **Connected; clock synchronized** means the receiver has synchronized
with the server, even when no source is playing.

Open Sonicfast, choose **Snapcast audio…** before sign-in or **Settings → Snapcast**:

| Field | Value |
| --- | --- |
| Snapcast server | `127.0.0.1` |
| Receiver port | `1714` |
| Trusted network | On |
| Receive audio | On |
| Receiver device | Default output |
| Send music | Off while the synthetic tone is running |

The settings apply automatically. You should hear a quiet stereo tone. Snapweb is at
<http://127.0.0.1:1781>; it lists the official reference rooms and your built-in
Sonicfast receiver. Choose the Sonicfast stream and adjust that room's volume.
The TCP control endpoint is `127.0.0.1:1715`.

For the other direction:

```sh
make snapcast-smoke-stop-tone
```

Connect Sonicfast to your usual OpenSubsonic server. Set **source port 4993**,
enable Send, then play music. Settings apply automatically. Also enable Receive if you want this
computer to listen through Snapcast; direct local output is replaced while
sending. The source format is always `44100:16:2`; use a dedicated TCP source port.
Never run the tone producer and Sonicfast's sender on the same source together.

To keep this experiment out of your everyday app profile, launch a Linux build
with dedicated XDG directories (native output still uses your normal device):

```sh
mkdir -p .cache/snapcast/smoke-app/{config,state,cache}
XDG_CONFIG_HOME="$PWD/.cache/snapcast/smoke-app/config" \
XDG_STATE_HOME="$PWD/.cache/snapcast/smoke-app/state" \
XDG_CACHE_HOME="$PWD/.cache/snapcast/smoke-app/cache" \
  cargo run --locked --bin sonicfast
```

A second desktop instance may be redirected to the existing instance; close your
normal Sonicfast instance first. Don't enable remembered sign-in just to receive
Snapcast audio; no library login is required.

Other computers can connect by explicitly binding the smoke audio ports to LAN:

```sh
SNAPCAST_BIND=0.0.0.0 make snapcast-smoke-up
```

Use this machine's LAN address and port 1714 there. Keep it on a trusted network.
The raw source port 4993 is also exposed with this opt-in. Control/Snapweb remain
bound to loopback; no music-server credentials are passed to these containers.

```sh
make snapcast-smoke-logs       # Recent logs
make snapcast-smoke-down       # Stop; retain server state
make snapcast-smoke-reset      # Remove this fixture and its server volume
```

The two manual reference clients discard their output, so leaving this playground
running does not grow raw-audio capture files.

The smoke project is `sonicfast-snapcast-smoke`, with its own volume and fixed
ports. It does not share integration-test state. `SNAPCAST_CODEC=pcm` selects PCM
when creating/recreating the smoke server; FLAC is the default.

## Synthetic music fixture

The integration-only music service is a minimal Subsonic test double, not another
real-server compatibility claim. `tone.flac` is the original two-second, 48 kHz mono
660 Hz fixture from [Libresonic](https://github.com/1800alex/libresonic) (MIT), with
SHA-256 `5e54077d47320ce845e5467e0e3a70a01584ffd857afb58e6d4f556b27aa8c41`.
It contains no personal music or third-party recording. Regenerate it with:

```sh
ffmpeg -f lavfi -i sine=frequency=660:sample_rate=48000:duration=2 \
  -ac 1 -codec:a flac -map_metadata -1 tone.flac
```

The music service runs only under the automated `native-test` profile and logs
no authenticated request targets. It is not part of the manual smoke project.
