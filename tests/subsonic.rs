//! Real application actions/views, opt-in disposable server and native output.
use sonicfast::{
    app::{App, AppOptions},
    backend::{AuthStatus, Waker},
    model::{Action, Loadable, Page},
    paths::AppDirs,
    player::Playback,
    settings::Settings,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn frame(app: &mut App, ctx: &egui::Context) {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 800.0),
            )),
            ..Default::default()
        },
        |ui| {
            app.background_frame(ctx);
            app.frame_ui(ui);
        },
    );
    output.textures_delta.clear();
}
fn until(app: &mut App, ctx: &egui::Context, ready: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        frame(app, ctx);
        if ready(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out: auth={:?}, local={:?}, page={:?}, busy={}, toasts={:?}, liked={:?}, playlists={:?}",
            app.auth,
            app.local,
            app.page(),
            app.playlist_busy,
            app.toasts
                .iter()
                .map(|toast| &toast.message)
                .collect::<Vec<_>>(),
            (
                app.library.liked.loaded_once,
                app.library.liked.loading,
                &app.library.liked.error,
                app.library.liked.items.len()
            ),
            app.playlist_pages
                .iter()
                .map(|(id, p)| (
                    id,
                    p.items.loading,
                    p.items.items.len(),
                    p.items.total,
                    p.pending_writes,
                    p.playlist
                        .get()
                        .map(|p| (&p.name, &p.description, p.public, p.track_total())),
                    &p.items.error
                ))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn dirs(name: &str) -> AppDirs {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/phase-3-app-tests")
        .join(format!("{name}-{}", std::process::id()));
    AppDirs {
        config: base.join("config"),
        state: base.join("state"),
        cache: base.join("cache"),
    }
}
fn create(dirs: AppDirs, restore: bool) -> App {
    dirs.ensure().unwrap();
    let settings = Settings {
        audio_device: std::env::var("LIBRESONIC_TEST_DEVICE").ok(),
        ..Default::default()
    };
    App::new_subsonic(
        &Waker::default(),
        dirs,
        settings,
        AppOptions {
            restore_sign_in: restore,
            media_controls: false,
            tray: false,
            autostart: false,
        },
    )
}
#[test]
#[ignore = "requires disposable Navidrome plus a native output (private PulseAudio is sufficient)"]
fn navidrome_login_browse_search_play_queue_resume_and_signout() {
    let dirs = dirs("flow");
    let ctx = egui::Context::default();
    let mut app = create(dirs.clone(), false);
    app.attach(&ctx);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::SignedOut));
    app.server_form.profile.name = "Navidrome fixture".into();
    app.server_form.profile.url =
        std::env::var("LIBRESONIC_TEST_URL").expect("explicit fixture URL");
    app.server_form.profile.username = "admin".into();
    app.server_form.profile.allow_http = true;
    app.server_form.secret = "wrong-password-fixture-only".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::Failed(_)));
    assert!(app.server_form.secret.is_empty());
    app.server_form.secret = "admin".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| {
        a.is_connected() && a.library.albums.loaded_once && a.library.artists.loaded_once
    });
    assert_eq!(app.library.albums.items.len(), 24);
    assert_eq!(app.library.artists.items.len(), 8);
    let album = app.library.albums.items[0].album.clone();
    app.actions
        .push(Action::Open(Page::Album(album.id.clone())));
    until(&mut app, &ctx, |a| {
        a.album_pages
            .get(&album.id)
            .is_some_and(|p| p.tracks.loaded_once)
    });
    let tracks = app.album_pages[&album.id].tracks.items.clone();
    assert_eq!(tracks.len(), 11);
    let mp3 = tracks
        .iter()
        .find(|t| t.is_playable == Some(true))
        .unwrap()
        .clone();
    let url = album.images[0].url.clone();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    until(&mut app, &ctx, |a| a.backend.art().is_ready(&url));
    let bytes = runtime.block_on(app.backend.art().fetch(&url)).unwrap();
    assert!(!bytes.is_empty());
    app.actions.push(Action::Search(mp3.name.clone()));
    until(&mut app, &ctx, |a| {
        matches!(a.search.results, Loadable::Loaded(_))
    });
    assert!(!app.search.results.get().unwrap().is_empty());
    // A later search row carries a one-song context, so its on-screen index
    // must become zero before resolving playback (the reported codec error).
    let query = mp3.name.split_whitespace().last().unwrap().to_string();
    app.actions.push(Action::Search(query.clone()));
    until(&mut app, &ctx, |a| {
        a.search.committed == query
            && !a.search.catalogue_pending
            && a.search
                .results
                .get()
                .and_then(|r| r.tracks.as_ref())
                .is_some_and(|p| {
                    p.items
                        .iter()
                        .filter(|t| t.is_playable == Some(true))
                        .count()
                        >= 2
                })
    });
    let results = &app
        .search
        .results
        .get()
        .unwrap()
        .tracks
        .as_ref()
        .unwrap()
        .items;
    let (index, chosen) = results
        .iter()
        .enumerate()
        .rev()
        .find(|(_, t)| t.is_playable == Some(true))
        .unwrap();
    assert!(index > 0, "fixture must exercise a non-first result");
    let chosen = chosen.uri.clone();
    app.actions.push(Action::PlayFromRow {
        context: sonicfast::model::RowContext::Uris(std::sync::Arc::from([chosen.clone()])),
        uri: chosen.clone(),
        index: index as u32,
    });
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing
            && a.local.track.as_ref().is_some_and(|t| t.uri == chosen)
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);

    app.actions.push(Action::Search("mixtape".into()));
    app.actions.push(Action::SetSearchFilter(
        sonicfast::model::SearchFilter::Playlists,
    ));
    until(&mut app, &ctx, |a| {
        a.search.committed == "mixtape"
            && a.search
                .results
                .get()
                .and_then(|r| r.playlists.as_ref())
                .is_some_and(|p| p.items.iter().any(|p| p.name == "mixtape"))
    });

    // Exercise M4A/AAC and FLAC through the same real UI action path.
    use libresonic::MusicService as _;
    let service = app.server_form.profile.client("admin").unwrap();
    let mut flac = None;
    let mut m4a = None;
    for saved in &app.library.albums.items {
        let domain = runtime
            .block_on(service.album(&sonicfast::subsonic::adapter::id(&saved.album.id).unwrap()))
            .unwrap();
        for track in domain.tracks {
            match track.suffix.as_deref() {
                Some("flac") if flac.is_none() => flac = Some(track),
                Some("m4a") if m4a.is_none() => m4a = Some(track),
                _ => {}
            }
        }
        if flac.is_some() && m4a.is_some() {
            break;
        }
    }
    let m4a = m4a.expect("generated fixture has an AAC/M4A album");
    let m4a_uri = sonicfast::subsonic::adapter::uri(&m4a.id);
    assert_eq!(
        sonicfast::subsonic::adapter::track(&m4a).is_playable,
        Some(true)
    );
    app.actions.push(Action::PlayUris {
        uris: vec![m4a_uri.clone()],
        index: 0,
    });
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing
            && a.local.track.as_ref().is_some_and(|t| t.uri == m4a_uri)
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    app.actions.push(Action::Seek(500));
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Paused && a.local.position_ms == 500
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing && a.local.position_ms >= 500
    });
    let flac = flac.expect("generated fixture has a FLAC album");
    let flac_uri = sonicfast::subsonic::adapter::uri(&flac.id);
    app.actions.push(Action::PlayUris {
        uris: vec![flac_uri.clone()],
        index: 0,
    });
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing
            && a.local.track.as_ref().is_some_and(|t| t.uri == flac_uri)
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    app.actions.push(Action::Seek(500));
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Paused && a.local.position_ms == 500
    });
    app.actions.push(Action::PlayUris {
        uris: vec![mp3.uri.clone(), mp3.uri.clone()],
        index: 0,
    });
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing
            && a.local.track.as_ref().is_some_and(|t| t.uri == mp3.uri)
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    app.actions.push(Action::QueueMany {
        songs: vec![
            (mp3.uri.clone(), mp3.name.clone()),
            (mp3.uri.clone(), mp3.name.clone()),
        ],
    });
    until(&mut app, &ctx, |a| a.manual_queue.len() == 2);
    assert_eq!(app.queued_rows_len(), 2);
    app.actions.push(Action::Seek(1000));
    until(&mut app, &ctx, |a| {
        a.local.position_ms == 1000 && a.local.playback == Playback::Paused
    });
    app.actions.push(Action::SetShuffle(true));
    app.actions
        .push(Action::SetRepeat(sonicfast::player::RepeatMode::Context));
    until(&mut app, &ctx, |a| {
        a.local.shuffle && a.local.repeat == sonicfast::player::RepeatMode::Context
    });
    app.actions.push(Action::ClearQueue);
    until(&mut app, &ctx, |a| a.manual_queue.is_empty());
    assert_eq!(app.queue.get().unwrap().queue.len(), 1);
    app.actions.push(Action::Next);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Playing);
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    let profile = app.server_form.profile.clone();
    app.save_state();
    app.backend.shutdown();
    drop(app);
    let mut restored = create(dirs.clone(), false);
    restored.attach(&ctx);
    until(&mut restored, &ctx, |a| {
        matches!(a.auth, AuthStatus::SignedOut)
    });
    restored.server_form.profile = profile;
    restored.server_form.secret = "admin".into();
    restored.actions.push(Action::ServerConnect);
    until(&mut restored, &ctx, |a| {
        a.is_connected() && a.local.track.is_some()
    });
    assert_eq!(restored.local.playback, Playback::Paused);
    restored.actions.push(Action::SignOut);
    until(&mut restored, &ctx, |a| {
        !a.is_connected() && a.local.track.is_none()
    });
    let profiles = std::fs::read_to_string(dirs.config.join("servers.json")).unwrap();
    assert!(!profiles.contains("password"));
    assert!(!profiles.contains("apiKey"));
    restored.backend.shutdown();
}

