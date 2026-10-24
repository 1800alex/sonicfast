use super::{RowText, filtered_row, section, section_matches};
use crate::{
    app::App,
    casting::{Kind, Settings},
    model::Action,
    theme::{self, Icon},
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
                app.actions
                    .push(if settings.output == Some(Kind::Chromecast) {
                        Action::ChooseCast(settings)
                    } else {
                        Action::CastingSettings(settings)
                    });
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
/// One output: its kind's icon and name, a check when it is in use, a
/// spinner while it is being checked. Unsupported receivers are dimmed and
/// say why on hover. Returns whether it was chosen.
fn output_row(
    ui: &mut egui::Ui,
    palette: &theme::Palette,
    icon: Icon,
    name: &str,
    state: RowState,
    reason: Option<&str>,
) -> bool {
    use egui::{Rect, Sense, Vec2, pos2, vec2};
    let enabled = reason.is_none();
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), 40.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            state == RowState::Selected,
            name,
        )
    });
    if response.hovered() && enabled {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), palette.surface_hover);
    }
    let color = match state {
        _ if !enabled => palette.secondary.gamma_multiply(0.6),
        RowState::Selected => palette.accent,
        _ => palette.text,
    };
    icon.image(color, 20.0).paint_at(
        ui,
        Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), Vec2::splat(20.0)),
    );
    crate::bidi::paint_line(
        &ui.painter().with_clip_rect(rect),
        rect.left() + 44.0,
        rect.right() - 36.0,
        rect.center().y,
        name,
        theme::medium(14.0),
        color,
    );
    let mark = Rect::from_center_size(
        pos2(rect.right() - 18.0, rect.center().y),
        Vec2::splat(16.0),
    );
    match state {
        RowState::Selected => Icon::Check.image(palette.accent, 16.0).paint_at(ui, mark),
        RowState::Busy => {
            let mut spin = ui.new_child(egui::UiBuilder::new().max_rect(mark).layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            ));
            theme::spinner(&mut spin, 16.0, palette.accent);
        }
        RowState::Idle => {}
    }
    if let Some(reason) = reason {
        response.on_hover_text(reason);
        return false;
    }
    response.clicked() && state != RowState::Selected
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowState {
    Idle,
    Selected,
    Busy,
}

/// AirPlay before Cast, then by name; one list for the picker.
fn sorted_receivers(mut receivers: Vec<crate::casting::Receiver>) -> Vec<crate::casting::Receiver> {
    receivers.sort_by_cached_key(|r| (r.kind != Kind::AirPlay, r.name.to_lowercase()));
    receivers
}

