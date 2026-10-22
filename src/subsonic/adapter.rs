//! Scoped neutral IDs mapped into the retained display models, never wire DTOs.
use crate::api::{ApiError, models as v};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use libresonic::{self as l, MediaId};

pub fn key(id: &MediaId) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(id).expect("media ID serializes"))
}
pub fn id(key: &str) -> Result<MediaId, l::Error> {
    let encoded = key.rsplit(':').next().unwrap_or(key);
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| l::Error::InvalidIdentifier)?;
    serde_json::from_slice(&bytes).map_err(|_| l::Error::InvalidIdentifier)
}
pub fn uri(id: &MediaId) -> String {
    let kind = match id.kind() {
        l::MediaKind::Track => "track",
        l::MediaKind::Album => "album",
        l::MediaKind::Artist => "artist",
        l::MediaKind::Playlist => "playlist",
        l::MediaKind::Artwork => "art",
    };
    format!("sonicfast:{kind}:{}", key(id))
}
pub fn images(id: Option<&MediaId>) -> Vec<v::Image> {
    id.map(|id| {
        vec![v::Image {
            url: format!("sonic-art:{}", key(id)),
            width: Some(512),
            height: Some(512),
        }]
    })
    .unwrap_or_default()
}
fn credits(artists: &[l::ArtistRef]) -> Vec<v::ArtistRef> {
    artists
        .iter()
        .map(|a| v::ArtistRef {
            id: a.id.as_ref().map(key),
            uri: a.id.as_ref().map(uri),
            name: a.name.clone(),
        })
        .collect()
}
pub fn artist(a: &l::Artist) -> v::Artist {
    v::Artist {
        id: key(&a.id),
        uri: uri(&a.id),
        name: a.name.clone(),
        images: images(a.artwork.as_ref()),
        ..Default::default()
    }
}
pub fn album(a: &l::Album) -> v::Album {
    v::Album {
        id: key(&a.id),
        uri: uri(&a.id),
        name: a.title.clone(),
        images: images(a.artwork.as_ref()),
        artists: credits(&a.artists),
        total_tracks: a.track_count.map(|n| n.min(u32::MAX as u64) as u32),
        release_date: a.year.map(|y| y.to_string()),
        album_type: Some("album".into()),
        ..Default::default()
    }
}
pub fn playlist(p: &l::Playlist) -> v::Playlist {
    v::Playlist {
        id: key(&p.id),
        uri: uri(&p.id),
        name: p.title.clone(),
        description: p.description.clone(),
        public: p.public,
        owner: v::Owner {
            id: p.owner.as_ref().map(|owner| {
                if p.owned == Some(true) {
                    p.id.server().as_str().to_owned()
                } else {
                    format!(
                        "sonic-user:{}:{}",
                        p.id.server().as_str(),
                        URL_SAFE_NO_PAD.encode(owner)
                    )
                }
            }),
            display_name: Some(p.owner.clone().unwrap_or_else(|| "Unknown owner".into())),
            ..Default::default()
        },
        images: images(p.artwork.as_ref()),
        editable: p.editable,
        tracks: Some(v::TrackCount {
            total: p.track_count.unwrap_or(0).min(u32::MAX as u64) as u32,
        }),
        ..Default::default()
    }
}
pub fn playlist_details(p: &l::PlaylistDetails) -> v::Playlist {
    let mut value = playlist(&p.playlist);
    value.snapshot_id = Some(p.revision.clone());
    value
}
pub fn playlist_items(p: &l::PlaylistDetails, offset: u32) -> v::Page<v::PlaylistItem> {
    v::Page {
        items: p
            .entries
            .iter()
            .skip(offset as usize)
            .map(|t| v::PlaylistItem {
                item: t.as_ref().map(|t| v::PlayableItem::Track(track(t))),
                ..Default::default()
            })
            .collect(),
        offset,
        total: p.entries.len() as u32,
        limit: p.entries.len().saturating_sub(offset as usize) as u32,
        ..Default::default()
    }
}
/// Metadata is advisory: let the decoder probe tracks without a recognized hint.
/// In particular, an omitted suffix is not evidence of an unsupported codec.
pub fn playable(t: &l::Track) -> bool {
    let suffix = t
        .suffix
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_start_matches('.');
    let mime = t
        .content_type
        .as_deref()
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if ["mp3", "flac", "m4a", "mp4"]
        .iter()
        .any(|s| suffix.eq_ignore_ascii_case(s))
        || [
            "audio/mpeg",
            "audio/mp3",
            "audio/flac",
            "audio/x-flac",
            "audio/mp4",
            "audio/x-m4a",
            "audio/m4a",
            "audio/alac",
            "audio/x-alac",
        ]
        .iter()
        .any(|m| mime.eq_ignore_ascii_case(m))
    {
        return true;
    }
    // Only decline formats we can positively identify as unsupported. Unknown
    // or sparse server metadata must still be playable through stream probing.
    ![
        "aac", "ogg", "oga", "opus", "wav", "wave", "aiff", "aif", "wma", "ape",
    ]
    .iter()
    .any(|s| suffix.eq_ignore_ascii_case(s))
        && ![
            "audio/aac",
            "audio/ogg",
            "audio/opus",
            "audio/wav",
            "audio/x-wav",
            "audio/aiff",
            "audio/x-ms-wma",
        ]
        .iter()
        .any(|m| mime.eq_ignore_ascii_case(m))
}
pub fn track(t: &l::Track) -> v::Track {
    v::Track {
        id: Some(key(&t.id)),
        uri: uri(&t.id),
        name: t.title.clone(),
        user_rating: t.rating,
        original_file: Some(Box::new(v::OriginalFile {
            path: t.file_path.clone(),
            size: t.file_size,
            suffix: t.suffix.clone(),
        })),
        artists: credits(&t.artists),
        duration_ms: t.duration_ms.unwrap_or_default().min(u32::MAX as u64) as u32,
        album: Some(v::Album {
            id: t
                .album
                .as_ref()
                .and_then(|a| a.id.as_ref())
                .map(key)
                .unwrap_or_default(),
            uri: t
                .album
                .as_ref()
                .and_then(|a| a.id.as_ref())
                .map(uri)
                .unwrap_or_default(),
            name: t
                .album
                .as_ref()
                .map(|a| a.title.clone())
                .unwrap_or_default(),
            images: images(t.artwork.as_ref()),
            ..Default::default()
        }),
        track_number: t.track_number,
        disc_number: t.disc_number,
        is_playable: Some(playable(t)),
        ..Default::default()
    }
}
pub fn error(e: l::Error) -> ApiError {
    ApiError::Network(e.to_string())
}
pub fn page<T>(items: Vec<T>, offset: u32, next: Option<l::PageRequest>) -> v::Page<T> {
    let count = items.len() as u32;
    v::Page {
        items,
        total: offset + count + u32::from(next.is_some()),
        limit: next.map_or(count, |p| p.offset().saturating_sub(offset)),
        offset,
        next: next.map(|_| "more".into()),
    }
}

