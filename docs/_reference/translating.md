---
title: Translating Sonicfast
description: Translation catalogs and preview commands.
nav_order: 8
---

Sonicfast inherits gettext-style `.po` catalogs from Spotifast. The existing
interface has translations; newer server, scrobbling and download controls still
need translation work. Do not assume every current string is translated.

Edit the catalog for your language under `assets/i18n/`. Preserve `msgid`,
`msgid_plural`, context and placeholders such as `{count}` and `{track}`. Review
fuzzy entries before clearing the fuzzy flag. The source template is
`assets/i18n/sonicfast.pot`; `.github/scripts/update-translations.sh` refreshes it.

For inherited interface previews, explicitly run the legacy demo executable:

```sh
cargo run --locked --bin sonicfast-legacy --features demo -- --demo --demo-language es
```

The demo uses sample data and does not test the OpenSubsonic sign-in/backend.
Test new server controls against a disposable server fixture as well.
Run `make test` and report which language and screens you reviewed. File issues in
[1800alex/sonicfast](https://github.com/1800alex/sonicfast/issues), with credentials
and private server details removed.
