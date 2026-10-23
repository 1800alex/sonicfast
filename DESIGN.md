# Sonicfast / Libresonic

**Technical Design Specification and Agent Implementation Plan**  
**Version:** 1.0 (implementation draft)  
**Date:** 2026-10-21  
**Status:** Phase 1 complete on Linux/Navidrome with recorded limits; see `docs/design/phase-1.md`
**Primary reference server:** Navidrome  
**Protocol target:** OpenSubsonic / compatible Subsonic implementations  
**Source baseline reviewed:** Spotifast `main`, `Cargo.toml` version 0.14.1, as viewed 2026-10-21. **Pin a commit SHA before any edits.**

**Recorded implementation decision (2026-10-21):** Fork baseline
`9c923873b9c3893d791922e3a66151b71d41e7f3`; upstream source parent
`e86d3de0cb72edba8f24b447218daa7f3af1f127`. The owner explicitly requested
Libresonic in a separate repository. This overrides the original in-tree
workspace and Phase 6 extraction timing below. Following public preparation,
Sonicfast now consumes published Libresonic 0.1.0 crates from crates.io, with no
checkout location requirement. Public source: https://github.com/1800alex/libresonic. See `docs/design/adr-001-boundaries.md`.

**Authorized extension (2026-10-23):** The owner requested v0.15.0 Snapcast
sending and a built-in receiver, with Docker integration and manual smoke
fixtures. ADR 011 defines this bounded Phase 7 audio-transport work. Other
casting and remote queue control remain deferred. See
`docs/design/adr-011-snapcast.md` and `docs/design/snapcast.md`.

> **Agent handoff:** This document is the product and technical brief for an intentional fork of Spotifast. Read it in full, then execute one gated phase at a time. Do not assume examples in this document are already implemented. Do not replace the interface, rewrite the audio device stack, or start remote synchronization work during the MVP.

## 1. Executive summary

