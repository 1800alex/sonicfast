# ADR 001: fork boundaries and local Libresonic development

Date: 2026-10-21. Status: accepted for Phase 0; client adoption provisional until
protocol tests pass.

Phase 1 follow-up: [ADR 002](adr-002-protocol-facade.md) records the decision
not to adopt the upstream client dependency after bounded-read and TLS feature
inspection. The evaluation below is the historical Phase 0 evidence.

Published-crate follow-up (2026-10-22): Sonicfast now consumes crates.io
Libresonic 0.1.0, superseding the temporary Git pin described below. The four
registry sources/checksums are recorded in Cargo.lock. The independent library
and native application boundaries remain. See
[public docs/workflow validation](public-docs-and-workflows.md).

## Context

Fork baseline: `9c923873b9c3893d791922e3a66151b71d41e7f3`.
Its parent, `e86d3de0cb72edba8f24b447218daa7f3af1f127`, is the local upstream
Spotifast baseline; the fork commit adds only DESIGN.md. Package version 0.14.1,
edition 2024, MSRV/toolchain pin 1.98.0.

The owner requested a separate Libresonic repository. This supersedes the
design's initial in-tree layout and
Phase 6 repository extraction, without implying an early stable API/publication.

## Decision

- Keep the app/UI intact. Bootstrap independent unpublished `libresonic`, with
  separate lockfiles and target directories. Public preparation consumes a pinned
  Git revision from https://github.com/1800alex/libresonic. The original empty
  crate established a build boundary;
  core/protocol/playback APIs belong to later phases, with crate splits then.
- Libresonic owns domain models/IDs, protocol/capability/error mapping and local
  playback/queue state. Sonicfast owns views, compatibility adapters, credential
  UX, native output devices, EQ, limiter and visualization wiring.
- No Spotify or wire DTOs in public domain types. Add an app compatibility
  adapter in Phase 3. Do not emulate Connect transfer or a shared server queue.
- Keep package/binary/library IDs during baseline validation; identify the fork
  in its README. Executable, desktop, secret-store and updater identities must
  migrate coherently before Sonicfast login/release. Never implicitly move
  existing Spotifast grants.
- Preserve fastframe-audio/rodio. Phase 2 must decouple `vis.rs` and `eq.rs` as
  well as `sink.rs`. The first proof can normalize to the existing stereo
  44.1 kHz processing rate, with explicit tested source format conversion.

## OpenSubsonic client evaluation

Inspected v0.6.1, peeled commit `1e0f302b01a2ae2b308bac751a9b9f724e4139de`,
released 2026-10-15, instead of assuming the design's 0.6.0 is current:
[client](https://github.com/M0Rf30/opensubsonic-rs/blob/1e0f302b01a2ae2b308bac751a9b9f724e4139de/src/client.rs),
[auth](https://github.com/M0Rf30/opensubsonic-rs/blob/1e0f302b01a2ae2b308bac751a9b9f724e4139de/src/auth.rs),
[manifest](https://github.com/M0Rf30/opensubsonic-rs/blob/1e0f302b01a2ae2b308bac751a9b9f724e4139de/Cargo.toml).

MIT OR Apache-2.0; edition 2024; MSRV 1.85. It uses reqwest 0.13 (app: 0.12),
so initially accept a duplicate transport instead of upgrading the app network
stack. Token auth uses random per-request salts; API-key auth omits username.
Auth Debug redacts secrets. Base path prefixes are preserved. Custom HTTP client
injection supports our deadlines/redirect policy; server_info exposes envelope
metadata. stream_chunked yields body chunks; stream buffers the whole file and
is unsuitable for playback. Artwork and paginated search/album options exist.

Decision: provisionally wrap registry opensubsonic 0.6.1 in Phase 1. Do not
vendor its source or expose DTOs. Required safeguards and unresolved tests:

1. Inject deadlines, verified TLS and disabled redirects. Default reqwest
   redirects are unsuitable for credential-bearing requests. Local HTTP needs
   explicit opt-in before any request.
2. Reject unsupported schemes, absent hosts, userinfo, queries and fragments
   in base URLs. The upstream builder preserves base queries.
3. Upstream logs redacted URLs, whereas our policy suppresses whole authenticated
   URLs. Disable its request debug logging in the app. Map raw HTTP errors,
   server strings and parse-error snippets to safe typed failures. Never expose
   authenticated Url values or raw transport Debug at the facade boundary.
4. Distinguish missing optional discovery (404/unsupported) from authentication,
   permissions, rate limits and server failures. No credential downgrade.
5. Test sparse metadata, pagination and cancellation by dropping futures. Bound
   artwork/JSON responses and playback input; chunking alone is not a budget.
   Upstream JSON/art helpers buffer complete responses without a size limit;
   wrapping their return values cannot cap those reads. Before adopting them,
   prove a bounded transport path or record an adapter/upstream change in an ADR.
6. time_offset exists, but accurate seek/range support is unproven. The chunked
   API has no obvious per-call Range hook. Evaluate seeking in Phase 2.

## Consequences

App builds fetch the pinned public Git dependency. Library tests remain
GUI-independent, and CI needs only the Sonicfast checkout. The original
local-development arrangement is superseded by public-source preparation. No playback, compatibility or public
release claim follows from this scaffold.
