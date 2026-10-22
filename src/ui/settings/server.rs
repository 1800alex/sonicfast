//! Sonicfast settings, using the same cards and rows as Appearance.
use super::{RowText, filtered_row, section, section_matches, setting_slider, slider_value_style};
use crate::{
    app::App,
    model::{Action, Loadable, Page},
    theme,
    ui::widgets,
};
use egui::{Align, CornerRadius, Frame, Layout, Margin};

pub(super) fn show(app: &mut App, ui: &mut egui::Ui, needle: &str) -> bool {
    let palette = app.palette;
    let mut visible = false;
    let server = [RowText::new(
        "Connected server",
        format!(
            "{}\n{}\nRemembered sign-in uses the system store. Sign out to switch servers or remove remembered sign-in.",
            app.server_form.profile.name, app.server_form.profile.url,
        ),
    )];
    if section_matches(needle, "Server", &server) {
        visible = true;
        section(ui, &palette, "Server", |ui| {
            filtered_row(ui, &palette, needle, "Server", &server[0], |ui| {
                if theme::soft_button(ui, &palette, None, "Sign out / switch server", false)
                    .clicked()
                {
                    app.actions.push(Action::SignOut);
                }
            });
            if let Some(notice) = &app.server_form.notice {
                theme::subtle(ui, &palette, notice);
            }
        });
    }

    let mut scrobble = app.settings.scrobbling.clone();
    let mut downloads = app.settings.downloads.clone();
    let mut changed = false;
    let listening = [
        RowText::new(
            "Report listening",
            "Send listening history to this server. Paused time and seek jumps do not count.",
        ),
        RowText::new(
            "Listening percentage",
            "Report after this much of a song has actually played.",
        ),
        RowText::new(
            "Time threshold",
            "Report once either threshold is reached. Use 0 seconds for percentage only.",
        ),
    ];
    if section_matches(needle, "Scrobbling", &listening) {
        visible = true;
        section(ui, &palette, "Scrobbling", |ui| {
            filtered_row(ui, &palette, needle, "Scrobbling", &listening[0], |ui| {
                changed |=
                    widgets::switch(ui, &palette, &listening[0].title, &mut scrobble.enabled)
                        .changed();
            });
            filtered_row(ui, &palette, needle, "Scrobbling", &listening[1], |ui| {
                ui.add_enabled_ui(scrobble.enabled, |ui| {
                    changed |= setting_slider(
                        ui,
                        &palette,
                        &mut scrobble.percent,
                        1..=100,
                        |slider| slider,
                        |value| value.suffix("%"),
                    )
                    .changed();
                });
            });
            filtered_row(ui, &palette, needle, "Scrobbling", &listening[2], |ui| {
                ui.add_enabled_ui(scrobble.enabled, |ui| {
                    slider_value_style(ui.style_mut(), &palette);
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut scrobble.seconds)
                                .range(0..=86400)
                                .suffix(" s"),
                        )
                        .changed();
                });
            });
        });
    }

    let saving = [
        RowText::new(
            "Enable downloads",
            "Save original files and prefer local audio. Disabling pauses the queue and keeps your files.",
        ),
        RowText::new(
            "Download folder",
            "The folder containing your artists and albums. Use an absolute path.",
        ),
        RowText::new("Download queue", "See progress and manage queued tracks."),
    ];
    if section_matches(needle, "Downloads", &saving) {
        visible = true;
        section(ui, &palette, "Downloads", |ui| {
            filtered_row(ui, &palette, needle, "Downloads", &saving[0], |ui| {
                changed |= widgets::switch(ui, &palette, &saving[0].title, &mut downloads.enabled)
                    .changed();
            });
            filtered_row(ui, &palette, needle, "Downloads", &saving[1], |ui| {
                // A stable draft survives filtering and is applied explicitly:
                // changing roots pauses work in the old download folder.
                let key = egui::Id::new("download-folder-draft");
                let mut path = ui
                    .data(|d| d.get_temp::<String>(key))
                    .unwrap_or_else(|| downloads.path.clone());
                ui.with_layout(Layout::top_down(Align::Max), |ui| {
                    let width = ui.available_width().min(240.0);
                    Frame::new()
                        .fill(palette.surface)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            widgets::text_edit(
                                ui,
                                app.locale,
                                egui::TextEdit::singleline(&mut path)
                                    .id(egui::Id::new("download-folder-field"))
                                    .font(theme::regular(14.0))
                                    .frame(Frame::NONE)
                                    .desired_width((width - 20.0).max(0.0))
                                    .hint_text("/home/you/Music"),
                            );
                        });
                    ui.add_space(6.0);
                    ui.add_enabled_ui(
                        !path.trim().is_empty() && path.trim() != downloads.path,
                        |ui| {
                            if theme::soft_button(ui, &palette, None, "Apply folder", false)
                                .clicked()
                            {
                                downloads.path = path.trim().to_string();
                                changed = true;
                            }
                        },
                    );
                });
                ui.data_mut(|d| d.insert_temp(key, path));
            });
            filtered_row(ui, &palette, needle, "Downloads", &saving[2], |ui| {
                if theme::soft_button(ui, &palette, None, "View queue", false).clicked() {
                    app.actions.push(Action::Open(Page::Downloads));
                }
            });
        });
    }
    if changed {
        app.actions.push(Action::LibrarySettings {
            scrobble,
            downloads,
        });
    }

    let playback = [
        RowText::new("Audio output", "Choose the device used for local playback."),
        RowText::new(
            "Equalizer / mini-player",
            "Adjust the sound or open the classic mini-player.",
        ),
    ];
    if section_matches(needle, "Playback", &playback) {
        visible = true;
        section(ui, &palette, "Playback", |ui| {
            filtered_row(ui, &palette, needle, "Playback", &playback[0], |ui| {
                if matches!(app.output_devices, Loadable::NotLoaded) {
                    app.actions.push(Action::ListOutputDevices);
                }
                let mut wanted = app.settings.audio_device.clone();
                let name = wanted.as_ref().map_or("System default", |id| {
                    app.output_devices
                        .get()
                        .into_iter()
                        .flatten()
                        .find(|d| d.id.as_str() == id)
                        .map_or("Saved output", |d| d.name.as_str())
                });
                ui.with_layout(Layout::top_down(Align::Max), |ui| {
                    egui::ComboBox::from_id_salt("sonicfast-audio-output")
                        .selected_text(name)
                        .width(240.0_f32.min(ui.available_width()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut wanted, None, "System default");
                            for device in app.output_devices.get().into_iter().flatten() {
                                ui.selectable_value(
                                    &mut wanted,
                                    Some(device.id.as_str().to_owned()),
                                    &device.name,
                                );
                            }
                            if app.output_devices.is_loading() {
                                ui.spinner();
                            }
                        });
                    ui.add_space(6.0);
                    if theme::soft_button(ui, &palette, None, "Refresh devices", false).clicked() {
                        app.actions.push(Action::ListOutputDevices);
                    }
                });
                if wanted != app.settings.audio_device {
                    app.actions.push(Action::SetAudioDevice(wanted));
                }
            });
            filtered_row(ui, &palette, needle, "Playback", &playback[1], |ui| {
                if theme::soft_button(ui, &palette, None, "Open mini-player", false).clicked() {
                    app.actions.push(Action::ToggleWinampWindow);
                }
            });
        });
    }
    visible
}