Build **Sonicfast**, a native Linux/macOS/Windows desktop music client that preserves the look and feel of [Spotifast](https://github.com/crmne/spotifast) while connecting to **OpenSubsonic** servers such as Navidrome. Build **Libresonic** as a reusable, server-independent Rust music-domain and playback layer developed *inside the Sonicfast workspace at first*; extract it only after real integration has stabilized its interfaces.

The user priority is **the Spotifast interface**: smooth browsing, queue control, native playback, EQ, visualizers, mini-player and desktop integration. Casting and tightly synchronized multiroom audio are valuable **later**, not required for the initial release.

**Architectural decision:** Do **not** fork and globally rewrite librespot. Spotify-specific protocol/session/Connect code is extensive. Instead, replace only the boundaries Sonicfast needs with an OpenSubsonic adapter, a local player and a thin compatibility adapter for existing Spotifast UI commands and state. Adapt/reuse the existing `src/sink.rs` audio-output path after decoupling it from librespot packet and mixer traits.

**MVP success statement:** A user can configure an OpenSubsonic server, browse and search its collection, open an album, play/pause/seek MP3 and FLAC, manage a local queue, and retain the recognizable Spotifast playback UI. Navidrome and at least one *different* OpenSubsonic-compatible server pass the core integration tests.

### 1.1 Decisions already made

| Decision | Resolution |
|---|---|
| Product | Sonicfast: Spotifast-derived UI for OpenSubsonic |
| Reusable engine | Libresonic: independent domain, protocol and playback modules |
| Protocol | OpenSubsonic first; legacy Subsonic fallback as capabilities permit |
| First test server | Navidrome, not an exclusive target |
| Playback | Local HTTP stream -> decoder -> reusable PCM path -> Spotifast sink |
| Frontend | Preserve egui/fastframe shell, visuals and layout wherever possible |
| Development model | One Rust workspace initially; extract stable crates later |
| Casting / multiroom | Explicitly deferred; separate control plane from synchronization |
| Synchronization later | Evaluate Snapcast rather than invent a clock/audio sync protocol |

## 2. Goals, constraints and non-goals

### 2.1 Required outcomes

1. Preserve recognizable Spotifast navigation, layouts, player controls, responsiveness, themes, EQ and visualizer architecture.
2. Support standards-based OpenSubsonic login, discovery, music/library operations and audio streaming without Navidrome-only assumptions.
3. Keep OpenSubsonic transport objects out of UI models and player command/state types.
4. Reuse the existing native audio sink/device management where technically practical. Avoid gratuitous rewrites.
5. Keep all network/decoding work off the UI thread. Emit state changes through the existing async/channel/repaint pattern.
6. Produce a maintainable codebase, cross-platform builds, reproducible tests and clear progress gates.
7. Design player control boundaries so a separate remote playback coordinator can be added without redesigning the UI.

### 2.2 MVP (release 0.1) in scope

- Configure and securely store server connection, auth method and client identity.
- `ping`/health check; server capability detection and graceful fallback.
- Album, artist, track browsing; library search; album cover art.
- MP3 and FLAC HTTP playback, play/pause/stop/seek, position, volume and output device handling.
- Track/album/playlist context loading and an **application-owned** local playback queue; next, previous, shuffle, repeat.
- Error states for authentication failure, unavailable server, unsupported media, buffering and device loss.
- Playlists and favorites where supported by the server; scrobbling added after stable playback.
- Existing desktop playback controls where already supported by Spotifast.

### 2.3 Explicitly out of scope for MVP

- Spotify authentication, Spotify Connect, Spotify social graph and Spotify recommendation parity.
- Distributed playback, device discovery, speaker groups, synchronized multiroom/casting, or a central Docker coordinator.
- Universal codec support, offline downloads, local file indexing, podcasts/audiobooks or every niche OpenSubsonic extension.
- Implementing a new HTTP client if an existing maintained Rust OpenSubsonic client meets the needs.
- A UI redesign, browser-based frontend, hosted service or telemetry platform.
- Guaranteeing gapless playback until the pipeline demonstrates correct track transitions and timestamps.

## 3. Verified source baseline and migration surface

The reviewed Spotifast `Cargo.toml` reports a Rust 2024 package (`rust-version = 1.98`, version 0.14.1). It uses egui/eframe, fastframe modules, a pinned `crmne/librespot` fork, `rodio`, `fastframe-audio`, `reqwest` and Symphonia with MP3/FLAC features. Treat these as **baseline observations**, not pinned future dependencies. Read the actual checkout at the Phase 0 commit.

| Current file / area | Confirmed role | Migration guidance |
|---|---|---|
| `src/player.rs` | librespot session, player, Connect/Spirc, `PlayerCommand`, `LocalState` | Map existing command/state contract; replace engine implementation, not whole UI |
| `src/sink.rs` | custom output path using fastframe-audio + rodio; imports librespot sink, packet, mixer and events | Extract PCM-facing boundary; preserve device recovery, fades, resampling and visualizer taps |
| `src/backend.rs` | very large UI/async bridge, Spotify auth/API/playback orchestration | Migrate behavior in vertical slices; retain channels/event-driven UI |
| `src/api/` | Spotify API transport and Spotify-specific models | Add separate OpenSubsonic adapter; migrate UI to neutral models |
| `src/auth.rs`, credentials/settings | Spotify tokens and existing secure-store conventions | New server profile/auth flow; do not repurpose old Spotify secrets |
| `src/app.rs`, `src/ui/`, `src/model.rs` | UI state/views/actions and Spotify-oriented models | Preserve visual appearance; replace service assumptions incrementally |
| `AGENTS.md`, `CONTRIBUTING.md` | Upstream policies for a Spotify-only app | **Replace/override upstream agent constraints in this fork** with Sonicfast policy |

**Special agent warning:** Upstream Spotifast's `AGENTS.md` says not to add alternate music sources; this is incompatible with the purpose of this intentional fork. At Phase 0, author a fork-specific `AGENTS.md` and examine inherited automation/checks for contradictory requirements. Preserve useful practices (nonblocking UI, safe credential storage, regression tests, platform discipline, accurate documentation), but do **not** blindly follow upstream Spotify-only product restrictions.

`PlayerCommand::Transfer`, `LocalState::active_client`, Spotify URI handling and existing queue semantics cannot be copied unchanged into a local-only OpenSubsonic player. Preserve the user-facing *behavior* when applicable, not Spotify's underlying Connect semantics. Some compatibility types may remain temporarily behind an adapter.

## 4. Target architecture

```text
                          SONICFAST (existing egui UI)
                     views / actions / player display
                                      |
                         UI application adapter
                       commands + domain view models
                         /                    \
                        v                      v
             LIBRESONIC-CORE              Playback state
           IDs, models, errors,               + queue
           traits, capability map                |
                        |                        v
                        v                 LIBRESONIC-PLAYBACK
          LIBRESONIC-OPENSUBSONIC          HTTP streams, decoder,
          API, auth, pagination,          buffering, transport
          artwork, scrobble                     |
                        |                        v
                        |                 PCM / audio output
                        |                        adapter
                        v                        |
            OpenSubsonic server                   v
           Navidrome, Gonic, etc.      Spotifast sink / EQ / tap /
                                       fastframe audio device

     FUTURE ONLY: headless playback + coordinator + Snapcast integration
```

### 4.1 Workspace layout (proposed, not current)

```text
sonicfast/                           # fork of Spotifast; root app stays buildable
  Cargo.toml                         # workspace root + Sonicfast app package
  AGENTS.md                          # new fork-specific agent contract
  src/                               # retained UI and integration code
    player_adapter.rs                # transitional compatibility layer
    sink.rs                          # retained/refactored native sink
    ...
  crates/
    libresonic-core/                 # neutral IDs, view models, service traits
    libresonic-opensubsonic/         # client adapter and capability negotiation
    libresonic-playback/             # queue, stream, decoder, player state/events
  tests/
    fixtures/                        # sanitized protocol fixtures and short media
    integration/                     # mock-server tests / opt-in live tests
  docs/
    design/                          # decisions, status, protocol matrix
```

Keep the app at the root initially if that minimizes Cargo churn. Avoid preemptively splitting into extra crates (especially a generic audio crate) before the sink proof of concept proves the real boundary.

### 4.2 Layer ownership rules

- **Core** owns `ServerId`, `MediaId`, `Track`, `Album`, `Artist`, `Playlist`, `CapabilitySet`, request/response agnostic errors and traits. Identifiers are scoped by server; never assume IDs are globally unique or Spotify URI-like.
- **OpenSubsonic adapter** owns REST transport, auth, typed response mapping, pagination, capability checks and server-specific fallbacks. All public outputs are neutral domain models.
- **Playback** owns transport commands, current item, local queue ordering, buffering/cancellation, seek, repeat/shuffle, player events and stream/decode lifecycle. No egui dependency.
- **Sonicfast app** owns navigation, UI-specific caches, optimistic view updates, art textures, settings UX, keyboard/media keys, output-device selection, EQ and visualizer wiring.
- **Future connect** owns remote sessions/coordinator protocols, never the local decoder API. Remote control and sample-accurate synchronization are separate components.

### 4.3 Interface sketches (illustrative, not compilation-ready)

```rust
struct ServerId(String);
struct MediaId { server: ServerId, item: String }
struct Track { id: MediaId, title: String, duration_ms: Option<u64> /* ... */ }

enum PlaybackCommand {
    Load { tracks: Vec<MediaId>, start: usize, play: bool },
    Toggle, Play, Pause, Stop, Next, Previous,
    SeekMs(u64), SetVolume(u16), Queue(MediaId), ClearQueue,
    SetShuffle(bool), SetRepeat(RepeatMode),
}

// Object safety, error types, async details and pagination are to be
// finalized in the Phase 0/1 ADRs, not inferred from this sketch.
trait MusicService { /* ping, capabilities, albums, artists, search,
                        playlists, favorites, media request, scrobble */ }
trait AudioOutput { /* accept normalized PCM frames + control lifecycle */ }
```

**Compatibility adapter:** At first, translate Spotifast's current `PlayerCommand` and `LocalState` into these neutral types while keeping `src/app.rs` call sites mostly intact. Then eliminate leftover Spotify-only fields explicitly. Mark `Transfer` as unavailable until remote control is designed; it must not silently imitate Spotify Connect.

## 5. OpenSubsonic protocol strategy

### 5.1 Client implementation decision

**Implementation decision (2026-10-21):** Phase 0 evaluated `opensubsonic` 0.6.1.
Phase 1 rejected its transport/dependency after proving that response reads
cannot be bounded through its public hooks, and that its unconditional TLS
features introduce a second crypto provider into the retained engine. The
facade uses existing reqwest 0.12/Rustls dependencies with a narrow endpoint
adapter instead. See `docs/design/adr-002-protocol-facade.md` for evidence,
contract and tests. The original evaluation proposal follows for context.

Evaluate the async Rust [`opensubsonic` crate](https://docs.rs/opensubsonic/latest/opensubsonic/) first. At review time, docs.rs exposes version **0.6.0**, including API-key, salt/token authentication, streaming URLs and a broad set of endpoints. Before committing, verify its license, maintained versions, transport hooks, timeouts, request signing, response handling and streaming needs. Wrap it behind **our** `libresonic-opensubsonic` facade to retain replacement flexibility.

Do not confuse API transport completeness with server feature support. A method existing in a client crate does not imply a given server implements it.

### 5.2 Authentication and connection

1. User enters server base URL and chooses an auth mode supported by that server: API key, or username/password with Subsonic salt/token authentication.
2. Prefer TLS; validate certificates by default. Allow explicitly acknowledged plaintext HTTP only for a local/self-hosted environment if truly required.
3. Test the provided method with `ping`; identify server/version. Request `getOpenSubsonicExtensions` when available. If it returns unsupported, continue with baseline Subsonic support rather than aborting.
4. Store secret material using the existing native protected-credential approach. Never write secrets to ordinary UI state, logs, bug-report bundles or artwork cache keys.
5. When an extension such as `apiKeyAuthentication` is advertised, enable it for appropriately configured credentials. **Do not assume a capability can be discovered before authenticating.** Do not forcibly downgrade credentials on failure.
6. Mask/redact **whole authenticated streaming/artwork URLs** when logging; OpenSubsonic authentication may use query parameters. Prevent authenticated redirects to untrusted hosts.

### 5.3 Required endpoint mapping

| Feature | Primary endpoint / approach | Capability/fallback note |
|---|---|---|
| Connection | `ping`, `getOpenSubsonicExtensions` | Extension discovery optional on legacy servers |
| Artists / albums | `getArtists`, `getArtist`, `getAlbumList2`, `getAlbum` | Normalize missing metadata and paging |
| Search | `search3` | Handle response differences and pagination |
| Track / cover | `getSong`, `getCoverArt` | Authenticated bytes passed to app image loader |
| Audio | `stream` | Direct play first; optional transcode handling later |
| Playlists | `getPlaylists`, `getPlaylist`, `createPlaylist`, `updatePlaylist`, `deletePlaylist` | Distinguish readonly/inaccessible playlists |
| Favorites | `getStarred2`, `star`, `unstar` | Treat mutation errors explicitly |
| Listening history | `scrobble` | Debounce/retry safely; no duplicate plays |
| Lyrics (post-MVP) | `getLyricsBySongId` | Only when supported; fallback/hidden UX |
| Playback reporting (post-MVP) | `reportPlayback` | Negotiate extension and semantic version |
| Server-side queue (post-MVP) | `savePlayQueue` / index-based extension | Never assume shared playback state |
| Transcoding (post-MVP) | `getTranscodeDecision`, `getTranscodeStream`, `stream` options | Feature negotiation; do not hard-code Navidrome |

### 5.4 Capability handling

Maintain a `CapabilitySet` with: server name/version; auth mechanism in use; discovered extension names+versions; core operation availability; and runtime feature failures. Feature-flag UI affordances from server evidence. An absent extension does **not** necessarily disable baseline Subsonic functionality. Prefer a server-generic path, then optional extension behavior. Record confirmed differences in `docs/design/server-matrix.md`.

Use **Navidrome** for the first manual integration, then **Gonic or another distinct OpenSubsonic server** for mandatory portability validation. Test a mock legacy server with no OpenSubsonic extension endpoint.

## 6. Streaming, playback and audio integration

### 6.1 End-to-end pipeline

```text
Track ID -> authenticated OpenSubsonic media request -> streaming HTTP body
  -> bounded input buffer -> Symphonia demux/decode (MP3, FLAC first)
  -> normalized interleaved PCM -> format conversion / resampling / EQ
  -> existing audio sink and its visualization tap -> fastframe audio device
```

`src/sink.rs` currently implements librespot's `Sink` and consumes librespot audio packet / mixer types. The first audio task is to introduce a **PCM-facing adapter**, preserving existing device restart behavior, volume ramps, fade logic, EQ tap placement and queue management where valid. Extract only the minimum helpers needed; do not rewrite everything before a sample can play.

### 6.2 Player state machine

States: `Stopped -> Loading -> Buffering -> Playing <-> Paused`, with `Seeking`, `Ended` and recoverable `Error` as explicit events/substates. Every command must define transitions during loading, seeking, device loss and track changes. State reports must reflect local intent immediately while stale backend events cannot undo newer UI actions. Use generation/sequence IDs for loads and seeks; cancel abandoned HTTP/decode tasks.

### 6.3 Playback requirements

- **Phase 2:** MP3 and FLAC play/pause/stop, volume, seek, output selection and position update; one track at a time.
- **Phase 3:** deterministic local queue, context album/playlist load, previous/next, shuffle, repeat and restart/persistence policy.
- **Phase 4:** network drop/retry, seek correctness with byte-range and/or API offset when supported, buffer bounds, sample-rate/channel changes, edge-case formats, scrobbling and lyric/art restoration.
- **Later:** gapless, normalization, more codecs, prefetch, crossfade and transcoding optimizations as independent features.

**Important:** HTTP byte-range support and track duration seeking vary by media container and server. Implement a measured strategy (decoder seek on buffered/seekable media or new authenticated stream with supported offset); never promise precise seek behavior based on URL construction alone.

## 7. UI preservation and data migration

**Preserve:** window chrome, navigation, search layout, library views, album/playlist views, now-playing footer, queue, mini-player, theming, EQ, MilkDrop/spectrum/oscilloscope, tray and media keys to the extent these features are not inherently Spotify-bound.

**Replace/adapt:** Spotify login with server profile; Spotify URIs with server-scoped IDs; Spotify's Home/discovery shelves with recently added/random/recently played/favorites where available; Connect device picker with local audio devices only; Spotify-only social, podcasts or recommendation sections with honest omissions or suitable generic equivalents.

The existing UI uses Spotify-oriented model objects and some player fields. Build mappers in the app adapter. Remove misleading labels and disabled features rather than leaving controls that appear to work. Artwork must use authenticated client fetching and sanitized cache keys, not an exposed URL containing credentials. Preserve fast async rendering via channels and repaint requests.

**Branding/licensing:** Rename package/binary/product/assets, audit icons/screenshots/names/trademarks, and retain license notices for reused MIT code and other dependencies. Check dependency licenses before release. A fork is not a maintenance branch of upstream; do not edit upstream repository policy to justify an upstream PR.

## 8. Delivery roadmap: gated phases

Estimates below are **planning ranges for a capable Rust developer with AI assistance**, not commitments. Phases may overlap after their prerequisites are proven. The first playable end-to-end slice is more important than broad but untested implementation.

### Phase 0 - Repository audit and fork foundations (2-4 working days)

**Goal:** Establish a clean, reproducible starting point and freeze the real interfaces.

**Tasks:**
- Fork Spotifast, pin and record upstream commit SHA, Rust toolchain and dependency revisions.
- Read `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `src/player.rs`, `src/sink.rs`, `src/backend.rs`, `src/app.rs`, `src/api/`, relevant queue docs/tests and current settings/credentials code.
- Inventory all `librespot_*`, Spotify URI and Spotify API references, separating UI-only display dependencies from protocol and playback dependencies.
- Verify baseline `cargo check`, test suite and demo run on the primary development OS. Log failures before making changes.
- Replace upstream `AGENTS.md` with Sonicfast-specific rules; rename product minimally; add `docs/design/adr-001-boundaries.md` and `server-matrix.md`.
- Evaluate `opensubsonic` crate 0.6.x and decide use/wrap vs alternate adapter; document rationale.

**Deliverables:** reproducible build notes, dependency/ownership map, source SHA, migration checklist, first ADR, fork agent guide.

**Exit gate:** unmodified baseline behavior documented; fork builds on at least one OS; agent can point to exact UI -> backend -> player -> sink call path. **Do not perform a broad rewrite in this phase.**

### Phase 1 - OpenSubsonic core and server connection (3-5 working days)

**Goal:** Connect and browse data with a standalone, testable API layer.

**Tasks:**
- Add `libresonic-core` domain models and `libresonic-opensubsonic` adapter.
- Implement base URL normalization, auth selections, `ping`, optional extension discovery, typed failures, pagination and cancellation.
- Implement `getArtists`, `getAlbum`, `getAlbumList2`, `search3`, `getSong`, `getCoverArt`; test response mapping including sparse metadata.
- Build CLI/test harness or temporary screen that lists artists/albums and fetches cover bytes. Build tests using fake server responses and opt-in live Navidrome credentials.

**Deliverables:** independent API client layer and protocol tests; minimal server connection workflow.

**Exit gate:** can connect to Navidrome, fetch albums/artwork/search and handle invalid credentials and unsupported extension discovery without crashing; no secrets leak into logs.

### Phase 2 - Playback proof of concept (5-10 working days; critical path)

**Goal:** Demonstrate real Navidrome FLAC/MP3 through Spotifast's output path **without a librespot session**.

**Tasks:**
- Isolate a PCM input interface from `src/sink.rs`, preserving native device lifecycle, EQ/visualizer hooks and error delivery.
- Implement authenticated stream retrieval, bounded buffering, MP3/FLAC decoding, stop/cancel, position and one-track transport commands.
- Build a debug harness that selects a known track ID and plays it through the existing sink. Add pause, resume, volume, seek and selectable output.
- Instrument startup delay, buffers, decoding/device errors and cancellation safely (no URL or token logs).
- Validate on Linux and at least one second OS where feasible; record real test coverage.

**Deliverables:** reusable playback crate, PCM adapter, sample app/test mode, real audio playback evidence.

**Exit gate:** a FLAC and MP3 play, pause, resume, seek and stop through Spotifast's native audio sink; no librespot session is created; an unavailable device produces a recoverable error. **Stop here and resolve if this fails.**

### Phase 3 - Sonicfast MVP UI integration (5-10 working days)

**Goal:** Make the main interface usable for everyday local playback.

**Tasks:**
- Replace the sign-in/config flow with server profile UX; persist credentials in protected storage.
- Add a UI/backend adapter mapping neutral tracks and player events into existing view models.
- Wire library, artist, album, search, artwork and now-playing/queue views.
- Implement application-owned queue/context load, next/previous, repeat/shuffle and local resume policy.
- Remove or honestly disable Connect/Spotify-specific controls without redesigning unaffected screens.
- Keep rendering event driven; add user-visible empty, loading, auth-failure and offline states.

**Deliverables:** daily-usable Sonicfast alpha, end-to-end happy-path tests and UI screenshot comparisons.

**Exit gate (MVP alpha):** install/connect/browse/search/play/pause/seek/queue flows work end-to-end on Navidrome; interface remains recognizably Spotifast; no action blocks the UI thread.

### Phase 4 - Library and experience parity (5-10 working days)

**Goal:** Restore priority features and correct common edge cases.

**Tasks:**
- Add playlist create/update/delete and favorite star/unstar with optimistic UI and rollback on errors.
- Implement scrobble start/completion policy, idempotent retries and duplicate-prevention tests.
- Restore compatible visualization, EQ, mini-player, media keys, tray, album-art cache and remembered device behavior.
- Handle empty library, unavailable tracks, permission limitations, duplicate titles/IDs across servers and metadata without artwork.
- Add server feature flags for lyrics or advanced discovery only when advertised and verified.

**Deliverables:** broader-feature beta and documented feature compatibility matrix.

**Exit gate:** core user workflows and preserved audio visualizations work; the remaining omissions are documented, not implied to function.

### Phase 5 - Reliability, portability and release candidate (5-10 working days)

**Goal:** Make a dependable cross-platform release and prove OpenSubsonic portability.

**Tasks:**
- Exercise slow/broken HTTP, cancelled playback, repeated seek, unavailable audio device, long session, server reconnect and auth rotation.
- Test Navidrome **and a second distinct OpenSubsonic server** (e.g. Gonic) with a shared fixture library; include mock legacy Subsonic behavior.
- Run formatting, lint, unit, adapter integration and platform build tests; conduct manual smoke tests on Linux, Windows and macOS as hardware/CI permit.
- Review user-visible branding, protected storage, authenticated URL redaction, HTTP fallback warnings, licensing and package metadata.
- Write README, server compatibility notes, setup guide, troubleshooting and changelog. Quantify actual startup/memory/streaming behavior versus baseline where possible.

**Deliverables:** tagged 0.1 release candidate, CI evidence, reproducible packaging and known-issues list.

**Exit gate (0.1 release):** all mandatory end-to-end cases pass, at least two servers verified, no critical security/streaming defects, and supported-platform claims match actual tests.

### Phase 6 - Libresonic extraction and API stabilization (3-7 working days; after release candidate)

**Goal:** Publish/use stable reusable library surfaces rather than a Spotifast-specific implementation.

**Tasks:** isolate UI-specific dependencies, document stable domain/player traits, add public examples and semver policy, define feature flags, and extract into a standalone repository only if library tests and Sonicfast integration no longer require private app internals.

**Exit gate:** stand-alone crate compiles/tests with no egui/Spotifast dependency; Sonicfast consumes it as an ordinary dependency; license and release process documented.

### Phase 7 - Optional remote playback / casting (separate project; not MVP)

**Goal:** Control remote players and optionally synchronize audio across devices.

**Track A - Remote control:** introduce device/session identity, coordinator API, shared queue/versioning and authenticated command/event exchange. Local and headless Libresonic instances implement the same command contract. This is Spotify-Connect-like *control*, not synchronized playback.

**Track B - Multiroom synchronization:** evaluate **Snapcast** with a Docker-hosted coordinator/audio source. The coordinator owns sessions and track choice; Snapcast handles stream distribution and clock synchronization. Do not assume a Docker container by itself can align output clocks. Document latency, grouping, drift and network failure tests before making sync claims.

**Exit gate:** separate design approval and proofs for control and sample-synchronous playback. No Phase 7 work can block 0.1.

### 8.1 Rough critical-path schedule

| Milestone | Main phases | Indicative effort |
|---|---|---|
| Audio + protocol feasibility | 0-2 | ~2-4 weeks |
| Playable desktop MVP alpha | 0-3 | ~3-6 weeks total |
| Polished 0.1 release candidate | 0-5 | ~6-10+ weeks total |
| Standalone reusable library | 6 | Additional ~1-2 weeks |
| Casting / synchronized audio | 7 | Separate estimate after prototype |

## 9. Test and acceptance strategy

### 9.1 Minimum automated test pyramid

- **Unit:** ID namespacing, sparse metadata mapping, shuffle/repeat ordering, queue invariants, stale event suppression, seek/load sequence behavior, auth URL redaction, capability map and serialization.
- **Protocol integration (mock):** successful calls, token/key auth, bad credential error, 404 unsupported extension, 429/5xx, pagination, invalid JSON/optional fields, image byte fetch, redirect leakage prevention.
- **Audio integration:** decode fixture MP3/FLAC, finite buffers, cancellation, end-of-stream, sample-rate changes, time/position, seek methods, no-output-device errors; audio hardware tests marked manual or gated.
- **Live opt-in:** containerized Navidrome + fixture album and an independent second server; never commit real passwords, API keys or personal music.
- **UI regression:** demo/mock server states and before/after screenshots at representative window sizes and both themes; tests for server prompt, library empty/loading/error and queue transitions.

### 9.2 Release acceptance scenarios (Given / When / Then)

1. **Connection:** Given valid Navidrome credentials, when a user connects, then the client shows the server library and stores credentials outside ordinary state JSON.
2. **Legacy server:** Given no `getOpenSubsonicExtensions`, when login succeeds, then supported baseline operations remain usable and unavailable features are hidden or disabled.
3. **Browsing:** Given albums with missing art/duration/artist metadata, when browsed, then the UI does not panic and shows sensible fallbacks.
4. **Playback:** Given valid MP3 and FLAC items, when played, paused and sought, then sound and UI position respond predictably and no Spotify session is initialized.
5. **Queue:** Given an album and manually queued tracks, when shuffled/repeated/skipped, then queue ordering follows documented rules and no silent duplicate inserts occur.
6. **Recovery:** Given an unreachable server or removed output device mid-track, when failure occurs, then Sonicfast remains responsive, clearly reports failure and can recover.
7. **Secrets:** Given API-key-authenticated streams, when logs, crash artifacts and cache indices are inspected, then no usable secret, auth URL or password is present.
8. **Portability:** Given the same test fixture library on Navidrome and a second server, when core browse/play workflows run, then both pass without server-name conditionals in UI/player code.
9. **Visual parity:** Given baseline screenshots, when representative screens are compared, then any unintended layout changes are corrected or explicitly documented.
10. **Platform truthfulness:** Given a release, when its supported-OS matrix is published, then each claim distinguishes built, automated-tested and manually-played results.

### 9.3 Nonfunctional principles

The UI must never synchronously wait on HTTP, decode or blocking disk operations. Keep memory/buffers bounded and audio work separated from widget rendering. Preserve fast startup and low resource use, but set numeric performance budgets **after collecting Phase 0 baseline measurements**; do not invent SLA claims from upstream marketing. Error and debug telemetry should be local and secret-safe by default.

## 10. Key risks and mitigations

| Risk | Severity | Mitigation / stop condition |
|---|---|---|
| Massive Spotify coupling in `src/backend.rs` and app models | High | Vertical slices with transition adapter; forbid one-shot backend rewrite |
| librespot types embedded in custom sink and EQ/vis chain | High | Phase 2 PCM adapter spike is mandatory stop/go gate |
| Inconsistent server implementations and auth | High | Capability negotiation, protocol fixtures and two-server release gate |
| Auth secrets in stream/art URLs and crash logs | High | Protected storage, URL redaction, redirect policy, leak tests |
| Seek and buffering differences across codecs/servers | Medium-high | Explicit seek strategy, stream cancellation and fixture coverage |
| Queue semantics inherited from Spotify Connect | Medium-high | New documented local queue semantics, deterministic tests |
| UI behavior drift and visual regression | Medium | Preserve widgets, compare screenshots, avoid incidental redesign |
| Patched git dependencies and Rust toolchain churn | Medium | Pin baseline, change one dependency family at a time, record revisions |
| Scope creep into casting, multiroom or full protocol parity | High | Phase 7 is separately approved and has no MVP dependencies |
| Branding, third-party license or upstream-policy conflicts | Medium | Fork-specific policies, dependency/license review before release |

## 11. Agent execution contract

1. **Phase gate discipline:** Complete Phase 0 first. Work on the smallest shippable vertical slice. Do not undertake later-phase features to “clean things up.” Every phase ends with a concise report and explicit pass/fail evidence against its exit gate.
2. **Inspect before changing:** All file paths, method names and dependencies in this document are leads. Verify them in the pinned checkout before editing. If upstream has changed, write an ADR and adjust the plan rather than guessing.
3. **One source of truth:** Domain models and player state live in Libresonic; network DTOs live in the protocol adapter; UI-only data remains in Sonicfast. No `SpotifyUri` or raw OpenSubsonic DTOs in persistent core models.
4. **No stealth substitutions:** Do not pretend Spotify Connect transfer exists, fabricate playlist capabilities, or remove a visualization without documenting why. Fail visibly and honestly.
5. **Preserve existing UX:** Avoid changing colors, navigation, layout, animations or keyboard semantics unless required by removing Spotify-only features. Add regression evidence for changes.
6. **Security by default:** Do not log secrets; never include credentials in source or tests. Treat every authenticated URL as a secret. Keep network activity off the UI thread.
7. **Verification:** Each patch includes test evidence (`cargo fmt`, `cargo clippy`, focused `cargo test`, relevant smoke test); report a skipped check as skipped, never as passed.
8. **Documentation:** Update the feature matrix, ADRs and phase checklist with each behavior change. Keep current known limitations visible.
9. **Fork policy:** Replace upstream Spotify-only `AGENTS.md` with a Sonicfast-specific document before delegating to coding agents. Retain applicable quality/security instructions, not contradictory scope restrictions.
10. **Escalation:** If Phase 2 cannot reuse the sink without significant engineering, document two alternatives (minimal sink adapter vs dedicated `rodio`/cpal output), estimate impact, and pause architectural expansion until an explicit decision is recorded.

### 11.1 Initial agent backlog (recommended commit/PR boundaries)

1. `audit: pin baseline and map Spotify/librespot call sites` - Phase 0 report only.
2. `chore: fork branding, AGENTS.md and workspace scaffold` - baseline still launches.
3. `feat(core): neutral identifiers, models and errors` - unit tests.
4. `feat(opensubsonic): auth/ping/capabilities/library facade` - fixture and live tests.
5. `refactor(audio): introduce librespot-independent PCM input adapter` - sink tests, baseline visuals preserved.
6. `feat(playback): streamed MP3/FLAC single-track transport` - independent harness.
7. `feat(app): server connect and library browsing` - UI screenshot/flow tests.
8. `feat(app): playback/queue controller integration` - full vertical slice.
9. `feat(library): playlists, favorites and scrobbling` - feature matrix.
10. `test/release: cross-server and cross-platform validation` - release checklist.

### 11.2 Required phase report format

```text
Phase: <number/name>        Baseline commit: <SHA>
Changes: <key files / behavior>
Tests run: <commands and actual results>
Manual evidence: <OS, server/version, flows demonstrated>
Open issues: <blockers, regressions, known limitations>
Decision log: <ADR links>
Exit gate: PASS / FAIL (+ rationale)
Next smallest task: <specific vertical slice>
```

### 11.3 First task to issue to a coding agent

> **Execute Phase 0 only** against a pinned fork of `crmne/spotifast`. Produce a dependency and call-path audit for `src/player.rs`, `src/sink.rs`, `src/backend.rs`, the UI model/actions and the current auth flow; identify every `librespot`/Spotify-only type crossing a module boundary. Run and report baseline checks, propose the minimal PCM sink interface, evaluate `opensubsonic` 0.6.x, and draft a Sonicfast-specific `AGENTS.md`. Do not implement the playback rewrite or alter UI layout in this task. Return exact source commit, test output, scope of next PR and Phase 0 gate result.

## 12. Open decisions, defaults and change control

| Question | Current default | Decide by |
|---|---|---|
| App naming/trademark clearance | Working names “Sonicfast” and “Libresonic” | Before public release |
| Supported minimum Rust version | Match pinned Spotifast baseline initially | Phase 0 |
| OpenSubsonic Rust crate | Prefer `opensubsonic` 0.6.x behind own facade | Phase 0 |
| HTTP plaintext policy | Explicit local-only opt-in, otherwise HTTPS | Phase 1 |
| Multiple server profiles | One active server for 0.1; model IDs with namespace | Phase 1 |
| Queue persistence on app restart | Local, opt-in persistence if straightforward | Phase 3 |
| Scrobble completion rule | Define a documented single-emission threshold | Phase 4 |
| Gapless and transcode support | Post-0.1 unless easy and reliable | Phase 4 review |
| Extract Libresonic repository | After robust app usage; no early split | Phase 6 |
| Remote/casting protocol | Deferred and separately designed | Phase 7 |

**Change policy:** If an assumption fails, update an ADR and the phase plan before widening scope. Preserve the priority order: **(1) Spotifast UI, (2) reliable local OpenSubsonic playback, (3) library polish, (4) optional remote control, (5) synchronized multiroom.**

## 13. References and verification trail

Reviewed on 2026-10-21. Revalidate specific commit/versions at project kickoff.

- Spotifast repository and license: https://github.com/crmne/spotifast
- Spotifast package/dependencies: https://raw.githubusercontent.com/crmne/spotifast/main/Cargo.toml
- Spotifast player types and Connect implementation: https://raw.githubusercontent.com/crmne/spotifast/main/src/player.rs
- Spotifast custom sink: https://raw.githubusercontent.com/crmne/spotifast/main/src/sink.rs
- Spotifast UI/backend orchestration: https://raw.githubusercontent.com/crmne/spotifast/main/src/backend.rs
- Upstream agent guide (requires fork-specific replacement): https://raw.githubusercontent.com/crmne/spotifast/main/AGENTS.md
- OpenSubsonic API and endpoint index: https://opensubsonic.netlify.app/docs/
- OpenSubsonic API-key extension: https://opensubsonic.netlify.app/docs/extensions/apikeyauth/
- OpenSubsonic extension discovery: https://opensubsonic.netlify.app/docs/endpoints/getopensubsonicextensions/
- Async Rust OpenSubsonic client, version reviewed 0.6.0: https://docs.rs/opensubsonic/latest/opensubsonic/
- Navidrome: https://github.com/navidrome/navidrome
- Gonic: https://github.com/sentriz/gonic
- Snapcast (future synchronization option): https://github.com/badaix/snapcast

---

**Definition of project success:** Sonicfast is a native, polished Spotifast-derived OpenSubsonic client. Its core transport/player is cleanly reusable, supported by actual cross-server and cross-platform evidence, and does not depend on Spotify credentials, protocols or Connect services.
