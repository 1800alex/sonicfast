# MPRIS artwork correction

Date: 2026-10-23. The owner reported missing covers in GNOME's media panel while
Firefox's music covers displayed correctly.

The inherited Linux branch assumed every remote artwork URL was directly
fetchable by MPRIS clients. OpenSubsonic uses credential-free `sonic-art:` IDs,
which only Sonicfast's authenticated artwork adapter can resolve. Sending that
ID as `mpris:artUrl` left the desktop unable to load the cover.

Linux now uses the existing artwork cache file for these private IDs, as it
already did for embedded local covers. The media-controls adapter publishes the
escaped `file://` URI once the file exists. Pending artwork has no art URL; its
completion updates metadata without needing a track change. Only HTTP(S) URLs
are eligible as public fallbacks; internal artwork IDs and authenticated music
server URLs are not exported. The authenticated artwork write is awaited on its
background worker before Ready/repaint, closing the file-publication race.

The regression runs the actual app artwork loader and native MPRIS publisher on
a private D-Bus session, using a loopback authenticated Subsonic artwork fixture
and a generated PNG. It checks initial missing-art metadata, arrival on the same
paused song, a readable file with exact PNG bytes, escaping spaces and `#` in the
cache path, replacement for a second song, clearing art for a coverless song, and
two server requests rather than a request each frame. No personal server,
credentials, speaker output or desktop media session is used.

The focused Linux private-bus test and `make verify` passed, including formatting,
policy checks, Clippy, default-feature tests (1,397 passed, five ignored),
all-feature tests (1,437 passed, five ignored), integration tests and Rust docs.
The native default-feature debug build, production Jekyll build and generated
site checks also passed. Rust commands used the installed stable override
described in the Phase 0 report.

GNOME Shell's rendered panel and other operating systems need a separate visual
check; inspecting the actual D-Bus metadata is not that check. An additional
native GdkPixbuf decode probe could not run because its Glycin loader's
Bubblewrap sandbox exited early inside the development container. The regression
does verify the exported file's exact PNG bytes and decodes them with the app's
image library.

Reference: [MPRIS metadata map](https://specifications.freedesktop.org/mpris/latest/Track_List_Interface.html#Mapping:Metadata_Map).