#[test]
#[ignore = "requires disposable Navidrome and an unlocked private system credential store"]
fn remembered_server_login_restores_paused_and_signout_revokes_restart() {
    let dirs = dirs("remembered");
    let ctx = egui::Context::default();
    let mut app = create(dirs.clone(), false);
    app.attach(&ctx);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::SignedOut));
    app.server_form.profile.name = "Remembered fixture".into();
    app.server_form.profile.url = std::env::var("LIBRESONIC_TEST_URL").expect("fixture URL");
    app.server_form.profile.username = "admin".into();
    app.server_form.profile.allow_http = true;
    app.server_form.profile.remember = true;
    app.server_form.secret = "admin".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| {
        a.is_connected() && a.library.albums.loaded_once && a.server_form.profile.remember
    });
    assert!(app.server_form.notice.is_none());
    let album = app.library.albums.items[0].album.id.clone();
    app.actions.push(Action::Open(Page::Album(album.clone())));
    until(&mut app, &ctx, |a| {
        a.album_pages
            .get(&album)
            .is_some_and(|p| p.tracks.loaded_once)
    });
    let track = app.album_pages[&album]
        .tracks
        .items
        .iter()
        .find(|t| t.is_playable == Some(true))
        .unwrap()
        .clone();
    app.actions.push(Action::PlayUris {
        uris: vec![track.uri],
        index: 0,
    });
    until(&mut app, &ctx, |a| a.local.playback == Playback::Playing);
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    assert!(!app.now_playing().unwrap().playing);
    app.backend.shutdown();
    drop(app);
    let mut restored = create(dirs.clone(), true);
    restored.attach(&ctx);
    until(&mut restored, &ctx, |a| {
        a.is_connected() && a.local.track.is_some()
    });
    assert_eq!(restored.local.playback, Playback::Paused);
    assert!(restored.server_form.secret.is_empty());
    restored.actions.push(Action::SignOut);
    until(&mut restored, &ctx, |a| {
        !a.is_connected()
            && std::fs::read(dirs.config.join("servers.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .is_some_and(|p| p["selected"].is_null())
    });
    restored.backend.shutdown();
    drop(restored);
    let mut signed_out = create(dirs, true);
    signed_out.attach(&ctx);
    until(&mut signed_out, &ctx, |a| {
        matches!(a.auth, AuthStatus::SignedOut)
    });
    for _ in 0..10 {
        frame(&mut signed_out, &ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!signed_out.is_connected());
    signed_out.backend.shutdown();
}

#[test]
fn slow_login_keeps_frames_responsive_and_cancel_suppresses_late_results() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let ended = stop.clone();
    let server = std::thread::spawn(move || {
        let mut connections = Vec::new();
        while !ended.load(Ordering::SeqCst) {
            if let Ok((socket, _)) = listener.accept() {
                connections.push(socket);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let ctx = egui::Context::default();
    let mut app = create(dirs("slow-login"), false);
    app.attach(&ctx);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::SignedOut));
    app.server_form.profile.name = "Slow local mock".into();
    app.server_form.profile.url = url;
    app.server_form.profile.allow_http = true;
    app.server_form.profile.username = "fixture".into();
    app.server_form.secret = "fixture-only".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::Connecting));
    let at = Instant::now();
    for _ in 0..10 {
        frame(&mut app, &ctx);
    }
    assert!(
        at.elapsed() < Duration::from_secs(2),
        "Frames waited for the pending HTTP response"
    );
    app.actions.push(Action::SignOut);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::SignedOut));
    for _ in 0..10 {
        frame(&mut app, &ctx);
    }
    assert!(!app.is_connected());
    app.backend.shutdown();
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
}

