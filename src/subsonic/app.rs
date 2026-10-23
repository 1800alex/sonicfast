use super::*;
use crate::subsonic::{Event as S, QueueAction, Request};
impl App {
    pub(crate) fn flush_audio_settings(&mut self, ctx: &egui::Context, now: std::time::Instant) {
        let delay = std::time::Duration::from_millis(500);
        if let Some((settings, at)) = &self.casting_draft
            && settings
                .validate_with_snapcast(&self.settings.snapcast)
                .is_ok()
        {
            if now.saturating_duration_since(*at) >= delay {
                self.actions.push(Action::CastingSettings(settings.clone()));
            } else {
                ctx.request_repaint_after(delay.saturating_sub(now.saturating_duration_since(*at)));
            }
        }
        if let Some((settings, at)) = &self.snapcast_draft
            && settings.validate().is_ok()
        {
            if now.saturating_duration_since(*at) >= delay {
                self.actions
                    .push(Action::SnapcastSettings(Box::new(settings.clone())));
            } else {
                ctx.request_repaint_after(delay.saturating_sub(now.saturating_duration_since(*at)));
            }
        }
    }

    pub(crate) fn begin_server_connection(&mut self) {
        self.handle_auth(AuthStatus::SignedOut);
        self.server_form.active = None;
        self.server_form.sync_requested = false;
        self.server_form.notice = None;
        self.dialog = None;
        self.track_cache.clear();
        self.history = vec![Page::Home];
        self.history_index = 0;
        self.clear_picked_rows();
        self.auth = AuthStatus::Connecting;
    }
    pub fn server_syncing(&self) -> bool {
        self.server_form.sync_requested
            || self.library.albums.loading
            || self.library.artists.loading
            || self.library.playlists_loading_since.is_some()
    }
    fn refresh_server_library(&mut self) {
        self.server_form.sync_requested = false;
        if !self.is_connected() {
            return;
        }
        self.library.albums.reset();
        self.library.artists.reset();
        self.album_pages.clear();
        self.artist_pages.clear();
        // Preserve optimistic playlist edits and their acknowledgements.
        self.playlist_pages
            .retain(|_, page| page.pending_writes > 0 || page.optimistic_snapshot.is_some());
        self.home = HomeData {
            generation: self.home.generation,
            top_songs_generation: self.home.top_songs_generation,
            made_for_you_generation: self.home.made_for_you_generation,
            new_releases_generation: self.home.new_releases_generation,
            new_release_tracks_generation: self.home.new_release_tracks_generation,
            ..Default::default()
        };
        self.saved.retain(|uri, _| self.saved_pending.contains(uri));
        self.server_ratings
            .retain(|uri, _| self.server_rating_pending.contains(uri));
        self.download_checks.clear();
        self.table_rows.clear();
        self.load_more(Page::Albums);
        self.load_more(Page::Artists);
        self.load_playlists();
        self.refresh_liked_songs();
        let current = self.page().clone();
        let pending_playlists: Vec<_> = self.playlist_pages.keys().cloned().collect();
        for id in pending_playlists {
            let page = Page::Playlist(id);
            if page != current {
                self.reload(page);
            }
        }
        self.reload(current);
        self.clear_picked_rows();
        let query = self.search.committed.clone();
        self.search.results = Loadable::NotLoaded;
        self.search.playlists = None;
        self.search.catalogue_pending = false;
        self.search.playlists_pending = false;
        if !query.is_empty() {
            self.run_search(query);
        }
        self.toast("Refreshing server library…");
    }
    pub fn song_rating(&self, track: &Track) -> u8 {
        self.server_ratings
            .get(&track.uri)
            .copied()
            .or(track.user_rating)
            .unwrap_or(0)
    }
    pub(crate) fn download_probe_due(&self, uri: &str) -> bool {
        !self.download_probes.contains(uri)
            && self
                .download_checks
                .get(uri)
                .is_none_or(|checked| checked.elapsed() >= Duration::from_secs(5))
    }
    pub fn song_rating_pending(&self, uri: &str) -> bool {
        self.server_rating_pending.contains(uri)
    }

