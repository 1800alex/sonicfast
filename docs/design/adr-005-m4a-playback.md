# ADR 005: M4A decoding and container byte seeks

Date: 2026-10-21. Status: accepted. The owner requested M4A support after the
Phase 3 playback fixes. This adds a codec/container feature, not the rest of
Phase 4.

M4A is a container. Enable the existing Symphonia 0.5.5 AAC, ALAC and ISO MP4
features in Libresonic. The lockfiles add three registry packages from that
decoder family; existing external versions and git revisions stay unchanged.
Verified formats are DRM-free AAC-LC and ALAC, mono/stereo within the existing
8..192 kHz output constraints. HE-AAC, protected media, other codecs, gapless
presentation/edit-list precision and transcoding are not promised.

- Extend core's `AudioStream` with optional complete byte length, seekability
  and cancellable absolute byte seeks. Default implementations remain streaming
  only. No URL, HTTP type, filesystem path or credential enters the contract.
- The protocol adapter advertises seekability only when the raw response has a
  known length and advertises `Accept-Ranges: bytes`. Every byte seek reuses
  the authenticated raw-stream endpoint and existing TLS, no-redirect and
  timeout policy. Require HTTP 206, exact `Content-Range` offset/end/total and
  matching remaining length. Ignored or inconsistent ranges fail safely.
- Sniff eight bytes with a bounded retained prefix. MP4 with seekable input uses
  feeder-owned byte seeks, allowing metadata after `mdat` without downloading
  the whole song. MP3/FLAC retain the producer/channel path. Fast-start M4A
  remains streamable without byte ranges; tail metadata requires seekability.
- Reads still yield at most 16 KiB. The MP4 source retains one such read, plus
  the initial prefix (at most 16 KiB + 7 bytes). Symphonia lookahead, HTTP's
  512 KiB retained chunk and MP4 metadata/sample tables are additional storage;
  input queue counters are not a total memory measure. No disk spool or offline
  cache is introduced. Decoder PCM limits and native device/EQ/tap code remain.
- User time seeks still cancel/reopen and decode/discard to the selected
  decoded sample. Byte seeks here serve container parsing; this does not claim
  efficient time-based seeking or API time-offset support.
- Cancellation interrupts header inspection, reads and outstanding range
  requests. Preserve safe service errors through Symphonia probe failures.
  The UI/worker share M4A metadata acceptance; actual decoder support remains
  authoritative, including channel/rate and AAC profile restrictions.

Generated AAC/ALAC fixtures cover front metadata and tail metadata outside the
64 KiB rewind buffer, exact decoded replay seeks, tiny input chunks, streaming
fallback and recovery. HTTP tests reject ignored/mismatched ranges. Feeder
tests cancel stalled seeks and retain safe authentication errors.

Current execution results and limitations: [M4A validation](m4a-playback.md).
