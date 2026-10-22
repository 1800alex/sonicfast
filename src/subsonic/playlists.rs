//! Serialized server playlist edits through the retained action/response flow.
use super::{Event as S, adapter as m};
use crate::{
    api::ApiError,
    backend::{ApiRequest as A, ApiResponse as R, Event as E},
};
use libresonic::{self as l, MusicService, opensubsonic::Client};

pub(super) enum Write {
    Rate {
        uri: String,
        rating: u8,
    },
    Create {
        name: String,
        public: bool,
        uris: Vec<String>,
        description: String,
    },
    Api(A),
    Rows {
        id: String,
        rows: Vec<(u32, String)>,
        revision: Option<String>,
    },
}
pub(super) fn is_write(a: &A) -> bool {
    matches!(
        a,
        A::SetSaved { .. }
            | A::CreatePlaylist { .. }
            | A::UpdatePlaylist { .. }
            | A::AddToPlaylist { .. }
            | A::RemoveFromPlaylist { .. }
            | A::ReorderPlaylist { .. }
            | A::FollowPlaylist { .. }
    )
}
fn songs(uris: &[String]) -> Result<Vec<l::MediaId>, l::Error> {
    uris.iter().map(|u| m::id(u)).collect()
}
fn unchanged(p: &l::PlaylistDetails, revision: Option<&str>) -> Result<(), l::Error> {
    if revision.is_some_and(|s| s != p.revision) {
        Err(l::Error::Conflict)
    } else {
        Ok(())
    }
}
fn complete(p: &l::PlaylistDetails) -> Result<Vec<l::MediaId>, l::Error> {
    p.entries
        .iter()
        .map(|t| {
            t.as_ref()
                .map(|t| t.id.clone())
                .ok_or(l::Error::Unsupported)
        })
        .collect()
}
async fn add(
    client: &Client,
    id: &str,
    uris: &[String],
    position: Option<u32>,
) -> Result<(), l::Error> {
    let key = m::id(id)?;
    let songs = songs(uris)?;
    if let Some(position) = position {
        let p = client.playlist(&key).await?;
        let mut all = complete(&p)?;
        if position as usize > all.len() {
            return Err(l::Error::Conflict);
        }
        all.splice(position as usize..position as usize, songs);
        client.replace_playlist(&key, &all).await?;
        Ok(())
    } else {
        client
            .update_playlist(
                &key,
                &l::PlaylistUpdate {
                    add: songs,
                    ..Default::default()
                },
            )
            .await
    }
}
async fn remove(
    client: &Client,
    id: &str,
    rows: &[(u32, String)],
    revision: Option<&str>,
) -> Result<(), l::Error> {
    let key = m::id(id)?;
    let p = client.playlist(&key).await?;
    unchanged(&p, revision)?;
    for (at, uri) in rows {
        let expected = m::id(uri)?;
        if p.entries
            .get(*at as usize)
            .and_then(Option::as_ref)
            .map(|t| &t.id)
            != Some(&expected)
        {
            return Err(l::Error::Conflict);
        }
    }
    client
        .update_playlist(
            &key,
            &l::PlaylistUpdate {
                remove: rows.iter().map(|(at, _)| *at).collect(),
                ..Default::default()
            },
        )
        .await
}
pub(super) fn failure(write: Write, error: ApiError) -> Vec<E> {
    let response = match write {
        Write::Rate { uri, rating } => {
            return vec![E::Subsonic(S::SongRated {
                uri,
                rating,
                result: Err(error.to_string()),
            })];
        }
        Write::Api(A::SetSaved { uris, saved }) => R::SavedChanged {
            uris,
            saved,
            result: Err(error),
        },
        Write::Rows { id, .. } => R::PlaylistItemsChanged {
            id,
            message: String::new(),
            result: Err(error),
        },
        Write::Create { .. } | Write::Api(A::CreatePlaylist { .. }) => {
            R::PlaylistCreated(Err(error))
        }
        Write::Api(A::UpdatePlaylist { id, .. }) => R::PlaylistUpdated {
            id,
            result: Err(error),
        },
        Write::Api(A::FollowPlaylist { id, follow }) => R::PlaylistFollowChanged {
            id,
            followed: follow,
            result: Err(error),
        },
        Write::Api(
            A::AddToPlaylist { playlist_id, .. }
            | A::RemoveFromPlaylist { playlist_id, .. }
            | A::ReorderPlaylist { playlist_id, .. },
        ) => R::PlaylistItemsChanged {
            id: playlist_id,
            message: String::new(),
            result: Err(error),
        },
        _ => return vec![],
    };
    let changed = match &response {
        R::PlaylistItemsChanged { id, .. } => Some(id.clone()),
        _ => None,
    };
    let mut events = vec![E::Api(Box::new(response))];
    if let Some(id) = changed {
        events.push(E::Subsonic(S::PlaylistChanged(id)));
    }
    events
}

