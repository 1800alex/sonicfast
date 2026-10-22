//! Server profile form using the existing native palette and action flow.
use super::Request;
use crate::{
    app::App,
    backend::{AuthStatus, Command},
    model::Action,
    theme,
};

pub fn login(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(palette.window))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add_space(60.0);
                theme::text(ui, "Sonicfast", theme::bold(34.0), palette.text);
                theme::text(
                    ui,
                    "Connect to your OpenSubsonic music server",
                    theme::regular(15.0),
                    palette.secondary,
                );
                ui.add_space(24.0);
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                        egui::pos2(ui.max_rect().center().x - 200.0, ui.cursor().min.y),
                        egui::vec2(400.0, ui.available_height()),
                    )),
                    |ui| {
                        let form = &mut app.server_form;
                        if !form.profiles.is_empty() {
                            let mut choice = form.profile.id.clone();
                            egui::ComboBox::from_id_salt("server-profile")
                                .selected_text(if form.profile.name.is_empty() {
                                    "Choose a saved server"
                                } else {
                                    &form.profile.name
                                })
                                .show_ui(ui, |ui| {
                                    for profile in &form.profiles {
                                        ui.selectable_value(
                                            &mut choice,
                                            profile.id.clone(),
                                            &profile.name,
                                        );
                                    }
                                    ui.selectable_value(
                                        &mut choice,
                                        String::new(),
                                        "Add a server…",
                                    );
                                });
                            if choice != form.profile.id {
                                form.profile = form
                                    .profiles
                                    .iter()
                                    .find(|p| p.id == choice)
                                    .cloned()
                                    .unwrap_or_default();
                                form.secret.clear();
                                form.notice = None;
                            }
                        }
                        for (label, field) in [
                            ("Profile name", &mut form.profile.name),
                            ("Server URL", &mut form.profile.url),
                            ("Username", &mut form.profile.username),
                        ] {
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(field).desired_width(f32::INFINITY));
                        }
                        ui.checkbox(&mut form.profile.api_key, "Use an API key");
                        ui.label(if form.profile.api_key {
                            "API key"
                        } else {
                            "Password"
                        });
                        let entered = ui.add(
                            egui::TextEdit::singleline(&mut form.secret)
                                .password(true)
                                .desired_width(f32::INFINITY),
                        );
                        ui.checkbox(
                            &mut form.profile.allow_http,
                            "Allow plaintext HTTP for this trusted server",
                        );
                        ui.checkbox(
                            &mut form.profile.remember,
                            "Remember sign-in in the system credential store",
                        );
                        if let Some(notice) = &form.notice {
                            ui.colored_label(palette.secondary, notice);
                        }
                        match &app.auth {
                            AuthStatus::Connecting | AuthStatus::Starting => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Connecting…");
                                });
                            }
                            AuthStatus::Failed(error) => {
                                ui.colored_label(palette.danger, error);
                            }
                            _ => {}
                        }
                        let busy =
                            matches!(app.auth, AuthStatus::Connecting | AuthStatus::Starting);
                        if ui
                            .add_enabled(
                                !busy && (!form.secret.is_empty() || form.profile.remember),
                                egui::Button::new("Connect"),
                            )
                            .clicked()
                            || (!busy
                                && entered.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && !form.secret.is_empty())
                        {
                            app.actions.push(Action::ServerConnect);
                        }
                        if busy && ui.button("Cancel").clicked() {
                            app.actions.push(Action::SignOut);
                        }
                    },
                );
            });
        });
}

pub fn connect(app: &mut App) {
    let profile = app.server_form.profile.clone();
    let secret = std::mem::take(&mut app.server_form.secret);
    app.server_form.notice = None;
    app.backend
        .send(Command::Subsonic(Request::Login(profile, secret)));
}

