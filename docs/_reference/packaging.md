---
title: Release packaging
description: Sonicfast release artifacts and workflow boundaries.
nav_order: 6
---

The [release workflow](https://github.com/1800alex/sonicfast/blob/main/.github/workflows/release.yml)
builds the tagged Sonicfast source with its committed Cargo lockfile. Libresonic
comes from crates.io. Tags must match the package version and have reviewed release
notes. Release jobs build the default `sonicfast` executable.

Targets include Linux x86-64/ARM64 archives, DEB/RPM packages and AppImages,
macOS universal DMG, and Windows x86-64/ARM64 ZIP and installer packages.
The Flatpak bundle is optional. Linux binaries target Ubuntu 24.04's glibc 2.39.
Platform jobs provide build checks; manual desktop/audio validation is separate.

The packaging workflow builds the current checkout for pull requests and main,
then checks Linux installs in clean containers. It does not download an upstream
release fixture or publish to third-party package repositories. AUR/Homebrew
recipes remain available for maintainers to adapt; no fork-owned public tap or
AUR publication is configured.

`checksums.txt` lists release artifact hashes. No fork signing key is configured,
and these checksums are not publisher authentication. The inherited upstream
updater key is used only by legacy comparison code. Sonicfast self-updates remain
disabled. Optional Apple credentials enable the existing DMG signing/notarization
path; unsigned builds must not be described as notarized.

See [PACKAGING.md](https://github.com/1800alex/sonicfast/blob/main/PACKAGING.md)
for commands and repository setup. Nix package publication requires a fresh vendor
hash and separate validation; it is not enabled in default CI.
