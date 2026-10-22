# Original disk paths and local-file recognition

Date: 2026-10-22. Follow-up to [filesystem availability](filesystem-availability.md)
and [ADR 009](adr-009-filesystem-availability.md).

## Cause and behavior

A compatible server can advertise `path` using artist/album/title tags instead
of the original on-disk filename. Such a virtual path can lose directory spelling
and filename prefixes. Sonicfast preserves the advertised path and cannot infer
arbitrary local layouts without scanning.

For consistent download/local matching, servers should expose an original path
relative to the music library and the original byte size. Absolute or outside-root
paths must not be treated as trusted local paths. A server returning virtual
metadata cannot identify an arbitrarily renamed original on the client.

Sonicfast accepts safe root-level filenames as well as nested paths. Its bounded
direct lookup keeps deterministic collision slots for earlier metadata basenames,
including title-only and portable names even after a server corrects its filename.
New downloads prefer the advertised original path. Existing files are not moved,
renamed or overwritten.

Availability comes from background checks with size, containment, symlink and
in-session checksum validation. Complete updates color the existing arrow. There
is no recursive scan, directory enumeration or new JSON index. Local playback
checks the file again before falling back to the server stream.

The configured download root should directly contain the artist directories.
After metadata changes, reopen the album or restart Sonicfast to fetch fresh
metadata. Matching originals appear on the next visible-row check, normally
within five seconds plus disk I/O. Unknown-size external copies are not adopted.

## Evidence

Linux with Rust/Cargo 1.99.1 (`+stable`); Rust 1.98, other operating systems and
physical audio hardware were not exercised. Historical checks included a narrow
additional compatible-server metadata/download probe; no production server writes
or personal music changes were made. Public reproducible coverage uses mocks and
the generated Navidrome fixture.

| Check | Result |
|---|---|
| Sonicfast default / all features | PASS: 1,405 / 1,448 top-level tests, plus one isolated child each; 10 ordinary-run ignores |
| Download regression | PASS: reported original/tag path discrepancy through wire metadata and the view adapter, background Complete updates, older hashed/title names, flat library, restart without index, local-only stream, and new download at the original path |
| Native Navidrome App | PASS: five App flows plus native credential round trip; external delete/restore status, blocked-stream local playback and paused queue restart included |
| App checks/build | PASS: default/all-feature warning-denied Clippy, fmt, docs/doctests, sonicfast binary and native playback example |
| Policy/launchers | PASS: source/review checks, 16 policy tests, four launcher tests |

The regressions cover differing tag/original paths, corrected metadata with older
download names, flat libraries and local-first playback. Logs remain in ignored
`.cache/validation/original-path-*` files. No new dependency was introduced by the
original-path correction.
