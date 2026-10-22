use super::{
    QueueAction, Request, adapter as m,
    profile::{Profile, Profiles, Secrets},
};
use crate::{
    api::models as v,
    backend::{self as b, ApiRequest as A, ApiResponse as R, Event as E},
    images::ArtLoader,
    paths::AppDirs,
    player::{EngineConfig, LoadSpec, LocalState, LocalTrack, Playback, PlayerCommand as P},
};
use libresonic::playback::{
    Phase, Player,
    queue::{Queue, Repeat},
};
use libresonic::{self as l, MusicService, opensubsonic::Client};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};

enum Outcome {
    Login(Profile, Arc<Client>, Option<String>),
    Events(Vec<E>),
    PlaylistWrite(Vec<E>),
    DownloadTracks(Result<Vec<l::Track>, String>),
    Load(
        u64,
        LoadSpec,
        Result<(Vec<Option<l::Track>>, Option<l::MediaId>), String>,
    ),
    Queue(u64, QueueAction, Result<Vec<l::Track>, String>),
}
type DownloadQueues = std::collections::HashMap<std::path::PathBuf, super::downloads::Pending>;

#[derive(serde::Serialize, serde::Deserialize)]
struct Resume {
    queue: Queue,
    position_ms: u64,
    #[serde(default)]
    source_positions: Vec<usize>,
    #[serde(default)]
    listening: l::playback::scrobble::Listening,
    #[serde(default)]
    downloads: DownloadQueues,
}
fn resume_path(dirs: &AppDirs, id: &str) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    dirs.state
        .join(format!("queue-{:x}.json", Sha256::digest(id)))
}
fn emit(tx: &std::sync::mpsc::Sender<E>, wake: &b::Waker, event: E) {
    let _ = tx.send(event);
    wake.wake();
}
fn playable(t: &l::Track) -> bool {
    m::playable(t)
}
async fn resolve(
    client: &Client,
    spec: &LoadSpec,
) -> Result<(Vec<Option<l::Track>>, Option<l::MediaId>), String> {
    let source = spec
        .context_uri
        .as_deref()
        .map(m::id)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut tracks = Vec::new();
    let mut playlist_entries = None;
    if !spec.uris.is_empty() {
        if spec.uris.len() > 2000 {
            return Err("Select at most 2000 songs at once".into());
        }
        for uri in &spec.uris {
            let id = m::id(uri).map_err(|e| e.to_string())?;
            match id.kind() {
                l::MediaKind::Track => {
                    tracks.push(client.song(&id).await.map_err(|e| e.to_string())?)
                }
                l::MediaKind::Album => {
                    tracks.extend(client.album(&id).await.map_err(|e| e.to_string())?.tracks)
                }
                l::MediaKind::Playlist => {
                    tracks.extend(
                        client
                            .playlist(&id)
                            .await
                            .map_err(|e| e.to_string())?
                            .entries
                            .into_iter()
                            .flatten(),
                    );
                }
                _ => return Err("Only songs, albums and playlists can be queued".into()),
            }
            if tracks.len() > 2000 {
                return Err("A queue addition is limited to 2000 songs".into());
            }
        }
    } else if let Some(id) = &source {
        match id.kind() {
            l::MediaKind::Track => tracks.push(client.song(id).await.map_err(|e| e.to_string())?),
            l::MediaKind::Album => {
                tracks = client.album(id).await.map_err(|e| e.to_string())?.tracks
            }
            l::MediaKind::Artist => {
                for album in client.artist(id).await.map_err(|e| e.to_string())?.albums {
                    tracks.extend(
                        client
                            .album(&album.id)
                            .await
                            .map_err(|e| e.to_string())?
                            .tracks,
                    );
                    if tracks.len() > 2000 {
                        return Err("Artist playback exceeds the 2000-song context limit".into());
                    }
                }
            }
            l::MediaKind::Playlist => {
                playlist_entries = Some(
                    client
                        .playlist(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .entries,
                );
            }
            _ => return Err("This context is not available yet".into()),
        }
    }
    let tracks = playlist_entries.unwrap_or_else(|| tracks.into_iter().map(Some).collect());
    if tracks.len() > 2000 {
        return Err("A playback context is limited to 2000 songs".into());
    }
    Ok((tracks, source))
}
fn repeat(mode: crate::player::RepeatMode) -> Repeat {
    match mode {
        crate::player::RepeatMode::Off => Repeat::Off,
        crate::player::RepeatMode::Context => Repeat::Context,
        crate::player::RepeatMode::Track => Repeat::Track,
    }
}
fn view_repeat(mode: Repeat) -> crate::player::RepeatMode {
    match mode {
        Repeat::Off => crate::player::RepeatMode::Off,
        Repeat::Context => crate::player::RepeatMode::Context,
        Repeat::Track => crate::player::RepeatMode::Track,
    }
}
fn queue_event(queue: &Queue, positions: &[usize]) -> E {
    E::Subsonic(super::Event::Queue {
        queue: Box::new(v::Queue {
            currently_playing: queue.current().map(|t| v::PlayableItem::Track(m::track(t))),
            queue: queue
                .upcoming()
                .iter()
                .map(|t| v::PlayableItem::Track(m::track(t)))
                .collect(),
        }),
        manual: queue.manual().iter().map(|t| m::uri(&t.id)).collect(),
        source: queue.source.as_ref().map(m::uri),
        current_index: queue
            .current_index()
            .map(|i| positions.get(i).copied().unwrap_or(i)),
    })
}
fn local(queue: &Queue, state: &l::playback::PlaybackState, username: &str) -> LocalState {
    let track = queue.current().map(m::track);
    LocalState {
        connected: true,
        username: username.into(),
        active_client: "Sonicfast".into(),
        track: track.map(|t| LocalTrack {
            uri: t.uri.clone(),
            title: t.name.clone(),
            artists: t.artists.clone(),
            album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            art_url: t.image(512).map(str::to_owned),
            art_small_url: t.image(64).map(str::to_owned),
            duration_ms: state
                .duration_ms
                .unwrap_or(u64::from(t.duration_ms))
                .min(u32::MAX as u64) as u32,
            is_episode: false,
        }),
        playback: match state.phase {
            Phase::Playing => Playback::Playing,
            Phase::Loading | Phase::Buffering | Phase::Seeking => Playback::Loading,
            Phase::Stopped if queue.current().is_none() => Playback::Stopped,
            _ => Playback::Paused,
        },
        position_ms: state.position_ms.min(u32::MAX as u64) as u32,
        position_at: (state.phase == Phase::Playing).then(Instant::now),
        volume: state.volume,
        shuffle: queue.shuffle,
        repeat: view_repeat(queue.repeat),
        error: state.error.as_ref().map(ToString::to_string),
        seek_sequence: state.generation,
        track_sequence: state.generation,
        loading: matches!(
            state.phase,
            Phase::Loading | Phase::Seeking | Phase::Buffering
        ),
        ..Default::default()
    }
}

/// Starts the Sonicfast runtime without creating a Spotify worker or session.
pub async fn run(
    dirs: AppDirs,
    mut config: EngineConfig,
    art: ArtLoader,
    tx: std::sync::mpsc::Sender<E>,
    wake: b::Waker,
    mut commands: mpsc::UnboundedReceiver<b::Command>,
    restore: bool,
) {
    let path = dirs.config.join("servers.json");
    let mut profiles = Profiles::load(&path);
    let secrets = Secrets::new(dirs.state.clone());
    let mut scrobble_settings = l::playback::scrobble::ScrobbleSettings::default();
    let mut download_settings = super::downloads::Settings::default();
    let mut downloads: Option<super::downloads::Manager> = None;
    let mut download_queues = DownloadQueues::new();
    let local_library = Arc::new(std::sync::Mutex::new(None));
    let mut download_epoch = 0u64;
    let mut download_pending = std::collections::VecDeque::new();
    let mut download_fetch: Option<tokio::task::AbortHandle> = None;
    let mut listening = l::playback::scrobble::Listening::default();
    let mut reporter: Option<super::scrobbling::Reporter> = None;
    let mut last_listening = Instant::now();
    let mut generation = 0u64;
    let mut serial = 0u64;
    let mut queue_serial = 0u64;
    let mut queue_pending = std::collections::VecDeque::new();
    let mut queue_fetch: Option<tokio::task::AbortHandle> = None;
    let mut lyrics_fetch: Option<tokio::task::AbortHandle> = None;
    let mut play_fetch: Option<tokio::task::AbortHandle> = None;
    let mut pending_spec: Option<LoadSpec> = None;
    let mut tasks: JoinSet<(u64, Result<Outcome, String>)> = JoinSet::new();
    let mut client: Option<Arc<Client>> = None;
    let mut player: Option<Player> = None;
    let mut updates: Option<tokio::sync::watch::Receiver<l::playback::PlaybackState>> = None;
    let mut selected: Option<Profile> = None;
    let mut queue = Queue::default();
    let mut source_positions = Vec::new();
    let mut playlist_writes = std::collections::VecDeque::new();
    let mut playlist_write_active = false;
    let mut last_end = None;
    let mut dirty = false;
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    emit(
        &tx,
        &wake,
        E::Subsonic(super::Event::Profiles(profiles.clone())),
    );
    emit(&tx, &wake, E::Auth(b::AuthStatus::SignedOut));
    if restore
        && let Some(profile) = profiles
            .profiles
            .iter()
            .find(|p| Some(&p.id) == profiles.selected.as_ref() && p.remember)
            .cloned()
    {
        let secrets = secrets.clone();
        emit(&tx, &wake, E::Auth(b::AuthStatus::Connecting));
        tasks.spawn(async move {
            (
                0,
                async {
                    let secret = secrets
                        .access(&profile.id, None)
                        .await?
                        .ok_or("Enter this server's password or API key again")?;
                    let client = Arc::new(profile.client(&secret).map_err(|e| e.to_string())?);
                    client.connect().await.map_err(|e| e.to_string())?;
                    Ok(Outcome::Login(profile, client, None))
                }
                .await,
            )
        });
    }
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    b::Command::Shutdown => break,
                    b::Command::Subsonic(Request::Login(mut profile, secret)) => {
                        generation += 1;
                        serial += 1;
                        if let Some(manager)=downloads.take() {manager.shutdown().await;}
                        *local_library.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=None;
                        reporter=None;listening=Default::default();download_pending.clear();download_fetch=None;
                        tasks.abort_all();
                        playlist_writes.clear();
                        playlist_write_active = false;
                        secrets.invalidate();
                        queue_pending.clear();
                        queue_fetch = None;
                        play_fetch = None;
                        pending_spec = None;
                        if let Some(old) = player.take() {
                            old.shutdown().await.ok();
                        }
                        updates = None;
                        client = None;
                        art.set_subsonic(None);
                        queue = Queue::default();
                        source_positions.clear();
                        if !profiles.profiles.iter().any(|p| {
                            p.id == profile.id
                                && p.url == profile.url
                                && p.username == profile.username
                                && p.api_key == profile.api_key
                        }) {
                            profile.id = format!("{:032x}", rand::random::<u128>());
                        }
                        emit(&tx, &wake, E::Auth(b::AuthStatus::Connecting));
                        let token = generation;
                        let secrets = secrets.clone();
                        let path = path.clone();
                        let mut saved = profiles.clone();
                        tasks.spawn(async move {
                            (
                                token,
                                async {
                                    let secret = if secret.is_empty() && profile.remember {
                                        secrets
                                            .access(&profile.id, None)
                                            .await?
                                            .ok_or("Enter this server's password or API key")?
                                    } else {
                                        secret
                                    };
                                    let client = Arc::new(profile.client(&secret).map_err(|e| e.to_string())?);
                                    client.connect().await.map_err(|e| e.to_string())?;
                                    // Never enable restoration until the native write/read verification succeeds.
                                    let remember = profile.remember;
                                    profile.remember = false;
                                    saved.profiles.retain(|p| p.id != profile.id);
                                    saved.profiles.push(profile.clone());
                                    saved.selected = Some(profile.id.clone());
                                    let save_path = path.clone();
                                    tokio::task::spawn_blocking(move || saved.save(&save_path))
                                        .await
                                        .map_err(|_| "Profile writer stopped")??;
                                    let warning = if remember {
                                        match secrets.access(&profile.id, Some(Some(secret))).await {
                                            Ok(_) => {
                                                profile.remember = true;
                                                None
                                            }
                                            Err(e) => Some(e),
                                        }
                                    } else {
                                        None
                                    };
                                    Ok(Outcome::Login(profile, client, warning))
                                }
                                .await,
                            )
                        });
                    }
                    b::Command::SignOut => {
                        generation += 1;
                        serial += 1;
                        if let Some(manager)=downloads.take() {manager.shutdown().await;}
                        *local_library.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=None;
                        reporter=None;listening=Default::default();download_pending.clear();download_fetch=None;
                        tasks.abort_all();
                        playlist_writes.clear();
                        playlist_write_active = false;
                        secrets.invalidate();
                        queue_pending.clear();
                        queue_fetch = None;
                        play_fetch = None;
                        pending_spec = None;
                        if let Some(old) = player.take() {
                            old.shutdown().await.ok();
                        }
                        updates = None;
                        client = None;
                        art.set_subsonic(None);
                        queue = Queue::default();
                        source_positions.clear();
                        if let Some(profile) = selected.take().or_else(|| {
                            profiles
                                .selected
                                .as_ref()
                                .and_then(|id| profiles.profiles.iter().find(|p| &p.id == id))
                                .cloned()
                        }) {
                            if let Some(p) = profiles.profiles.iter_mut().find(|p| p.id == profile.id) {
                                p.remember = false;
                            }
                            profiles.selected = None;
                            // Persist the non-secret restoration revocation before touching a possibly locked store.
                            let saved = profiles.clone();
                            let path = path.clone();
                            if !matches!(
                                tokio::task::spawn_blocking(move || saved.save(&path)).await,
                                Ok(Ok(()))
                            ) {
                                emit(&tx, &wake, E::Subsonic(super::Event::Notice("Unable to persist sign-out. Unlock the credential store and retry sign-out before closing.".into())));
                            }
                            let secrets = secrets.clone();
                            tokio::spawn(async move {
                                let _ = secrets.access(&profile.id, Some(None)).await;
                            });
                        }
                        emit(&tx, &wake, E::Auth(b::AuthStatus::SignedOut));
                        emit(
                            &tx,
                            &wake,
                            E::Subsonic(super::Event::Profiles(profiles.clone())),
                        );
                    }
                    b::Command::Subsonic(Request::Configure {scrobble,downloads:new_downloads}) => {
                        if scrobble_settings!=scrobble {
                            let toggled=scrobble_settings.enabled!=scrobble.enabled;
                            scrobble_settings=scrobble;
                            if toggled {
                                listening.suspend();reporter=None;
                            }
                            if toggled && scrobble_settings.enabled && let Some(client)=&client {
                                let tx=tx.clone();let wake=wake.clone();
                                reporter=Some(super::scrobbling::Reporter::spawn(client.clone(),receipt_path(&dirs,client.capabilities().server.as_str()),Arc::new(move |s|emit(&tx,&wake,E::Error(s)))));
                            }
                        }
                        if download_settings!=new_downloads {
                            download_settings=new_downloads;
                            if let Some(manager)=downloads.take() {manager.shutdown().await;}
                            *local_library.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=None;
                            download_pending.clear();if let Some(fetch)=download_fetch.take() {fetch.abort();}
                            download_epoch+=1;
                            emit(&tx,&wake,E::Subsonic(super::Event::Downloads {epoch:download_epoch,update:super::downloads::Update::Reset(vec![],false)}));
                            if download_settings.enabled && let Some(client)=&client {
                                match download_manager(&download_settings,client.clone(),&tx,&wake,download_epoch,&download_queues).await {
                                    Ok(manager)=>{*local_library.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=Some(manager.library.clone());downloads=Some(manager);}
                                    Err(error)=>emit(&tx,&wake,E::Error(error)),
                                }
                            }
                        }
                    }
                    b::Command::Subsonic(Request::Download(uris)) => {
                        if downloads.is_none() {emit(&tx,&wake,E::Error("Enable downloads and choose a writable absolute folder in Settings".into()));}
                        else if download_pending.len()<64 {download_pending.push_back(uris);}
                        else {emit(&tx,&wake,E::Error("Pending download selections are full; try again shortly".into()));}
                    }
                    b::Command::Subsonic(Request::ProbeDownloads(tracks)) => {
                        if let Some(manager)=&mut downloads {manager.probe(tracks.iter().filter_map(|t|m::neutral_track(t).ok()).collect());}
                    }
                    b::Command::Subsonic(Request::DownloadControl {uri,action}) => {
                        if let Some(manager)=&mut downloads {
                            let id=uri.as_deref().map(m::id).transpose();
                            if let Ok(id)=id {manager.control(id.as_ref(),&action);}
                        }
                    }
                    b::Command::Lyrics(request) => {
                        if let Some(old) = lyrics_fetch.take() { old.abort(); }
                        let token = generation;
                        if tasks.len() >= 32 {
                            emit(&tx, &wake, E::Lyrics { uri: request.uri, result: Err("Lyrics requests are busy; try again shortly".into()) });
                        } else {
                            let cache = dirs.lyrics_cache_dir();
                            lyrics_fetch = Some(tasks.spawn(async move {
                                let result = async {
                                    let http = reqwest::Client::builder().timeout(Duration::from_secs(20)).connect_timeout(Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).user_agent("Sonicfast/0.14.1 (LRCLIB lyrics)").build().map_err(|_| "Unable to initialize lyrics requests".to_string())?;
                                    crate::lyrics::fetch(&http, &cache, &request.query).await.map_err(|e| e.to_string())
                                }.await;
                                (token, Ok(Outcome::Events(vec![E::Lyrics { uri: request.uri, result }])))
                            }));
                        }
                    }
                    b::Command::Subsonic(Request::RateSong { uri, rating }) => {
                        let write = super::playlists::Write::Rate {uri, rating};
                        if playlist_writes.len() >= 64 {
                            for e in super::playlists::failure(write, crate::api::ApiError::Network("Pending song edits are full".into())) { emit(&tx, &wake, e); }
                        } else { playlist_writes.push_back(write); }
                    }
                    b::Command::Subsonic(Request::CreatePlaylist { name, public, uris }) => {
                        let write = super::playlists::Write::Create { name, public, uris, description: String::new() };
                        if playlist_writes.len() >= 64 {
                            for e in super::playlists::failure(write, crate::api::ApiError::Network("Pending playlist edits are full".into())) { emit(&tx, &wake, e); }
                        } else { playlist_writes.push_back(write); }
                    }
                    b::Command::Subsonic(Request::PlaylistRows { id, rows, revision }) => {
                        let write = super::playlists::Write::Rows { id, rows, revision };
                        if playlist_writes.len() >= 64 {
                            for e in super::playlists::failure(write, crate::api::ApiError::Network("Pending playlist edits are full".into())) { emit(&tx, &wake, e); }
                        } else { playlist_writes.push_back(write); }
                    }
                    b::Command::Api(request) if super::playlists::is_write(&request) => {
                        let write = super::playlists::Write::Api(request);
                        if playlist_writes.len() >= 64 {
                            for e in super::playlists::failure(write, crate::api::ApiError::Network("Pending playlist edits are full".into())) { emit(&tx, &wake, e); }
                        } else { playlist_writes.push_back(write); }
                    }
                    b::Command::Api(request) => {
                        if let Some(client) = &client {
                            if tasks.len() >= 32 {
                                for e in super::playlists::read_failure(request, Some(client.capabilities().server.as_str().to_owned())) { emit(&tx, &wake, e); }
                                continue;
                            }
                            let client = client.clone();
                            let token = generation;
                            tasks.spawn(
                                async move { (token, Ok(Outcome::Events(read(&client, request).await))) },
                            );
                        }
                    }
                    b::Command::Subsonic(Request::Load(spec)) | b::Command::Player(P::Load(spec)) => {
                        if let Some(client) = &client {
                            serial += 1;
                            let request = serial;
                            let token = generation;
                            let client = client.clone();
                            if let Some(old) = play_fetch.take() {
                                old.abort();
                            }
                            pending_spec = Some(spec.clone());
                            if let Some(player) = &player {
                                player.stop();
                            }
                            play_fetch = Some(tasks.spawn(async move {
                                let result = resolve(&client, &spec).await;
                                (token, Ok(Outcome::Load(request, spec, result)))
                            }));
                        }
                    }
                    b::Command::Subsonic(Request::Queue(action)) => {
                        match action {
                            QueueAction::Add(ref uris) | QueueAction::Insert(_, ref uris)
                                if !uris.is_empty() =>
                            {
                                if queue_pending.len() >= 64 {
                                    emit(
                                        &tx,
                                        &wake,
                                        E::Error("Pending queue additions are full; try again shortly".into()),
                                    );
                                } else {
                                    queue_pending.push_back(action);
                                }
                            }
                            QueueAction::Clear => {
                                queue_serial += 1;
                                queue.clear();
                                queue_pending.clear();
                                if let Some(fetch) = queue_fetch.take() {
                                    fetch.abort();
                                }
                            }
                            QueueAction::Move(from, to) => queue.move_row(from, to),
                            QueueAction::Remove(rows) => {
                                for (at, key) in rows.into_iter().rev() {
                                    if let Ok(id) = m::id(&key) {
                                        queue.remove(at, &id);
                                    }
                                }
                            }
                            QueueAction::Skip(index) => {
                                if let Some(track) = queue.skip_to(index)
                                    && let Some(p) = &player
                                {
                                    listening.start(track.id.clone(),unix_ms());
                                    let _ = p.load(track.id, true);
                                }
                            }
                            _ => {}
                        }
                        dirty = true;
                        emit(&tx, &wake, queue_event(&queue, &source_positions));
                    }
                    b::Command::Player(command) | b::Command::Subsonic(Request::Transport(command)) => {
                        if let Some(p) = &player {
                            if matches!(command, P::Next | P::Previous) {
                                serial += 1;
                                pending_spec = None;
                                if let Some(fetch) = play_fetch.take() {
                                    fetch.abort();
                                }
                            }
                            if let Some(spec) = &mut pending_spec {
                                match command {
                                    P::Toggle => {
                                        spec.play = !spec.play;
                                        continue;
                                    }
                                    P::Seek(ms) => {
                                        spec.position_ms = ms;
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            let track = match command {
                                P::Toggle => {
                                    if matches!(
                                        p.snapshot().phase,
                                        Phase::Playing | Phase::Loading | Phase::Buffering | Phase::Seeking
                                    ) {
                                        p.pause();
                                    } else {
                                        let _ = p.play();
                                    }
                                    None
                                }
                                P::Next => {
                                    serial += 1;
                                    let next = queue.advance(false);
                                    if next.is_none() {
                                        p.stop();
                                    }
                                    next
                                }
                                P::Previous => {
                                    if p.snapshot().position_ms > 3000 {
                                        let _ = p.seek(0);
                                        None
                                    } else {
                                        queue.previous()
                                    }
                                }
                                P::Seek(ms) => {
                                    let _ = p.seek(u64::from(ms));
                                    None
                                }
                                P::Volume(v) | P::VolumePreview(v) => {
                                    p.set_volume(v);
                                    None
                                }
                                P::Shuffle(shuffle) => {
                                    queue.set_shuffle(shuffle);
                                    None
                                }
                                P::Repeat(mode) => {
                                    queue.repeat = repeat(mode);
                                    None
                                }
                                P::ClearQueue => {
                                    queue.clear();
                                    None
                                }
                                P::Transfer => {
                                    emit(
                                        &tx,
                                        &wake,
                                        E::Error("Remote playback is not available".into()),
                                    );
                                    None
                                }
                                _ => None,
                            };
                            if let Some(track) = track {
                                listening.start(track.id.clone(),unix_ms());
                                    let _ = p.load(track.id, true);
                            }
                            emit(
                                &tx,
                                &wake,
                                E::Local(Box::new(local(
                                    &queue,
                                    &p.snapshot(),
                                    selected.as_ref().map_or("", |p| &p.username),
                                ))),
                            );
                            emit(&tx, &wake, queue_event(&queue, &source_positions));
                            dirty = true;
                        }
                    }
                    b::Command::RestartEngine(next) => {
                        config = next;
                        if let (Some(client), Some(old)) = (&client, player.take()) {
                            let state = old.snapshot();
                            old.shutdown().await.ok();
                            match make_player(client.clone(), &config,local_library.clone()) {
                                Ok(p) => {
                                    if let Some(track) = queue.current() {
                                        if let Some(manager)=&downloads {manager.library.register(std::slice::from_ref(track));}
                                        if listening.track()!=Some(&track.id) {listening.start(track.id.clone(),unix_ms());}
                                        let _ = p.load(track.id.clone(), false);
                                        let _ = p.seek(state.position_ms);
                                        if state.phase == Phase::Playing {
                                            let _ = p.play();
                                        }
                                    }
                                    last_end = None;
                                    updates = Some(p.subscribe());
                                    player = Some(p);
                                }
                                Err(e) => emit(&tx, &wake, E::Error(e)),
                            }
                        }
                    }
                    b::Command::ListOutputDevices => {
                        let devices = tokio::task::spawn_blocking(fastframe_audio::output_devices).await;
                        emit(
                            &tx,
                            &wake,
                            E::OutputDevices(
                                devices
                                    .ok()
                                    .and_then(Result::ok)
                                    .ok_or("Unable to enumerate outputs".into()),
                            ),
                        );
                    }
                    b::Command::Accent { url } if tasks.len() < 32 => {
                        let art = art.clone();
                        let token = generation;
                        tasks.spawn(async move {
                            let events = match art.fetch(&url).await {
                                Ok(bytes) => {
                                    let palette = crate::images::art_palette(&bytes);
                                    palette
                                        .first()
                                        .copied()
                                        .map(|color| {
                                            vec![E::Accent {
                                                url,
                                                color,
                                                palette,
                                            }]
                                        })
                                        .unwrap_or_default()
                                }
                                Err(_) => vec![],
                            };
                            (token, Ok(Outcome::Events(events)))
                        });
                    }
                    _ => {}
                }
            }
            result = tasks.join_next(), if !tasks.is_empty() => {
                let Some(Ok((token, outcome))) = result else {
                    continue;
                };
                if token != generation {
                    continue;
                }
                match outcome {
                    Err(error) => {
                        emit(
                            &tx,
                            &wake,
                            if client.is_none() {
                                E::Auth(b::AuthStatus::Failed(error))
                            } else {
                                E::Error(error)
                            },
                        );
                    }
                    Ok(Outcome::Login(profile, service, warning)) => {
                        let saved_resume = read_resume(&dirs, &profile.id).await;
                        download_queues = saved_resume.as_ref().map(|resume| resume.downloads.clone()).unwrap_or_default();
                        profiles.profiles.retain(|p| p.id != profile.id);
                        profiles.profiles.push(profile.clone());
                        profiles.selected = Some(profile.id.clone());
                        let saved = profiles.clone();
                        let path = path.clone();
                        if !matches!(tokio::task::spawn_blocking(move || saved.save(&path)).await, Ok(Ok(()))) {
                            emit(&tx, &wake, E::Subsonic(super::Event::Notice("Server profile could not be saved; remembered login is unavailable until the configuration directory is writable.".into())));
                        }
                        if download_settings.enabled {
                            download_epoch+=1;
                            match download_manager(&download_settings,service.clone(),&tx,&wake,download_epoch,&download_queues).await {
                                Ok(manager)=>{*local_library.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=Some(manager.library.clone());downloads=Some(manager);}
                                Err(error)=>emit(&tx,&wake,E::Error(error)),
                            }
                        }
                        if scrobble_settings.enabled {
                            let tx=tx.clone();let wake=wake.clone();
                            reporter=Some(super::scrobbling::Reporter::spawn(service.clone(),receipt_path(&dirs,&profile.id),Arc::new(move |s|emit(&tx,&wake,E::Error(s)))));
                        }
                        match make_player(service.clone(), &config,local_library.clone()) {
                            Ok(p) => {
                                if let Some(resume) = saved_resume
                                    && resume
                                        .queue
                                        .valid_for(&l::ServerId::new(&profile.id).unwrap())
                                {
                                    listening=resume.listening;listening.suspend();
                                    source_positions = resume.source_positions;
                                    queue = resume.queue;
                                    if let Some(track) = queue.current() {
                                        if let Some(manager)=&downloads {manager.library.register(std::slice::from_ref(track));}
                                        if listening.track()!=Some(&track.id) {listening.start(track.id.clone(),unix_ms());}
                                        let _ = p.load(track.id.clone(), false);
                                        let _ = p.seek(resume.position_ms);
                                    }
                                }
                                last_end = None;
                                updates = Some(p.subscribe());
                                player = Some(p);
                            }
                            Err(e) => {
                                emit(&tx, &wake, E::Error(e));
                                continue;
                            }
                        }
                        art.set_subsonic(Some(service.clone()));
                        client = Some(service);
                        selected = Some(profile.clone());
                        emit(
                            &tx,
                            &wake,
                            E::Auth(b::AuthStatus::Connected {
                                username: profile.username.clone(),
                            }),
                        );
                        emit(
                            &tx,
                            &wake,
                            E::Api(Box::new(R::Me(Ok(v::User {
                                id: profile.id.clone(),
                                display_name: Some(profile.name.clone()),
                                ..Default::default()
                            })))),
                        );
                        emit(
                            &tx,
                            &wake,
                            E::Subsonic(super::Event::Profiles(profiles.clone())),
                        );
                        emit(
                            &tx,
                            &wake,
                            E::Playback(b::LocalPlayback::Ready {
                                device_id: "sonicfast-local".into(),
                            }),
                        );
                        emit(&tx, &wake, queue_event(&queue, &source_positions));
                        if let Some(p) = &player {
                            emit(
                                &tx,
                                &wake,
                                E::Local(Box::new(local(&queue, &p.snapshot(), &profile.username))),
                            );
                        }
                        if let Some(manager)=&downloads {manager.publish();}
                        if let Some(warning) = warning {
                            emit(&tx, &wake, E::Subsonic(super::Event::Notice(warning)));
                        }
                    }
                    Ok(Outcome::DownloadTracks(result)) => {
                        download_fetch=None;
                        match result {
                            Ok(tracks)=>if let Some(manager)=&mut downloads {manager.enqueue(tracks);},
                            Err(error)=>emit(&tx,&wake,E::Error(error)),
                        }
                    }
                    Ok(Outcome::PlaylistWrite(events)) => {
                        playlist_write_active = false;
                        for e in events { emit(&tx, &wake, e); }
                    }
                    Ok(Outcome::Events(events)) => {
                        for e in events {
                            emit(&tx, &wake, e);
                        }
                    }
                    Ok(Outcome::Load(request, spec, result)) if request == serial => {
                        play_fetch = None;
                        let spec = pending_spec.take().unwrap_or(spec);
                        let (tracks, source) = match result {
                            Ok(result) => result,
                            Err(error) => {
                                emit(&tx, &wake, E::Error(error));
                                continue;
                            }
                        };
                        let at = spec
                            .offset_index
                            .map(|i| i as usize)
                            .or_else(|| {
                                spec.offset_uri.as_ref().and_then(|u| {
                                    tracks
                                        .iter()
                                        .position(|t| t.as_ref().is_some_and(|t| m::id(u).ok().as_ref() == Some(&t.id)))
                                })
                            })
                            .unwrap_or_else(|| tracks.iter().position(|t| t.as_ref().is_some_and(playable)).unwrap_or(0));
                        if tracks.get(at).and_then(Option::as_ref).is_none_or(|t| !playable(t)) {
                            emit(
                                &tx,
                                &wake,
                                E::Error("This track needs an unsupported codec; select MP3, FLAC or M4A (AAC/ALAC)".into()),
                            );
                            continue;
                        }

                        if let Some(manager)=&downloads {manager.library.register(&tracks.iter().flatten().cloned().collect::<Vec<_>>());}
                        let filtered: Vec<_> = tracks
                            .into_iter()
                            .enumerate()
                            .filter_map(|(i, t)| t.map(|t| (i, t)))
                            .filter(|(i, t)| *i == at || playable(t))
                            .collect();
                        let index = filtered
                            .iter()
                            .position(|(original, _)| *original == at)
                            .unwrap_or(0);
                        source_positions = filtered.iter().map(|(i, _)| *i).collect();
                        if let Some(shuffle) = spec.shuffle {
                            queue.set_shuffle(shuffle);
                        }
                        if let Some(mode) = spec.repeat {
                            queue.repeat = repeat(mode);
                        }
                        if let Some(track) = queue.start(
                            filtered.into_iter().map(|(_, t)| t).collect(),
                            index,
                            source,
                        ) && let Some(p) = &player
                        {
                            listening.start(track.id.clone(),unix_ms());
                            let _ = p.load(track.id, spec.play);
                            if spec.position_ms > 0 {
                                let _ = p.seek(u64::from(spec.position_ms));
                            }
                        }
                        emit(&tx, &wake, queue_event(&queue, &source_positions));
                        dirty = true;
                    }
                    Ok(Outcome::Queue(request, action, result)) => {
                        queue_fetch = None;
                        if request != queue_serial {
                            continue;
                        }
                        let tracks = match result {
                            Ok(tracks) => tracks,
                            Err(error) => {
                                emit(&tx, &wake, E::Error(error));
                                Vec::new()
                            }
                        };
                        let tracks = tracks.into_iter().filter(playable).collect();
                        match action {
                            QueueAction::Add(_) => queue.add(tracks),
                            QueueAction::Insert(at, _) => queue.insert(at, tracks),
                            _ => {}
                        }
                        emit(&tx, &wake, queue_event(&queue, &source_positions));
                        dirty = true;
                    }
                    _ => {}
                }
            }
            _ = async { match &mut updates { Some(updates) => { let _ = updates.changed().await; }, None => std::future::pending().await } } => {
                if let Some(p) = &player {
                    let state = p.snapshot();
                    if queue.current().is_some_and(|t|state.track.as_ref()==Some(&t.id)) {
                        let elapsed=last_listening.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                        for report in listening.observe(&scrobble_settings,state.position_ms,state.duration_ms.or_else(||queue.current().and_then(|t|t.duration_ms)),state.generation,state.phase==Phase::Playing,elapsed) {
                            if let Some(reporter)=&reporter && !reporter.send(report) {emit(&tx,&wake,E::Error("Scrobble reporting is busy or stopped; this report was not queued".into()));}
                        }
                    }
                    last_listening=Instant::now();
                    if state.phase == Phase::Ended && last_end != Some(state.generation) {
                        last_end = Some(state.generation);
                        if let Some(track) = queue.advance(true) {
                            listening.start(track.id.clone(),unix_ms());
                                    let _ = p.load(track.id, true);
                        }
                        emit(&tx, &wake, queue_event(&queue, &source_positions));
                    }
                    emit(
                        &tx,
                        &wake,
                        E::Local(Box::new(local(
                            &queue,
                            &p.snapshot(),
                            selected.as_ref().map_or("", |p| &p.username),
                        ))),
                    );
                    dirty = true;
                }
            }
            _ = interval.tick() => {
                if dirty && let (Some(profile), Some(p)) = (&selected, &player) {
                    let _ = save_resume(&dirs, &profile.id, &queue, &source_positions, p.snapshot().position_ms,&listening,&download_queues).await;
                    dirty = false;
                }
            }
        }
        if let Some(manager) = &mut downloads {
            if let Some(pending) = manager.take_pending() {
                download_queues.insert(manager.root().to_owned(), pending);
                if let Some(profile) = &selected
                    && let Err(error) = save_resume(
                        &dirs,
                        &profile.id,
                        &queue,
                        &source_positions,
                        player.as_ref().map_or(0, |p| p.snapshot().position_ms),
                        &listening,
                        &download_queues,
                    )
                    .await
                {
                    emit(&tx, &wake, E::Error(error));
                    manager.control(None, "pause");
                    continue;
                }
            }
            manager.tick().await;
        }
        if download_fetch.is_none()
            && let (Some(client), Some(uris)) = (&client, download_pending.pop_front())
        {
            let client = client.clone();
            let token = generation;
            download_fetch = Some(tasks.spawn(async move {
                (
                    token,
                    Ok(Outcome::DownloadTracks(
                        resolve(
                            &client,
                            &LoadSpec {
                                uris,
                                ..Default::default()
                            },
                        )
                        .await
                        .map(|(tracks, _)| tracks.into_iter().flatten().collect()),
                    )),
                )
            }));
        }
        if !playlist_write_active
            && let (Some(client), Some(write)) = (&client, playlist_writes.pop_front())
        {
            playlist_write_active = true;
            let client = client.clone();
            let token = generation;
            tasks.spawn(async move {
                (
                    token,
                    Ok(Outcome::PlaylistWrite(
                        super::playlists::write(&client, write).await,
                    )),
                )
            });
        }
        if queue_fetch.is_none()
            && let (Some(client), Some(action)) = (&client, queue_pending.pop_front())
        {
            let client = client.clone();
            let token = generation;
            let request = queue_serial;
            let uris = match &action {
                QueueAction::Add(uris) | QueueAction::Insert(_, uris) => uris.clone(),
                _ => vec![],
            };
            queue_fetch = Some(tasks.spawn(async move {
                let spec = LoadSpec {
                    uris,
                    ..Default::default()
                };
                (
                    token,
                    Ok(Outcome::Queue(
                        request,
                        action,
                        resolve(&client, &spec)
                            .await
                            .map(|(tracks, _)| tracks.into_iter().flatten().collect()),
                    )),
                )
            }));
        }
    }
    if let Some(manager) = &downloads {
        download_queues.insert(manager.root().to_owned(), manager.pending());
    }
    if let (Some(profile), Some(p)) = (&selected, &player) {
        let _ = save_resume(
            &dirs,
            &profile.id,
            &queue,
            &source_positions,
            p.snapshot().position_ms,
            &listening,
            &download_queues,
        )
        .await;
    }
    if let Some(manager) = downloads.take() {
        manager.shutdown().await;
    }
    drop(reporter);
    tasks.abort_all();
    secrets.invalidate();
    art.set_subsonic(None);
    if let Some(p) = player {
        let _ = p.shutdown().await;
    }
}
async fn save_resume(
    dirs: &AppDirs,
    id: &str,
    queue: &Queue,
    source_positions: &[usize],
    position_ms: u64,
    listening: &l::playback::scrobble::Listening,
    downloads: &DownloadQueues,
) -> Result<(), String> {
    let path = resume_path(dirs, id);
    let resume = Resume {
        downloads: downloads.clone(),
        listening: listening.clone(),
        queue: queue.clone(),
        source_positions: source_positions.to_vec(),
        position_ms,
    };
    tokio::task::spawn_blocking(move || {
        let bytes = serde_json::to_vec(&resume)
            .map_err(|_| "Unable to encode playback/download queue state".to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Playback/download queue state exceeds 32 MiB".into());
        }
        fastframe_file::write_private(&path, bytes).map_err(|_| {
            "Unable to save playback/download queue state; downloads are paused".to_string()
        })
    })
    .await
    .map_err(|_| "Unable to save playback/download queue state".to_string())?
}
async fn read_resume(dirs: &AppDirs, id: &str) -> Option<Resume> {
    let path = resume_path(dirs, id);
    if tokio::fs::metadata(&path).await.ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()
}

