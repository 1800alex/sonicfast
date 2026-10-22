---
title: Download and build
description: Release downloads and native build prerequisites.
nav_order: 2
---

Get Sonicfast packages from the
[Sonicfast releases page](https://github.com/1800alex/sonicfast/releases).
Use assets from this repository; upstream Spotifast releases contain a different application.
If there is no Sonicfast release for your platform, build from source.

Release automation targets Linux x86-64 and ARM64, macOS universal, and Windows
x86-64 and ARM64. Linux archives and AppImages require a glibc 2.39 or newer
system, such as Ubuntu 24.04. AppImages need the system's native GUI/audio libraries;
they are not fully self-contained. DEB/RPM packages declare these dependencies.
A workflow target is not a claim of manual testing on that platform.

Automatic application updates are disabled. Install a new release manually.
Compare downloaded files with the release's `checksums.txt` to detect corruption.
These checksums are not a publisher signature. No AUR package, Homebrew tap,
Flathub listing or public Nix cache is currently configured for this fork.

## Build from source

Install the Rust toolchain pinned in `rust-toolchain.toml`. On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config cmake clang libclang-dev \
  libasound2-dev libpipewire-0.3-dev libxkbcommon-dev libwayland-dev \
  libgl1-mesa-dev
git clone https://github.com/1800alex/sonicfast.git
cd sonicfast
cargo build --locked --release --bin sonicfast
./target/release/sonicfast
```

Cargo downloads Libresonic from crates.io. No sibling checkout is needed.
PipeWire development headers are required, including when MilkDrop is disabled.
`--no-default-features` omits MilkDrop; it does not remove native audio support.
For a debug build use `make build`; `make release` builds the optimized binary.

On macOS, install the Xcode command line tools and CMake. Windows builds use
MSVC, CMake/Ninja, libclang and a matching static GLEW vcpkg triplet for MilkDrop.
The repository's CI and release workflows record the platform setup. See
[CONTRIBUTING.md](https://github.com/1800alex/sonicfast/blob/main/CONTRIBUTING.md)
for development checks and [packaging]({{ '/packaging/' | relative_url }}) for release details.
