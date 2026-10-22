# Settings and artist/menu visual alignment

Date: 2026-10-22. Owner-requested UI follow-up; Sonicfast only, with the legacy
Spotifast following control retained. No Libresonic or custom server changes.

The Server, Scrobbling, Downloads and Playback settings now use the Appearance
section's existing framed cards, label/description rows, responsive control
placement, typography and palette. The same switches, slider/value pills,
soft buttons and field framing are reused. These sections now participate in
settings search and the existing no-results behavior. Their renderer lives under
`src/ui/settings/server.rs`; it keeps the existing Action boundary.

The folder field retains a draft until Apply and keeps it while filtering.
Audio output uses a compact picker plus refresh; listing runs in the existing
background worker. Scrobbling thresholds, download enablement/path, queue
navigation, output selection and mini-player actions retain their behavior.
The redundant alpha feature-status paragraph is removed from Settings.

Sonicfast artist pages omit the Follow/Following button. Song rating options and
Clear rating use the same menu-item painter and separator as the other context
menu actions, without selected-label borders. Clear is disabled for an unrated
song and while a rating write is pending; the parent menu still shows the current
rating. Existing hearts, rating writes and duplicate confirmation are unchanged.

The native fixture walkthrough now captures the open rating submenu, an artist,
each settings section, Appearance for comparison, and light/narrow download
settings. Captures stay outside git in `.cache/settings-style-review/native/`.
Logs use `.cache/validation/settings-style-*`. Review uses Linux/Rust 1.99.1
(`+stable`); other platforms and pinned Rust 1.98 are not validated. Personal
servers, files and credentials are not used. No dependencies or lockfiles change.

| Check | Actual result |
|---|---|
| Default / all-feature tests | PASS: 1,405 / 1,448 top-level tests plus one isolated child each; ten ordinary-run ignores |
| Native App | PASS: all five Navidrome flows and native credential-store round trip |
| Native visual review | PASS: 28 captures; reviewed artist, rating submenu, Server/Scrobbling/Downloads/Playback, Appearance, and wide/narrow light Downloads at 1,200 × 850 / 680 × 850 |
| Quality/build | PASS: fmt, default/all-feature warning-denied Clippy, doctests, warning-denied docs, sonicfast and native examples |
| Policy/launchers | PASS: dependency/review checks, 16 policy tests and four launcher tests |

The first concurrent validation attempt hit existing timing-sensitive failures:
a three-second download cancellation setup deadline and the short-track native
scrobble/pause sequence. The all-feature suite and native flows passed on separate
reruns without changing their code. Both initial and successful logs are retained.
The visual walkthrough also caught that assigning ThemeChoice alone did not apply
its palette; it now invokes SetTheme and asserts that the light palette is active
before capturing. Final light captures were reviewed after this correction.

The disposable Navidrome container/volume were removed after verification.
Changes remain local. The unrelated `TODO.md` is untouched.