/// Neutral metadata for direct filesystem lookup, without another server read.
pub fn neutral_track(t: &v::Track) -> Result<l::Track, l::Error> {
    Ok(l::Track {
        id: id(&t.uri)?,
        title: t.name.clone(),
        artists: t
            .artists
            .iter()
            .map(|a| l::ArtistRef {
                id: a.id.as_deref().map(id).transpose().ok().flatten(),
                name: a.name.clone(),
            })
            .collect(),
        album: t.album.as_ref().map(|a| l::AlbumRef {
            id: id(&a.id).ok(),
            title: a.name.clone(),
        }),
        duration_ms: Some(u64::from(t.duration_ms)),
        track_number: t.track_number,
        disc_number: t.disc_number,
        content_type: None,
        suffix: t.original_file.as_ref().and_then(|f| f.suffix.clone()),
        artwork: None,
        favorite: false,
        rating: t.user_rating,
        file_path: t.original_file.as_ref().and_then(|f| f.path.clone()),
        file_size: t.original_file.as_ref().and_then(|f| f.size),
        play_count: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playlist_display_preserves_unavailable_slots_and_keeps_shared_owner_distinct() {
        let server = l::ServerId::new("server-one").unwrap();
        let song = serde_json::from_value::<l::Track>(serde_json::json!({
            "id": MediaId::new(server.clone(), l::MediaKind::Track, "a").unwrap(),
            "title":"A", "artists":[], "favorite":false
        }))
        .unwrap();
        let details = l::PlaylistDetails {
            playlist: l::Playlist {
                id: MediaId::new(server, l::MediaKind::Playlist, "p").unwrap(),
                title: "Shared".into(),
                track_count: Some(3),
                artwork: None,
                owner: Some("other".into()),
                editable: Some(true),
                owned: Some(false),
                description: None,
                public: None,
            },
            entries: vec![Some(song.clone()), None, Some(song)],
            revision: "snapshot".into(),
        };
        let p = playlist_details(&details);
        assert_eq!(p.editable, Some(true));
        assert!(!p.owned_by("server-one"));
        let page = playlist_items(&details, 1);
        assert_eq!((page.offset, page.total, page.items.len()), (1, 3, 2));
        assert!(page.items[0].playable().is_none());
        assert_eq!(
            id(page.items[1].playable().unwrap().uri()).unwrap().item(),
            "a"
        );
    }
    #[test]
    fn codec_hints_are_case_insensitive_and_sparse_metadata_can_be_probed() {
        let mut song = l::Track {
            id: MediaId::new(
                l::ServerId::new("one").unwrap(),
                l::MediaKind::Track,
                "song",
            )
            .unwrap(),
            title: "Song".into(),
            artists: vec![],
            album: None,
            duration_ms: None,
            track_number: None,
            disc_number: None,
            content_type: None,
            suffix: None,
            artwork: None,
            favorite: false,
            rating: None,
            file_path: None,
            file_size: None,
            play_count: None,
        };
        for (suffix, mime, expected) in [
            (Some("mp3"), None, true),
            (Some("FLAC"), None, true),
            (Some(" .MP3 "), None, true),
            (None, Some("Audio/FLAC; x=value"), true),
            (None, Some("audio/mpeg"), true),
            (None, None, true),
            (None, Some("application/octet-stream"), true),
            (Some("bin"), None, true),
            (Some("m4a"), None, true),
            (Some("M4A"), Some("audio/mp4"), true),
            (None, Some("Audio/X-M4A; codecs=mp4a.40.2"), true),
            (Some("OPUS"), None, false),
            (None, Some("audio/aac"), false),
        ] {
            song.suffix = suffix.map(str::to_owned);
            song.content_type = mime.map(str::to_owned);
            assert_eq!(
                playable(&song),
                expected,
                "suffix={suffix:?}, mime={mime:?}"
            );
            assert_eq!(track(&song).is_playable, Some(expected));
        }
    }
    #[test]
    fn display_keys_preserve_server_kind_and_opaque_identifiers() {
        let first = MediaId::new(
            l::ServerId::new("one").unwrap(),
            l::MediaKind::Track,
            "opaque:/雪?&=x",
        )
        .unwrap();
        let second = MediaId::new(
            l::ServerId::new("two").unwrap(),
            l::MediaKind::Track,
            first.item(),
        )
        .unwrap();
        assert_ne!(key(&first), key(&second));
        assert_eq!(id(&uri(&first)).unwrap(), first);
        assert!(id("https://host?password=secret").is_err());
        let art = MediaId::new(
            l::ServerId::new("one").unwrap(),
            l::MediaKind::Artwork,
            "cover",
        )
        .unwrap();
        assert!(!images(Some(&art))[0].url.contains("http"));
    }
}
