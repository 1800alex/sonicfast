# ADR 009: filesystem availability without a download catalog

Date: 2026-10-22. Status: accepted for the owner's explicit request to avoid the
JSON download index and reconcile icons with files changed outside Sonicfast.
Supersedes ADR 008's persistent download catalog; its protocol/audio boundaries,
original-byte transfers, cancellation, publication and scrobbling remain intact.

## Persistence

No new `.sonicfast/index.json` is created or written. Completed files and their
availability are not persisted as a catalog. The download folder retains only
music, the exclusive writer lock and internal partials while work is active.

Unfinished download requests, queue order, retry state and pause state live in a
new defaulted field of the app's **existing per-profile playback resume file**.
That existing app state still uses JSON; this change adds no separate download
JSON file. This storage preserves the previously requested restartable queue.
States are keyed by canonical download root and scoped song IDs. Active work is
restored queued; completed/remote/missing availability is excluded. Cancellation
and failed requests remain visible across restart. Returning to a configured root
restores its unfinished work. The worker checkpoints changes before starting
new transfers; inability to save pauses the queue with an error. Whole resume
state is bounded to 32 MiB; unfinished queues remain capped at 10,000 entries.

Old index files are optional read-only migration hints for paths and unfinished
requests. Saved Complete flags become Remote until checked; old checksum/mtime evidence
is discarded, and unverified legacy paths do not reserve filenames. Missing, malformed,
oversized or invalid legacy data cannot disable filesystem lookup or downloads;
bad data is preserved and reported. A saved app queue supersedes the legacy
queue. Existing legacy files are not automatically deleted or renamed.

## Availability and UI

Visible library tracks and download-page rows request a background check when unchecked or five seconds
have elapsed since the last result. Pending requests are deduplicated, and a
repaint is scheduled so a paused player/idle page also refreshes. The UI performs
no filesystem work. Current transfer events keep active progress responsive;
active transfers acknowledge probes without probing their own partials. Queue
and compact rows continue to omit download arrows.

Two bounded workers check exact paths: a current-session or legacy path hint,
the safe server-relative path, metadata-based artist/album candidates, and prior
portable/scoped-ID filenames. There is no recursive scan or directory enumeration.
A move can be discovered only into one of these candidate paths; arbitrary
renaming/reorganizing cannot be inferred without a scan or additional metadata.

A file must be a regular, nonempty original with expected byte length (or an
already-known transferred length), within the existing 8 GiB cap. Existing path
containment and symlink rejection remain. Files are hashed on first discovery;
size/mtime and checksums are cached in memory for later checks. Changed files are
rehashed; restoring the known original or moving it into another candidate can
restore availability. Missing or mismatched files lose local status. New copies
at matching paths become available without having been downloaded by Sonicfast.
Playback independently checks the file again before opening, then falls back to
streaming when unavailable.

No persisted checksum means filename/size matching after restart is a heuristic,
not cryptographic proof that a file is the server recording. The server's
standard metadata has no content hash. Known in-session originals retain their
checksum evidence; matching size and mtime deliberately can evade that cache.
Externally added originals without any known server byte length are not adopted.
These are explicit limits of removing the catalog, rather than claims of full
music-library reconciliation.

Availability and track metadata caches are bounded; queue persistence excludes
those caches. The downloads page retains current-session discovered files and
unfinished work, without loading a complete downloaded-library history at startup.

## Names and compatibility

Natural paths from the naming correction remain preferred. Collision paths now
use the scoped-ID hash plus one of eight deterministic slots, so a future lookup
can rediscover them without stored mappings. Existing files and other reserved
identities are never overwritten. Exhausting these slots is a visible failure.
Legacy deterministic hashed names remain direct candidates. Exceptional old
random-suffix paths still benefit from the optional legacy path hints; arbitrary
nonmatching names cannot be rediscovered after those hints are removed.

The initial catalog removal changed no dependency, lockfile, protocol, native
device, EQ or decoder. The later [original-path correction](original-file-paths.md)
extends direct lookup compatibility for corrected server metadata. Linux evidence and limitations: [execution report](filesystem-availability.md).

## Original-path correction (2026-10-22)

Servers may correct virtual tag-derived paths to original library-relative
filenames. Sonicfast accepts a single
filename for flat libraries as well as nested relative paths. It checks bounded
collision slots for every metadata basename variant (including the old title),
so correcting the server filename does not hide earlier title-based hashed
downloads. The same traversal/symlink/size checks and background workers apply;
there is no scan or new persistent index. Existing files are not renamed.
