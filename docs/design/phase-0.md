# Phase 0: repository audit and fork foundations

Date: 2026-10-21. Working report; validation results are recorded below.

## Frozen source and environment

- Fork: `9c923873b9c3893d791922e3a66151b71d41e7f3` (only adds DESIGN.md).
- Upstream source parent: `e86d3de0cb72edba8f24b447218daa7f3af1f127`.
- Sonicfast checkout was clean before changes. Empty Libresonic baseline:
  `bdc176f2b8fa8ae0c8dced09f5793a824db501cd`.
- Spotifast package/library/binary: 0.14.1, Rust 2024, minimum/pinned 1.98.0.
  This Linux x86_64 container has Rust/Cargo 1.99.1 installed as `stable`.
  Compiler pin has not been changed. Tests using `+stable` are not proof of MSRV.
- Initial plain cargo check failed while installing the pinned toolchain:
  `/usr/local/rustup/tmp` is root-owned. `cargo +stable check --locked` then
  failed writing `/usr/local/cargo/git`. `$HOME/.cache` is also root-owned.
  No project source failure was hidden by those errors.
- Use `CARGO_HOME="$PWD/.cache/cargo" cargo +stable ...` in this checkout.
  Build artifacts stay in `target/`. A full-feature baseline check then passed
  in 4m32s, without dependency/version changes.
- Installed missing host development prerequisites: libpipewire-0.3-dev,
  libpulse-dev, libclang-dev and Ruby. Existing ALSA/XKB/Wayland/GL packages
  were already available. No claim is made for Windows/macOS or audio hardware.

See [migration inventory](migration-inventory.md) for every source file matching
the Spotify/librespot scan and every git revision in the baseline lockfile.
The lockfile remains the source of truth for registry versions and checksums.

## Call path and ownership audit

These references are to the pinned source; additions in this phase do not alter
the application source layout.

| Boundary | Actual code path | Migration responsibility |
|---|---|---|
| UI intent | `src/ui/collection.rs:306-322` emits TogglePlay/PlayFromRow/PlayContext; player/footer and Winamp emit the same model Actions | Keep widgets and layout; adapt models at the app boundary |
| Action application | `App::apply_actions` in `src/app.rs:10428`; `start_play:8775`, toggle:9247, seek:9313, next/previous:10655/10677 | Preserve optimistic behavior; translate to neutral commands in Phase 3 |
| Async submission | `Backend::player`, `src/backend.rs:1543`, sends `Command::Player`; `Worker` command branch:2275 calls engine.command | Keep runtime/channel/repaint bridge; do not perform HTTP on the UI thread |
| Engine setup | `Worker::connect_engine:3620` spawns a task, opens cache and calls `Engine::connect:3668` | Replace Spotify initialization only after the independent playback proof |
| Spotify engine | `src/player.rs:402` creates Session, Player/mixer and Spirc; command:711/730 dispatches load/seek/queue/transfer | Future Libresonic owns local state/queue; no Connect transfer equivalent |
| Audio construction | `Engine::start_player:497` calls `sink_builder:838`, creates RodioSink and wraps it in vis::Tapped | Move the common PCM input behind an adapter, retain device implementation |
| Processing | `src/vis.rs:202-249` processes samples: EQ, post-EQ/pre-volume tap, limiter, inner sink | Decouple packet/mixer types while preserving ordering and reset behavior |
| Device delivery | `src/sink.rs:806` converts f64 to f32, resamples, appends to bounded rodio queue; MixerRender renders via fastframe-audio | Retain device changes/recovery, volume ramps, transport fades, queue pacing |
| State return | `src/player.rs:915` run_events rejects stale request IDs and folds PlayerEvent into LocalState; Notify emits EngineEvent, Worker emits Event and wakes app | Neutral generation IDs for load/seek; app handles stale optimistic state |
| Library reads | `ApiRequest`/`ApiResponse` through Worker/handle, ApiGateway and ApiClient; session_reads handles proprietary alternatives | Replace with neutral service facade in vertical slices, not a backend rewrite |

### Types crossing module boundaries

- `player` -> `backend`: `Engine`, `EngineConfig`, `PlayerCommand`, `LoadSpec`,
  `LocalState`/`LocalTrack`, `EngineEvent`, `Interrupted`, `PlaybackResume`,
  `KeptPlayer`, `Rootlist`/`RootlistEntry`. `PlaybackResume::Session` contains
  `Arc<librespot_connect::PlaybackSnapshot>`; `Engine::connect` takes librespot
  Credentials/Cache and `Engine::session` exposes Session. `KeptPlayer` retains
  Player/Mixer privately. These cannot define the reusable library API.
