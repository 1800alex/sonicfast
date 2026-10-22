---
title: Server connections
description: Save several servers, connect to one at a time and refresh the library.
nav_order: 4
---

Sonicfast can save several OpenSubsonic/Subsonic connections. **Only one server is
connected at a time.** The signed-out screen lists saved connections. While
connected, open **Settings → Server → Manage connections** to reach the same list.

## Add a connection

Choose **Add connection**, then enter a name, server URL, username and password or
supported API key. Enable plaintext HTTP only for a trusted HTTP server. Choose
**Connect** to validate and save the connection. An empty name uses the server's
hostname.

Connection details are saved without the password or API key. Enable **Remember
sign-in** to keep the secret in the operating system's protected credential store.
With that option off, the list keeps the connection details and offers **Sign in**
when you return. Remembered connections offer **Connect**. A locked credential
store may require unlocking it or entering the secret again.

## Switch, disconnect or remove

Choose **Connect** or **Sign in** beside the desired server. Switching stops the
current playback session and background downloads, saves its queue and unfinished
work, and clears the previous server's views before connecting. Returning to a
connection restores its own queue paused. Already downloaded music stays on disk.

**Disconnect** stops the session and returns to the connection list. It preserves
remembered credentials for an explicit reconnect and disables automatic reconnect
on the next launch. If you quit while connected with remembered credentials,
Sonicfast reconnects to that last connection on startup.

**Remove** asks for confirmation, removes the saved connection and deletes its
remembered credential. Removing the active connection also stops its session.
Removing an inactive connection keeps the current one playing. Downloaded music
is kept. If the configuration or credential store is unavailable, Sonicfast shows
the failure so you can retry.

The account menu's **Sign out** revokes the current remembered sign-in while
leaving its connection details in the list. Use **Disconnect** when you want to
keep that sign-in.

## Edit a connection

Choose **Edit**. With Remember sign-in enabled, leave the secret blank to use the
stored credential. Connect to apply the edited settings. Closing the form discards
its entered secret and leaves the current session running.

Changing the URL, username or authentication method, or entering a replacement
API key, creates a separate connection
identity, preserving the original entry and its queue. Remove the original entry
when it is no longer needed. Renaming a connection keeps its identity.

## Sync now

Choose **Settings → Server → Sync now** after changing library metadata, playlists,
hearts or ratings through another client. Sonicfast reloads artists, albums,
playlists, song feedback and the current page. It replaces stale catalogue reads
and checks visible download paths again. Playback, its queue, downloads and
scrobbling continue.

Sync uses the server's existing library metadata. It does **not** start an
administrative music-directory scan, download the whole library or create an
on-disk library index. Normal browsing still loads pages as needed. Changes you
make inside Sonicfast already refresh their affected views; manual Sync is mainly
for changes made elsewhere.
