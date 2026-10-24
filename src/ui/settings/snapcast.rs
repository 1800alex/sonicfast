//! Snapcast controls share Appearance's cards, labels and settings rows.
use super::{RowText, filtered_row, section, section_matches, slider_value_style};
use crate::{app::App, model::Action, theme, ui::widgets};
use egui::{CornerRadius, Frame, Margin};

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, needle: &str) -> bool {
    let palette = app.palette;
    let mut draft = app
        .snapcast_draft
        .as_ref()
        .map(|(s, _)| s.clone())
        .unwrap_or_else(|| app.settings.snapcast.clone());
    let before = draft.clone();
    let mut immediate = false;
    let rows = [
        RowText::new(
            "Snapcast output",
            "List Snapcast in the player's output button. Choose it there to play your music through Snapserver instead of this computer.",
        ),
        RowText::new(
            "Receive audio",
            "Hear a synchronized Snapcast stream with the built-in receiver. No companion program is required.",
        ),
        RowText::new(
            "Trusted network",
            "Allow unencrypted Snapcast audio on this network. Use a VPN for remote access; these ports have no authentication.",
        ),
        RowText::new(
            "Snapcast server",
            "Hostname or IP address, without a URL or credentials.",
        ),
        RowText::new(
            "Source port",
            "Snapserver TCP input for sending audio (44100:16:2). This is not the receiver port.",
        ),
        RowText::new(
            "Receiver port",
            "Snapcast audio streaming port, normally 1704. Supports PCM and FLAC streams.",
        ),
        RowText::new(
            "Receiver device",
            "Choose the local audio output for received streams. Default follows your system output.",
        ),
        RowText::new(
            "Latency correction",
            "Advance receiver playback by this many milliseconds to compensate for additional device latency. Usually 0.",
        ),
        RowText::new(
            "Receiver identity",
            "Optional unique room ID. Blank generates and saves an identity when receiving is enabled.",
        ),
        RowText::new("Sender status", app.snapcast_status.sender.clone()),
        RowText::new("Receiver status", app.snapcast_status.receiver.clone()),
        RowText::new(
            "Connection",
            "Changes apply automatically after a short typing pause. Playing music continues unless Receive takes over Local output. Choose Snapcast as the output and enable Receive to hear your own music in sync.",
        ),
        RowText::new(
            "About Snapcast",
            "Snapcast plays one stream in sync on speakers in every room. Sonicfast can send to it, receive from it, or both.",
        ),
    ];
    if !section_matches(needle, "Snapcast", &rows) {
        return false;
    }
    section(ui, &palette, "Snapcast", |ui| {
        filtered_row(ui, &palette, needle, "Snapcast", &rows[12], |ui| {
            if theme::soft_button(
                ui,
                &palette,
                Some(theme::Icon::Info),
                "How Snapcast works",
                false,
            )
            .clicked()
            {
                app.actions
                    .push(Action::ShowDialog(crate::model::Dialog::SnapcastHelp));
            }
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[0], |ui| {
            if widgets::switch(ui, &palette, "Snapcast output", &mut draft.offer).changed() {
                immediate = true;
                // Turning it off while sending returns the music to this computer.
                if !draft.offer {
                    draft.send = false;
                }
            }
        });
        if draft.offer && !draft.can_send() {
            super::audio_notice(
                ui,
                &palette,
                "Turn on Trusted network and enter your Snapcast server to list it as an output.",
            );
            ui.add_space(6.0);
        } else if draft.offer && !draft.send {
            theme::subtle(
                ui,
                &palette,
                "Choose Snapcast from the player's output button to send music.",
            );
        }
        filtered_row(ui, &palette, needle, "Snapcast", &rows[1], |ui| {
            immediate |=
                widgets::switch(ui, &palette, "Receive audio", &mut draft.receive).changed();
        });
        if let Some(warning) = app.settings.casting.playback_warning(&draft) {
            super::audio_notice(ui, &palette, warning);
        }
        filtered_row(ui, &palette, needle, "Snapcast", &rows[2], |ui| {
            immediate |=
                widgets::switch(ui, &palette, "Trusted network", &mut draft.allow_insecure)
                    .changed();
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[3], |ui| {
            field(
                ui,
                app.locale,
                &palette,
                &mut draft.host,
                "127.0.0.1",
                "snapcast-host",
            );
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[4], |ui| {
            slider_value_style(ui.style_mut(), &palette);
            ui.add(egui::DragValue::new(&mut draft.source_port).range(1..=65535));
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[5], |ui| {
            slider_value_style(ui.style_mut(), &palette);
            ui.add(egui::DragValue::new(&mut draft.stream_port).range(1..=65535));
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[6], |ui| {
            if matches!(app.output_devices, crate::model::Loadable::NotLoaded) {
                app.actions.push(Action::ListOutputDevices);
            }
            let name = if draft.receiver_device.is_empty() {
                "Default output"
            } else {
                app.output_devices
                    .get()
                    .into_iter()
                    .flatten()
                    .find(|device| device.id.as_str() == draft.receiver_device)
                    .map_or("Saved output", |device| device.name.as_str())
            };
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                egui::ComboBox::from_id_salt("snapcast-receiver-device")
                    .selected_text(name)
                    .width(210.0_f32.min(ui.available_width()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.receiver_device,
                            String::new(),
                            "Default output",
                        );
                        for device in app.output_devices.get().into_iter().flatten() {
                            ui.selectable_value(
                                &mut draft.receiver_device,
                                device.id.as_str().to_owned(),
                                &device.name,
                            );
                        }
                        if app.output_devices.is_loading() {
                            ui.spinner();
                        }
                    });
                if theme::soft_button(ui, &palette, None, "Refresh devices", false).clicked() {
                    app.actions.push(Action::ListOutputDevices);
                }
            });
        });
        immediate |= draft.receiver_device != before.receiver_device;
        filtered_row(ui, &palette, needle, "Snapcast", &rows[7], |ui| {
            slider_value_style(ui.style_mut(), &palette);
            ui.add(
                egui::DragValue::new(&mut draft.latency_ms)
                    .range(-1000..=1000)
                    .suffix(" ms"),
            );
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[8], |ui| {
            field(
                ui,
                app.locale,
                &palette,
                &mut draft.client_id,
                "Generate identity",
                "snapcast-identity",
            );
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[9], |_| {});
        filtered_row(ui, &palette, needle, "Snapcast", &rows[10], |_| {});
        filtered_row(ui, &palette, needle, "Snapcast", &rows[11], |ui| {
            if theme::soft_button(ui, &palette, None, "Reconnect", false).clicked() {
                app.actions
                    .push(Action::SnapcastSettings(Box::new(draft.clone())));
            }
        });
        if let Err(error) = draft.validate() {
            super::audio_notice(ui, &palette, format!("Not applied: {error}"));
        } else if draft != app.settings.snapcast {
            theme::subtle(ui, &palette, "Updating Snapcast automatically…");
        }
    });
    if draft != before {
        if !draft.allow_insecure && before.allow_insecure {
            draft.send = false;
            draft.receive = false;
        }
        app.actions.push(Action::EditSnapcastSettings {
            settings: Box::new(draft),
            immediate,
        });
    }
    true
}
fn field(
    ui: &mut egui::Ui,
    locale: crate::i18n::Locale,
    palette: &theme::Palette,
    text: &mut String,
    hint: &str,
    id: &str,
) {
    Frame::new()
        .fill(palette.surface)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            widgets::text_edit(
                ui,
                locale,
                egui::TextEdit::singleline(text)
                    .id(egui::Id::new(id))
                    .font(theme::regular(14.0))
                    .frame(Frame::NONE)
                    .desired_width(190.0)
                    .hint_text(hint),
            );
        });
}

/// Receiving does not require a music-server login.
pub(crate) fn signed_out(app: &mut App, ui: &mut egui::Ui) {
    let key = egui::Id::new("snapcast-signed-out-open");
    let mut open = ui.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
    if theme::soft_button(ui, &app.palette, None, "Snapcast audio…", false).clicked() {
        open = true;
    }
    egui::Window::new("Snapcast audio")
        .id(key)
        .open(&mut open)
        .default_width(760.0)
        .max_height(600.0)
        .vscroll(true)
        .show(ui.ctx(), |ui| {
            show(app, ui, "");
        });
    ui.data_mut(|data| data.insert_temp(key, open));
}
