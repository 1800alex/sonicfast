# Phase 3: desktop OpenSubsonic alpha

Subsequent owner-requested codec expansion: [M4A playback](m4a-playback.md)
adds AAC-LC/ALAC support after the alpha and playback-stop fixes recorded here.
The subsequent [playlist step](playlists.md) adds server playlist reads and edits.
Song hearts, ratings and LRCLIB lyrics are covered by the subsequent
[song feedback/lyrics report](song-feedback-and-lyrics.md).

Date: 2026-10-21. Starting commits: Sonicfast
`dc8897c` and Libresonic `e4ecc65` (committed Phase 2). No commits or pushes
were issued during this phase. [ADR 004](adr-004-desktop-integration.md) records
ownership, identity, credential and local queue decisions.

## User-visible behavior

`cargo run --locked --bin sonicfast` now opens the existing native interface
with an OpenSubsonic server profile form. Enter a profile name, URL and password
or API key. Plain HTTP needs explicit per-profile opt-in. Remembering a login
uses the native protected store; session-only login works when remembering is
unchecked or unavailable. Saved profiles are selectable at sign-in. Sign out /
switch server revokes the selected profile's remembered login.

After connecting, Home shows the existing album grid; the sidebar offers albums
and artists. Album, artist, search, artwork, player-bar and queue views use the
retained rendering components. Native local playback accepts the supported MP3/
FLAC tracks. Unsupported known suffixes are disabled. Errors remain visible and
retryable; empty/loading states use the existing page widgets. The server form
reports authentication/connection errors without raw URLs or server bodies.

Play starts a collection or exact row occurrence. Player-bar Play resumes the
paused selection. Manually added songs, including duplicates, play before the
context; starting another context preserves them. Add/insert/remove/reorder/
clear, queue-row selection, Next/Previous, shuffle and repeat are local operations.
Repeat-track affects natural end; explicit Next still advances. Queue metadata
additions resolve in order and are canceled by Clear/sign-out. Context resolution
is capped at 2,000 songs, pending additions at 64 batches; previous history keeps
256 occurrences. Seek still reopens/decodes forward, as in Phase 2.

The queue/context/order/modes and position are saved per server. Restart restores
them paused, including after a previously playing session. It never starts audio
without a fresh play command. Position checkpoints run every two seconds while
changed, and shutdown writes the latest snapshot. Existing UI settings/session
and history files stay separate from the neutral queue snapshot.

Connect transfer, radio, social activity, lyrics, favorite/playlist mutations,
podcasts and unsupported Spotify CLI helpers are hidden, disabled or rejected.
Playlist reads/management and proxy support are outside this alpha. These controls
never silently fall through to a Spotify network worker. General appearance,
custom themes, skins, MilkDrop, EQ, the mini-player and the native device path
remain in the app. The output button opens local output settings.

## Boundaries and protected persistence

- `src/subsonic/` implements server forms/profiles, display adapters, a runtime
  coordinator and App action/event integration. Native actions are applied after
  drawing, using the existing action/channel pattern.
- `libresonic-playback::queue` contains occurrence/context/manual ordering and
  repeat/shuffle/previous policies. It depends only on the neutral domain, serde
  and existing rand; there is no GUI/Spotify/audio-device dependency.
- Neutral server/kind/item IDs are encoded into app display keys. The adapter
  decodes and validates them before requests. Artwork bytes travel through
  `sonic-art:` identities, the existing scaler and bounded loader. URL, token,
  password and API-key parameters never enter artwork cache names or player state.
- Native secrets use `org.sonicfast.server`, with a state-directory scope and
  stable profile ID. A separate bounded worker thread performs read/write/readback/
  delete; its failures/timeouts yield safe warnings. Form/secret types have no
  Debug/serialization implementation that could persist the usable secret.
- `servers.json` contains names, validated base URLs, usernames, auth-method flags,
  HTTP opt-in and remember state. The profile reader caps its input at 1 MiB.
  Ordinary state files contain no password or API key. Native access never adopts
  the legacy Spotify namespace. Changing endpoint/account/auth mechanism generates
  another server ID, so old credentials/cache/queue cannot follow it implicitly.