#[test]
#[ignore = "requires disposable Navidrome and native output; creates/deletes only a synthetic test playlist"]
fn navidrome_playlists_create_play_duplicate_rows_edit_conflict_rollback_and_delete() {
    use libresonic::MusicService as _;
    use sonicfast::{api::models::PlayableItem, model::Dialog, subsonic::adapter as m};
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut app = create(dirs("playlists"), false);
    app.attach(&ctx);
    app.server_form.profile.name = "Playlist fixture".into();
    app.server_form.profile.url =
        std::env::var("LIBRESONIC_TEST_URL").expect("explicit fixture URL");
    app.server_form.profile.username = "admin".into();
    app.server_form.profile.allow_http = true;
    app.server_form.secret = "admin".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| {
        a.is_connected()
            && a.library.playlists.get().is_some()
            && !a.library.albums.items.is_empty()
    });
    let album = app.library.albums.items[0].album.id.clone();
    app.actions.push(Action::Open(Page::Album(album.clone())));
    until(&mut app, &ctx, |a| {
        a.album_pages
            .get(&album)
            .is_some_and(|p| p.tracks.loaded_once)
    });
    let tracks: Vec<_> = app.album_pages[&album]
        .tracks
        .items
        .iter()
        .filter(|t| t.is_playable == Some(true))
        .take(2)
        .cloned()
        .collect();
    let first = tracks[0].clone();
    let second = tracks[1].clone();
    let service = app.server_form.profile.client("admin").unwrap();
    eprintln!("playlist stage: CreatePlaylist");
    app.actions.push(Action::CreatePlaylist {
        name: format!("Sonicfast synthetic playlist {}", std::process::id()),
        public: false,
        add_uris: vec![first.uri.clone(), first.uri.clone(), second.uri.clone()],
    });
    until(&mut app, &ctx, |a| matches!(a.page(), Page::Playlist(_)));
    let Page::Playlist(id) = app.page().clone() else {
        unreachable!()
    };
    let key = m::id(&id).unwrap();
    let ready = |a: &App, expected: &[&str]| {
        !a.playlist_busy
            && a.playlist_pages.get(&id).is_some_and(|p| {
                !p.items.loading
                    && p.playlist
                        .get()
                        .is_some_and(|p| p.track_total() as usize == expected.len())
                    && p.items
                        .items
                        .iter()
                        .filter_map(|i| i.playable())
                        .map(|t| t.uri())
                        .collect::<Vec<_>>()
                        == expected
            })
    };
    until(&mut app, &ctx, |a| {
        ready(a, &[&first.uri, &first.uri, &second.uri])
    });
    assert!(app.can_edit_playlist(app.known_playlist(&id).unwrap()));
    let p = runtime.block_on(service.playlist(&key)).unwrap();
    assert_ne!(p.playlist.public, Some(true)); // Navidrome omits false booleans.
    assert_eq!(p.entries[0], p.entries[1]);
    let context = sonicfast::model::RowContext::Context {
        uri: m::uri(&key),
        editable_playlist: None,
    };
    app.actions.push(Action::PlayContext {
        uri: m::uri(&key),
        offset_uri: None,
        offset_index: Some(1),
    });
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing && a.row_is_played_copy(&context, 1, &first.uri)
    });
    assert!(!app.row_is_played_copy(&context, 0, &first.uri));
    app.actions.push(Action::Next);
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing
            && a.local.track.as_ref().is_some_and(|t| t.uri == second.uri)
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    eprintln!("playlist stage: RemoveFromPlaylist");
    app.actions.push(Action::RemoveFromPlaylist {
        playlist_id: id.clone(),
        uris: vec![first.uri.clone()],
        positions: vec![1],
    });
    until(&mut app, &ctx, |a| ready(a, &[&first.uri, &second.uri]));
    eprintln!("playlist stage: MoveInPlaylist");
    app.actions.push(Action::MoveInPlaylist {
        playlist_id: id.clone(),
        from: 1,
        to: 0,
    });
    until(&mut app, &ctx, |a| ready(a, &[&second.uri, &first.uri]));
    eprintln!("playlist stage: InsertInPlaylist");
    app.actions.push(Action::InsertInPlaylist {
        playlist_id: id.clone(),
        position: 1,
        items: vec![PlayableItem::Track(first.clone())],
    });
    until(&mut app, &ctx, |a| {
        matches!(a.dialog, Some(Dialog::ConfirmPlaylistDuplicates { .. }))
    });
    eprintln!("playlist stage: ConfirmAddToPlaylist");
    app.actions.push(Action::ConfirmAddToPlaylist {
        playlist_id: id.clone(),
        playlist_name: "Fixture".into(),
        items: vec![PlayableItem::Track(first.clone())],
        position: Some(1),
    });
    until(&mut app, &ctx, |a| {
        ready(a, &[&second.uri, &first.uri, &first.uri])
    });
    eprintln!("playlist stage: UpdatePlaylist");
    app.actions.push(Action::UpdatePlaylist {
        id: id.clone(),
        name: "Renamed synthetic playlist".into(),
        description: "temporary comment".into(),
        public: Some(true),
    });
    until(&mut app, &ctx, |a| {
        !a.playlist_busy
            && a.known_playlist(&id)
                .is_some_and(|p| p.name == "Renamed synthetic playlist" && p.public == Some(true))
    });
    eprintln!("playlist stage: UpdatePlaylist");
    app.actions.push(Action::UpdatePlaylist {
        id: id.clone(),
        name: "Renamed synthetic playlist".into(),
        description: String::new(),
        public: Some(false),
    });
    until(&mut app, &ctx, |a| {
        !a.playlist_busy
            && a.known_playlist(&id).is_some_and(|p| {
                p.description.as_deref().unwrap_or("").is_empty() && p.public != Some(true)
            })
    });
    until(&mut app, &ctx, |a| {
        ready(a, &[&second.uri, &first.uri, &first.uri])
    });

    // A rejected addition must remove the optimistic ghost row, leaving the server intact.
    let mut foreign = first.clone();
    foreign.uri = m::uri(
        &libresonic::MediaId::new(
            libresonic::ServerId::new("another-server").unwrap(),
            libresonic::MediaKind::Track,
            "foreign",
        )
        .unwrap(),
    );
    eprintln!("playlist stage: AddToPlaylist");
    app.actions.push(Action::AddToPlaylist {
        playlist_id: id.clone(),
        playlist_name: "Fixture".into(),
        items: vec![PlayableItem::Track(foreign)],
    });
    frame(&mut app, &ctx);
    until(&mut app, &ctx, |a| {
        ready(a, &[&second.uri, &first.uri, &first.uri])
    });
    assert_eq!(
        runtime
            .block_on(service.playlist(&key))
            .unwrap()
            .entries
            .len(),
        3
    );

    // Simulate another client editing after our rows were loaded. Stale removal
    // must reconcile instead of deleting whichever occurrence now occupies it.
    let remote = m::id(&second.uri).unwrap();
    runtime
        .block_on(service.update_playlist(
            &key,
            &libresonic::PlaylistUpdate {
                add: vec![remote],
                ..Default::default()
            },
        ))
        .unwrap();
    eprintln!("playlist stage: RemoveFromPlaylist");
    app.actions.push(Action::RemoveFromPlaylist {
        playlist_id: id.clone(),
        uris: vec![first.uri.clone()],
        positions: vec![1],
    });
    frame(&mut app, &ctx);
    until(&mut app, &ctx, |a| {
        ready(a, &[&second.uri, &first.uri, &first.uri, &second.uri])
    });
    assert_eq!(
        runtime
            .block_on(service.playlist(&key))
            .unwrap()
            .entries
            .len(),
        4
    );
    eprintln!("playlist stage: DeletePlaylist");
    app.actions.push(Action::DeletePlaylist(id.clone()));
    until(&mut app, &ctx, |a| {
        !a.playlist_busy
            && a.library
                .playlists
                .get()
                .is_some_and(|p| !p.iter().any(|p| p.id == id))
            && !matches!(a.page(), Page::Playlist(current) if *current == id)
    });
    assert_eq!(
        runtime.block_on(service.playlist(&key)),
        Err(libresonic::Error::NotFound)
    );
    app.backend.shutdown();
}