pub(super) fn read_failure(request: A, account_id: Option<String>) -> Vec<E> {
    let error = ApiError::Network("Server requests are busy; try again shortly".into());
    let response = match request {
        A::Contains { uris } => R::Contains {
            uris,
            result: Err(error),
        },
        A::SavedTracks { offset, generation } => R::SavedTracks {
            offset,
            generation,
            account_id,
            result: Err(error),
        },
        A::MyPlaylists { offset, generation } => R::MyPlaylists {
            offset,
            generation,
            result: Err(error),
        },
        A::Playlist { id, generation } => R::Playlist {
            id,
            generation,
            result: Err(error),
        },
        A::PlaylistItems {
            id,
            offset,
            generation,
        } => R::PlaylistItems {
            id,
            offset,
            generation,
            result: Err(error),
        },
        A::PlaylistSample { id, generation, .. } => R::PlaylistSample {
            id,
            generation,
            result: Err(error),
        },
        A::SearchPlaylists { query, serial } => R::SearchPlaylists {
            query,
            serial,
            result: Err(error),
        },
        A::CheckPlaylistDuplicates {
            playlist_id,
            playlist_name,
            items,
            position,
        } => R::PlaylistDuplicatesChecked {
            playlist_id,
            playlist_name,
            items,
            position,
            result: Err(error),
        },
        _ => return vec![E::Error(error.to_string())],
    };
    vec![E::Api(Box::new(response))]
}
pub(super) async fn write(client: &Client, write: Write) -> Vec<E> {
    let write = match write {
        Write::Api(A::CreatePlaylist {
            name,
            public,
            description,
        }) => Write::Create {
            name,
            public,
            description,
            uris: vec![],
        },
        other => other,
    };
    let mut changed = None;
    let mut notices = Vec::new();
    let response = match write {
        Write::Rate { uri, rating } => {
            let result = match m::id(&uri) {
                Ok(id) => client.set_song_rating(&id, rating).await,
                Err(e) => Err(e),
            }
            .map_err(|e| e.to_string());
            return vec![E::Subsonic(S::SongRated {
                uri,
                rating,
                result,
            })];
        }
        Write::Api(A::SetSaved { uris, saved }) => {
            let result = async {
                let ids = songs(&uris)?;
                client.set_favorite_songs(&ids, saved).await
            }
            .await
            .map_err(m::error);
            R::SavedChanged {
                uris,
                saved,
                result,
            }
        }
        Write::Create {
            name,
            public,
            uris,
            description,
        } => {
            let result = async {
                let ids = songs(&uris)?;
                client.create_playlist(&name, &ids).await
            }
            .await;
            let result = match result {
                Ok(mut p) => {
                    if p.playlist.public != Some(public) || !description.is_empty() {
                        let result = client
                            .update_playlist(
                                &p.playlist.id,
                                &l::PlaylistUpdate {
                                    public: Some(public),
                                    description: (!description.is_empty()).then_some(description),
                                    ..Default::default()
                                },
                            )
                            .await;
                        if let Err(e) = result {
                            notices.push(E::Error(format!("Playlist created, but its visibility/comment could not be updated: {e}")));
                        } else {
                            p.playlist.public = Some(public);
                        }
                    }
                    changed = Some(m::key(&p.playlist.id));
                    Ok(m::playlist_details(&p))
                }
                Err(e) => Err(m::error(e)),
            };
            R::PlaylistCreated(result)
        }
        Write::Api(A::UpdatePlaylist {
            id,
            name,
            description,
            public,
        }) => {
            let result = async {
                let key = m::id(&id)?;
                client
                    .update_playlist(
                        &key,
                        &l::PlaylistUpdate {
                            name,
                            description,
                            public,
                            ..Default::default()
                        },
                    )
                    .await
            }
            .await
            .map_err(m::error);
            R::PlaylistUpdated { id, result }
        }
        Write::Api(A::FollowPlaylist { id, follow }) => {
            let result = if follow {
                Err(l::Error::Unsupported)
            } else {
                match m::id(&id) {
                    Ok(key) => client.delete_playlist(&key).await,
                    Err(e) => Err(e),
                }
            };
            R::PlaylistFollowChanged {
                id,
                followed: follow,
                result: result.map_err(m::error),
            }
        }
        Write::Rows { id, rows, revision } => {
            let result = remove(client, &id, &rows, revision.as_deref()).await;
            changed = Some(id.clone());
            R::PlaylistItemsChanged {
                id,
                message: "Playlist updated".into(),
                result: result.map(|_| None).map_err(m::error),
            }
        }
        Write::Api(a) => {
            let (id, result) = match a {
                A::AddToPlaylist {
                    playlist_id,
                    uris,
                    position,
                    ..
                } => {
                    let result = add(client, &playlist_id, &uris, position).await;
                    (playlist_id, result)
                }
                A::RemoveFromPlaylist {
                    playlist_id,
                    uris,
                    snapshot_id,
                } => {
                    let result = async {
                        let key = m::id(&playlist_id)?;
                        let p = client.playlist(&key).await?;
                        unchanged(&p, snapshot_id.as_deref())?;
                        let ids = songs(&uris)?;
                        let remove = p
                            .entries
                            .iter()
                            .enumerate()
                            .filter_map(|(i, t)| {
                                t.as_ref().filter(|t| ids.contains(&t.id)).map(|_| i as u32)
                            })
                            .collect();
                        client
                            .update_playlist(
                                &key,
                                &l::PlaylistUpdate {
                                    remove,
                                    ..Default::default()
                                },
                            )
                            .await
                    }
                    .await;
                    (playlist_id, result)
                }
                A::ReorderPlaylist {
                    playlist_id,
                    range_start,
                    insert_before,
                    snapshot_id,
                } => {
                    let result = async {
                        let key = m::id(&playlist_id)?;
                        let p = client.playlist(&key).await?;
                        unchanged(&p, snapshot_id.as_deref())?;
                        let mut ids = complete(&p)?;
                        let from = range_start as usize;
                        let to = insert_before as usize;
                        if from >= ids.len() || to > ids.len() {
                            return Err(l::Error::Conflict);
                        }
                        let song = ids.remove(from);
                        ids.insert(if to > from { to - 1 } else { to }, song);
                        client.replace_playlist(&key, &ids).await?;
                        Ok(())
                    }
                    .await;
                    (playlist_id, result)
                }
                _ => return vec![],
            };
            changed = Some(id.clone());
            R::PlaylistItemsChanged {
                id,
                message: "Playlist updated".into(),
                result: result.map(|_| None).map_err(m::error),
            }
        }
    };
    let mut events = vec![E::Api(Box::new(response))];
    events.append(&mut notices);
    if let Some(id) = changed {
        events.push(E::Subsonic(S::PlaylistChanged(id)));
    }
    events
}
