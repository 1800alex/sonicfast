# Contributing to Sonicfast

Sonicfast preserves upstream Spotifast's native interface while replacing its backend
with OpenSubsonic and Libresonic. Read [DESIGN.md](DESIGN.md) and the current
[Phase 0 report](docs/design/phase-0.md). The upstream Spotify-only product and
contribution restrictions are superseded by this fork's scope.

Keep rendering responsive, credentials protected, and existing audio/device
processing intact. Test behavior that can regress. No browser, telemetry or
hosted backend is planned. Snapcast audio transport is authorized from 0.15.0 (ADR 011);
AirPlay Classic and Cast outputs are authorized by ADR 012; ADR 013 adds native
PIN-paired encrypted single-receiver AirPlay audio. General remote queue
coordination remains outside scope. Avoid unrelated
settings and UI redesigns. Keep review screenshots outside git.

Libresonic is a separate [public repository](https://github.com/1800alex/libresonic),
consumed from crates.io (currently 0.1.0). Registry dependencies and inherited
pinned `crmne` forks are permitted. Explain
new dependencies and git overrides; do not replace them merely to pass checks.
Keep upstream license notices. Keep changes local unless commits/pushes are
requested, and report actual validation and limitations.

Public repository setup and the initial upload order are recorded in
[public source preparation](docs/design/public-source-preparation.md).

Use `make help` to list development targets. `make build`/`make debug` and
`make release` build the Sonicfast binary; `make run` and `make run-release`
launch it and accept `ARGS`. `make check` provides a quick type check.

`make verify` runs the following checks in order, including under `make -j`.
Individual targets are `fmt-check`, `policy`, `lint`, `lint-all`, `test`,
`test-all`, `test-doc` and `doc`. Use `make fmt` to apply formatting.
`CARGO` and `PYTHON` are overridable, for example
`make test CARGO='cargo +stable'`; normal Cargo environment variables are retained.

Equivalent Cargo/Python commands:

```sh
python3 .github/scripts/check-dependency-sources.py
python3 .github/scripts/check-review-evidence.py
python3 .github/scripts/test-policy-checks.py
python3 packaging/test-launchers.py
python3 packaging/test-release.py
python3 packaging/test-release-names.py
python3 packaging/flatpak/test-metainfo.py
python3 .github/scripts/test-docs.py
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
cargo test --locked --all-features --doc
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
```

Linux needs ALSA, PipeWire, XKB, Wayland and OpenGL development packages.
On Debian/Ubuntu, install the native build prerequisites with:

```sh
sudo apt install build-essential pkg-config cmake clang libclang-dev \
  libasound2-dev libpipewire-0.3-dev libxkbcommon-dev libwayland-dev \
  libgl1-mesa-dev
```

`libpipewire-0.3-dev` is required by the default native audio build, including
when building with `--no-default-features` (that flag only removes MilkDrop).
MilkDrop also needs CMake, a C++ compiler and libclang. Launcher tests need Ruby;
command compatibility tests need `dbus-run-session`. `--no-default-features`
omits MilkDrop and must be reported as reduced coverage. Check the library
separately in its own checkout. Rust remains pinned to 1.98.0; report any override.

Credential changes require a native store round trip on an unlocked desktop.
Packaging changes require the relevant packaging checks. Lockfile changes also
require refreshing/validating the Nix vendor hash before a Nix release. See [PACKAGING.md](PACKAGING.md) for fork-owned release workflows, documentation
builds and package checks. The default application keeps self-updates disabled;
the inherited upstream updater key is not used to sign Sonicfast releases.

Contributions use the project's MIT license.

Snapcast changes require `make snapcast-test` in addition to unit/Clippy checks.
It builds the actual sender/native receiver probe and tests both PCM and FLAC
against pinned Docker Snapcast packages, without a private library or audio
hardware. `integration/snapcast/README.md` documents the independent manual smoke
project. Do not substitute virtual-device alignment for a physical-speaker claim.

AirPlay and Cast changes also require `make casting-test`. It checks the native
ALAC sender and actual app login/decode/output paths against Shairport Sync and
a certificate-pinned software Cast receiver, with isolated audio captures. Retain
Cast authentication and temporary HTTP lease tests. See
[integration/casting/README.md](integration/casting/README.md) for prerequisites
and limits; software targets do not establish Google SDK or hardware compatibility.
