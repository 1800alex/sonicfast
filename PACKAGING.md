# Sonicfast packaging

Sonicfast packages the default OpenSubsonic executable. The retained
`sonicfast-legacy` binary and its upstream updater are comparison tools and are
not included in normal release packages.

## GitHub setup

The public repository is `1800alex/sonicfast`. Enable GitHub Actions and select
**GitHub Actions** as the Pages source in **Settings → Pages**. The Docs workflow
builds `docs/` and deploys to `https://1800alex.github.io/sonicfast/` on main or
manual dispatch. It uses the built-in token with Pages/OIDC permissions only in
the deployment job. No custom domain, analytics service or deployment secret is
configured. CI builds and checks the site on pull requests without deploying it.

The release workflow uses the built-in token to attach assets to this repository.
It needs no upstream signing key, AUR SSH key or Homebrew publishing credential.
There is no default downstream package publication. The inherited project board,
Copilot triage and funding automation are not configured for this fork.

## Prepare a release

1. Update `package.version` in `Cargo.toml` and refresh `Cargo.lock` with Cargo
   (for example, `cargo metadata --format-version 1 > /dev/null`). The lockfile's
   Sonicfast package version must match; release builds use `--locked`.
2. Update `sonicfast_version` in `docs/_config.yml` and the first `<release>` entry
   in `packaging/flatpak/org.sonicfast.Sonicfast.metainfo.xml` (version and date).
3. Add reviewed `packaging/release-notes/vVERSION.md` describing Sonicfast's
   behavior, validation and limitations. Earlier release-note files are upstream
   history; do not use them as evidence of fork validation.
4. Run `make verify`, `python3 packaging/test-release.py`,
   `python3 packaging/test-release-names.py`, `python3 packaging/flatpak/test-metainfo.py`
   and `python3 packaging/check-release.py vVERSION`. Check platform packaging CI
   and inspect the artifacts before tagging the reviewed commit.
5. Push the matching `vVERSION` tag when ready to publish. The workflow publishes
   a GitHub release automatically; tags with a prerelease suffix are prereleases.

Tag validation runs before builds and requires the Cargo manifest/lockfile versions, notes and
AppStream version to agree. Builds use `--locked` and `--bin sonicfast`, preserving
default features including MilkDrop. Cargo fetches published Libresonic crates;
no local checkout or private backend is required.

All release metadata must be committed before tagging. Rerunning a failed tag's
workflow checks out that same tagged commit; it does not pick up later fixes on
the branch. After correcting metadata, use a new version/tag or deliberately
replace the failed tag so it points to the corrected commit.

## Artifacts

| Platform | Output |
| --- | --- |
| Linux x86-64, ARM64 | tar.gz, DEB, RPM, AppImage and matching `.zsync` |
| macOS universal | DMG with Intel and Apple Silicon executables |
| Windows x86-64, ARM64 | ZIP, Inno Setup executable and per-user WiX MSI |
| Flatpak x86-64 | Optional bundle from the Linux archive |
| Source | Tagged source tar.gz |

Linux builds run on Ubuntu 24.04 and target glibc 2.39. AppImages do not bundle
every system GUI/audio library; DEB/RPM dependencies include those libraries.
Windows checks inspect the PE machine and reject dynamic MSVC runtime imports.
The macOS bundle and default Windows installers do not claim Spotify URL handling.

`packaging/release-names.py` validates AppImage update metadata and `.zsync`
contents, then writes `checksums.txt`. AppImage metadata points to
`gh-releases-zsync|1800alex|sonicfast|latest|sonicfast-*-ARCH.AppImage.zsync` for
external AppImage tools. The default client's in-app updater is disabled.

Release checksums detect corruption; they are not publisher authentication.
The upstream public key in `assets/update-public-key.hex` belongs to the retained
legacy updater. It is not a Sonicfast release trust root, and the release workflow
does not use it. Configure a fork-owned trust root in a separately reviewed change
before enabling signed application updates.

Native macOS signing/notarization is optional through the existing native-packages
integration. It requires all of `APPLE_CERTIFICATE_P12`,
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`
and `APPLE_APP_PASSWORD`. See the pinned tool's
[Apple setup](https://github.com/crmne/native-packages/blob/v0.10.0/docs/apple-notarization.md).
Without a complete configuration, do not advertise the DMG as notarized.

## Packaging checks

`native-packages.yaml` defines application-specific inputs, runtime dependencies
and recipes. Common builders use the pinned
[native-packages 0.10.0](https://github.com/crmne/native-packages/tree/v0.10.0) gem.
The tool's repository is an intentional dependency, not the Sonicfast release feed.

The Packaging workflow builds the current checkout for both Linux architectures,
packages its own archive, starts the AppImage with `--version`, checks its update
metadata and tests DEB/RPM install/removal in Ubuntu 24.04, Debian 13 and Fedora
containers. These checks inspect runtime libraries and files; they do not establish
physical audio or desktop behavior. CI builds and exercises Windows installer
fixtures on Windows and checks the macOS bundle on macOS. The optional Flatpak
job cannot block native release publication; the manual Flatpak-from-release
workflow can create a later bundle from a checksum-verified Sonicfast archive.

To build packages from a local Linux archive matching the configured filename:

```sh
gem install native-packages --version 0.10.0 --no-document
bash .github/scripts/install-nfpm.sh
export PATH="$PWD/.cache/bin:$PATH"
native-packages build --version VERSION --target linux-amd64 --target appimage-amd64 \
  --defer-recipes --output dist/native-packages
bash packaging/test-install.sh ubuntu:24.04 dist/native-packages
```

Select `--target linux-arm64 --target appimage-arm64` on ARM64. Input archives contain the executable,
README, license, desktop entry, icon and Omarchy integration under
`sonicfast-vVERSION-TARGET/`, as assembled in the release/packaging workflows.
The AUR/Homebrew templates can be adapted manually; no fork-owned AUR package,
Homebrew tap or Flathub listing is configured. Nothing pushes to upstream package
repositories.

The Nix development shell remains available. The package derivation's fixed-output
vendor hash needs refreshing and a full Nix build after lockfile changes before
Nix distribution. The dedicated CI job is opt-in with `ENABLE_NIX_PACKAGE=true`;
no public Sonicfast binary cache is configured by default.

## Build the documentation locally

With Ruby 4 and Bundler 4.0.16:

```sh
cd docs
bundle install
bundle exec jekyll build
cd ..
python3 .github/scripts/test-docs.py
python3 .github/scripts/check-docs.py docs/_site --baseurl /sonicfast
```

The link check validates internal targets, anchors, the project-site prefix and
inherited upstream site/analytics URLs. It does not request external links.
`docs/design/` is an engineering history archive, excluded from the published site
and linked through GitHub. Historical evidence and upstream credits retain their
original terminology.
