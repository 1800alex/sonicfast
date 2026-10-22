# Public documentation and workflow follow-up

Date: 2026-10-22. Scope: current Sonicfast documentation, GitHub automation,
published Libresonic consumption, and removing Spotify custom order from the
OpenSubsonic library menu. No commit, push, tag, release or Pages deployment was
performed. Existing Git history and upstream license notices are preserved.

## Decisions and changes

- Consume crates.io Libresonic 0.1.0. Only the four Libresonic lockfile sources
  change from the public Git revision to registry packages and checksums.
- Replace the inherited Spotify user manual with OpenSubsonic guides for server
  login, codecs, playlists, feedback, LRCLIB, scrobbling and local downloads.
  Keep engineering reports as historical evidence, excluded from Jekyll output
  and linked through GitHub. Retain explicit Spotifast fork attribution.
- Publish-site configuration targets `https://1800alex.github.io/sonicfast/`.
  Remove the upstream CNAME, analytics override and promotional captures/videos.
  Check generated links, anchors and project-site prefixes in CI and Pages builds.
- Build release packages from the tagged, locked Sonicfast source. Validate tag,
  package version, notes and AppStream metadata before building. Require no
  upstream signing key; application self-updates stay disabled. Hashes detect
  corruption and are not publisher signatures.
- Use fork-owned release URLs, AppImage metadata and package links. Build Linux
  DEB/RPM/AppImage artifacts from the current checkout for packaging PRs rather
  than downloading the old upstream fixture. Install pinned nFPM explicitly.
  Remove inherited board/triage/funding automation and downstream publishers.
- Keep default macOS/Windows packages from claiming Spotify URL handling. The
  Windows link regression fixture explicitly opts into retained legacy code.
  Correct its stale Inno fixture identity. Foreign-platform execution remains CI
  work; local Linux checks cannot establish those package behaviors.
- Nix package CI is opt-in pending vendor-hash refresh and native validation.
  The development shell remains. No public fork cache/tap/AUR/Flathub service is
  assumed. See PACKAGING.md for repository setup and release steps.
- Hide Spotify custom order in Sonicfast and reject it as a selected preference.
  Old stored values fall back to recent playback or existing local order. Keep
  the serialized variant and legacy comparison behavior compatible.

## Validation

Local Rust/Cargo 1.99.1 (`+stable`), overriding the committed 1.98.0 pin:

| Check | Result |
| --- | --- |
| Formatting and dependency/review policy | Passed |
| Clippy, default and all features, all targets, warnings denied | Passed |
| Default tests | 1,409 passed, 10 opt-in ignores |
| All-feature tests | 1,453 passed, 10 opt-in ignores |
| Doctests and warning-denied Rustdoc | Passed |
| Python policy, launchers, release metadata, release names, Flatpak metadata, docs checker | 45 tests passed |
| Actionlint 1.7.12 | Passed for all retained workflows |
| Jekyll 4.4.1, Ruby 4.0, Bundler 4.0.16 | Production site builds; generated links/base paths passed |
| native-packages 0.10.0 configuration/templates | Passed |
| Linux amd64 DEB/RPM/AppImage build | Passed using a stripped debug binary, not a release-mode build |
| AppImage startup, update metadata and zsync validation | Passed; Sonicfast 0.14.1 and 1800alex/sonicfast feed |
| DEB install/runtime probe/removal on Ubuntu 26.10 | Passed, preserving the settings fixture |
| RPM install/runtime probe/removal on Fedora latest (44) | Passed, preserving the settings fixture |
| Navidrome 0.64.0 fixture smoke | Passed, 264 tracks and 24 albums |
| Native credential store and five native App flows | Passed |
| Native UI walkthrough | 32 captures completed; library sort menu reviewed with an old Spotify preference loaded |

Review captures are ignored under `.cache/publication-review/native/`; the sort
capture shows Recently played and Name, with no Spotify option. Logs are retained
under `.cache/publication-*.log`, `.cache/docs-*.log`, `.cache/package-*.log` and
`.cache/actionlint.log`. The fixture container/database and private native services
were stopped after validation.

## Failures and limits

The initial all-feature compilation exhausted disk space. Only superseded local
Sonicfast root build products/incremental directories were removed; dependency
caches were retained. The complete final contributor check then passed. No
`cargo clean` or shared target directory was used.

Host Ruby 3.3 lacked native extension headers. Building in a container with the Ruby 4.0
version used by CI resolved this without changing the site dependency versions.

This host links three libm symbols from glibc 2.43. Its local debug-derived DEB
correctly requires libc6 >= 2.43, so Ubuntu 24.04 and Debian 13 install checks
rejected it. No dependency constraints were weakened. Release/packaging CI builds
on Ubuntu 24.04 for the intended glibc 2.39 baseline; that baseline is not claimed
as locally verified. The newer Ubuntu/Fedora checks above validate package wiring.

The first Fedora probe failed because a development-container /tmp directory was
not visible to the external Docker daemon. The probe now lives beside its packages
on the workspace mount; the full Fedora rerun and Ubuntu 26.10 check passed.

macOS, Windows, ARM64, Flatpak and release-mode binaries were not built locally.
The Nix vendor hash was not refreshed or validated. GitHub-hosted workflow runs,
Apple signing/notarization and public deployment require repository-side setup;
local configuration checks are not evidence of those operations succeeding.