pub(crate) fn popup(app: &mut App, ctx: &egui::Context) {
    let scanned_id = egui::Id::new("casting-output-scanned");
    if !app.show_devices {
        ctx.data_mut(|d| d.remove::<bool>(scanned_id));
        return;
    }
    let palette = app.palette;
    let anchor = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(super::super::devices::BUTTON_RECT_ID)))
        .unwrap_or(egui::Rect::from_min_size(
            egui::pos2(400.0, 500.0),
            egui::Vec2::ZERO,
        ));
    let settings = app.settings.casting.clone();
    let active = settings.kind(app.settings.snapcast.send);
    // Look for receivers once each time the picker opens.
    if settings.allow_lan
        && !app.casting_scanning
        && !ctx.data(|d| d.get_temp::<bool>(scanned_id).unwrap_or(false))
    {
        ctx.data_mut(|d| d.insert_temp(scanned_id, true));
        app.actions.push(Action::DiscoverOutputs);
    }
    let width = 300.0;
    let area = egui::Area::new(egui::Id::new("casting-output-picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(
            (anchor.right() - width).max(8.0),
            (anchor.top() - 12.0).max(8.0),
        ))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .show(ctx, |ui| {
            widgets::menu_frame(&palette).show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    theme::text(ui, "Audio output", theme::bold(16.0), palette.text);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if app.casting_scanning {
                            theme::spinner(ui, 16.0, palette.accent);
                        } else if settings.allow_lan
                            && theme::icon_button(
                                ui,
                                Icon::Refresh,
                                15.0,
                                palette.secondary,
                                palette.text,
                                "Find receivers",
                            )
                            .clicked()
                        {
                            app.actions.push(Action::DiscoverOutputs);
                        }
                    });
                });
                ui.add_space(4.0);
                receiving_warning(app, ui);
                let choose = |kind| {
                    Action::CastingSettings(Settings {
                        output: Some(kind),
                        ..settings.clone()
                    })
                };
                let state = |selected| {
                    if selected {
                        RowState::Selected
                    } else {
                        RowState::Idle
                    }
                };
                if output_row(
                    ui,
                    &palette,
                    Icon::Speaker,
                    "This device",
                    state(active == Kind::Local),
                    None,
                ) {
                    app.actions.push(choose(Kind::Local));
                }
                if (active == Kind::Snapcast || app.settings.snapcast.can_send())
                    && output_row(
                        ui,
                        &palette,
                        Icon::Cast,
                        "Snapcast",
                        state(active == Kind::Snapcast),
                        None,
                    )
                {
                    app.actions.push(choose(Kind::Snapcast));
                }
                if settings.allow_lan {
                    let checking = (app.cast_check.stage == crate::casting::CheckStage::Checking)
                        .then_some(&app.cast_check.settings);
                    egui::ScrollArea::vertical()
                        .id_salt("casting-receivers")
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for receiver in sorted_receivers(app.casting_receivers.clone()) {
                                let host = receiver.address.ip().to_string();
                                let port = receiver.address.port();
                                let here = |s: &Settings| s.host == host && s.port == port;
                                let row = if checking.is_some_and(here) {
                                    RowState::Busy
                                } else {
                                    state(active == receiver.kind && here(&settings))
                                };
                                let icon = if receiver.kind == Kind::AirPlay {
                                    Icon::Airplay
                                } else {
                                    Icon::Cast
                                };
                                ui.push_id((&host, port, receiver.kind.label()), |ui| {
                                    if output_row(
                                        ui,
                                        &palette,
                                        icon,
                                        &receiver.name,
                                        row,
                                        receiver.reason.as_deref(),
                                    ) {
                                        let chosen = settings.for_receiver(&receiver);
                                        app.actions.push(match receiver.kind {
                                            Kind::Chromecast => Action::ChooseCast(chosen),
                                            _ if receiver.pairing => Action::PairAirPlay {
                                                settings: chosen,
                                                force: false,
                                            },
                                            _ => Action::CastingSettings(chosen),
                                        });
                                    }
                                });
                            }
                        });
                }
                widgets::menu_separator(ui, &palette);
                if !settings.allow_lan {
                    let mut allow = false;
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        ui.label("Find AirPlay and Cast receivers");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::switch(ui, &palette, "Allow LAN casting", &mut allow)
                                .changed()
                            {
                                app.actions.push(Action::CastingSettings(Settings {
                                    allow_lan: true,
                                    ..settings.clone()
                                }));
                            }
                        });
                    });
                }
                if widgets::menu_item(ui, &palette, Some(Icon::Settings), "Output settings…") {
                    app.actions.push(Action::Open(crate::model::Page::Settings));
                    app.show_devices = false;
                }
            });
        });
    let popup_rect = area.response.rect;
    let clicked_outside = ctx.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| !popup_rect.contains(pos) && !anchor.contains(pos))
    });
    if clicked_outside {
        app.show_devices = false;
    }
}

fn receiving_warning(app: &App, ui: &mut egui::Ui) {
    if app.settings.snapcast.receive {
        let text = app.settings.casting.playback_warning(&app.settings.snapcast).unwrap_or(
            "Snapcast Receive is also enabled and plays through this computer's receiver device. Turn it off in Settings to listen only on the selected output.");
        super::audio_notice(ui, &app.palette, text);
    }
}
