# Migration inventory

Pinned upstream source: `e86d3de0cb72edba8f24b447218daa7f3af1f127`. Generated from that source, including tests
and comments, so counts locate coupling rather than measure runtime dependencies.

Reproduce the raw references with:

```sh
rg -n 'librespot_|SpotifyUri|SpotifyId|spotify:|api\.spotify\.com|accounts\.spotify\.com|spclient|pathfinder' src
```

| File | librespot lines | identity/URI lines | Spotify endpoint lines |
|---|---:|---:|---:|
| [`src/account_cache.rs`](../../src/account_cache.rs) | 0 | 1 | 0 |
| [`src/api/client.rs`](../../src/api/client.rs) | 0 | 18 | 3 |
| [`src/api/models.rs`](../../src/api/models.rs) | 0 | 12 | 0 |
| [`src/app.rs`](../../src/app.rs) | 2 | 980 | 4 |
| [`src/auth.rs`](../../src/auth.rs) | 0 | 0 | 2 |
| [`src/backend.rs`](../../src/backend.rs) | 7 | 21 | 0 |
| [`src/cast.rs`](../../src/cast.rs) | 1 | 0 | 1 |
| [`src/credentials.rs`](../../src/credentials.rs) | 2 | 0 | 0 |
| [`src/demo.rs`](../../src/demo.rs) | 2 | 97 | 2 |
| [`src/entrypoint.rs`](../../src/entrypoint.rs) | 0 | 3 | 0 |
| [`src/eq.rs`](../../src/eq.rs) | 1 | 0 | 0 |
| [`src/history.rs`](../../src/history.rs) | 0 | 24 | 0 |
| [`src/http.rs`](../../src/http.rs) | 2 | 0 | 0 |
| [`src/liked.rs`](../../src/liked.rs) | 0 | 2 | 0 |
| [`src/link.rs`](../../src/link.rs) | 0 | 55 | 0 |
| [`src/made_for_you.rs`](../../src/made_for_you.rs) | 2 | 12 | 2 |
| [`src/new_releases.rs`](../../src/new_releases.rs) | 1 | 9 | 2 |
| [`src/notch.rs`](../../src/notch.rs) | 0 | 5 | 0 |
| [`src/player.rs`](../../src/player.rs) | 12 | 37 | 1 |
| [`src/prerelease.rs`](../../src/prerelease.rs) | 0 | 12 | 0 |
| [`src/session_reads.rs`](../../src/session_reads.rs) | 59 | 169 | 18 |
| [`src/settings.rs`](../../src/settings.rs) | 11 | 5 | 0 |
| [`src/single_instance.rs`](../../src/single_instance.rs) | 0 | 15 | 0 |
| [`src/sink.rs`](../../src/sink.rs) | 14 | 3 | 0 |
| [`src/sonos.rs`](../../src/sonos.rs) | 1 | 9 | 5 |
| [`src/ui/collection.rs`](../../src/ui/collection.rs) | 0 | 57 | 0 |
| [`src/ui/dialogs.rs`](../../src/ui/dialogs.rs) | 0 | 5 | 0 |
| [`src/ui/friends.rs`](../../src/ui/friends.rs) | 0 | 6 | 0 |
| [`src/ui/home.rs`](../../src/ui/home.rs) | 0 | 7 | 0 |
| [`src/ui/keys.rs`](../../src/ui/keys.rs) | 0 | 1 | 0 |
| [`src/ui/prerelease.rs`](../../src/ui/prerelease.rs) | 0 | 8 | 0 |
| [`src/ui/search.rs`](../../src/ui/search.rs) | 0 | 1 | 0 |
| [`src/ui/show.rs`](../../src/ui/show.rs) | 0 | 1 | 0 |
| [`src/ui/sidebar.rs`](../../src/ui/sidebar.rs) | 0 | 7 | 0 |
| [`src/ui/widgets.rs`](../../src/ui/widgets.rs) | 0 | 4 | 0 |
| [`src/ui/winamp/mod.rs`](../../src/ui/winamp/mod.rs) | 0 | 1 | 0 |
| [`src/util.rs`](../../src/util.rs) | 0 | 14 | 0 |
| [`src/vis.rs`](../../src/vis.rs) | 8 | 1 | 0 |
| [`src/zeroconf.rs`](../../src/zeroconf.rs) | 5 | 0 | 0 |

## Pinned git sources

The full registry versions/checksums are in the baseline Cargo.lock. Unique git
revisions below come from the lockfile, not movable branch names.