#[test]
#[ignore = "requires disposable Navidrome, private audio/keyring; changes only synthetic song feedback"]
fn navidrome_song_hearts_ratings_lyrics_and_duplicate_confirmation() {
    use libresonic::MusicService;
    use sonicfast::{api::models::PlayableItem, model::Dialog, subsonic::adapter as m};
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut app = create(dirs("song-feedback"), false);
    app.attach(&ctx);
    app.server_form.profile.name = "Feedback fixture".into();
    app.server_form.profile.url =
        std::env::var("LIBRESONIC_TEST_URL").expect("explicit fixture URL");
    app.server_form.profile.username = "admin".into();
    app.server_form.profile.allow_http = true;
    app.server_form.secret = "admin".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| {
        a.is_connected() && a.user_id().is_some() && !a.library.albums.items.is_empty()
    });
    let album = app.library.albums.items[0].album.id.clone();
    let service = app.server_form.profile.client("admin").unwrap();
    let setup = runtime
        .block_on(service.album(&m::id(&album).unwrap()))
        .unwrap();
    let setup_id = &setup.tracks.iter().find(|t| m::playable(t)).unwrap().id;
    runtime
        .block_on(service.set_favorite_songs(std::slice::from_ref(setup_id), false))
        .unwrap();
    runtime
        .block_on(service.set_song_rating(setup_id, 0))
        .unwrap();
    app.actions.push(Action::Open(Page::Album(album.clone())));
    until(&mut app, &ctx, |a| {
        a.album_pages
            .get(&album)
            .is_some_and(|p| p.tracks.loaded_once)
    });
    let track = app.album_pages[&album]
        .tracks
        .items
        .iter()
        .find(|t| t.is_playable == Some(true))
        .unwrap()
        .clone();
    let id = m::id(&track.uri).unwrap();
    eprintln!("feedback stage: LikedSongs");
    app.actions.push(Action::Open(Page::LikedSongs));
    until(&mut app, &ctx, |a| {
        a.library.liked.loaded_once && !a.library.liked.loading
    });
    app.actions.push(Action::ToggleSaved(track.uri.clone()));
    until(&mut app, &ctx, |a| {
        a.is_saved(&track.uri) == Some(true)
            && a.library
                .liked
                .items
                .iter()
                .any(|t| t.track.uri == track.uri)
    });
    // Wait for server confirmation rather than just the optimistic heart.
    until(&mut app, &ctx, |_| {
        runtime.block_on(service.song(&id)).unwrap().favorite
    });
    eprintln!("feedback stage: ratings");
    for rating in 1..=5 {
        app.actions.push(Action::SetSongRating {
            uri: track.uri.clone(),
            rating,
        });
        until(&mut app, &ctx, |a| {
            !a.song_rating_pending(&track.uri) && a.song_rating(&track) == rating
        });
        let song = runtime.block_on(service.song(&id)).unwrap();
        assert_eq!(song.rating, Some(rating));
        assert!(song.favorite, "rating must not change the heart");
    }
    app.actions.push(Action::SetSongRating {
        uri: track.uri.clone(),
        rating: 0,
    });
    until(&mut app, &ctx, |a| {
        !a.song_rating_pending(&track.uri) && a.song_rating(&track) == 0
    });
    assert_eq!(runtime.block_on(service.song(&id)).unwrap().rating, None);
    // Reuse the actual LRCLIB cache and existing native lyrics panel, without
    // sending the synthetic fixture song to a public service.
    // Same view-context action emitted by the Liked Songs header.
    app.actions.push(Action::PlayFromRow {
        context: sonicfast::model::RowContext::View {
            uris: vec![track.uri.clone()].into(),
            context_uri: format!("sonicfast:liked:{}", app.user_id().unwrap()),
            editable_playlist: None,
        },
        uri: String::new(),
        index: 0,
    });
    until(&mut app, &ctx, |a| a.local.playback == Playback::Playing);
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    let now = app.now_playing().unwrap();
    let query = sonicfast::lyrics::Query {
        artist: now.artists.first().unwrap().name.clone(),
        title: now.title,
        album: now.album_name,
        duration_ms: now.duration_ms,
    };
    let path = sonicfast::lyrics::cache_path(&app.dirs.lyrics_cache_dir(), &query);
    sonicfast::lyrics::store(
        &path,
        &sonicfast::lyrics::from_text("[00:00.00]Synthetic lyric line\n[00:01.00]Second line"),
    );
    eprintln!("feedback stage: lyrics");
    app.actions.push(Action::ToggleLyricsPanel);
    until(
        &mut app,
        &ctx,
        |a| matches!(&a.lyrics,Loadable::Loaded(Some(l)) if l.synced && l.lines[0].text=="Synthetic lyric line"),
    );
    eprintln!("feedback stage: duplicates");
    app.actions.push(Action::CreatePlaylist {
        name: "Synthetic duplicate confirmation".into(),
        public: false,
        add_uris: vec![track.uri.clone()],
    });
    until(&mut app, &ctx, |a| {
        matches!(a.page(), Page::Playlist(_)) && !a.playlist_busy
    });
    let Page::Playlist(playlist) = app.page().clone() else {
        unreachable!()
    };
    let key = m::id(&playlist).unwrap();
    until(&mut app, &ctx, |a| {
        a.playlist_pages
            .get(&playlist)
            .is_some_and(|p| p.items.loaded_once && !p.items.loading)
    });
    app.actions.push(Action::AddToPlaylist {
        playlist_id: playlist.clone(),
        playlist_name: "Synthetic".into(),
        items: vec![PlayableItem::Track(track.clone())],
    });
    until(&mut app, &ctx, |a| {
        matches!(a.dialog, Some(Dialog::ConfirmPlaylistDuplicates { .. }))
    });
    assert_eq!(
        runtime
            .block_on(service.playlist(&key))
            .unwrap()
            .entries
            .len(),
        1
    );
    app.actions.push(Action::CloseDialog);
    frame(&mut app, &ctx);
    assert_eq!(
        runtime
            .block_on(service.playlist(&key))
            .unwrap()
            .entries
            .len(),
        1,
        "Cancel changes nothing"
    );
    app.actions.push(Action::AddToPlaylist {
        playlist_id: playlist.clone(),
        playlist_name: "Synthetic".into(),
        items: vec![PlayableItem::Track(track.clone())],
    });
    until(&mut app, &ctx, |a| {
        matches!(a.dialog, Some(Dialog::ConfirmPlaylistDuplicates { .. }))
    });
    app.actions.push(Action::ConfirmAddToPlaylist {
        playlist_id: playlist.clone(),
        playlist_name: "Synthetic".into(),
        items: vec![PlayableItem::Track(track.clone())],
        position: None,
    });
    until(&mut app, &ctx, |a| {
        !a.playlist_busy
            && runtime
                .block_on(service.playlist(&key))
                .unwrap()
                .entries
                .len()
                == 2
    });
    app.actions.push(Action::DeletePlaylist(playlist));
    until(&mut app, &ctx, |a| !a.playlist_busy);
    app.actions.push(Action::ToggleSaved(track.uri.clone()));
    until(&mut app, &ctx, |a| {
        a.is_saved(&track.uri) == Some(false)
            && !runtime.block_on(service.song(&id)).unwrap().favorite
    });
    app.actions.push(Action::SignOut);
    until(&mut app, &ctx, |a| matches!(a.auth, AuthStatus::SignedOut));
    app.backend.shutdown();
}

