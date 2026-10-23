use super::{RowText, filtered_row, section, section_matches};
use crate::{
    app::App,
    casting::{Kind, Settings},
    model::Action,
    theme,
    ui::widgets,
};
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, needle: &str) -> bool {
    let mut draft = app
        .casting_draft
        .as_ref()
        .map(|(s, _)| s.clone())
        .unwrap_or_else(|| app.settings.casting.clone());
    if draft.port == 0 {
        draft.port = 5000;
    }
    let before = draft.clone();
    let mut immediate = false;
    let rows = [
        RowText::new(
            "Output",
            "Choose local playback, AirPlay, Chromecast or the configured Snapcast server.",
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
            "AirPlay Classic normally uses 5000; newer AirPlay receivers often use 7000. Chromecast normally uses 8009.",
        ),
        RowText::new(
            "Receiver certificate",
            "For a self-hosted Cast receiver, enter its SHA-256 certificate fingerprint. Leave blank for genuine Google Cast devices.",
        ),
        RowText::new("Playback status", app.casting_status.clone()),
        RowText::new(
            "Connection",
            "Changes apply automatically after a short typing pause. Playing music continues on the new output; paused music stays paused.",
        ),
        RowText::new(
            "AirPlay pairing",
            "Use an on-screen PIN with newer receivers, including Roku. Pairing keys are remembered in the system credential store.",
        ),
    ];
    if !section_matches(needle, "Casting", &rows) {
        return false;
    }
    let palette = app.palette;
    section(ui, &palette, "Casting", |ui| {
        receiving_warning(app, ui);
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
                immediate = true;
                draft.output = Some(kind);
                draft.port = if kind == Kind::AirPlay { 5000 } else { 8009 };
            }
        });
        filtered_row(ui, &palette, needle, "Casting", &rows[1], |ui| {
            if widgets::switch(ui, &palette, "Allow LAN casting", &mut draft.allow_lan).changed() {
                immediate = true;
                if !draft.allow_lan
                    && matches!(
                        draft.kind(app.settings.snapcast.send),
                        Kind::AirPlay | Kind::Chromecast
                    )
                {
                    draft.output = Some(Kind::Local);
                }
            }
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
            if theme::soft_button(ui, &palette, None, "Reconnect", false).clicked() {
                let mut settings = draft.clone();
                settings.output = Some(settings.kind(app.settings.snapcast.send));
                app.actions.push(Action::CastingSettings(settings));
            }
        });
        if draft.kind(app.settings.snapcast.send) == Kind::AirPlay {
            filtered_row(ui, &palette, needle, "Casting", &rows[7], |ui| {
                if widgets::switch(
                    ui,
                    &palette,
                    "Receiver uses AirPlay PIN pairing",
                    &mut draft.airplay2,
                )
                .changed()
                {
                    immediate = true;
                }
                if draft.airplay2
                    && theme::soft_button(ui, &palette, None, "Pair receiver…", false).clicked()
                {
                    app.actions.push(Action::PairAirPlay {
                        settings: draft.clone(),
                        force: true,
                    });
                }
            });
        }
        if draft.kind(app.settings.snapcast.send) == Kind::Chromecast {
            theme::subtle(
                ui,
                &palette,
                "Chromecast decodes the original file. EQ and visualization apply to Local, AirPlay and Snapcast outputs. Allow the app’s media listener through your LAN firewall.",
            );
        }
        if let Err(error) = draft.validate_with_snapcast(&app.settings.snapcast) {
            super::audio_notice(ui, &palette, format!("Not applied: {error}"));
        } else if app.casting_draft.is_some() || draft != before {
            theme::subtle(ui, &palette, "Updating output automatically…");
        }
    });
    if draft != before {
        draft.output = Some(draft.kind(app.settings.snapcast.send));
        app.actions.push(Action::EditCastingSettings {
            settings: draft,
            immediate,
        });
    }
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
            receiving_warning(app, ui);
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
                        let settings = Settings {
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
                            airplay2: receiver.airplay2,
                        };
                        app.actions.push(if receiver.pairing {
                            Action::PairAirPlay {
                                settings,
                                force: false,
                            }
                        } else {
                            Action::CastingSettings(settings)
                        });
                    }
                });
                if !receiver.supported {
                    theme::subtle(
                        ui,
                        &palette,
                        receiver
                            .reason
                            .as_deref()
                            .unwrap_or("Receiver is not supported"),
                    );
                } else if receiver.pairing {
                    theme::subtle(ui, &palette, "AirPlay · on-screen PIN on first connection");
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

fn receiving_warning(app: &App, ui: &mut egui::Ui) {
    if app.settings.snapcast.receive {
        let text = app.settings.casting.playback_warning(&app.settings.snapcast).unwrap_or(
            "Snapcast Receive is also enabled and plays through this computer's receiver device. Turn it off in Settings to listen only on the selected output.");
        super::audio_notice(ui, &app.palette, text);
    }
}
