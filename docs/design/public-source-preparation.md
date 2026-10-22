# Public source preparation

Date: 2026-10-22. Intended repositories:

- https://github.com/1800alex/sonicfast
- https://github.com/1800alex/libresonic

The owner requested current-tree cleanup and explicitly chose to preserve
existing Git history. No history was rewritten. Earlier commits can still contain
references removed from the current files. Upstream attribution and licenses are
retained. No commit, push or publication was performed by the agent.

## Published-crate follow-up

Libresonic is now published on crates.io. Sonicfast consumes version 0.1.0,
with all four library package sources/checksums in Cargo.lock. The Git pin below
records the earlier preparation stage and no longer describes the build.
Public guides and automation are finalized in the
[documentation/workflow follow-up](public-docs-and-workflows.md).

## Source layout

Sonicfast now consumes Libresonic through HTTPS Git at revision
`0b229123873d7e232790c5f3958a11b2f8534eec`. Cargo.lock records the same source
for the facade, core, protocol and playback packages. Builds no longer require
another repository at a particular filesystem location. Libresonic's internal
workspace paths remain ordinary repository-local dependencies.

Cross-repository documentation uses the intended GitHub URLs. Historical reports
anonymize additional-server evidence and remove personal directories and private
implementation references. Server-relative audio paths and repository-local
documentation paths remain part of the implementation.

The dependency policy permits the specific Libresonic repository alongside
crates.io and the retained upstream forks. Regressions reject external filesystem
dependencies, lookalike owners and unrelated repositories while retaining internal
workspace paths. No registry dependencies were upgraded; Cargo also normalized
the previously unsorted application entry in the lockfile.

## Reproducible fixture

Libresonic's Navidrome fixture generates its own original tones, tags, PNG art and
playlists with FFmpeg and Python's standard library. It needs no external source
tree or personal media. The fixture has eight artists, 24 albums, 264 songs and
MP3/FLAC/AAC-M4A coverage. The first album has a unique searchable first title;
other Path titles retain the non-first-row search regression.

Existing unmarked output directories are refused, generation uses an owned
staging directory, and failures leave no partial final library. Two safety tests
exercise preservation of an existing directory and encoder failure cleanup.
Generated media remains ignored. Existing generated fixture folders were retained
in ignored cache while replacing the old setup.

Libresonic now has a GitHub workflow for Rust checks, generator safety tests and
the disposable server proof. The commands were exercised locally; a hosted
GitHub Actions run is not claimed.

## Publication order

Both checkouts have a `github` remote with the corresponding intended URL.
Existing remotes are retained. Commit the remaining changes in each checkout,
then upload Libresonic first so Sonicfast's pinned commit is fetchable:

```sh
# In the Libresonic checkout, after committing:
git push github HEAD:main

# In the Sonicfast checkout, after committing:
git push github HEAD:main
```

Using `HEAD:main` also handles the application's current development branch.
Sonicfast's Cargo source was resolved using a command-scoped read-only local Git
mirror because the public repository did not yet expose the pinned commit.
No URL rewrite was stored in repository configuration or source. After upload,
`cargo fetch --locked` should be checked against the actual public repository.
This is source preparation, not a package release: inherited release destinations,
signing and the Nix vendor hash still require their release audit.

## Verification

Linux and Rust/Cargo 1.99.1 (`+stable`); MSRV 1.98, other OSes, physical devices
and release installers were not exercised. Logs stay in ignored
`.cache/validation/public-prep-*` files in each checkout.

Libresonic: fmt, warning-denied Clippy, workspace tests/doctests and warning-denied
documentation passed (62 ordinary tests, one opt-in ignore). Both generator
safety tests passed. A fresh generated fixture passed its Python smoke and
the opt-in Rust service test. Native Sonicfast verification passed all five App
flows and protected-store round trip; all 31 capture stages completed. That native
run used the preceding pin with identical Rust library sources; the final pin
adds the public metadata, fixture and CI cleanup. Its Rust sources are unchanged.
The disposable server, volume and private native services were stopped.

Final Sonicfast `make verify` and debug build passed with the cleaned pin:
formatting, dependency/review policy, 18 policy tests, four launcher tests,
default/all-feature warning-denied Clippy, 1,409 default and 1,452 all-feature tests,
doctests and warning-denied documentation. The ordinary test suites retain their
10 opt-in ignores. An initial release-page assertion still expected the upstream
repository; it now expects Sonicfast's GitHub repository, matching package metadata.

The final scan covers both tracked files and new nonignored publication files.
It found no private project names, personal workspace paths, supplied server/key
markers or external cross-checkout paths. Ignored caches and existing Git history
are outside this current-tree cleanup. GitHub's repository was still empty at the
last remote check, so public retrieval of the locked commit remains a post-upload
check. The build used the exact committed Git object through the local mirror.
