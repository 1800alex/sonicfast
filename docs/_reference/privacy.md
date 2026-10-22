---
title: Privacy
description: Where requests and local data go.
nav_order: 5
---

Sonicfast has no application telemetry or hosted Sonicfast backend. This
documentation site includes no analytics script.

| Destination | Data and purpose |
| --- | --- |
| Your configured music server | Authentication, library requests, playback streams, playlist/heart/rating changes and enabled scrobble reports |
| LRCLIB | Artist, title, album and duration for lyric lookup |
| Visualizer preset sources | Optional preset downloads used by MilkDrop |
| Your operating system's credential store | Remembered server secrets |
| Your selected download directory | Original music files you download |

Normal web hosting can produce access logs outside the application's control.
The server and external services have their own data-retention policies.

Configuration, session state, history, caches and logs are stored locally. Download
availability comes from filesystem checks, with no persistent download index.
Disconnect preserves saved sign-in; Remove deletes the selected connection and its
remembered secret. Connection details in `servers.json` contain no password/API key.
Neither operation deletes downloaded music. The default application's
self-updater is disabled; install releases manually.

HTTPS certificates are validated. HTTP servers require an explicit insecure-HTTP
opt-in. Authenticated requests do not follow redirects. Never publish API keys,
passwords or complete authenticated URLs in issues or logs.

The separate legacy comparison executable retains its original integrations;
these OpenSubsonic guides describe the default Sonicfast application.
