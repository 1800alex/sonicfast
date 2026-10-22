# Song feedback and lyrics integration

Date: 2026-10-21. Implements [ADR 007](adr-007-song-feedback-and-lyrics.md).
Baselines: Sonicfast `e3f99979d525b07951acc838cf0589c226ed1942`, Libresonic
`afed493cc23778fb55cb0715fe467208355e94fe`.

- Use a song's row/player heart or menu to save it to Liked Songs. On the retained
  player bar, an already-liked heart opens the playlist picker with Remove from
  Liked Songs beneath it. Row hearts toggle directly.
- Open Liked Songs from the sidebar and play the displayed songs.
- In a song's context menu (also available from now playing), choose Rating and
  one through five stars. Clear rating removes the server rating independently
  of its heart. Failed updates retain the previous confirmed value.
- Open the existing Lyrics panel for plain/synced LRCLIB lyrics. Fullscreen,
  following the active line, and clicking a synced line to seek are retained.
- Add to playlist uses the existing picker. If any songs are already present,
  confirm Add anyway or Cancel before writing.

The existing Rust LRCLIB client uses bounded fallback, cache and throttle
behavior.
No personal server or music is used in verification. The disposable Navidrome
fixture uses generated music and its own container/volume. Synthetic lyrics are
preloaded into the actual cache for native tests/captures, so fixture metadata
is not sent to LRCLIB. HTTP lookup behavior is covered by local mock responses.
This does not claim a successful public-service lookup for real music. A live
synthetic `/api/get` probe returned HTTP 503 from this environment with both the
Sonicfast and browser user agents. The client reports such
transient errors and leaves them out of the negative cache.

Protocol regressions cover sparse ratings/favorites, repeated form parameters,
all ratings including clearing, independent hearts, kind/server validation,
limits, explicit unsupported support evidence, redaction and permission failures
without automatic retries. App tests cover heart row updates/rollback and rating failure/clearing/sign-out,
lookup fallback and matching, rate limits, response caps and timed-text expansion.

The opt-in actual App test hearts/unhearts a synthetic song, opens Liked Songs,
sets every personal rating and clears it while checking Navidrome's persisted
state, opens synced lyrics through the real worker/cache/panel, and verifies
that Cancel leaves a duplicate unchanged while Add anyway appends it. The
existing playlist workflow and playback/remembered-login flows run alongside it.

Early native runs found a Spotify-only URI prefix in the optimistic Liked Songs
helper; it now accepts Sonicfast song identities, with rollback coverage. Test
setup was also corrected to reset server feedback before loading UI metadata,
so stale fixture hearts do not invert the first toggle. The native wait budget
is now 30 seconds, exceeding the protocol's 20-second request deadline after an
initial authentication check exceeded the old 15-second harness deadline.
Visual review caught a missing Liked Songs header Play control; it now uses the
visible track list, including sorted/filtered views.

Validation used Linux x86_64 and Rust/Cargo 1.99.1 (`+stable`), separate per-checkout
caches/targets, and the pinned Navidrome 0.64.0 fixture. No new dependencies or
lockfile changes were needed.

| Check | Actual result |
|---|---|
| Libresonic workspace tests | PASS: 57 tests, including 4 new annotation protocol tests; 1 opt-in browse test ignored |
| App default / all-feature tests | PASS: 1,391 / 1,434 top-level tests; 8 ignored in ordinary runs; the isolated child-process test also passes in each run |
| Lyrics regressions | PASS: all 16 tests, including fallback requests, strict recording matching, 429 handling, bounded bodies and timestamp expansion |
| Native isolated tests | PASS: credential-store round trip and all 4 actual App flows, including hearts, every rating and clearing, cached synced lyrics, duplicate Cancel/Add anyway, and Liked Songs view playback |
| Native UI | PASS: 15 captures; Liked Songs, song menu with the confirmed rating, and synced lyrics reviewed visually |
| Both repositories | PASS: formatting, warning-denied Clippy, doctests and warning-denied documentation; app checks include all features |
| Policy/launcher regression | PASS: dependency-source and review-evidence checks, 16 policy tests and 4 launcher tests |
| Native build | PASS: default-feature `sonicfast`, `subsonic_ui` and `subsonic_playback` |
| Public LRCLIB lookup | NOT VALIDATED: synthetic live probes returned HTTP 503 with both user agents; cached/native and mock HTTP paths pass |

The native container still logs unavailable FUSE/GTK desktop-portal services;
keyring, virtual audio and egui flows pass. Physical hardware and file-picker
integration are not inferred from those results.

Logs use ignored `.cache/feedback-*` in Libresonic and
`.cache/validation/feedback-*` in Sonicfast; native review captures use
`.cache/feedback-review/native/`. The disposable server/container volume is
removed after verification. Changes remain local and uncommitted.

To reproduce the native flow after building the examples:

Set `LIBRESONIC_CHECKOUT` to the absolute path of a checkout of
[Libresonic](https://github.com/1800alex/libresonic) for fixture commands.

```sh
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" up
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" smoke
LIBRESONIC_TEST_URL=http://127.0.0.1:4535 python3 scripts/test-subsonic-app.py --captures
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" down
```

The song feedback and local lyric integration gate passes for Linux/Navidrome.
A real public-service lyric match remains an open verification item.

Limits: getStarred2 remains bounded to 4 MiB; playlist/write/playback limits remain
in ADR 006. Ratings reflect this session's last confirmed edit until reconnection;
server permissions remain authoritative. API-key annotation paths have protocol
coverage, not a real API-key native test. Other servers, operating systems,
physical hardware, Rust 1.98 and Nix packaging remain unvalidated.

Remaining Phase 4 work: define and test scrobbling start/completion and retry
policy, then extend network/device recovery evidence. Release work still needs
a distinct second server, supported-platform builds/listening, and packaging/
identity/license/update audits. Casting, remote control and multiroom remain
separately deferred.
