//! Opt-in native UI walkthrough using only the disposable Navidrome fixture.
//! Saves review captures outside tracked files; takes no secret command arguments.
use sonicfast::{
    app::{App, AppOptions},
    backend::{AuthStatus, Waker},
    model::{Action, Dialog, Page},
    paths::AppDirs,
    player::Playback,
    settings::Settings,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Walkthrough {
    app: App,
    output: PathBuf,
    stage: usize,
    pending: Option<&'static str>,
    ready_at: Option<Instant>,
    started: Instant,
    album: String,
    playlist: String,
    track: String,
    title: String,
    ctx: Option<egui::Context>,
    menu_pressed: bool,
    menu_released: bool,
}
impl Walkthrough {
    fn review_settings(&mut self, filter: &str) {
        self.ctx
            .as_ref()
            .unwrap()
            .data_mut(|d| d.insert_temp(egui::Id::new("settings-filter"), filter.to_string()));
        self.app.actions.push(Action::Open(Page::Settings));
    }
    fn next(&mut self, shot: &str) {
        println!("Captured {shot}");
        self.ready_at = None;
        match shot {
            "login" => self.app.actions.push(Action::ServerConnect),
            "library" => {
                self.album = self.app.library.albums.items[0].album.id.clone();
                self.app
                    .actions
                    .push(Action::Open(Page::Album(self.album.clone())));
            }
            "album" => {
                let tracks = &self.app.album_pages[&self.album].tracks.items;
                let track = tracks
                    .iter()
                    .find(|t| t.is_playable == Some(true))
                    .expect("fixture playable track");
                self.track = track.uri.clone();
                self.title = track.name.clone();
                self.app.actions.push(Action::PlayUris {
                    uris: vec![self.track.clone(), self.track.clone()],
                    index: 0,
                });
                self.app.actions.push(Action::QueueMany {
                    songs: vec![
                        (self.track.clone(), self.title.clone()),
                        (self.track.clone(), self.title.clone()),
                    ],
                });
                self.app.show_queue_panel = true;
            }
            "playing-bars" => {}
            "playing-bars-later" => {
                self.app.actions.push(Action::TogglePlay);
            }
            "queue" => {
                self.app.actions.push(Action::Search(self.title.clone()));
                self.app.actions.push(Action::Open(Page::Search));
            }
            "search" => self
                .app
                .actions
                .push(Action::ShowDialog(Dialog::CreatePlaylist {
                    name: "Native fixture playlist".into(),
                    public: false,
                    add_uris: vec![self.track.clone(), self.track.clone()],
                })),
            "playlist-create" => self.app.actions.push(Action::CreatePlaylist {
                name: "Native fixture playlist".into(),
                public: false,
                add_uris: vec![self.track.clone(), self.track.clone()],
            }),
            "playlist" => {
                let Page::Playlist(id) = self.app.page().clone() else {
                    unreachable!()
                };
                self.playlist = id;
                self.app
                    .actions
                    .push(Action::Search("Native fixture playlist".into()));
                self.app.actions.push(Action::SetSearchFilter(
                    sonicfast::model::SearchFilter::Playlists,
                ));
            }
            "playlist-search" => {
                self.app
                    .actions
                    .push(Action::Open(Page::Playlist(self.playlist.clone())));
                self.app
                    .actions
                    .push(Action::ShowDialog(Dialog::EditPlaylist {
                        cover: Default::default(),
                        id: self.playlist.clone(),
                        name: "Native fixture playlist".into(),
                        description: "Playlist notes".into(),
                        public: Some(false),
                    }));
            }
            "playlist-edit" => {
                self.app
                    .actions
                    .push(Action::ShowDialog(Dialog::ConfirmDeletePlaylist {
                        id: self.playlist.clone(),
                        name: "Native fixture playlist".into(),
                        owned: true,
                    }))
            }
            "playlist-delete" => {
                self.app
                    .actions
                    .push(Action::DeletePlaylist(self.playlist.clone()));
                self.app.actions.push(Action::SetSongRating {
                    uri: self.track.clone(),
                    rating: 4,
                });
                self.app.actions.push(Action::Open(Page::LikedSongs));
            }
            "player-unsaved" => {}
            "player-saved" => {}
            "player-playlists" => {
                let now = self.app.now_playing().unwrap();
                let query = sonicfast::lyrics::Query {
                    artist: now.artists.first().unwrap().name.clone(),
                    title: now.title,
                    album: now.album_name,
                    duration_ms: now.duration_ms,
                };
                let path = sonicfast::lyrics::cache_path(&self.app.dirs.lyrics_cache_dir(), &query);
                sonicfast::lyrics::store(
                    &path,
                    &sonicfast::lyrics::from_text(
                        "[00:00.00]Synthetic fixture lyrics\n[00:01.00]Words follow the player\n[00:02.00]Generated review text",
                    ),
                );
                self.app.actions.push(Action::ToggleLyricsPanel);
            }
            "lyrics" => {
                self.app.actions.push(Action::ToggleLyricsPanel);
                self.app
                    .actions
                    .push(Action::ToggleSaved(self.track.clone()));
                self.app.actions.push(Action::SetSongRating {
                    uri: self.track.clone(),
                    rating: 0,
                });
                self.app.actions.push(Action::LibrarySettings {
                    scrobble: libresonic::playback::scrobble::ScrobbleSettings {
                        enabled: true,
                        percent: 50,
                        seconds: 240,
                    },
                    downloads: sonicfast::subsonic::downloads::Settings {
                        enabled: true,
                        path: self
                            .app
                            .dirs
                            .cache
                            .join("downloaded-originals")
                            .display()
                            .to_string(),
                    },
                });
                self.app.actions.push(Action::Open(Page::Settings));
            }
            "settings" => {
                self.app
                    .actions
                    .push(Action::DownloadTracks(vec![self.track.clone()]));
                self.app
                    .actions
                    .push(Action::Open(Page::Album(self.album.clone())));
                self.app.show_queue_panel = false;
            }
            "download-active" => self.app.actions.push(Action::Open(Page::Downloads)),
            "downloads-queue" => self
                .app
                .actions
                .push(Action::Open(Page::Album(self.album.clone()))),
            "download-complete" => {
                self.app.show_queue_panel = true;
            }
            "download-queue-panel" => {
                self.app.show_queue_panel = false;
                let artist = self.app.album_pages[&self.album].tracks.items[0].artists[0]
                    .id
                    .clone()
                    .expect("fixture song artist ID");
                self.app.actions.push(Action::Open(Page::Artist(artist)));
            }
            "artist" => self.review_settings("Server"),
            "settings-server" => self.review_settings("Scrobbling"),
            "settings-scrobbling" => self.review_settings("Downloads"),
            "settings-downloads" => self.review_settings("Playback"),
            "settings-playback" => self.review_settings("Appearance"),
            "settings-appearance" => {
                self.app
                    .actions
                    .push(Action::SetTheme(sonicfast::settings::ThemeChoice::Light));
                self.review_settings("Downloads");
            }
            "settings-downloads-light" => {
                assert!(
                    !self.app.palette.dark,
                    "light theme must be applied before capture"
                );
                self.ctx
                    .as_ref()
                    .unwrap()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(680.0, 850.0)));
            }
            "settings-downloads-narrow" => {
                self.ctx
                    .as_ref()
                    .unwrap()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1200.0, 850.0)));
                self.app.actions.push(Action::Open(Page::Home));
            }
            "library-light" => {
                // A stale preference must neither label nor offer a Spotify sort.
                self.app.settings.library_sort.insert(
                    sonicfast::settings::LibraryShelf::Playlists,
                    sonicfast::settings::LibrarySort::Spotify,
                );
            }
            "library-sort-menu" => {
                egui::Popup::close_all(self.ctx.as_ref().unwrap());
                self.app
                    .actions
                    .push(Action::SetTheme(sonicfast::settings::ThemeChoice::Dark));
                self.app.search.query.clear();
                let track = self.app.album_pages[&self.album]
                    .tracks
                    .items
                    .iter()
                    .rev()
                    .max_by_key(|t| t.duration_ms)
                    .unwrap();
                self.track = track.uri.clone();
                self.app.actions.push(Action::ClearQueue);
                self.app.actions.push(Action::PlayContext {
                    uri: format!("sonicfast:album:{}", self.album),
                    offset_uri: Some(self.track.clone()),
                    offset_index: None,
                });
                self.app
                    .actions
                    .push(Action::Open(Page::Album(self.album.clone())));
                self.app.show_queue_panel = true;
            }
            "readme-demo" => self.app.actions.push(Action::ServerConnections),
            "connections" => self.app.actions.push(Action::ServerEdit(None)),
            "connection-add" => {
                self.app
                    .actions
                    .push(Action::SetTheme(sonicfast::settings::ThemeChoice::Light));
                self.ctx
                    .as_ref()
                    .unwrap()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(680.0, 850.0)));
            }
            "connection-add-light" => {
                self.app.actions.push(Action::CloseDialog);
                self.app.server_form.profile = self.app.server_form.profiles[0].clone();
                self.app.actions.push(Action::SignOut);
                self.app.server_form.secret = "wrong-password-fixture-only".into();
            }
            "auth-error" => {}
            _ => {}
        }
        self.stage += 1;
        self.menu_pressed = false;
        self.menu_released = false;
    }
}
impl eframe::App for Walkthrough {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if matches!(self.stage, 13 | 14 | 30) && !self.menu_released {
            // First save through the outlined plus, then open the saved heart's
            // existing playlist picker at this fixed capture size.
            let pos = if self.stage == 30 {
                egui::pos2(83.0, 222.0)
            } else {
                egui::pos2(180.0, 798.0)
            };
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: !self.menu_pressed,
                modifiers: egui::Modifiers::NONE,
            });
            if self.menu_pressed {
                self.menu_released = true;
            }
            self.menu_pressed = true;
        }
        if self.stage == 15 {
            egui::Popup::close_all(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.ctx = Some(ctx.clone());
        assert!(
            self.started.elapsed() < Duration::from_secs(120),
            "UI walkthrough timed out"
        );
        self.app.background_frame(&ctx);
        self.app.frame_ui(ui);
        if let Some(name) = self.pending {
            let image = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = image {
                let pixels: Vec<_> = image
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_srgba_unmultiplied())
                    .collect();
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels)
                    .unwrap()
                    .save(self.output.join(format!("{name}.png")))
                    .unwrap();
                self.pending = None;
                self.next(name);
            }
        } else {
            let shot = match self.stage {
                0 if matches!(self.app.auth, AuthStatus::SignedOut) => Some("login"),
                1 if self.app.is_connected() && !self.app.library.albums.items.is_empty() => {
                    Some("library")
                }
                2 if self
                    .app
                    .album_pages
                    .get(&self.album)
                    .is_some_and(|p| p.tracks.loaded_once) =>
                {
                    Some("album")
                }
                3 if self.app.local.playback == Playback::Playing => Some("playing-bars"),
                4 if self.app.local.playback == Playback::Playing => Some("playing-bars-later"),
                5 if self.app.local.playback == Playback::Paused
                    && self.app.manual_queue.len() == 2 =>
                {
                    Some("queue")
                }
                6 if self.app.search.results.get().is_some() => Some("search"),
                7 if matches!(self.app.dialog, Some(Dialog::CreatePlaylist { .. })) => {
                    Some("playlist-create")
                }
                8 if matches!(self.app.page(), Page::Playlist(id) if self.app.playlist_pages.get(id).is_some_and(|p| p.items.items.len() == 2 && !p.items.loading))
                    && !self.app.playlist_busy =>
                {
                    Some("playlist")
                }
                9 if self
                    .app
                    .search
                    .results
                    .get()
                    .and_then(|r| r.playlists.as_ref())
                    .is_some_and(|p| {
                        p.items.iter().any(|p| p.name == "Native fixture playlist")
                    }) =>
                {
                    Some("playlist-search")
                }
                10 if matches!(self.app.dialog, Some(Dialog::EditPlaylist { .. })) => {
                    Some("playlist-edit")
                }
                11 if matches!(self.app.dialog, Some(Dialog::ConfirmDeletePlaylist { .. })) => {
                    Some("playlist-delete")
                }
                12 if !self.app.playlist_busy
                    && self.app.library.liked.loaded_once
                    && self.app.is_saved(&self.track) == Some(false) =>
                {
                    Some("player-unsaved")
                }
                13 if self.menu_released
                    && self.app.is_saved(&self.track) == Some(true)
                    && !egui::Popup::is_any_open(&ctx) =>
                {
                    Some("player-saved")
                }
                14 if self.menu_released && egui::Popup::is_any_open(&ctx) => {
                    Some("player-playlists")
                }
                15 if matches!(
                    &self.app.lyrics,
                    sonicfast::model::Loadable::Loaded(Some(_))
                ) =>
                {
                    Some("lyrics")
                }
                16 => Some("settings"),
                17 if self.app.download_jobs.get(&self.track).is_some_and(|j| {
                    j.status == sonicfast::subsonic::downloads::Status::Active
                }) =>
                {
                    Some("download-active")
                }
                18 => Some("downloads-queue"),
                19 if self.app.download_jobs.get(&self.track).is_some_and(|j| {
                    j.status == sonicfast::subsonic::downloads::Status::Complete
                }) =>
                {
                    Some("download-complete")
                }
                20 => Some("download-queue-panel"),
                21 if matches!(self.app.page(), Page::Artist(id) if self.app.artist_pages.get(id).is_some_and(|p| p.artist.get().is_some())) => {
                    Some("artist")
                }
                22 => Some("settings-server"),
                23 => Some("settings-scrobbling"),
                24 => Some("settings-downloads"),
                25 => Some("settings-playback"),
                26 => Some("settings-appearance"),
                27 => Some("settings-downloads-light"),
                28 => Some("settings-downloads-narrow"),
                29 => Some("library-light"),
                30 if egui::Popup::is_any_open(&ctx) => Some("library-sort-menu"),
                31 if self.app.palette.dark
                    && self.app.local.playback == Playback::Playing
                    && self
                        .app
                        .local
                        .track
                        .as_ref()
                        .is_some_and(|t| t.uri == self.track)
                    && self.app.manual_queue.is_empty() =>
                {
                    Some("readme-demo")
                }
                32 if matches!(self.app.dialog, Some(Dialog::ServerConnections)) => {
                    Some("connections")
                }
                33 if self.app.server_form.editing => Some("connection-add"),
                34 if !self.app.palette.dark => Some("connection-add-light"),
                35 if matches!(self.app.auth, AuthStatus::SignedOut) => {
                    self.app.server_form.secret = "wrong-password-fixture-only".into();
                    self.app.actions.push(Action::ServerConnect);
                    None
                }
                35 if matches!(self.app.auth, AuthStatus::Failed(_)) => Some("auth-error"),
                36 => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    None
                }
                _ => None,
            };
            if let Some(name) = shot {
                let at = self.ready_at.get_or_insert_with(Instant::now);
                if at.elapsed() > Duration::from_millis(650) {
                    self.pending = Some(name);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(20));
    }
}
fn main() -> eframe::Result<()> {
    let output = std::env::var_os("SONICFAST_REVIEW_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/downloads-review/native")
        });
    std::fs::create_dir_all(&output).unwrap();
    let base = output.join(format!("profile-{}", std::process::id()));
    let dirs = AppDirs {
        config: base.join("config"),
        state: base.join("state"),
        cache: base.join("cache"),
    };
    dirs.ensure().unwrap();
    let settings = Settings {
        audio_device: std::env::var("LIBRESONIC_TEST_DEVICE").ok(),
        ..Default::default()
    };
    let waker = Waker::default();
    let mut app = App::new_subsonic(
        &waker,
        dirs,
        settings,
        AppOptions {
            restore_sign_in: false,
            media_controls: false,
            tray: false,
            autostart: false,
        },
    );
    app.server_form.profile.name = "Navidrome fixture".into();
    app.server_form.profile.url =
        std::env::var("LIBRESONIC_TEST_URL").expect("Set the disposable fixture URL");
    app.server_form.profile.username = "admin".into();
    app.server_form.profile.allow_http = true;
    app.server_form.secret = "admin".into();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 850.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sonicfast UI walkthrough",
        options,
        Box::new(move |cc| {
            app.attach(&cc.egui_ctx);
            waker.attach(&cc.egui_ctx);
            Ok(Box::new(Walkthrough {
                app,
                output,
                stage: 0,
                pending: None,
                ready_at: None,
                started: Instant::now(),
                album: String::new(),
                playlist: String::new(),
                track: String::new(),
                title: String::new(),
                ctx: None,
                menu_pressed: false,
                menu_released: false,
            }))
        }),
    )
}
