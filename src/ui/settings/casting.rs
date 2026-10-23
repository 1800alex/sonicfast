use super::{RowText, filtered_row, section, section_matches};
use crate::{
    app::App,
    casting::{Kind, Settings},
    model::Action,
    theme,
    ui::widgets,
};
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, needle: &str) -> bool {
    let key = egui::Id::new("casting-draft");
    let (saved, mut draft) = ui
        .data(|d| d.get_temp::<(Settings, Settings)>(key))
        .unwrap_or_else(|| (app.settings.casting.clone(), app.settings.casting.clone()));
    if saved != app.settings.casting {
        draft = app.settings.casting.clone();
    }
    if draft.port == 0 {
        draft.port = 5000;
    }
    let rows = [
        RowText::new(
            "Output",
            "Choose local playback, AirPlay Classic, Chromecast or the configured Snapcast server.",
        ),
        RowText::new(
            "Trusted network",
            "Allow LAN casting. AirPlay audio and Chromecast's temporary media URLs use your trusted network.",
        ),
        RowText::new(
            "Receiver host",
            "Hostname or IP address, without a URL. Discover nearby receivers from the player’s output button.",
        ),
        RowText::new(
            "Receiver port",
            "AirPlay normally uses 5000; Chromecast normally uses 8009.",
        ),
        RowText::new(
            "Receiver certificate",
            "For a self-hosted Cast receiver, enter its SHA-256 certificate fingerprint. Leave blank for genuine Google Cast devices.",
        ),
        RowText::new("Playback status", app.casting_status.clone()),
        RowText::new(
            "Apply output",
            "Changes stop the old output and preserve the song and position paused. Press Play to continue.",
        ),
    ];
    if !section_matches(needle, "Casting", &rows) {
        return false;
    }
    let palette = app.palette;
    section(ui, &palette, "Casting", |ui| {
        filtered_row(ui, &palette, needle, "Casting", &rows[0], |ui| {
            let before = draft.kind(app.settings.snapcast.send);
            let mut kind = before;
            egui::ComboBox::from_id_salt("casting-kind")
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for k in [Kind::Local, Kind::AirPlay, Kind::Chromecast, Kind::Snapcast] {
                        ui.selectable_value(&mut kind, k, k.label());
                    }
                });
            if before != kind {
                draft.output = Some(kind);
                draft.port = if kind == Kind::AirPlay { 5000 } else { 8009 };
            }
        });
        filtered_row(ui, &palette, needle, "Casting", &rows[1], |ui| {
            widgets::switch(ui, &palette, "Allow LAN casting", &mut draft.allow_lan);
        });
        filtered_row(ui, &palette, needle, "Casting", &rows[2], |ui| {
            egui::Frame::new()
                .fill(palette.surface)
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    widgets::text_edit(
                        ui,
                        app.locale,
                        egui::TextEdit::singleline(&mut draft.host)
                            .desired_width(190.0)
                            .hint_text("Receiver hostname")
                            .frame(egui::Frame::NONE),
                    );
                });
        });
        filtered_row(ui, &palette, needle, "Casting", &rows[3], |ui| {
            ui.add(egui::DragValue::new(&mut draft.port).range(1..=65535));
        });
        filtered_row(ui, &palette, needle, "Casting", &rows[4], |ui| {
            egui::Frame::new()
                .fill(palette.surface)
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    widgets::text_edit(
                        ui,
                        app.locale,
                        egui::TextEdit::singleline(&mut draft.certificate_sha256)
                            .desired_width(190.0)
                            .hint_text("SHA-256 fingerprint")
                            .frame(egui::Frame::NONE),
                    );
                });
        });
        filtered_row(ui, &palette, needle, "Casting", &rows[5], |_| {});
        filtered_row(ui, &palette, needle, "Casting", &rows[6], |ui| {
            if theme::soft_button(ui, &palette, None, "Apply output", false).clicked() {
                draft.output = Some(draft.kind(app.settings.snapcast.send));
                app.actions.push(Action::CastingSettings(draft.clone()));
            }
        });
        if draft.kind(app.settings.snapcast.send) == Kind::Chromecast {
            theme::subtle(
                ui,
                &palette,
                "Chromecast decodes the original file. EQ and visualization apply to Local, AirPlay and Snapcast outputs. Allow the app’s media listener through your LAN firewall.",
            );
        }
        if let Err(error) = draft.validate() {
            theme::subtle(ui, &palette, &error);
        }
    });
    ui.data_mut(|d| d.insert_temp(key, (app.settings.casting.clone(), draft)));
    true
}
pub(crate) fn popup(app: &mut App, ctx: &egui::Context) {
    if !app.show_devices {
        return;
    }
    let mut open = true;
    let anchor = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(super::super::devices::BUTTON_RECT_ID)))
        .unwrap_or(egui::Rect::from_min_size(
            egui::pos2(400.0, 500.0),
            egui::Vec2::ZERO,
        ));
    egui::Window::new("Audio output")
        .id(egui::Id::new("casting-output-picker"))
        .open(&mut open)
        .default_width(360.0)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(egui::pos2(anchor.right(), (anchor.top() - 12.0).max(8.0)))
        .resizable(false)
        .max_height(500.0)
        .vscroll(true)
        .collapsible(false)
        .show(ctx, |ui| {
            let palette = app.palette;
            let active = app.settings.casting.kind(app.settings.snapcast.send);
            for kind in [Kind::Local, Kind::Snapcast] {
                if theme::soft_button(ui, &palette, None, kind.label(), active == kind).clicked() {
                    let mut s = app.settings.casting.clone();
                    s.output = Some(kind);
                    app.actions.push(Action::CastingSettings(s));
                }
            }
            ui.separator();
            let mut allow = app.settings.casting.allow_lan;
            if ui
                .horizontal(|ui| {
                    ui.label("Allow LAN casting");
                    widgets::switch(ui, &palette, "Allow LAN casting", &mut allow)
                })
                .inner
                .changed()
            {
                app.actions.push(Action::CastingSettings(Settings {
                    allow_lan: allow,
                    output: if !allow && matches!(active, Kind::AirPlay | Kind::Chromecast) {
                        Some(Kind::Local)
                    } else {
                        app.settings.casting.output
                    },
                    ..app.settings.casting.clone()
                }));
            }
            if theme::soft_button(ui, &palette, None, "Find receivers", false).clicked() {
                app.actions.push(Action::DiscoverOutputs);
            }
            if app.casting_scanning {
                ui.spinner();
            }
            for receiver in app.casting_receivers.clone() {
                let selected = active == receiver.kind
                    && app.settings.casting.host == receiver.address.ip().to_string()
                    && app.settings.casting.port == receiver.address.port();
                ui.add_enabled_ui(receiver.supported, |ui| {
                    if theme::soft_button(
                        ui,
                        &palette,
                        None,
                        &format!("{} · {}", receiver.name, receiver.kind.label()),
                        selected,
                    )
                    .clicked()
                    {
                        app.actions.push(Action::CastingSettings(Settings {
                            output: Some(receiver.kind),
                            host: receiver.address.ip().to_string(),
                            port: receiver.address.port(),
                            name: receiver.name.clone(),
                            allow_lan: allow,
                            certificate_sha256: if app.settings.casting.host
                                == receiver.address.ip().to_string()
                                && app.settings.casting.port == receiver.address.port()
                            {
                                app.settings.casting.certificate_sha256.clone()
                            } else {
                                String::new()
                            },
                        }));
                    }
                });
                if !receiver.supported {
                    theme::subtle(
                        ui,
                        &palette,
                        "Requires AirPlay pairing, password or encryption; not supported yet",
                    );
                }
            }
            theme::subtle(ui, &palette, &app.casting_status);
            if theme::soft_button(ui, &palette, None, "Output settings…", false).clicked() {
                app.actions.push(Action::Open(crate::model::Page::Settings));
                open_settings(app);
            }
        });
    app.show_devices &= open;
}
fn open_settings(app: &mut App) {
    app.show_devices = false;
}
