# ADR 002: bounded protocol transport and Phase 1 service contract

Date: 2026-10-21. Status: accepted and validated in Phase 1; supersedes ADR 001's
provisional upstream client dependency.

## Evidence and decision

In opensubsonic 0.6.1, Client::load_envelope calls response.text() and get_binary
calls response.bytes(). Both collect unbounded bodies. These helpers are private;
injecting reqwest cannot intercept/limit a body. The client also logs redacted
authenticated URLs and includes response snippets in parse errors. Our contract
requires bounded reads and whole authenticated URLs kept out of public errors.

Further dependency inspection found a second incompatibility: opensubsonic
unconditionally enables reqwest 0.13's rustls feature, which selects aws-lc-rs.
The retained app uses ring. Rustls cannot infer a default when both features
are enabled, while the pinned librespot core's http_client.rs uses
HttpsConnectorBuilder::new().with_native_roots(), which takes that implicit
provider path. The app does not install a process-wide default. Requiring a
new global crypto decision merely to browse a library would widen this phase.

Decision: do not adopt opensubsonic 0.6.1 as a dependency. Use Sonicfast's existing
reqwest 0.12/native-root Rustls family for a narrow adapter, with verified TLS,
disabled redirects/retries, deadlines and incremental body limits. Implement
only the Phase 1 endpoint DTOs and the specified token formula using the existing
md-5 0.10 and rand 0.9 families. Each token uses a fresh 128-bit random salt.
No upstream source is copied/vendored and no git override is added. This is an
endpoint adapter using an existing maintained HTTP client, not a new HTTP stack.
It replaces the provisional choice in ADR 001 based on measured incompatibilities.
An upstream client with bounded transport hooks and configurable TLS features
can be reconsidered behind the same neutral facade.

## Contract

- Keep two implementation crates in the separate repository: libresonic-core
  (domain/errors/service trait) and libresonic-opensubsonic (private protocol
  dependencies). Root libresonic reexports both facades. No playback crate yet.
- IDs include server profile namespace and item kind. Identical raw IDs from
  different servers or different entity kinds cannot collide. Constructors and
  deserialization enforce nonempty IDs. No URL/credentials are ID components.
- MusicService is object-safe, Send + Sync, returning boxed Send futures.
  Dropping a request future cancels that request; no background retries/tasks
  outlive it. Synchronous capability snapshots take only a short mutex lock.
- Each request has connect/whole-request deadlines. Defaults: 5s connect, 20s
  request, 4 MiB JSON, 16 MiB artwork. Caps apply during reads, including bodies
  without Content-Length. Rejection never includes bodies/URLs in Error sources.
- HTTPS is the default; explicit allow_http is required for self-hosted HTTP.
  Reject base userinfo/query/fragment and preserve reverse-proxy path prefixes.
  Requests use direct connections for this proof; app proxy integration is later.
- Ping runs with the selected credentials before optional extension discovery.
  HTTP 404/501, not-found, or an explicit unsupported response on discovery may
  fall back to baseline support. Authentication/permission/429/5xx/parse failures
  remain errors and never trigger a credential downgrade. Missing extensions do
  not disable baseline browsing. Capability availability is learned by successful
  requests, with safe runtime failure records; no blanket feature claims.
- Paging is bounded to 1..500 items per request. A full page offers a next offset;
  a short page ends it. No invented total. Search uses the requested offset/size
  for each result type and returns independent next-page hints. No automatic
  unbounded traversal or retry loop is hidden in the service.
- Sparse optional metadata remains absent. Missing/null display names receive
  neutral fallbacks; mandatory IDs and invalid wire shapes fail safely. Artwork
  is fetched as bytes using a scoped ID, never exposed as an authenticated URL.
- The CLI example is a temporary connection/browse harness. It accepts secrets
  from environment variables, not command-line flags, and stores no credentials.
  Main app UI/login and protected persistence remain Phase 3.

## Validation gate

Mock tests must cover token/API-key signing, path prefixes, sparse metadata,
namespaced IDs, pagination, legacy discovery, typed auth/HTTP errors, invalid
JSON, response limits, redirects, timeout and cancellation, with no credentials
or raw response text in errors. Live Navidrome must pass the Rust facade's
connection/artists/albums/album/song/search/artwork flow and wrong-password case.
Real playback, a second real server and platform coverage are separate gates.
