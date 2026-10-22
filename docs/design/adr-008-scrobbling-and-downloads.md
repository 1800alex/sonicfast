# ADR 008: scrobbling and indexed original-file downloads

Date: 2026-10-22. Status: accepted for this owner-requested follow-up.

Persistent download-catalog behavior below is superseded by
[ADR 009](adr-009-filesystem-availability.md), at the owner's explicit request.
Scrobbling, transport and audio boundaries remain applicable.

The owner requested configurable scrobbling and a persistent download queue with
local-first playback. This explicitly extends DESIGN.md's original exclusion of
offline downloads. It does not add an offline library/database, casting or a new
audio engine. No new dependencies or lockfile changes are required.

## Boundaries and protocol

Libresonic retains neutral optional original path, byte size and play count on
Track; older serialized tracks default these fields to unknown. It exposes
scoped song-only download and scrobble operations with learned capabilities.
Sonicfast owns folders, queue persistence, native settings, receipts and UI.

OpenSubsonic's [Child](https://opensubsonic.netlify.app/docs/responses/child/)
provides optional `path`, `size` and `suffix`; it provides no standard content
hash. Original server paths are filename hints, never trusted local paths.
[download](https://opensubsonic.netlify.app/docs/endpoints/download/) retrieves
original bytes without format/bitrate parameters. The adapter requires HTTP 200,
rejects redirects and non-audio/XML/HTML responses, bounds JSON errors, and uses
bounded chunks and per-read deadlines. Existing TLS, HTTP opt-in and credential
redaction policies apply.

## Scrobbling

Settings default off. The listened threshold is a clamped percentage (1–100%)
or an optional seconds cap (zero disables the cap), whichever comes first.
Unknown duration uses the seconds cap; if both are unavailable no completion is
submitted. Only advancing playback positions earn credit, capped by elapsed
wall time; paused/buffering time and seek generations cannot invent a listen.
Each new queue occurrence, including repeats of the same song, gets a distinct
random identity and UTC start time. Seeking/device restart preserves it. Resume
checkpoints preserve heard time/decisions and restore paused.

[scrobble](https://opensubsonic.netlify.app/docs/endpoints/scrobble/) uses
`submission=false` for now playing and true for a qualified listen, with the
occurrence's start timestamp in milliseconds. Advertised form POST is preferred;
legacy bounded GET remains supported. Reports are serialized through a bounded
64-item queue. Private, atomic receipts are saved before wire attempts; the last
4,096 report identities per profile prevent replay after restart. Receipt writes
are serialized across reporter replacement in this process.

Subsonic supplies no idempotency key. An ambiguous failure cannot safely be
retried, so reports are attempted at most once, with a visible error on failure.
A crash between receipt and send may lose a report. Separate client processes
are not an exactly-once system. Disabling prevents new report enqueueing and
aborts its reporter; a request already accepted by the server cannot be undone.

## Download queue and filesystem

Downloads default off. Users enable them and apply an absolute folder in Settings.
A worker-side manager owns a per-folder process lock, versioned private atomic
index, two transfers and two bounded probe workers. Queue order, pause state,
completed records and failures survive restart; interrupted transfers restart
from byte zero. Cancel removes the owned partial; Resume never resurrects
cancelled/failed jobs. Users can retry a job or move it to the front. Network,
timeout, 429 and 5xx failures get up to three total attempts with backoff;
Retry-After is respected. Permissions/protocol/size failures require manual retry.

An original is streamed in bounded chunks to an internal partial file, checked
against available server/HTTP length, hashed with SHA-256, flushed and synced.
Publication uses a same-filesystem hard link that cannot replace an existing
file, followed by partial removal and an atomic index update. The selected
filesystem must support hard links. Queue/index caps are 10,000 records and
32 MiB; each file is capped at 8 GiB. Corrupt/unsupported index files are preserved
and reported, rather than silently reset. Symbolic links and non-relative index
paths are rejected; components sanitize server-controlled separators/reserved
names. This protects routine operation, not hostile concurrent filesystem edits.

Naming correction (2026-10-22): prefer the server's safe relative `path`, including
its folder structure and basename. Reject absolute/drive/UNC paths, traversal,
empty components and the internal index namespace. Sanitize components for the
actual OS; Linux preserves colons instead of applying Windows restrictions.
When no usable relative path exists, use artist/album and the original basename
or track number/title/suffix. Files are directly beneath the user-selected root;
no extra Music prefix is added or stripped.

Natural filenames are the default. Reserve them under the shared index lock so
two concurrent transfers cannot select the same name. Only collisions with an
existing file or another indexed identity receive a scoped-ID suffix (then a
random suffix if needed). Publication still cannot replace existing files.
Sign-out and disabling keep completed files. Existing indexed names remain
valid, and exact probes also recognize the previous sanitized/hash layout;
this change does not automatically rename users' files or scan directories.

There is no full directory enumeration. For a displayed/played song, bounded
background probes check indexed paths, the validated server-relative path and
exact metadata/legacy candidates. Some servers synthesize virtual
`Artist/Album/Title.suffix` paths instead of the original on-disk filename.
Adoption requires known equal original size, one initial hash and an unclaimed
path. Filename/size matching is a heuristic without a server content hash; it
cannot prove that an arbitrary pre-existing same-size file is the right recording.
Indexed files use size/mtime checks and rehash when mtime changes; preserving both
size and mtime deliberately can evade this cache check. Concurrent probes claim
filenames under the same index lock. Metadata/known-song probes are bounded.

The existing player receives a StreamProvider wrapper: a valid local original
opens as a seekable bounded stream; missing, changed or unreadable files fall
back to the authenticated server stream. No disk/network/hash work runs during
UI drawing. The download page shows queue states, byte progress and controls;
library track duration cells show an arrow, active progress ring and completed
accent color. Compact rows and queue views omit the arrow to preserve duration
space. The existing native sink, EQ, visualizers and decoder remain intact.
This is local audio preference while connected: login/catalog browsing are still
server-backed. Unsupported original codecs remain unsupported after downloading.

[Execution evidence and usage](scrobbling-and-downloads.md).

The later [original disk-path correction](original-file-paths.md) records how
Sonicfast handles original filenames after a server corrects virtual path metadata. ADR 009's compatibility lookup covers prior title-based
filenames without a persistent availability index.

The [settings visual alignment](settings-visuals.md) moves Sonicfast's server,
scrobbling, download and output controls into the existing Appearance-style
settings rows/cards and search. It retains the existing worker/actions and
explicit folder Apply behavior; no persistence or protocol changes.
