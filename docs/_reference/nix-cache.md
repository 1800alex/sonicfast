---
title: Nix development
description: Development shell and package validation status.
nav_order: 7
---

The source repository includes a Nix flake and development shell:

```sh
nix develop
cargo build --locked --bin sonicfast
```

No public Sonicfast binary cache is configured. The inherited package derivation
uses a fixed-output Cargo vendor hash, which must be refreshed and validated after
lockfile changes before building or distributing a Nix package. The dedicated Nix
CI job is opt-in through the repository variable `ENABLE_NIX_PACKAGE` set to `true`.
This is separate from the development shell.

Do not use an upstream cache or its trust key as a Sonicfast release guarantee.
Follow the [contributor guide](https://github.com/1800alex/sonicfast/blob/main/CONTRIBUTING.md)
when updating the derivation.