- `player` -> `sink`/`vis`: `Box<dyn Sink>`, SinkResult/SinkError, AudioPacket,
  Converter, VolumeGetter, PlayerEvent and librespot SAMPLE_RATE/NUM_CHANNELS.
  `eq.rs` also imports those format constants. Output device types themselves
  come from fastframe-audio, rather than Spotify.
- `backend` -> `credentials`/`auth`: librespot Credentials in Grant::Playback,
  OAuth Grant/StoredToken and playback bearer-token conversion. `credentials`
  has a dedicated native-store worker with generations, timeouts and revocation
  markers. Retain that pattern, add a separate Sonicfast server namespace later.
- `player`/`backend` -> `session_reads`, `new_releases`, `cast`: Session plus
  SpotifyUri/SpotifyId and proprietary playlist/metadata protobufs. These are
  Spotify facilities, not generic music-service requirements. `zeroconf` derives
  credentials from librespot and `sonos`/`cast` control Spotify receivers.
- `api` -> `model`/`app`/views: Spotify Track/Album/ArtistRef/Playlist,
  PlayableItem/Episode, SavedTrack/Album, Page, PlaybackState, Device and Queue.
  Plain String fields carry Spotify URI assumptions even without a Rust
  SpotifyUri type. `Page::from_uri`, links, library filters, queue identity,
  history, liked/account caches and persisted settings/session state depend on
  these semantics. Artwork URLs are assumed publicly fetchable; future art
  must be fetched as authenticated bytes and cached by neutral identity.
- `LocalState.connected`, username and active_client describe Connect/session
  ownership; do not map them mechanically to local playback readiness.
  LoadSpec context/offset URI, autoplay, Transfer and Rootlist folder permissions
  have Spotify semantics. VolumePreview exists to avoid Connect round trips;
  the future local volume path need not simulate those requests.
- UI-only declarations and deterministic Spotify demo/test fixtures can remain
  until their view migrates. The source inventory includes tests/comments;
  classify production references before migrating, not by replacing all strings.

### Authentication and persistence

`src/auth.rs` owns browser PKCE: separate playback and shared/personal Web API
grants, fixed loopback ports and scopes, refresh and cancellation. ApiClient
reads WebTokens/TokenProvider; backend converts playback OAuth access into
librespot credentials and connects the engine. `credentials.rs` stores durable
grants via Secret Service, Keychain or Windows native storage/DPAPI; settings
store nonsecret preferences/session/queue. Network proxy secrets have their own
slot. None is an OpenSubsonic server profile. Preserve old readability and never
reinterpret an existing Spotify grant as a server password.

### Minimal PCM proposal (not implemented)

Introduce a neutral PCM block with explicit sample rate/channel count and
interleaved samples, with bounded frame count, plus an output lifecycle covering
start, write, pause/resume, discard-on-seek/skip, drain-on-end and stop. Writes
run on a feeder thread and need cancellation while waiting for queue room.
Output errors remain recoverable, and callbacks feed the existing UI channel.
Use one documented initial processing format (stereo 44.1 kHz) and convert
decoded source formats before EQ/tap, preserving existing limiter precision.

Keep the old librespot Sink wrapper delegating to this shared implementation
while proving native MP3/FLAC separately. Replace VolumeGetter with shared gain
and PlayerEvent coupling with explicit seek/track/reset methods. Preserve the
200 ms queue-ahead budget, 20 ms refill and bounded drain/device-error handling.
Do not attach raw decoder audio directly to RodioSink and bypass Tapped.
No new output backend is justified until this proof fails.

## Inherited automation audit

- AGENTS.md and CONTRIBUTING.md have been replaced with fork rules. README
  explicitly labels the remaining upstream material as unmigrated reference.
- Dependency-source policy currently accepts registry crates and inherited
  crmne pins; normal local path dependencies are already accepted. Its git-only
  restriction applies to patch/replace entries, not our ordinary path dependency.
  Add regression evidence for the authorized path; no wholesale lint removal.
- The settings approval check is PR-only and no Settings fields change here.
- CI assumes a single checkout. The initial local dependency needs explicit library
  acquisition before automated app builds. Do not claim inherited CI is ready.
- flake.nix uses src=self and an upstream vendor hash. The initial external dependency is outside
  its source closure; Nix packaging is not supported by this local arrangement
  until source assembly and the hash are migrated. Do not fake a hash.
- docs, native-packages.yaml, release/Flatpak/Homebrew workflows and updater
  metadata point to Spotifast/crmne destinations. Board/triage automation also
  assumes upstream governance. These remain migration work, not publication
  authorization. Keep all publication disabled in practice.

## Migration checklist

