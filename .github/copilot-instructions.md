# Sonicfast repository guidance

Read AGENTS.md, DESIGN.md and CONTRIBUTING.md before changing code or reviewing.
Sonicfast is the OpenSubsonic fork of Spotifast. The default executable uses
Libresonic; sonicfast-legacy is retained for regression comparisons.

Keep UI actions separate from background HTTP, decoding and filesystem work.
Protect server secrets, keep profiles separate and preserve native audio behavior.
Keep upstream notices and historical evidence accurate. Do not apply inherited
Spotify-only product restrictions to this fork. Use this repository for issues,
releases and documentation. Run the checks listed in CONTRIBUTING.md.
