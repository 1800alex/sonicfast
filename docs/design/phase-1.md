# Phase 1: independent OpenSubsonic connection and browsing

Date: 2026-10-21. Baseline fork commit remains
`9c923873b9c3893d791922e3a66151b71d41e7f3` (upstream source parent
`e86d3de0cb72edba8f24b447218daa7f3af1f127`). Phase 0 changes were already
present, including staged Sonicfast files. This work does not commit, push or
alter the index.

## Implemented boundary

The separate Libresonic repository is now a workspace with two implementation
crates and its existing root facade:

| Area | Implemented behavior |
|---|---|
| libresonic-core | Validated server/kind-scoped IDs, serializable neutral artist/album/track/playlist models, safe errors, bounded page requests, capability evidence, object-safe Send/Sync MusicService returning boxed Send futures |
| libresonic-opensubsonic | Config validation; in-memory token/API-key auth; authenticated ping then optional extensions; getArtists/getArtist/getAlbumList2/getAlbum/getSong/search3/getCoverArt; private DTOs mapped to core types |
| Root libresonic | Reexports core and protocol facade; no GUI or Spotify dependencies |
| examples/library.rs | Environment-based connection/browse harness, lists artists/albums, opens album/song, searches and fetches artwork bytes |
| tests/navidrome.rs | Opt-in live Rust service test against the generated fixture; normal Cargo runs ignore it |
| integration/navidrome | Existing Python readiness checks plus rust-smoke/test-rust with guaranteed fixture teardown |
| Sonicfast | Same local dependency path; lockfile adds the two local implementation crates. No application source, login or layout changes |

Identifiers include both server profile and entity kind. Constructors and
deserialization reject empty IDs; requests reject foreign-server/wrong-kind IDs
before HTTP. Optional metadata remains absent, missing/null display text gets
neutral fallbacks, null collection slots are tolerated and repeated track
occurrences retain order. Expanded artist credits may have a name without a
resolvable ID. Artwork is fetched by scoped identity as bytes, not as public URLs.

Album/search pages have explicit 1..500 sizes. Continuation uses the raw server
slot count before null-row filtering; no total is fabricated. Random album
selection offers no continuation because order is unstable. Search applies the
requested offset/size to each category and returns independent continuation hints.

## Transport and client decision

[ADR 002](adr-002-protocol-facade.md) replaces the provisional upstream client
dependency after inspecting its public body-reading hooks and feature graph.
The initial implementation using its signing/DTOs passed mocks/live tests, but
integration inspection found that its unconditional reqwest 0.13 Rustls feature
would add aws-lc-rs alongside the retained app's ring provider. Pinned librespot
uses an implicit HttpsConnectorBuilder provider path, which cannot select between
both without an application-wide default. That experiment was replaced before
the final app integration; no upstream source was copied or vendored.

The final facade uses existing reqwest 0.12, native certificate verification,
md-5 0.10, rand 0.9 and serde families. The app lockfile retains every external
package version and git revision; only local library package entries change.
This avoids a global crypto policy change or network-stack upgrade in the app.

- Default connect/whole-request deadlines: 5s/20s, including response bodies.
- Incremental collection limits: 4 MiB metadata, 16 MiB artwork. Content-Length
  and streamed bodies without it are both bounded; no unbounded byte collection.
- Redirects and automatic retries are disabled. Rate limits are returned with
  a safe Retry-After delay when expressed in seconds.
- HTTPS is required unless explicitly allowing self-hosted HTTP. Base userinfo,
  query and fragment are rejected. Reverse-proxy path prefixes are preserved.
- Requests use direct connections. Existing app proxy integration is deferred
  until the UI/backend migration; the current desktop behavior is unaffected.
- Token auth uses a fresh random 128-bit salt per request; API keys send no
  username or password. Failures do not downgrade to another mechanism.
- HTTP errors, parse failures and server errors become typed core errors with
  no raw URL, free-form server message or underlying credential-bearing source.
  Credentials and client Debug are redacted; credentials have no serialization.
- Optional discovery falls back only on explicit absence/unsupported results.
  Authentication, permission, generic protocol, 429, 5xx and parse errors remain
  errors. Successful ping evidence is retained even when discovery later fails.
- Capability availability begins unknown, is learned from requests and preserves
  prior success across transient errors. Missing extensions do not disable core
  browsing. Dropped request futures cannot publish a late capability result.

## Evidence and limitations