pub fn context_menu(ui: &mut egui::Ui, app: &mut App, uri: &str, name: &str) {
    use crate::ui::widgets::menu_item;
    if menu_item(ui, &app.palette, None, "Play") {
        app.actions.push(Action::PlayContext {
            uri: uri.into(),
            offset_uri: None,
            offset_index: None,
        });
    }
    if menu_item(ui, &app.palette, None, "Shuffle play") {
        app.actions.push(Action::ShufflePlay(uri.into()));
    }
    if app.settings.downloads.enabled
        && matches!(crate::util::uri_kind(uri), Some("album" | "playlist"))
        && menu_item(ui, &app.palette, None, "Download")
    {
        app.actions.push(Action::DownloadTracks(vec![uri.into()]));
    }
    if matches!(crate::util::uri_kind(uri), Some("album" | "playlist"))
        && menu_item(ui, &app.palette, None, "Add to queue")
    {
        app.actions.push(Action::AddToQueue {
            uri: uri.into(),
            label: name.into(),
        });
    }
    if crate::util::uri_kind(uri) == Some("playlist")
        && let Some(p) = crate::util::uri_id(uri)
            .and_then(|id| app.known_playlist(id))
            .filter(|p| app.can_edit_playlist(p))
            .cloned()
    {
        if menu_item(ui, &app.palette, None, "Edit details") {
            app.actions
                .push(Action::ShowDialog(crate::model::Dialog::EditPlaylist {
                    cover: Default::default(),
                    id: p.id.clone(),
                    name: p.name.clone(),
                    description: p.description.clone().unwrap_or_default(),
                    public: p.public,
                }));
        }
        if app.user_id().is_some_and(|user| p.owned_by(user))
            && menu_item(ui, &app.palette, None, "Delete playlist")
        {
            app.actions.push(Action::ShowDialog(
                crate::model::Dialog::ConfirmDeletePlaylist {
                    id: p.id,
                    name: p.name,
                    owned: true,
                },
            ));
        }
    }
}
pub fn item_menu(
    ui: &mut egui::Ui,
    app: &mut App,
    item: &crate::api::models::PlayableItem,
    context: Option<&crate::model::RowContext>,
    index: Option<usize>,
    position: Option<u32>,
) {
    use crate::ui::widgets::{menu_item, menu_item_enabled};
    ui.set_min_width(220.0);
    ui.set_max_width(300.0);
    let available =
        !matches!(item, crate::api::models::PlayableItem::Track(t) if t.is_playable == Some(false));
    if menu_item_enabled(ui, &app.palette, None, "Play", available) {
        app.actions.push(match (context, index) {
            (Some(context), Some(index)) => Action::PlayFromRow {
                context: context.clone(),
                uri: item.uri().into(),
                index: index as u32,
            },
            _ => Action::PlayUris {
                uris: vec![item.uri().into()],
                index: 0,
            },
        });
    }
    if menu_item_enabled(ui, &app.palette, None, "Add to queue", available) {
        app.actions.push(Action::AddToQueue {
            uri: item.uri().into(),
            label: item.name().into(),
        });
    }
    if menu_item_enabled(
        ui,
        &app.palette,
        None,
        "Play next",
        available && app.queue_locally_reorderable(),
    ) {
        app.actions.push(Action::PlayNext {
            items: vec![item.clone()],
        });
    }
    if let Some(index) = index
        && matches!(context, Some(crate::model::RowContext::Queue))
        && app.can_remove_from_queue(index)
        && menu_item(ui, &app.palette, None, "Remove from queue")
    {
        app.actions.push(Action::RemoveFromQueue {
            rows: vec![(index, item.uri().into())],
        });
    }
    if let crate::api::models::PlayableItem::Track(track) = item {
        let saved = app.is_saved(&track.uri).unwrap_or(false);
        if menu_item(
            ui,
            &app.palette,
            None,
            if saved {
                "Remove from Liked Songs"
            } else {
                "Save to Liked Songs"
            },
        ) {
            app.actions.push(Action::ToggleSaved(track.uri.clone()));
        }
        let current = app.song_rating(track);
        ui.menu_button(
            if current == 0 {
                "Rating: unrated".to_string()
            } else {
                format!("Rating: {}", "★".repeat(current as usize))
            },
            |ui| {
                ui.set_min_width(180.0);
                ui.set_max_width(260.0);
                let pending = app.song_rating_pending(&track.uri);
                for rating in 1..=5 {
                    if menu_item_enabled(
                        ui,
                        &app.palette,
                        None,
                        &"★".repeat(rating as usize),
                        !pending,
                    ) {
                        app.actions.push(Action::SetSongRating {
                            uri: track.uri.clone(),
                            rating,
                        });
                    }
                }
                crate::ui::widgets::menu_separator(ui, &app.palette);
                if menu_item_enabled(
                    ui,
                    &app.palette,
                    None,
                    "Clear rating",
                    !pending && current > 0,
                ) {
                    app.actions.push(Action::SetSongRating {
                        uri: track.uri.clone(),
                        rating: 0,
                    });
                }
            },
        );
    }
    if app.settings.downloads.enabled
        && item.is_track()
        && menu_item(ui, &app.palette, None, "Download")
    {
        app.actions
            .push(Action::DownloadTracks(vec![item.uri().into()]));
    }
    crate::ui::widgets::add_to_playlist_menu(ui, app, std::slice::from_ref(item));
    let editable = match context {
        Some(crate::model::RowContext::Context {
            editable_playlist: Some((id, _)),
            ..
        }) => Some((id, true)),
        Some(crate::model::RowContext::View {
            editable_playlist: Some((id, _)),
            ..
        }) => Some((id, false)),
        _ => None,
    };
    if let Some((id, can_move)) = editable {
        if can_move && let Some(index) = index {
            if index > 0 && menu_item(ui, &app.palette, None, "Move up") {
                app.actions.push(Action::MoveInPlaylist {
                    playlist_id: id.clone(),
                    from: index as u32,
                    to: index as u32 - 1,
                });
            }
            if menu_item(ui, &app.palette, None, "Move down") {
                app.actions.push(Action::MoveInPlaylist {
                    playlist_id: id.clone(),
                    from: index as u32,
                    to: index as u32 + 2,
                });
            }
        }
        if menu_item(ui, &app.palette, None, "Remove from this playlist") {
            app.actions.push(Action::RemoveFromPlaylist {
                playlist_id: id.clone(),
                uris: vec![item.uri().into()],
                positions: position.into_iter().collect(),
            });
        }
    }
    if let crate::api::models::PlayableItem::Track(track) = item {
        if let Some(album) = &track.album
            && !album.id.is_empty()
            && menu_item(ui, &app.palette, None, "Go to album")
        {
            app.actions
                .push(Action::Open(crate::model::Page::Album(album.id.clone())));
        }
        for artist in &track.artists {
            if let Some(id) = &artist.id
                && menu_item(ui, &app.palette, None, &format!("Go to {}", artist.name))
            {
                app.actions
                    .push(Action::Open(crate::model::Page::Artist(id.clone())));
            }
        }
    }
}