- [x] Pin source, read design, map UI -> backend -> player -> processing -> device.
- [x] Inspect auth, protected store, settings and queue behavior/tests.
- [x] Record dependency revisions and source references.
- [x] Replace conflicting fork policy and label inherited README behavior.
- [x] Record separate library/layout override and evaluate OpenSubsonic client.
- [x] Bootstrap independent unpublished library without speculative APIs.
- [x] Copy synthetic Navidrome fixture into an isolated test project.
- [x] Finish baseline tests/demo and final scaffold validation.
- [ ] Implement domain/service types and protocol facade only after this gate.

## Validation and phase decision

Commands use Rust/Cargo 1.99.1 through `+stable` and the writable Cargo home
described above. Baseline commands ran before changing Cargo.toml/Cargo.lock.

| Check | Actual result |
|---|---|
| Baseline `cargo +stable check --locked` | PASS, default features including MilkDrop, 4m32s |
| Baseline `cargo +stable test --locked --all-targets` | PASS, 1,360 passed, 3 ignored, 20 target summaries, build 2m37s |
| Baseline demo under Xvfb | PASS, 1280x800 Home view inspected; no Spotify session or personal credentials used |
| Local dependency `cargo +stable check --offline` | PASS, only adds libresonic 0.1.0 to the existing lockfile, 8.84s |
| Scaffold fmt, clippy default/all features, tests default/all features, doctests and docs | PASS; default tests 1,360 passed/3 ignored; all features 1,401 passed/3 ignored; clippy/rustdoc warnings denied; all Cargo test commands used --locked |
| Dependency/evidence/settings policy, 16 policy regression tests, 4 launcher tests | PASS (launcher failures from missing Ruby resolved by installing prerequisite) |
| Libresonic fmt/clippy/test/doc | PASS; empty crate has zero behavioral tests, no protocol/playback implemented |
| Fresh Navidrome `make test` | PASS, isolated 264-song fixture, including initial scan wait and guaranteed teardown |
| Python syntax and Compose config | PASS |
| Rust 1.98 MSRV, macOS, Windows, native audio, native credential round trip | NOT RUN; no credential/audio code changed |
| Nix package and Jekyll site build | NOT RUN; nix and bundle absent; dependency source assembly and vendor hash remain packaging work |

Baseline demo command (build completed in 52.45s, process exited successfully):

```sh
CARGO_HOME="$PWD/.cache/cargo" env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 \
  xvfb-run -a cargo +stable run --locked --features demo -- \
  --demo --demo-data .cache/demo --demo-shot .cache/validation/baseline.png \
  --demo-size 1280x800
```

The inspected screenshot was 588,877 bytes, SHA-256
`815397b70d37e88763d5e5172a178f00af697945f13d6ef14cee4e1adbd63ead`.
It shows the inherited dark Home, sidebar and player. Xvfb emitted XSETTINGS/
XRandR warnings, and Mesa disabled its unwritable user shader cache; rendering
still succeeded. This is a functional demo smoke, not a startup/memory benchmark
or full visual regression suite. No UI source or layout changed in this phase.

The first Navidrome smoke reached authenticated ping before browsing was ready.
The harness now waits for getScanStatus to show an idle scan of at least 264
songs. A fresh database rerun passed; containers, network and volume were removed.
The copied tones remain ignored for reuse. Requesting media bytes proves the
fixture, not decoding, seeking, playback or Rust transport correctness.

Local command logs are under ignored `.cache/validation/`. The temporary client
source review checkout and demo profile were removed after recording results.

See [ADR 001](adr-001-boundaries.md) and [server matrix](server-matrix.md).

```text
Phase: 0 / repository audit and fork foundations
Baseline commit: 9c923873b9c3893d791922e3a66151b71d41e7f3
Changes: fork policy/README; architecture/coupling audit; unpublished independent
         libresonic crate and app path dependency; isolated Navidrome fixture.
Tests run: baseline check/tests/demo; scaffold check/fmt/clippy/tests/doc;
           policy/launcher checks; library checks; fresh fixture make test.
           All executed final checks passed. See table for explicit skips.
Manual evidence: Linux x86_64, inspected Xvfb Home demo at 1280x800;
                 Navidrome 0.64.0 (1072e9f7) synthetic-library fixture only.
Open issues: MSRV 1.98 and other OSes unverified; no Rust protocol/playback;
             Nix/single-checkout CI cannot yet acquire the separate library;
             inherited executable/desktop/updater/release identities remain.
Decision log: adr-001-boundaries.md (local repository override, ownership,
              provisional opensubsonic 0.6.1 wrapper, PCM proposal).
Exit gate: PASS for Phase 0: the Linux fork builds, baseline tests and demo
           work, and UI -> backend -> player -> processing -> sink is mapped.
           This does not pass any protocol, playback or release gate.
Next smallest task: Phase 1 neutral server-scoped IDs/models/errors and tested
                    OpenSubsonic facade, first ping/capabilities, then browsing.
```
