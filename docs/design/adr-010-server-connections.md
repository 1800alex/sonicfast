# ADR 010: saved server connections and manual metadata refresh

Date: 2026-10-22. Status: accepted for the requested connection-list follow-up.

## Context

The non-secret profile file already held a list, but the connected UI offered
sign-out as the way to change servers. Sign-out revoked the current credential.
The owner requested multiple saved connections, one active at a time, and a
manual refresh control without making library browsing slow.

## Decision

Keep the existing profile IDs and `servers.json` layout. Each successful login
saves a connection. Passwords/API keys remain in the native store under the
existing application-state scope and profile ID; session-only secrets stay in
memory. The optional `credential_cleanup` flag records that a profile may have
stored credentials even after sign-out. Older profile files load without it.

Expose the saved list on the signed-out screen and in Settings → Server → Manage
connections. Share the same add/edit form. Views emit actions; login, profile
writes and native-store operations remain on backend workers.

Separate Disconnect from Sign out and Remove. Disconnect preserves credentials
and clears the durable auto-connect selection. Sign out revokes remembered login
and keeps connection details. Remove first persists restoration revocation, then
deletes a remembered credential on the bounded native worker before removing the
entry. A failure leaves the entry available for retry. An entry that never stored
credentials can be removed without opening the native store. Credential-removal
jobs are independent of connection generations, so changing servers does not
cancel deletion of an unrelated credential.

Only one client/player session exists. Before changing it, checkpoint its own
queue, listening progress and unfinished downloads. Stop that session, cancel its
requests, clear artwork routing and views, then connect the next server. Returning
to a connection restores its queue paused. Changing endpoint, username or auth
method, or entering a replacement API key, creates a new identity and keeps the original entry; a name-only edit
preserves the identity. Keep recent-history display scoped to the active profile.

A remembered reconnect reads the existing secret without rewriting it. Cancelling
a newly entered credential write restores the previous native-store value rather
than deleting a previously working sign-in. Entered form secrets are cleared on
connect, cancellation, selection changes and closing the form.

Sync now cancels only catalogue reads and reloads artists, albums, playlists,
feedback, the current page and any existing search. Preserve optimistic playlist
writes and reissue their confirmation reads. Do not navigate away from the current
page to repeat an old search. Playback/queue resolution, writes, downloads and
scrobbling keep their generation and continue. This is a metadata refresh, not
an administrative server scan or complete library download.

Hide By You and ignore its old preference in Sonicfast. Keep the legacy comparison
executable's behavior and serialized settings compatible.

## Validation and limits

See [connection-list validation](connections-and-sync.md). These changes introduce
no dependency or library API changes. They do not add concurrent servers, casting,
remote transfer or synchronization between players. User preferences and the
configured download root remain application-wide; queue/resume state is scoped to
each connection.
