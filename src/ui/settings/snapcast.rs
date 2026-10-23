//! Snapcast controls share Appearance's cards, labels and settings rows.
use super::{RowText, filtered_row, section, section_matches, slider_value_style};
use crate::{app::App, model::Action, theme, ui::widgets};
use egui::{CornerRadius, Frame, Margin};

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, needle: &str) -> bool {
    let palette = app.palette;
    let key = egui::Id::new("snapcast-settings-draft");
    let mut draft = ui
        .data(|data| data.get_temp::<crate::snapcast::Settings>(key))
        .unwrap_or_else(|| app.settings.snapcast.clone());
    let rows = [
        RowText::new(
            "Send music",
            "Play your music through Snapserver instead of the direct local output. Player volume affects this source.",
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
            "Optional unique room ID. Blank generates an identity when you apply; it is saved for future connections.",
        ),
        RowText::new("Sender status", app.snapcast_status.sender.clone()),
        RowText::new("Receiver status", app.snapcast_status.receiver.clone()),
        RowText::new(
            "Apply routing",
            "Applying pauses local music and restarts the receiver. Press Play to send. To hear your own source in sync, enable both Send and Receive, then select Sonicfast in Snapserver.",
        ),
    ];
    if !section_matches(needle, "Snapcast", &rows) {
        return false;
    }
    section(ui, &palette, "Snapcast", |ui| {
        filtered_row(ui, &palette, needle, "Snapcast", &rows[0], |ui| {
            widgets::switch(ui, &palette, "Send music", &mut draft.send);
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[1], |ui| {
            widgets::switch(ui, &palette, "Receive audio", &mut draft.receive);
        });
        filtered_row(ui, &palette, needle, "Snapcast", &rows[2], |ui| {
            widgets::switch(ui, &palette, "Trusted network", &mut draft.allow_insecure);
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
            if theme::soft_button(ui, &palette, None, "Apply / reconnect", false).clicked() {
                if draft.receive && draft.client_id.is_empty() {
                    draft.client_id = format!("sonicfast-{:016x}", rand::random::<u64>());
                }
                app.actions
                    .push(Action::SnapcastSettings(Box::new(draft.clone())));
            }
        });
        if let Err(error) = draft.validate() {
            theme::subtle(ui, &palette, &error);
        }
    });
    ui.data_mut(|data| data.insert_temp(key, draft));
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
