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
| Your optional Snapcast server | Processed audio when Send is enabled; receiver identity, clock messages and received audio when Receive is enabled |
| Selected AirPlay receiver | Processed audio, volume and timing messages after enabling LAN casting |
| Selected Cast receiver | Song metadata, playback controls and a temporary URL for the selected original; the receiver fetches it from this computer |
| LRCLIB | Artist, title, album and duration for lyric lookup |
| Visualizer preset sources | Optional preset downloads used by MilkDrop |
| Your operating system's credential store | Remembered server secrets |
| Your selected download directory | Original music files you download |

Snapcast is disabled by default and requires independent consent to unencrypted TCP
on a trusted network. Music-server credentials are never sent to Snapserver.
The generated receiver ID is stored in settings and identifies this app to that
server. Received audio does not create local song history or scrobbles.

AirPlay/Cast sending also requires explicit LAN consent. AirPlay Classic is
unencrypted. Cast control uses authenticated TLS (Google device authentication
or an explicitly entered certificate fingerprint for an owner-operated target).
The selected original is served over LAN HTTP with a per-track bearer path,
restricted to that receiver's IP; it is revoked on replacement/disconnect/shutdown.
Music-server credentials and original authenticated URLs are never shared.

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