| Repository | Locked revision |
|---|---|
| `git+https://github.com/crmne/cpal?rev=fcd8b6ebee9d822426fa8d6839cdd89af6e0d491` | `fcd8b6ebee9d822426fa8d6839cdd89af6e0d491` |
| `git+https://github.com/crmne/egui?rev=6ce633ba5f7f64d19ef6d204f9377da560bb546b` | `6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `git+https://github.com/crmne/fastframe?tag=v0.14.0` | `dfd2db7bc6a5ae0c62a98383c84235f64da2cace` |
| `git+https://github.com/crmne/hyper-proxy2?rev=efca4811371ddae81c06be168de54194938d70b1` | `efca4811371ddae81c06be168de54194938d70b1` |
| `git+https://github.com/crmne/librespot?rev=8c68baffac5a3052cd0f24a6fbc1970131589790` | `8c68baffac5a3052cd0f24a6fbc1970131589790` |
| `git+https://github.com/crmne/projectm-rs?rev=93852ff7299e5a63ed1315437593fc70915c0863` | `93852ff7299e5a63ed1315437593fc70915c0863` |
| `git+https://github.com/crmne/winit?rev=0bed9dc0a7db02590a279fb8bac795fa39ee6948` | `0bed9dc0a7db02590a279fb8bac795fa39ee6948` |

## Exported boundary types

Names below are all top-level public structs, enums and aliases (including
crate-visible declarations) in the audited modules at the pinned source.
Neutral-looking containers can hold Spotify DTOs/URIs. This inventory does not
mean every helper must be replaced. Function signatures exposing Session and
implicit identity semantics are discussed in the phase report.

| Module | Public structs, enums and type aliases |
|---|---|
| `src/api/models.rs` | `Page`, `PositionedPage`, `CursorPage`, `Cursors`, `Image`, `ExternalUrls`, `Followers`, `ArtistRef`, `Artist`, `Album`, `Copyright`, `Track`, `ExternalIds`, `LinkedTrack`, `ResumePoint`, `Episode`, `Show`, `Owner`, `TrackCount`, `Playlist`, `PlayableItem`, `PlaylistItem`, `UserRef`, `SavedTrack`, `SavedAlbum`, `SavedShow`, `SavedEpisode`, `FollowedArtists`, `PlayHistory`, `Context`, `Device`, `DeviceList`, `PlaybackState`, `Queue`, `SearchResults`, `User`, `TopTracks`, `RelatedArtists`, `Recommendations`, `SnapshotId`, `ApiErrorBody`, `ApiErrorDetail` |
| `src/api/client.rs` | `ApiError`, `Result`, `TokenProvider`, `WebTokens`, `PlayRequest`, `NetActivity`, `ApiClient` |
| `src/api/gateway.rs` | `AccountId`, `PlaylistId`, `SessionState`, `PlaylistAccess`, `Operation`, `ApiGateway` |
| `src/api/mod.rs` | `ApiSource` |
| `src/auth.rs` | `Grant`, `Flow`, `TokenResponse`, `TokenEndpointError`, `StoredToken` |
| `src/credentials.rs` | `Slot`, `Grant`, `ProxyPassword`, `Error`, `Store`, `Lease`, `Loaded` |
| `src/player.rs` | `EngineConfig`, `Playback`, `RepeatMode`, `LocalTrack`, `LocalState`, `Interrupted`, `LoadSpec`, `PlaybackResume`, `PlayerCommand`, `EngineEvent`, `Notify`, `KeptPlayer`, `Engine`, `Heard`, `Rootlist`, `RootlistEntry` |
| `src/sink.rs` | `ErrorHook`, `DeviceHook`, `AudioControl`, `RodioSink`, `Saved` |
| `src/vis.rs` | `AudioTap`, `Tapped`, `Bar`, `Analyser`, `WideAnalyser` |
| `src/backend.rs` | `ApiResult`, `AuthStatus`, `RemoteAction`, `RecentsFor`, `ApiRequest`, `ApiResponse`, `PlaylistCacheRows`, `AutostartRequest`, `Command`, `LyricsRequest`, `Event`, `LocalPlayback`, `Backend` |
| `src/model.rs` | `TableItem`, `TableRowsCache`, `Page`, `QueueTab`, `Loadable`, `PagedList`, `RowSelection`, `RowPick`, `CursorList`, `Library`, `PlaylistEdit`, `HomeData`, `BackgroundActivity`, `HomeSnapshot`, `SearchFilter`, `SearchState`, `PlaylistPage`, `PlaylistCache`, `PlaylistCachePending`, `AlbumPage`, `DiscographyFilter`, `ArtistPage`, `RadioPage`, `ShowPage`, `TableSort`, `SortColumn`, `RowContext`, `DragTrack`, `PlayingFrom`, `DragEntry`, `Dialog`, `ToastKind`, `Toast`, `Action` |
| `src/settings.rs` | `LibraryShelf`, `LibrarySort`, `ThemeChoice`, `TrayIcon`, `HomeShelfSettings`, `HomeSettings`, `PlayerBarVis`, `VisMode`, `LanguageChoice`, `ProxyMode`, `ProxyPreferences`, `UserEqPreset`, `Settings`, `ProxyConfig`, `ManualProxy`, `CachedRootlist`, `SessionState`, `WindowMode` |
| `src/session_reads.rs` | `Failure`, `RemovedRow`, `UserProfile`, `FriendActivity`, `FriendNames` |
| `src/new_releases.rs` | `NewReleases` |
| `src/cast.rs` |  |
| `src/zeroconf.rs` | `ReceiverUpdates`, `Receiver`, `Info`, `Credentials` |