- Non-secret remember revocation is written before best-effort native deletion;
  canceled credential work cannot re-enable restoration. A failed durable
  revocation is reported. Secret writes are verified; failure keeps remember off.
- Network tasks carry session generations. Loads have cancellation handles/serials;
  newer loads and transport intent win. Queue additions have separate invalidation
  and FIFO resolution. Views and credential jobs never await HTTP/native calls.
- The new executable owns Sonicfast folders, instance/MPRIS/window identities and
  skips the inherited updater. The Rust package/library and legacy executable
  retain their old names for compatibility. Installer/bundle/website identities
  are not a finished Sonicfast release and must not be published as one.

## Executed evidence

Linux x86_64, Rust/Cargo 1.99.1 (`+stable`), with each repository's own Cargo home
and target. The declared 1.98 toolchain was not changed. Navidrome 0.64.0
(1072e9f7), API 1.16.1, uses the pinned digest and 264 generated tracks from the
copied custom server fixture. The test stack and keyring are disposable, with dummy
admin credentials. The container has no physical sound endpoint.

| Validation | Actual result |
|---|---|
| Library workspace tests | PASS: 39 ordinary tests, including 5 new queue tests; opt-in browse test ignored normally |
| Library fmt/clippy/doctests/docs | PASS, warnings denied |
| App default/all-feature Clippy | PASS, warnings denied |
| App default/all-feature tests | PASS: 1,381 / 1,424 passed; 6 ignored in each ordinary run |
| App doctests/rustdoc/fmt | PASS; rustdoc warnings denied |
| Policy/evidence/dependency checks, policy tests, launchers | PASS: 16 policy + 4 launcher tests |
| Native store round trip | PASS: write, verified read, generation invalidation and delete in private unlocked GNOME Keyring |
| Actual App Navidrome flow | PASS: rejected password; login; 24 albums, 8 artists; album tracks; loader-ready artwork; search; MP3 and FLAC playback/pause/seek; duplicate manual queue; clear/next; repeat/shuffle; paused queue restart; sign-out |
| Remembered native login | PASS: remembered connection, restart without re-entering password, paused restoration, sign-out revocation survives another restart |
| Slow server/cancel | PASS: normal regression renders ten warmed UI frames while HTTP is stalled, cancels login and rejects stale connection state |
| Native UI captures | PASS: real egui/GL under Xvfb, private PulseAudio output; login, albums, album, queue, search, settings, light theme, authentication failure |
| Native binaries | PASS: both executables built with all features; source launch tested |
| Physical devices / other OSes / Rust 1.98 / Nix / Jekyll | NOT RUN |

The isolated runner uses a private PulseAudio null sink, D-Bus session, unlocked
GNOME Keyring, XDG folders and Xvfb display. It never changes host audio defaults
or touches existing desktop credentials. Native fixture tests are ignored in
ordinary Cargo runs. Headless GL capture uses the actual App/backend; it is not
sample data replacing server responses. The retained legacy demo provides a
layout comparison. Captures stay ignored under `.cache/phase-3-review/`, and logs
under `.cache/validation/phase-3-*`. No review images are tracked.