fn make_player(
    client: Arc<Client>,
    config: &EngineConfig,
    library: Arc<std::sync::Mutex<Option<super::downloads::Library>>>,
) -> Result<Player, String> {
    let p = Player::spawn(
        Arc::new(super::downloads::LocalFirst { client, library }),
        Box::new(crate::sink::PcmOutput::new(
            config.audio_device.clone(),
            config.eq.clone(),
            config.tap.clone(),
        )),
        tokio::runtime::Handle::current(),
    )
    .map_err(|e| e.to_string())?;
    p.set_volume(config.initial_volume);
    Ok(p)
}

async fn read(client: &Client, request: A) -> Vec<E> {
    let mut events = Vec::new();
    let response = match request {
        A::Me => return vec![],
        A::SavedTracks { offset, generation } => R::SavedTracks {
            offset,
            generation,
            account_id: Some(client.capabilities().server.as_str().to_owned()),
            result: client
                .favorite_songs()
                .await
                .map(|songs| {
                    let mut page = v::Page::whole(
                        songs
                            .iter()
                            .map(|t| v::SavedTrack {
                                track: m::track(t),
                                added_at: None,
                            })
                            .collect(),
                    );
                    page.items = page.items.into_iter().skip(offset as usize).collect();
                    page.offset = offset;
                    page
                })
                .map_err(m::error),
        },
        A::MyPlaylists { offset, generation } => R::MyPlaylists {
            offset,
            generation,
            result: client
                .playlists()
                .await
                .map(|items| {
                    let mut page = v::Page::whole(items.iter().map(m::playlist).collect());
                    page.items = page.items.into_iter().skip(offset as usize).collect();
                    page.offset = offset;
                    page
                })
                .map_err(m::error),
        },
        A::Playlist { id, generation } => R::Playlist {
            result: match m::id(&id) {
                Ok(key) => client
                    .playlist(&key)
                    .await
                    .map(|p| m::playlist_details(&p))
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
            generation,
        },
        A::PlaylistItems {
            id,
            offset,
            generation,
        } => R::PlaylistItems {
            result: match m::id(&id) {
                Ok(key) => client
                    .playlist(&key)
                    .await
                    .map(|p| m::playlist_items(&p, offset))
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
            offset,
            generation,
        },
        A::PlaylistSample {
            id,
            offset,
            generation,
        } => R::PlaylistSample {
            result: match m::id(&id) {
                Ok(key) => client
                    .playlist(&key)
                    .await
                    .map(|p| m::playlist_items(&p, offset))
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
            generation,
        },
        A::CheckPlaylistDuplicates {
            playlist_id,
            playlist_name,
            items,
            position,
        } => R::PlaylistDuplicatesChecked {
            result: match m::id(&playlist_id) {
                Ok(key) => client
                    .playlist(&key)
                    .await
                    .map(|p| {
                        items
                            .iter()
                            .filter(|item| {
                                m::id(item.uri()).ok().is_some_and(|id| {
                                    p.entries.iter().flatten().any(|t| t.id == id)
                                })
                            })
                            .map(|item| item.uri().to_owned())
                            .collect()
                    })
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            playlist_id,
            playlist_name,
            items,
            position,
        },
        A::SavedAlbums { offset } => R::SavedAlbums {
            offset,
            result: client
                .albums(
                    l::AlbumOrder::Title,
                    l::PageRequest::new(offset, 50).unwrap(),
                )
                .await
                .map(|p| {
                    m::page(
                        p.items
                            .iter()
                            .map(|a| v::SavedAlbum {
                                album: m::album(a),
                                added_at: None,
                            })
                            .collect(),
                        offset,
                        p.next,
                    )
                })
                .map_err(m::error),
        },
        A::FollowedArtists { after } => R::FollowedArtists {
            after,
            result: client
                .artists()
                .await
                .map(|artists| v::CursorPage {
                    items: artists.iter().map(m::artist).collect(),
                    ..Default::default()
                })
                .map_err(m::error),
        },
        A::Album { id } => R::Album {
            result: match m::id(&id) {
                Ok(key) => client
                    .album(&key)
                    .await
                    .map(|a| {
                        let mut album = m::album(&a.album);
                        album.tracks =
                            Some(v::Page::whole(a.tracks.iter().map(m::track).collect()));
                        album
                    })
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
        },
        A::AlbumTracks {
            id,
            offset,
            generation,
        } => R::AlbumTracks {
            result: match m::id(&id) {
                Ok(key) => client
                    .album(&key)
                    .await
                    .map(|a| {
                        let tracks = a
                            .tracks
                            .iter()
                            .skip(offset as usize)
                            .map(m::track)
                            .collect();
                        let mut p = v::Page::whole(tracks);
                        p.offset = offset;
                        p
                    })
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
            offset,
            generation,
        },
        A::AlbumQueueTracks {
            id,
            offset,
            request,
        } => R::AlbumQueueTracks {
            result: match m::id(&id) {
                Ok(key) => client
                    .album(&key)
                    .await
                    .map(|a| {
                        v::Page::whole(
                            a.tracks
                                .iter()
                                .skip(offset as usize)
                                .map(m::track)
                                .collect(),
                        )
                    })
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            offset,
            request,
        },
        A::Artist { id } => R::Artist {
            result: match m::id(&id) {
                Ok(key) => client
                    .artist(&key)
                    .await
                    .map(|a| m::artist(&a.artist))
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
        },
        A::ArtistAlbums { id, groups, offset } => R::ArtistAlbums {
            result: match m::id(&id) {
                Ok(key) => client
                    .artist(&key)
                    .await
                    .map(|a| {
                        let mut p = v::Page::whole(
                            a.albums
                                .iter()
                                .skip(offset as usize)
                                .map(m::album)
                                .collect(),
                        );
                        p.offset = offset;
                        p
                    })
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
            groups,
            offset,
        },
        A::ArtistTopTracks { id } => R::ArtistTopTracks {
            id,
            result: Ok(vec![]),
        },
        A::RelatedArtists { id } => R::RelatedArtists {
            id,
            result: Ok(vec![]),
        },
        A::Track { id } => R::Track {
            result: match m::id(&id) {
                Ok(key) => client
                    .song(&key)
                    .await
                    .map(|t| m::track(&t))
                    .map_err(m::error),
                Err(e) => Err(m::error(e)),
            },
            id,
        },
        A::Search { query, serial } | A::SearchCatalogue { query, serial } => {
            events.push(E::Api(Box::new(R::SearchStarted {
                query: query.clone(),
                serial,
                split: true,
            })));
            let (catalogue, playlists) = tokio::join!(
                client.search(&query, l::PageRequest::new(0, 50).unwrap()),
                client.playlists(),
            );
            events.push(E::Api(Box::new(R::SearchPlaylists {
                query: query.clone(),
                serial,
                result: playlists
                    .map(|items| matching_playlists(&items, &query))
                    .map_err(m::error),
            })));
            let result = catalogue
                .map(|r| v::SearchResults {
                    tracks: Some(m::page(
                        r.tracks.items.iter().map(m::track).collect(),
                        0,
                        r.tracks.next,
                    )),
                    albums: Some(m::page(
                        r.albums.items.iter().map(m::album).collect(),
                        0,
                        r.albums.next,
                    )),
                    artists: Some(m::page(
                        r.artists.items.iter().map(m::artist).collect(),
                        0,
                        r.artists.next,
                    )),
                    ..Default::default()
                })
                .map_err(m::error);
            R::Search {
                query,
                serial,
                result,
            }
        }
        A::SearchPlaylists { query, serial } => R::SearchPlaylists {
            result: client
                .playlists()
                .await
                .map(|items| matching_playlists(&items, &query))
                .map_err(m::error),
            query,
            serial,
        },
        A::Contains { uris } => R::Contains {
            result: client
                .favorite_songs()
                .await
                .map(|songs| {
                    let ids: std::collections::HashSet<_> =
                        songs.into_iter().map(|t| t.id).collect();
                    uris.iter()
                        .map(|uri| m::id(uri).ok().is_some_and(|id| ids.contains(&id)))
                        .collect()
                })
                .map_err(m::error),
            uris,
        },
        A::NewReleases { generation } => R::NewReleases {
            generation,
            result: client
                .albums(l::AlbumOrder::Newest, l::PageRequest::new(0, 50).unwrap())
                .await
                .map(|p| p.items.iter().map(m::album).collect())
                .map_err(m::error),
        },
        A::TopArtists { generation } => R::TopArtists {
            generation,
            result: Ok(vec![]),
        },
        A::TopTracks {
            offset,
            full,
            generation,
        } => R::TopTracks {
            offset,
            full,
            generation,
            result: Ok(v::Page::whole(vec![])),
        },
        A::RecentlyPlayed {
            who,
            generation,
            limit,
            ..
        } => R::RecentlyPlayed {
            who,
            generation,
            limit,
            result: Ok(v::CursorPage::default()),
        },
        A::Recommendations { generation, .. } => R::Recommendations {
            generation,
            result: Ok(vec![]),
        },
        A::MadeForYou { generation, .. } => R::MadeForYou {
            generation,
            personalized: false,
            result: Ok(vec![]),
            top_mixes: Ok(vec![]),
        },
        A::Devices => R::Devices(Ok(vec![v::Device {
            id: Some("sonicfast-local".into()),
            name: "This computer".into(),
            is_active: true,
            supports_volume: Some(true),
            ..Default::default()
        }])),
        A::PlaybackState { seq } => R::PlaybackState {
            seq,
            result: Ok(None),
        },
        _ => return events,
    };
    events.push(E::Api(Box::new(response)));
    events
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
fn receipt_path(dirs: &AppDirs, profile: &str) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    dirs.state
        .join(format!("scrobble-{:x}.json", Sha256::digest(profile)))
}
async fn download_manager(
    settings: &super::downloads::Settings,
    client: Arc<Client>,
    tx: &std::sync::mpsc::Sender<E>,
    wake: &b::Waker,
    epoch: u64,
    pending: &DownloadQueues,
) -> Result<super::downloads::Manager, String> {
    let notify_tx = tx.clone();
    let notify_wake = wake.clone();
    let mut manager = super::downloads::Manager::new(
        settings.path.clone().into(),
        client,
        Arc::new(move |update| {
            emit(
                &notify_tx,
                &notify_wake,
                E::Subsonic(super::Event::Downloads { epoch, update }),
            )
        }),
    )
    .await?;
    if let Some(state) = pending.get(manager.root())
        && let Err(error) = manager.restore(state.clone())
    {
        let _ = manager.restore(super::downloads::Pending::default());
        emit(
            tx,
            wake,
            E::Error(format!("{error}; checking local files with an empty queue")),
        );
    }
    Ok(manager)
}

fn matching_playlists(items: &[l::Playlist], query: &str) -> v::Page<v::Playlist> {
    let needle = query.trim().to_lowercase();
    v::Page::whole(
        items
            .iter()
            .filter(|p| p.title.to_lowercase().contains(&needle))
            .map(m::playlist)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsonic::test_server::{Reply, Server};

    #[tokio::test]
    async fn top_search_includes_playlists_and_keeps_them_when_catalogue_fails() {
        for fail_catalogue in [false, true] {
            let server = Server::start(move |r| match r.path.as_str() {
                "/rest/search3.view" if fail_catalogue => {
                    let mut reply = Reply::json(serde_json::json!({"status": "failed"}));
                    reply.status = 500;
                    reply
                }
                "/rest/search3.view" => Reply::json(serde_json::json!({
                    "status": "ok",
                    "searchResult3": {"song": [], "album": [], "artist": []}
                })),
                "/rest/getPlaylists.view" => Reply::json(serde_json::json!({
                    "status": "ok",
                    "playlists": {"playlist": [
                        {"id": "p", "name": "Weekend MIX", "songCount": 1, "duration": 3},
                        {"id": "q", "name": "Other", "songCount": 0, "duration": 0}
                    ]}
                })),
                _ => panic!("unexpected fixture endpoint"),
            })
            .await;
            let events = read(
                &server.client(),
                A::Search {
                    query: "mix".into(),
                    serial: 42,
                },
            )
            .await;
            let mut found = false;
            for e in events {
                if let E::Api(e) = e {
                    match *e {
                        R::SearchPlaylists {
                            query,
                            serial,
                            result,
                        } => {
                            assert_eq!(query, "mix");
                            assert_eq!(serial, 42);
                            let page = result.unwrap();
                            assert_eq!(page.items.len(), 1);
                            assert_eq!(page.items[0].name, "Weekend MIX");
                            found = true;
                        }
                        R::Search { result, .. } => assert_eq!(result.is_err(), fail_catalogue),
                        _ => {}
                    }
                }
            }
            assert!(found, "playlist matches must survive a catalogue error");
        }
    }
}
