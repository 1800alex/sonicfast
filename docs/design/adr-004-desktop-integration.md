# ADR 004: Sonicfast desktop integration and local queue

Date: 2026-10-21. Status: accepted for Phase 3.

- Keep the existing App, egui/fastframe shell, themes, navigation, grids, tables,
  player bar, queue panel, EQ, visualizers and Winamp mini-player. Add a Sonicfast
  backend branch; it never constructs the legacy Worker, Engine or Session.
  The retained `spotifast` binary remains available for regression/demo comparison.
- Add the `sonicfast` binary and make it Cargo's default. Give it separate
  configuration/state/cache folders, instance slot, MPRIS identity, window ID
  and credential namespace. Do not adopt/copy/delete Spotify grants. Disable
  inherited update interception and checks for the fork. Release packaging and
  upstream website/download identities still need a later release audit.
- App-only adapters project neutral Libresonic metadata into retained display
  structs. Scoped MediaId JSON is encoded into opaque display keys; internal
  `sonicfast:` and `sonic-art:` identities carry no URL or auth parameters.
  Some retained views construct legacy URI prefixes around these keys; the
  adapter decodes the full scoped identity before any protocol request.
- Libresonic owns queue occurrences, context order, manual priority, previous,
  repeat/shuffle and serializable resume state. Manual duplicates stay distinct;
  changing context keeps manual songs. Next bypasses repeat-track, natural end
  respects it, and errors pause for an explicit retry rather than skipping songs.
  Previous keeps 256 occurrences. Restoration is always paused.
- The app coordinator owns one protocol client and one neutral Player per active
  server. Session generations reject stale metadata; load serials and abort
  handles cancel superseded resolutions. Queue metadata additions resolve in
  command order, with 64 pending batches and a 2,000-song resolution limit.
  A cleared queue invalidates pending additions. The inherited UI command/event
  channels remain; they are not a new bounded-memory claim for the whole app.
- Persist only validated non-secret server profiles. Password/API-key strings
  use `org.sonicfast.server`, keyed by state-directory scope and profile ID,
  through a dedicated native-store thread with a bounded job channel and 20s
  deadlines. Verify writes by reading them back. A locked/unavailable store
  leaves a session usable with an explicit warning and remember disabled.
- Persist remember=false/selected=None before a possibly blocked native deletion.
  Generation checks reject late writes and delete a stale completed write.
  Failure to persist revocation is reported. URL/account/auth-mechanism changes
  create a new server identity, preventing reuse of another endpoint's secret,
  metadata, queue or artwork keys.
- No credential-store operation, HTTP request, codec operation or state write is
  performed by a view action. Writes use existing atomic file helpers on workers.
  Artwork uses the retained bounded loader/scaler, with authenticated bytes from
  Libresonic and credential-free cache keys.
- Replace sign-in/account configuration with server profiles. Preserve general
  Appearance/skins/MilkDrop/EQ/storage settings. Omit unsupported Spotify account,
  proxy, playback-tuning and release controls. Hide/disable playlist/favorite,
  radio, social and remote controls; reject unsupported CLI verbs explicitly.
  Phase 4 covers broader server features and metadata/experience parity.

Evidence and remaining limits: [Phase 3 report](phase-3.md).
