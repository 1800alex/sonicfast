# Sonicfast agent guide

Read `DESIGN.md`, `CONTRIBUTING.md`, and `docs/design/phase-0.md` first.
Current protocol decisions are in `docs/design/adr-002-protocol-facade.md`.
Playback decisions/evidence are in `docs/design/adr-003-streamed-playback.md` and
`docs/design/phase-2.md`; the native proof uses `examples/subsonic_playback.rs`.
Desktop integration/queue/credential decisions are in ADR 004 and `docs/design/phase-3.md`.
M4A/optional byte-seek input decisions and evidence are in ADR 005 and `docs/design/m4a-playback.md`.
Server playlist decisions/evidence are in ADR 006 and `docs/design/playlists.md`.
Song hearts/ratings and LRCLIB lyrics decisions/evidence are in ADR 007 and
`docs/design/song-feedback-and-lyrics.md`.
Scrobbling and original-file download decisions/evidence are in ADR 008
and `docs/design/scrobbling-and-downloads.md`; the owner's download request extends
the original offline-download exclusion.
ADR 009 and `docs/design/filesystem-availability.md` supersede the persistent
download index with filesystem checks and unfinished work in existing app state.
ADR 010 and `docs/design/connections-and-sync.md` cover saved connections,
credential-preserving disconnect and catalogue-only Sync.
ADR 011 covers the owner's v0.15.0 Snapcast sender/receiver scope, superseding
the original deferral for this integration. Evidence is in `docs/design/snapcast.md`;
run `make snapcast-test` for changes to that boundary. General remote queue control stays deferred.
ADR 012 covers the owner's subsequent AirPlay Classic/Cast output authorization,
the shared output picker and explicitly certificate-pinned software Cast targets.
Run `make casting-test` for AirPlay changes; retain device-authentication and
temporary-media-lease tests for Cast changes. Do not claim Google SDK or physical
device compatibility from a software receiver test.
Run `sonicfast` for OpenSubsonic; `sonicfast-legacy` is the retained Spotify/demo comparison binary.
This intentional OpenSubsonic fork of Spotifast supersedes inherited Spotify-only policy.
Application/crate/packaging branding is Sonicfast. Preserve upstream attribution, real
source URLs, dependency pins and historical design evidence. Keep legacy Spotify
storage and IPC separate from Sonicfast server profiles.

- Execute one gated design phase at a time. Record actual results and failures
  before advancing. Do not redesign the UI or rewrite the audio device stack.
  Snapcast, AirPlay Classic and Cast audio outputs are authorized (ADRs 011/012);
  a general remote queue coordinator remains deferred.
- Preserve egui/fastframe, navigation, themes, EQ, visualizers and mini-player.
  Review intended visible changes with demo captures kept outside tracked files.
- Views in `src/ui/` emit `Action`s; apply them after drawing in `src/app.rs`.
  HTTP, decoding, credential-store calls and blocking disk writes stay off the
  UI thread. Retain channels, repaint notifications and stale-generation checks.
- Libresonic is developed independently at https://github.com/1800alex/libresonic
  and consumed from crates.io (currently 0.1.0). A separate checkout can live
  anywhere; ordinary Sonicfast builds fetch the published crates through Cargo.
  This supersedes the initial local-development arrangement in ADR 001.
- Libresonic owns server-scoped IDs, neutral models, protocol adaptation and
  playback/queue state. It has no egui, Spotify, librespot or app dependency.
  Keep wire DTOs and authenticated URLs inside its protocol adapter.
- Sonicfast owns native output devices, EQ and visualization wiring. The PCM
  proof includes `src/vis.rs` and `src/eq.rs` as well as `sink.rs`. Preserve
  device recovery, bounded queues, fades and post-EQ/pre-volume taps.
- Read `docs/_reference/queue.md` and nearby tests before queue changes.
  Preserve applicable user behavior, not Connect implementation. `Transfer`
  stays unavailable until remote playback has a separate approved design.
- Treat whole authenticated URLs as secrets. Never log credentials, grants,
  raw transport errors or arbitrary server response bodies. Disable redirects
  for authenticated requests. Test redaction at the library boundary.
- Validate TLS; local/self-hosted HTTP requires explicit opt-in. Never silently
  downgrade authentication or disable certificate verification.
- Store server secrets in native protected storage with a separate Sonicfast
  namespace. Do not implicitly reuse, migrate or delete Spotify grants.
  State JSON and artwork cache keys must not contain credentials.
- Prefer registry dependencies; retain inherited `crmne` pins during Phase 0.
  Libresonic is now published and consumed from crates.io. Explain other new
  dependencies/overrides. Preserve upstream license notices.
- Keep platform code cfg-gated and report built, tested and manually exercised
  platforms separately. Do not imply compatibility from available API methods.
- Keep changes focused. Do not commit or push unless asked. Add meaningful
  regression coverage and run checks from `CONTRIBUTING.md`; report skips.
- Update ADRs, phase reports and `docs/design/server-matrix.md` with evidence.
- Each checkout uses its own `target/`. Writable caches can use ignored
  `.cache/`. Never put build output in `/tmp`, share target directories, or
  run `cargo clean` to reclaim space.
- Release and Pages workflows target 1800alex/sonicfast. See PACKAGING.md.
  Sonicfast keeps self-updates disabled; the retained legacy updater key belongs
  to upstream and must not be used as the fork release trust root.