Review found and corrected two artwork integration gaps (loader/scaler scheme
recognition and the cover widget's scale path), plus duplicate-row context
tracking and pending queue ordering. The final captures show generated cover
images, paused player controls and distinct manual/context queue sections. The
private runner provides a display before portal activation; early attempts
without it hit the container's unavailable Wayland/desktop portal setup. An early
fixture run also selected the wrong audio-host device name; enumeration now
selects the exposed native ID. These failures were fixed before the final proof.

## Follow-up: source builds and playback stopping (2026-10-21)

A user reported a missing Debian/Ubuntu build package, codec rejection and
playback starting before stopping with a generic byte-limit error. The build
instructions now name `libpipewire-0.3-dev` and list native prerequisites.

An 8 MiB mock audio response with a slow consumer reproduced the playback
failure: Hyper's adaptive HTTP/1 reader returned a legitimate chunk larger than
the old 256 KiB transport cap. The regression failed with `audio transport chunk`
at 262,144 bytes before the fix, and passes after raising the retained transport
chunk budget to 512 KiB. Hyper's current default read maximum is 417,792 bytes.
The public stream chunks stay at most 16 KiB; the 16-chunk decoder queue and
288 KiB encoded accounting budget are unchanged. This is a per-chunk guard,
not a whole-song size limit. The original small synthetic songs did not expose
this failure, so the initial alpha proof was insufficient for large streams.

Collected response limit errors now identify the fixed endpoint label and byte
budget without including URLs, item IDs or arbitrary server text. The full,
unpaginated artist index has a separate 32 MiB default; ordinary JSON remains
4 MiB and artwork 16 MiB. A 20,000-artist mock response verifies the larger
index budget and explicit override/rejection behavior. This index allowance is
independent of the reproduced playback failure.

The UI and worker share a codec metadata check that accepts case-insensitive
MP3/FLAC suffixes and MIME types, and lets unknown metadata reach stream probing.
Recognized unsupported formats remain disabled; M4A/AAC support was not added.
The retained preset downloader now receives the actual network configuration at
startup instead of waiting for a legacy proxy-restore event that this backend
never emits. Both cases have application regression coverage.

Follow-up validation on the same Linux/Rust 1.99.1 environment: 40 library tests,
1,383 default app tests and 1,426 all-feature app tests passed (6 app tests remain
ignored in ordinary runs). Both Clippy configurations, fmt, rustdoc/doctests and
dependency/evidence policy checks passed. The native store round trip and both
opt-in actual App/Navidrome flows passed again, including MP3/FLAC playback and
remembered paused restoration. The `sonicfast` binary and native proof examples
were rebuilt. Logs use `.cache/phase-3-fixes-*` in Libresonic and
`.cache/validation/phase-3-fixes-*` in Sonicfast. No new physical-device, M4A/AAC,
other-platform or Rust 1.98 validation is claimed.

## Reproduce

```sh
# Keep separate checkouts; install native build prerequisites from CONTRIBUTING.
# In sonicfast, regular development starts the new backend:
cargo run --locked --bin sonicfast

# Disposable tests (also need pulseaudio utilities, GNOME Keyring, D-Bus and Xvfb):
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" up
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" smoke
CARGO_HOME="$PWD/.cache/cargo" cargo +stable build --locked \
  --example subsonic_playback --example subsonic_ui
LIBRESONIC_TEST_URL=http://127.0.0.1:4535 python3 scripts/test-subsonic-app.py --captures
make -C "$LIBRESONIC_CHECKOUT/integration/navidrome" down
```

Run the complete checks in `CONTRIBUTING.md` and the library README separately.
Use `CARGO_HOME="$PWD/.cache/cargo" cargo +stable ...` only as needed in this
container; this is not evidence that Rust 1.98 or a second OS works.

## Limits and next gate

This is a source-built Linux alpha, not a published installer. Only Navidrome was
run as a real server; a second distinct server and live API-key server flow remain
release gates. Physical listening, hotplug, protected stores on Windows/macOS,
full media-key/tray/mini-player end-to-end exercise and native installers remain
unverified. Their existing code/tests are retained, not new platform evidence.
Inherited single-checkout CI and Nix vendor/source assembly still require independent
Libresonic integration before releases. External app lockfile packages/revisions
remain unchanged; only local playback dependency edges add serde/rand.

Albums/artists/search are implemented; playlist/favorite editing, scrobbling,
transcoding, complete library/search pagination UX, long-track seeking performance,
network/device recovery scenarios and additional server feature parity belong to
Phase 4. The inherited display models require integer totals; continuation counts
are projections until a final page, not a newly invented Libresonic server total.
Playback errors require an explicit retry; they do not consume future queue songs.
The Phase 2 feeder checkpoint/read-timeout limitations still apply during stalls.

```text
Phase: 3 / desktop MVP integration
Exit gate: PASS for the tested Linux/Navidrome source-built alpha: connect,
           browse, artwork, search, local playback/seek/queue, protected login,
           paused restart and sign-out work through the actual App.
UI review: existing native layout retained; intended profile/unsupported-control
           changes reviewed in ignored before/after captures.
Next smallest task: Phase 4 library/experience parity, followed by second-server,
                    physical-device/platform and packaging validation.
```