The final implementation has 22 ordinary tests: 3 core and 19 real-HTTP protocol
tests (several exercise multiple server responses). They cover ID namespace and
serde invariants, token/API-key signing, nonce freshness, path prefixes, empty/
sparse metadata, expanded artist credits, raw-slot pagination, malformed shapes,
wrong credentials, optional legacy discovery, 429/5xx, safe error formatting,
binary artwork versus JSON errors, cross-host redirects, advertised/chunked byte
limits, stalled-body timeout, cancellation and runtime capability failures.

Live Rust validation against Navidrome 0.64.0 (1072e9f7), API 1.16.1:
authenticated connection, discovery (7 extensions), 8 artists, artist albums,
24 albums across pages, 11 tracks from an album, song lookup, matching search,
artwork bytes, rejected wrong password. The CLI also listed 24 albums, opened an
11-track album, searched a track and fetched a 1,966-byte artwork image. Only the
synthetic library and disposable admin credentials were used.

This is connection/browse support in the library, not desktop OpenSubsonic login
or playback. No credentials are persisted. No stream decoder/player, protected
store changes, playlist/favorite mutations, scrobbling, audio hardware or native
OS controls have been added. JSON responses are required; artwork needs an image
Content-Type. A second real server, real API-key server flow, proxy integration,
Rust 1.98 MSRV, Windows/macOS and Nix packaging remain unverified. The inherited
Nix source closure/single-checkout CI still needs dependency-source assembly.

## Validation commands

Use the writable Cargo home for each repository in this container and `+stable`
(installed Rust/Cargo 1.99.1); the declared 1.98 toolchain was not changed.

```sh
# In libresonic:
cargo +stable fmt --all --check
cargo +stable clippy --locked --workspace --all-targets -- -D warnings
cargo +stable test --locked --workspace --all-targets
cargo +stable test --locked --workspace --doc
RUSTDOCFLAGS='-D warnings' cargo +stable doc --locked --workspace --no-deps
CARGO_HOME="$PWD/.cache/cargo" make -C integration/navidrome test-rust CARGO="cargo +stable"

# In sonicfast: full inherited Cargo/policy/launcher checks from CONTRIBUTING.md.
```

Library command logs are ignored under `libresonic/.cache/`; app results under
`sonicfast/.cache/validation/phase-1-*`.

| Final validation | Actual result |
|---|---|
| Library workspace all-target tests | PASS: 22 passed, live fixture test ignored in ordinary runs |
| Library fmt/clippy/doctests/docs | PASS, clippy/rustdoc warnings denied |
| Fresh Navidrome test-rust | PASS: Python readiness plus the real Rust facade; containers/network/volume removed |
| Standalone library example | PASS: 24 albums, album/song/search/artwork flow |
| Sonicfast cargo check with local library | PASS; external lockfile versions/sources verified unchanged |
| Sonicfast default/all-feature clippy | PASS, warnings denied |
| Sonicfast default tests | PASS: 1,360 passed, 3 ignored |
| Sonicfast all-feature tests | PASS: 1,401 passed, 3 ignored |
| Sonicfast fmt/doctests/rustdoc | PASS, rustdoc warnings denied |
| Dependency/evidence/settings policy, policy regressions, launchers | PASS |
| Native credentials/audio, other OSes, Rust 1.98, Nix, Jekyll | NOT RUN; same environment/packaging limits as Phase 0 |

```text
Phase: 1 / OpenSubsonic core and server connection
Baseline commit: 9c923873b9c3893d791922e3a66151b71d41e7f3
Changes: independent core/protocol crates, typed MusicService, bounded transport,
         private DTO mapping, CLI browse example, mock and opt-in Rust live tests.
Tests run: workspace tests/fmt/clippy/docs; fresh-container test-rust;
           full Sonicfast Cargo/policy/launcher checks. All executed checks pass.
Manual evidence: Linux x86_64 / Rust 1.99.1; Navidrome 0.64.0 synthetic library;
                 24-album CLI browse, song/search/artwork demonstrated.
Open issues: no desktop server login or playback; no protected persistence;
             second real server, API-key live flow, proxy/platform/MSRV and
             inherited packaging/CI source assembly still need validation.
Decision log: adr-002-protocol-facade.md (bounded adapter and existing TLS family).
Exit gate: PASS. The independent service connects to Navidrome, browses albums,
           searches and fetches artwork, rejects invalid credentials, supports
           mocked legacy discovery absence and keeps raw secrets out of errors.
Next smallest task: Phase 2 PCM-facing sink/vis/EQ adapter, followed by bounded
                    MP3/FLAC decoding through the retained native audio path.
```