    pub(super) fn subsonic_event(&mut self, event: S) {
        match event {
            S::CastingStatus(status) => self.casting_status = status,
            S::Receivers(result) => {
                self.casting_scanning = false;
                match result {
                    Ok(receivers) => self.casting_receivers = receivers,
                    Err(error) => self.toast_error(&error),
                }
            }
            S::Snapcast(status) => self.snapcast_status = status,
            S::LibraryRefresh => self.refresh_server_library(),
            S::ProfileRemoving { id, busy } => {
                if busy {
                    self.server_form.removing.insert(id);
                } else {
                    self.server_form.removing.remove(&id);
                }
            }
            S::Downloads { epoch, update } => {
                use crate::subsonic::downloads::Update;
                if let Update::Reset(jobs, paused) = update {
                    if epoch < self.download_epoch {
                        return;
                    }
                    self.download_epoch = epoch;
                    self.download_jobs.clear();
                    self.download_order.clear();
                    self.download_probes.clear();
                    self.download_checks.clear();
                    self.downloads_paused = paused;
                    for job in jobs {
                        let uri = crate::subsonic::adapter::uri(&job.track.id);
                        self.download_order.push(uri.clone());
                        self.download_jobs.insert(uri, job);
                    }
                    return;
                }
                if epoch != self.download_epoch {
                    return;
                }
                match update {
                    Update::Job(job) => {
                        let uri = crate::subsonic::adapter::uri(&job.track.id);
                        self.download_probes.remove(&uri);
                        self.download_checks.insert(uri.clone(), Instant::now());
                        if !self.download_jobs.contains_key(&uri)
                            && self.download_jobs.len() >= 20_000
                            && let Some(old) = self
                                .download_order
                                .iter()
                                .find(|uri| {
                                    self.download_jobs.get(*uri).is_some_and(|job| {
                                        matches!(
                                            job.status,
                                            crate::subsonic::downloads::Status::Remote
                                                | crate::subsonic::downloads::Status::Complete
                                                | crate::subsonic::downloads::Status::Missing
                                        )
                                    })
                                })
                                .cloned()
                        {
                            self.download_jobs.remove(&old);
                            self.download_checks.remove(&old);
                            self.download_order.retain(|uri| uri != &old);
                        }
                        if !self.download_jobs.contains_key(&uri) {
                            self.download_order.push(uri.clone());
                        }
                        self.download_jobs.insert(uri, *job);
                    }
                    Update::Notice(error) => self.toast_error(error),
                    Update::Paused(paused) => self.downloads_paused = paused,
                    Update::Reset(..) => {}
                }
            }

            S::SongRated {
                uri,
                rating,
                result,
            } => {
                self.server_rating_pending.remove(&uri);
                match result {
                    Ok(()) => {
                        self.server_ratings.insert(uri, rating);
                        self.toast("Song rating saved");
                    }
                    Err(error) => self.toast_error(format!("Couldn't save song rating: {error}")),
                }
            }
            S::PlaylistChanged(id) => {
                self.load_playlists();
                if self.playlist_pages.contains_key(&id) {
                    self.load_playlist_items_at(&id, 0);
                    let generation = self.playlist_pages[&id].generation;
                    self.backend.api(ApiRequest::Playlist { id, generation });
                }
            }
            S::Profiles(profiles) => {
                let selected = profiles
                    .selected
                    .as_ref()
                    .and_then(|id| profiles.profiles.iter().find(|p| &p.id == id))
                    .cloned();
                if self.is_connected() {
                    let changed = self.server_form.active.as_ref() != profiles.selected.as_ref();
                    self.server_form.active = profiles.selected;
                    if let Some(profile) = selected
                        && (changed || !self.server_form.editing)
                    {
                        self.server_form.profile = profile;
                        self.server_form.editing = false;
                    }
                    self.rebuild_recents();
                } else if !self.server_form.editing
                    && self.server_form.profile.url.is_empty()
                    && self.server_form.secret.is_empty()
                    && let Some(profile) = selected
                {
                    self.server_form.profile = profile;
                }
                self.server_form.profiles = profiles.profiles;
            }
            S::Notice(notice) => {
                self.server_form.notice = Some(notice.clone());
                self.toast_error(notice);
            }
            S::Queue {
                queue,
                manual,
                source,
                current_index,
            } => {
                for item in queue.currently_playing.iter().chain(&queue.queue) {
                    if let PlayableItem::Track(track) = item
                        && let Some(id) = &track.id
                    {
                        self.track_cache.insert(id.clone(), track.clone());
                    }
                }
                self.queue = Loadable::Loaded(*queue);
                self.manual_queue = manual;
                self.server_source = source;
                self.server_current_index = current_index;
            }
        }
    }
    pub(super) fn subsonic_action(&mut self, action: Action) -> Option<Action> {
        if !self.subsonic {
            return Some(action);
        }
        let edit = match &action {
            Action::AddToPlaylist { playlist_id, .. }
            | Action::InsertInPlaylist { playlist_id, .. }
            | Action::ConfirmAddToPlaylist { playlist_id, .. }
            | Action::RemoveFromPlaylist { playlist_id, .. }
            | Action::MoveInPlaylist { playlist_id, .. } => Some(playlist_id),
            Action::UpdatePlaylist { id, .. } | Action::DeletePlaylist(id) => Some(id),
            _ => None,
        };
        if (edit.is_some() || matches!(&action, Action::CreatePlaylist { .. }))
            && !self.is_connected()
        {
            self.toast_error("Connect to a server before editing playlists");
            return None;
        }
        if (edit.is_some() || matches!(&action, Action::CreatePlaylist { .. }))
            && self.playlist_busy
        {
            self.toast("Wait for the current playlist edit to finish");
            return None;
        }
        if let Some(id) = edit
            && self
                .known_playlist(id)
                .is_none_or(|p| !self.can_edit_playlist(p))
        {
            self.toast_error("This playlist is read-only for the current account");
            return None;
        }
        if let Action::DeletePlaylist(id) = &action
            && self
                .known_playlist(id)
                .is_none_or(|p| !self.user_id().is_some_and(|user| p.owned_by(user)))
        {
            self.toast_error("Only a verified playlist owner can delete it");
            return None;
        }
        let request = match action {
            Action::DiscoverOutputs => {
                if self.casting_scanning {
                    return None;
                }
                self.casting_scanning = true;
                Request::DiscoverOutputs
            }
            Action::EditCastingSettings {
                settings,
                immediate,
            } => {
                self.casting_draft = Some((settings.clone(), std::time::Instant::now()));
                if immediate
                    && settings
                        .validate_with_snapcast(&self.settings.snapcast)
                        .is_ok()
                {
                    return self.subsonic_action(Action::CastingSettings(settings));
                }
                return None;
            }
            Action::EditSnapcastSettings {
                settings,
                immediate,
            } => {
                self.snapcast_draft = Some((*settings.clone(), std::time::Instant::now()));
                if immediate && settings.validate().is_ok() {
                    return self.subsonic_action(Action::SnapcastSettings(settings));
                }
                return None;
            }
            Action::CastingSettings(settings) => {
                if let Err(error) = settings.validate_with_snapcast(&self.settings.snapcast) {
                    self.toast_error(&error);
                    return None;
                }
                self.casting_draft = None;
                if settings.output == Some(crate::casting::Kind::Local)
                    && self.settings.snapcast.receive
                {
                    self.settings.snapcast.receive = false;
                    self.toast("Snapcast Receive turned off for Local playback");
                }
                self.settings.snapcast.send =
                    settings.output == Some(crate::casting::Kind::Snapcast);
                if let Some((draft, _)) = &mut self.snapcast_draft {
                    draft.send = self.settings.snapcast.send;
                    draft.receive = self.settings.snapcast.receive;
                }
                self.settings.casting = settings.clone();
                self.mark_settings_dirty();
                Request::Route {
                    output: settings,
                    snapcast: self.settings.snapcast.clone(),
                }
            }
            Action::SnapcastSettings(mut settings) => {
                if let Err(error) = settings.validate() {
                    self.toast_error(&error);
                    return None;
                }
                if settings.receive && settings.client_id.is_empty() {
                    settings.client_id = format!("sonicfast-{:016x}", rand::random::<u64>());
                }
                self.snapcast_draft = None;
                let old_output = self.settings.casting.kind(self.settings.snapcast.send);
                self.settings.snapcast = *settings.clone();
                if settings.send {
                    self.settings.casting.output = Some(crate::casting::Kind::Snapcast);
                } else if self.settings.casting.output == Some(crate::casting::Kind::Snapcast) {
                    self.settings.casting.output = Some(crate::casting::Kind::Local);
                }
                if self.settings.casting.kind(settings.send) != old_output {
                    self.casting_draft = None;
                }
                if let Some(warning) = self.settings.casting.playback_warning(&settings) {
                    self.toast(warning);
                }
                self.mark_settings_dirty();
                Request::Snapcast(*settings)
            }
            Action::LibrarySettings {
                scrobble,
                downloads,
            } => {
                self.settings.scrobbling = scrobble.clone();
                self.settings.downloads = downloads.clone();
                self.mark_settings_dirty();
                Request::Configure {
                    scrobble,
                    downloads,
                }
            }
            Action::DownloadTracks(uris) => {
                if !self.is_connected() || !self.settings.downloads.enabled {
                    self.toast_error("Enable downloads in Settings and connect to a server first");
                    return None;
                }
                Request::Download(uris)
            }
            Action::ProbeDownloads(track) => {
                if !self.settings.downloads.enabled
                    || !self.is_connected()
                    || !self.download_probe_due(&track.uri)
                    || !self.download_probes.insert(track.uri.clone())
                {
                    return None;
                }
                Request::ProbeDownloads(vec![*track])
            }
            Action::DownloadControl { uri, action } => Request::DownloadControl { uri, action },

            Action::SetSongRating { uri, rating } => {
                if !self.is_connected() || rating > 5 {
                    self.toast_error("Connect to a server and choose a rating from 1 to 5");
                    return None;
                }
                if !self.server_rating_pending.insert(uri.clone()) {
                    return None;
                }
                Request::RateSong { uri, rating }
            }
            Action::ToggleSaved(_) | Action::SetSavedMany { .. } if !self.is_connected() => {
                self.toast_error("Connect to a server before changing song hearts");
                return None;
            }
            Action::ToggleSaved(ref uri) if crate::util::uri_kind(uri) != Some("track") => {
                self.toast("Hearts are supported for songs");
                return None;
            }
            Action::SetSavedMany { ref uris, .. }
                if uris
                    .iter()
                    .any(|uri| crate::util::uri_kind(uri) != Some("track")) =>
            {
                self.toast("Hearts are supported for songs");
                return None;
            }
            Action::PasteSongs { .. } => {
                self.toast(
                    "Song-link imports are not supported; use Add to playlist or drag songs",
                );
                return None;
            }
            Action::CreatePlaylist {
                name,
                public,
                add_uris,
            } => {
                self.playlist_busy = true;
                self.dialog = Some(Dialog::CreatePlaylist {
                    name: name.clone(),
                    public,
                    add_uris: add_uris.clone(),
                });
                Request::CreatePlaylist {
                    name,
                    public,
                    uris: add_uris,
                }
            }
            Action::UpdatePlaylist {
                id,
                name,
                description,
                public,
            } => {
                self.dialog = None;
                self.playlist_busy = true;
                self.backend.api(ApiRequest::UpdatePlaylist {
                    id,
                    name: Some(name),
                    description: Some(description),
                    public,
                });
                return None;
            }
            Action::DeletePlaylist(id) => {
                self.dialog = None;
                self.playlist_busy = true;
                self.backend
                    .api(ApiRequest::FollowPlaylist { id, follow: false });
                return None;
            }
            Action::ChoosePlaylistCover(_) | Action::UploadPlaylistCover(_) => {
                self.toast("This server does not provide playlist cover uploads");
                return None;
            }
            Action::CloseDialog if matches!(self.dialog, Some(Dialog::ServerConnections)) => {
                self.server_form.secret.clear();
                self.server_form.editing = false;
                return Some(Action::CloseDialog);
            }
            Action::ServerConnections => {
                self.server_form.editing = false;
                self.server_form.secret.clear();
                self.dialog = self.is_connected().then_some(Dialog::ServerConnections);
                return None;
            }
            Action::ServerEdit(id) => {
                self.server_form.profile = id
                    .and_then(|id| {
                        self.server_form
                            .profiles
                            .iter()
                            .find(|p| p.id == id)
                            .cloned()
                    })
                    .unwrap_or_default();
                self.server_form.secret.clear();
                self.server_form.notice = None;
                self.server_form.editing = true;
                self.dialog = self.is_connected().then_some(Dialog::ServerConnections);
                return None;
            }
            Action::ServerUse(id) => {
                if self.server_form.removing.contains(&id) {
                    return None;
                }
                if let Some(profile) = self
                    .server_form
                    .profiles
                    .iter()
                    .find(|p| p.id == id)
                    .cloned()
                {
                    self.server_form.profile = profile;
                    self.server_form.secret.clear();
                    self.server_form.editing = true;
                    self.server_form.notice = None;
                    if self.server_form.profile.remember {
                        crate::subsonic::ui::connect(self);
                    } else {
                        self.dialog = self.is_connected().then_some(Dialog::ServerConnections);
                    }
                }
                return None;
            }
            Action::ServerDisconnect => {
                self.backend.send(Command::Subsonic(Request::Disconnect));
                self.begin_server_connection();
                self.handle_auth(AuthStatus::SignedOut);
                self.server_form.secret.clear();
                self.server_form.editing = false;
                return None;
            }
            Action::ServerRemove(id) => {
                if self.server_form.removing.contains(&id) {
                    return None;
                }
                if self.server_form.active.as_ref() == Some(&id) {
                    self.begin_server_connection();
                    self.handle_auth(AuthStatus::SignedOut);
                }
                if self.server_form.profile.id == id {
                    self.server_form.profile = Default::default();
                }
                self.backend
                    .send(Command::Subsonic(Request::RemoveProfile(id)));
                self.server_form.secret.clear();
                self.server_form.editing = false;
                self.dialog = self.is_connected().then_some(Dialog::ServerConnections);
                return None;
            }
            Action::ServerSync => {
                if self.is_connected() && !self.server_syncing() {
                    self.server_form.sync_requested = true;
                    self.backend.send(Command::Subsonic(Request::Sync));
                }
                return None;
            }
            Action::ServerConnect => {
                crate::subsonic::ui::connect(self);
                return None;
            }
            Action::SignOut | Action::CancelSignIn => {
                self.backend.send(Command::SignOut);
                self.handle_auth(AuthStatus::SignedOut);
                self.server_form.secret.clear();
                return None;
            }
            Action::PlayContext {
                uri,
                offset_uri,
                offset_index,
            } => Request::Load(LoadSpec {
                context_uri: Some(uri),
                offset_uri,
                offset_index,
                play: true,
                ..Default::default()
            }),
            Action::PlayUris { uris, index } => Request::Load(LoadSpec {
                uris,
                offset_index: Some(index),
                play: true,
                ..Default::default()
            }),
            Action::PlayFromRow {
                context,
                uri,
                index,
            } => match context {
                RowContext::Queue => Request::Queue(QueueAction::Skip(index as usize)),
                RowContext::Uris(uris) | RowContext::View { uris, .. } => {
                    let Some(index) = row_offset(&uris, &uri, index) else {
                        self.toast_error(
                            "This song is no longer in the selection; refresh and try again",
                        );
                        return None;
                    };
                    Request::Load(LoadSpec {
                        uris: uris.to_vec(),
                        offset_index: Some(index),
                        play: true,
                        ..Default::default()
                    })
                }
                RowContext::Context { uri: context, .. } => Request::Load(LoadSpec {
                    context_uri: Some(context),
                    offset_uri: Some(uri),
                    offset_index: Some(index),
                    play: true,
                    ..Default::default()
                }),
            },
            Action::ShufflePlay(uri) => Request::Load(LoadSpec {
                context_uri: Some(uri),
                shuffle: Some(true),
                play: true,
                ..Default::default()
            }),
            Action::TogglePlay => Request::Transport(PlayerCommand::Toggle),
            Action::Next => Request::Transport(PlayerCommand::Next),
            Action::Previous => Request::Transport(PlayerCommand::Previous),
            Action::Seek(ms) => Request::Transport(PlayerCommand::Seek(ms)),
            Action::SeekBy(delta) => Request::Transport(PlayerCommand::Seek(
                (i64::from(self.local.position_now()) + delta).clamp(0, i64::from(u32::MAX)) as u32,
            )),
            Action::ToggleShuffle => {
                Request::Transport(PlayerCommand::Shuffle(!self.local.shuffle))
            }
            Action::SetShuffle(value) => Request::Transport(PlayerCommand::Shuffle(value)),
            Action::CycleRepeat => {
                Request::Transport(PlayerCommand::Repeat(self.local.repeat.next()))
            }
            Action::SetRepeat(mode) => Request::Transport(PlayerCommand::Repeat(mode)),
            Action::AddToQueue { uri, .. } => Request::Queue(QueueAction::Add(vec![uri])),
            Action::QueueMany { songs } => Request::Queue(QueueAction::Add(
                songs.into_iter().map(|(uri, _)| uri).collect(),
            )),
            Action::PlayNext { items } => Request::Queue(QueueAction::Insert(
                0,
                items.iter().map(|i| i.uri().to_owned()).collect(),
            )),
            Action::InsertInQueue { items, position } => Request::Queue(QueueAction::Insert(
                position,
                items.iter().map(|i| i.uri().to_owned()).collect(),
            )),
            Action::MoveInQueue { from, to } => Request::Queue(QueueAction::Move(from, to)),
            Action::RemoveFromQueue { rows } => Request::Queue(QueueAction::Remove(rows)),
            Action::ClearQueue => Request::Queue(QueueAction::Clear),
            Action::ToggleDevicesPopup => {
                self.show_devices = !self.show_devices;
                self.backend.send(Command::ListOutputDevices);
                if self.show_devices && !self.casting_scanning {
                    self.casting_scanning = true;
                    self.backend
                        .send(Command::Subsonic(Request::DiscoverOutputs));
                }
                return None;
            }
            Action::Open(
                Page::TopSongs
                | Page::NewReleases
                | Page::Podcasts
                | Page::Episodes
                | Page::Radio(_)
                | Page::Show(_),
            )
            | Action::ToggleFriendsPanel
            | Action::OpenSongRadio { .. }
            | Action::CheckForUpdates
            | Action::Transfer(_) => {
                self.toast("This feature is not available in the OpenSubsonic alpha");
                return None;
            }
            other => return Some(other),
        };
        self.backend.send(Command::Subsonic(request));
        None
    }
}

// A row's screen index need not be its index in the list it plays: search and
// Recent rows can each carry a one-song list. Preserve a matching occurrence
// for duplicate playlist songs; otherwise locate the song the user chose.
fn row_offset(uris: &[String], uri: &str, index: u32) -> Option<u32> {
    if uris
        .get(index as usize)
        .is_some_and(|held| uri.is_empty() || held == uri)
    {
        Some(index)
    } else {
        uris.iter().position(|held| held == uri).map(|at| at as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::row_offset;
    #[test]
    fn search_row_index_is_mapped_into_its_own_song_list() {
        let uris = vec!["chosen".into()];
        assert_eq!(row_offset(&uris, "chosen", 17), Some(0));
        assert_eq!(row_offset(&uris, "missing", 17), None);
        assert_eq!(row_offset(&uris, "", 0), Some(0));
        assert_eq!(row_offset(&uris, "", 17), None);
        let uris = vec!["same".into(), "same".into(), "other".into()];
        assert_eq!(row_offset(&uris, "same", 1), Some(1));
        assert_eq!(row_offset(&uris, "other", 0), Some(2));
    }
}