#[test]
#[ignore = "requires disposable Navidrome, private output/keyring; original downloads and real play-count updates"]
fn navidrome_downloads_local_playback_restart_and_configurable_scrobbling() {
    use libresonic::MusicService;
    use sonicfast::subsonic::{
        adapter as m,
        downloads::{Settings as DownloadSettings, Status},
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let dirs = dirs("downloads-scrobbling");
    let download_root = dirs.cache.join("originals");
    let mut app = create(dirs.clone(), false);
    app.attach(&ctx);
    app.server_form.profile.name = "Download fixture".into();
    app.server_form.profile.url = std::env::var("LIBRESONIC_TEST_LOCAL_URL")
        .or_else(|_| std::env::var("LIBRESONIC_TEST_URL"))
        .expect("fixture URL");
    app.server_form.profile.username = "admin".into();
    app.server_form.profile.allow_http = true;
    app.server_form.secret = "admin".into();
    app.actions.push(Action::ServerConnect);
    until(&mut app, &ctx, |a| {
        a.is_connected() && !a.library.albums.items.is_empty()
    });
    let album = app.library.albums.items[0].album.id.clone();
    app.actions.push(Action::Open(Page::Album(album.clone())));
    until(&mut app, &ctx, |a| {
        a.album_pages
            .get(&album)
            .is_some_and(|p| p.tracks.loaded_once)
    });
    let track = app.album_pages[&album]
        .tracks
        .items
        .iter()
        .find(|t| t.is_playable == Some(true) && t.duration_ms >= 2500)
        .unwrap()
        .clone();
    let id = m::id(&track.uri).unwrap();
    let service = app.server_form.profile.client("admin").unwrap();
    let count = || {
        runtime
            .block_on(service.song(&id))
            .unwrap()
            .play_count
            .unwrap_or(0)
    };
    let baseline = count();
    let downloads = DownloadSettings {
        enabled: true,
        path: download_root.display().to_string(),
    };
    let mut scrobble = libresonic::playback::scrobble::ScrobbleSettings {
        enabled: false,
        percent: 50,
        seconds: 1,
    };
    app.actions.push(Action::LibrarySettings {
        scrobble: scrobble.clone(),
        downloads: downloads.clone(),
    });
    app.actions.push(Action::DownloadControl {
        uri: None,
        action: "pause".into(),
    });
    app.actions.push(Action::DownloadTracks(vec![
        track.uri.clone(),
        track.uri.clone(),
    ]));
    until(&mut app, &ctx, |a| {
        a.downloads_paused
            && a.download_jobs
                .get(&track.uri)
                .is_some_and(|j| j.status == Status::Queued)
    });
    app.actions.push(Action::Open(Page::Downloads));
    frame(&mut app, &ctx);
    app.actions.push(Action::DownloadControl {
        uri: None,
        action: "resume".into(),
    });
    until(&mut app, &ctx, |a| {
        a.download_jobs
            .get(&track.uri)
            .is_some_and(|j| j.status == Status::Complete)
    });
    let job = &app.download_jobs[&track.uri];
    let original = download_root.join(job.relative.as_ref().unwrap());
    assert_eq!(std::fs::metadata(&original).unwrap().len(), job.bytes);
    assert!(job.sha256.is_some());
    assert_eq!(job.total, Some(job.bytes));
    assert!(!download_root.join(".sonicfast/index.json").exists());
    eprintln!("download/scrobble stage: visible icons follow external delete and restore");
    app.actions.push(Action::Open(Page::Album(album.clone())));
    until(&mut app, &ctx, |a| matches!(a.page(), Page::Album(_)));
    let parked = original.with_extension("removed-for-test");
    std::fs::rename(&original, &parked).unwrap();
    until(&mut app, &ctx, |a| {
        a.download_jobs
            .get(&track.uri)
            .is_some_and(|job| job.status == Status::Remote)
    });
    std::fs::rename(&parked, &original).unwrap();
    until(&mut app, &ctx, |a| {
        a.download_jobs
            .get(&track.uri)
            .is_some_and(|job| job.status == Status::Complete)
    });
    eprintln!(
        "download/scrobble stage: local playback with streaming blocked and reporting disabled"
    );
    app.actions.push(Action::PlayUris {
        uris: vec![track.uri.clone()],
        index: 0,
    });
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing && a.local.position_now() >= 1100
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    assert_eq!(
        count(),
        baseline,
        "disabled reporting must not record a play"
    );
    eprintln!("download/scrobble stage: enable and listen through threshold");
    scrobble.enabled = true;
    app.actions.push(Action::LibrarySettings {
        scrobble: scrobble.clone(),
        downloads: downloads.clone(),
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |_| count() == baseline + 1);
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    app.actions.push(Action::Seek(0));
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| {
        a.local.playback == Playback::Playing && a.local.position_now() >= 1200
    });
    app.actions.push(Action::TogglePlay);
    until(&mut app, &ctx, |a| a.local.playback == Playback::Paused);
    assert_eq!(
        count(),
        baseline + 1,
        "seek/replay within the same occurrence cannot duplicate a submission"
    );
    scrobble.enabled = false;
    app.actions.push(Action::LibrarySettings {
        scrobble: scrobble.clone(),
        downloads: downloads.clone(),
    });
    frame(&mut app, &ctx);
    let pending_track = app.album_pages[&album]
        .tracks
        .items
        .iter()
        .find(|song| song.uri != track.uri && song.is_playable == Some(true))
        .unwrap()
        .clone();
    app.actions.push(Action::DownloadControl {
        uri: None,
        action: "pause".into(),
    });
    app.actions
        .push(Action::DownloadTracks(vec![pending_track.uri.clone()]));
    until(&mut app, &ctx, |a| {
        a.downloads_paused
            && a.download_jobs
                .get(&pending_track.uri)
                .is_some_and(|job| job.status == Status::Queued)
    });
    let profile = app.server_form.profile.clone();
    app.backend.shutdown();
    eprintln!(
        "download/scrobble stage: restore unfinished queue and discover original without an index"
    );
    let mut restored = create(dirs, false);
    restored.attach(&ctx);
    until(&mut restored, &ctx, |a| {
        matches!(a.auth, AuthStatus::SignedOut)
    });
    restored.server_form.profile = profile;
    restored.server_form.secret = "admin".into();
    restored.actions.push(Action::LibrarySettings {
        scrobble,
        downloads: downloads.clone(),
    });
    restored.actions.push(Action::ServerConnect);
    until(&mut restored, &ctx, |a| {
        a.is_connected()
            && a.downloads_paused
            && a.download_jobs
                .get(&pending_track.uri)
                .is_some_and(|job| job.status == Status::Queued)
    });
    restored.actions.push(Action::Open(Page::Album(album)));
    until(&mut restored, &ctx, |a| {
        a.download_jobs
            .get(&track.uri)
            .is_some_and(|job| job.status == Status::Complete)
    });
    assert!(!download_root.join(".sonicfast/index.json").exists());
    restored.actions.push(Action::PlayUris {
        uris: vec![track.uri.clone()],
        index: 0,
    });
    until(&mut restored, &ctx, |a| {
        a.local.playback == Playback::Playing && a.local.position_now() >= 500
    });
    restored.actions.push(Action::TogglePlay);
    until(&mut restored, &ctx, |a| {
        a.local.playback == Playback::Paused
    });
    assert_eq!(count(), baseline + 1);
    restored.actions.push(Action::SignOut);
    until(&mut restored, &ctx, |a| {
        matches!(a.auth, AuthStatus::SignedOut)
    });
    assert!(
        original.is_file(),
        "sign-out must preserve downloaded files"
    );
    restored.backend.shutdown();
}