pub fn downloads_page(app: &mut App, ui: &mut egui::Ui) {
    use crate::{model::Page, subsonic::downloads::Status};
    ui.heading("Downloads");
    if !app.settings.downloads.enabled {
        ui.label("Enable downloads and choose a folder in Settings to start or resume the queue.");
        if ui.button("Open Settings").clicked() {
            app.actions.push(Action::Open(Page::Settings));
        }
        return;
    }
    ui.label(&app.settings.downloads.path);
    let jobs: Vec<_> = app
        .download_order
        .iter()
        .filter_map(|uri| app.download_jobs.get(uri).map(|job| (uri, job)))
        .filter(|(_, j)| j.status != Status::Remote)
        .collect();
    let active = jobs
        .iter()
        .filter(|(_, j)| j.status == Status::Active)
        .count();
    let queued = jobs
        .iter()
        .filter(|(_, j)| matches!(j.status, Status::Queued | Status::Retrying))
        .count();
    ui.horizontal(|ui| {
        ui.label(format!(
            "{active} active · {queued} queued · {} total",
            jobs.len()
        ));
        if ui
            .button(if app.downloads_paused {
                "Resume queue"
            } else {
                "Pause queue"
            })
            .clicked()
        {
            app.actions.push(Action::DownloadControl {
                uri: None,
                action: if app.downloads_paused {
                    "resume"
                } else {
                    "pause"
                }
                .into(),
            });
        }
    });
    if jobs.is_empty() {
        ui.label(
            "Use the down arrow beside a song, or Download in a song, album or playlist menu.",
        );
    }
    let mut probes = Vec::new();
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(5));
    egui::ScrollArea::vertical()
        .max_height(ui.available_height().max(300.0))
        .show_rows(ui, 78.0, jobs.len(), |ui, range| {
            for index in range {
                let (uri, job) = jobs[index];
                probes.push((uri, &job.track));
                ui.push_id(uri, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&job.track.title).strong());
                        ui.label(format!("{:?}", job.status));
                        if matches!(job.status, Status::Queued | Status::Retrying)
                            && ui.small_button("Move first").clicked()
                        {
                            app.actions.push(Action::DownloadControl {
                                uri: Some(uri.clone()),
                                action: "first".into(),
                            });
                        }
                        if matches!(
                            job.status,
                            Status::Queued | Status::Active | Status::Retrying
                        ) && ui.small_button("Cancel").clicked()
                        {
                            app.actions.push(Action::DownloadControl {
                                uri: Some(uri.clone()),
                                action: "cancel".into(),
                            });
                        }
                        if matches!(
                            job.status,
                            Status::Failed | Status::Cancelled | Status::Missing
                        ) && ui.small_button("Retry").clicked()
                        {
                            app.actions.push(Action::DownloadControl {
                                uri: Some(uri.clone()),
                                action: "retry".into(),
                            });
                        }
                    });
                    let artist = job
                        .track
                        .artists
                        .first()
                        .map_or("Unknown artist", |a| a.name.as_str());
                    ui.label(format!(
                        "{artist} · {}",
                        job.track
                            .album
                            .as_ref()
                            .map_or("Unknown album", |a| a.title.as_str())
                    ));
                    if job.status == Status::Active {
                        let progress = job
                            .total
                            .filter(|n| *n > 0)
                            .map(|n| job.bytes as f32 / n as f32)
                            .unwrap_or(0.0);
                        let text = job
                            .total
                            .map(|n| {
                                format!("{} / {}", download_bytes(job.bytes), download_bytes(n))
                            })
                            .unwrap_or_else(|| download_bytes(job.bytes));
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .text(text)
                                .animate(job.total.is_none()),
                        );
                    } else if let Some(error) = &job.error {
                        ui.colored_label(app.palette.danger, error);
                    } else if job.status == Status::Complete {
                        ui.colored_label(
                            app.palette.accent,
                            job.relative
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "Available locally".into()),
                        );
                    }
                });
            }
        });
    for (uri, track) in probes {
        if app.download_probe_due(uri) {
            app.actions
                .push(Action::ProbeDownloads(Box::new(super::adapter::track(
                    track,
                ))));
        }
    }
}
fn download_bytes(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KiB", bytes as f64 / 1024.0),
        1_048_576..1_073_741_824 => format!("{:.1} MiB", bytes as f64 / 1_048_576.0),
        _ => format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0),
    }
}
/// A cached status lookup and event emission only; no filesystem calls while drawing.
pub fn download_arrow(
    ui: &mut egui::Ui,
    app: &mut App,
    track: &crate::api::models::Track,
    rect: egui::Rect,
) {
    use crate::subsonic::downloads::Status;
    if !app.subsonic || !app.settings.downloads.enabled {
        return;
    }
    let job = app.download_jobs.get(&track.uri);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(5));
    if app.download_probe_due(&track.uri) {
        app.actions
            .push(Action::ProbeDownloads(Box::new(track.clone())));
    }
    let status = job.map(|j| &j.status);
    let active = status == Some(&Status::Active);
    let color = match status {
        Some(Status::Complete) => app.palette.accent,
        Some(Status::Failed | Status::Missing) => app.palette.danger,
        _ => app.palette.secondary,
    };
    let response = ui.interact(
        rect,
        ui.id().with(("download", &track.uri, rect.top().to_bits())),
        egui::Sense::click(),
    );
    let center = rect.center();
    let stroke = egui::Stroke::new(1.6, color);
    ui.painter().line_segment(
        [
            center + egui::vec2(0.0, -5.0),
            center + egui::vec2(0.0, 4.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(-3.5, 0.5),
            center + egui::vec2(0.0, 4.0),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [center + egui::vec2(3.5, 0.5), center + egui::vec2(0.0, 4.0)],
        stroke,
    );
    if active {
        let fraction = job.and_then(|j| j.total.map(|n| j.bytes as f32 / n.max(1) as f32));
        let time = ui.input(|i| i.time) as f32;
        let start = if fraction.is_some() {
            -std::f32::consts::FRAC_PI_2
        } else {
            time * 3.0
        };
        let length = fraction.unwrap_or(0.25).clamp(0.02, 1.0) * std::f32::consts::TAU;
        let points = (0..=40)
            .map(|i| {
                center
                    + egui::vec2(
                        (start + length * i as f32 / 40.0).cos(),
                        (start + length * i as f32 / 40.0).sin(),
                    ) * 9.0
            })
            .collect::<Vec<_>>();
        ui.painter().add(egui::Shape::line(
            points,
            egui::Stroke::new(1.5, app.palette.accent),
        ));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let tooltip = job
        .map(|j| {
            format!(
                "Download: {:?} · {}{}",
                j.status,
                download_bytes(j.bytes),
                j.error
                    .as_ref()
                    .map(|e| format!(" · {e}"))
                    .unwrap_or_default()
            )
        })
        .unwrap_or_else(|| "Download this song".into());
    if response.on_hover_text(tooltip).clicked() {
        if matches!(
            status,
            Some(Status::Active | Status::Queued | Status::Retrying | Status::Complete)
        ) {
            app.actions
                .push(Action::Open(crate::model::Page::Downloads));
        } else {
            app.actions
                .push(Action::DownloadTracks(vec![track.uri.clone()]));
        }
    }
}
