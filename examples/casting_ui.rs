//! Native visual review of Casting controls with a disposable app profile.
//! Synthetic view state only; no library login or audio route is enabled.
use sonicfast::{
    app::{App, AppOptions},
    backend::{AuthStatus, Waker},
    model::{Action, Page},
    paths::AppDirs,
    settings::{Settings, ThemeChoice},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
struct Review {
    app: App,
    output: PathBuf,
    stage: usize,
    at: Instant,
    pending: bool,
}
impl eframe::App for Review {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.app.background_frame(&ctx);
        self.app.auth = if (0..=9).contains(&self.stage) {
            AuthStatus::Connected {
                username: "Visual fixture".into(),
            }
        } else {
            AuthStatus::SignedOut
        };
        self.app.user = if (0..=9).contains(&self.stage) {
            Some(
                serde_json::from_value(
                    serde_json::json!({"id":"visual-fixture","display_name":"Visual fixture"}),
                )
                .unwrap(),
            )
        } else {
            None
        };
        self.app.frame_ui(ui);
        if self.pending {
            if let Some(image) = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            }) {
                let pixels: Vec<_> = image
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_srgba_unmultiplied())
                    .collect();
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels)
                    .unwrap()
                    .save(self.output.join(format!("casting-{}.png", self.stage)))
                    .unwrap();
                self.stage += 1;
                self.pending = false;
                self.at = Instant::now();
                match self.stage {
                    1 => {
                        // LAN casting on, with the receivers above; the
                        // picker's own scan is marked done so none starts.
                        self.app.settings.casting.allow_lan = true;
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("casting-output-scanned"), true)
                        });
                        self.app.show_devices = true;
                    }
                    2 => {
                        self.app.show_devices = false;
                        // View-state fixture only; do not start a network receiver.
                        self.app.settings.snapcast.receive = true;
                        self.app.settings.snapcast.allow_insecure = true;
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("casting-unused"), false);
                            d.insert_temp(
                                egui::Id::new("settings-filter"),
                                "Receive audio".to_string(),
                            );
                        });
                        self.app.actions.push(Action::Open(Page::Settings));
                    }
                    3 => ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("settings-filter"), "Casting".to_string());
                    }),
                    4 => self.app.actions.push(Action::SetTheme(ThemeChoice::Light)),
                    5 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            680.0, 850.0,
                        )));
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("casting-unused"), true);
                        });
                    }
                    6 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            1100.0, 850.0,
                        )));
                        self.app.actions.push(Action::SetTheme(ThemeChoice::Dark));
                        self.app.airplay_pairing = sonicfast::casting::airplay::pairing::Status {
                            attempt: 800,
                            settings: sonicfast::casting::Settings {
                                name: "Living room Roku".into(),
                                ..Default::default()
                            },
                            stage: sonicfast::casting::airplay::pairing::Stage::Pin,
                            message: "Enter the four-digit AirPlay code shown on your TV.".into(),
                        };
                        self.app.dialog = Some(sonicfast::model::Dialog::AirPlayPairing);
                    }
                    7 => self.app.actions.push(Action::SetTheme(ThemeChoice::Light)),
                    8 => {
                        self.app.airplay_pairing.stage =
                            sonicfast::casting::airplay::pairing::Stage::Failed;
                        self.app.airplay_pairing.message =
                            sonicfast::casting::airplay::pairing::Error::Authentication.to_string();
                    }
                    9 => {
                        self.app.dialog = None;
                        self.app.cast_check = sonicfast::casting::CastCheck {
                            attempt: 900,
                            settings: sonicfast::casting::Settings {
                                name: "Garage Cast".into(),
                                ..Default::default()
                            },
                            stage: sonicfast::casting::CheckStage::Untrusted { pinned: false },
                        };
                        self.app.dialog = Some(sonicfast::model::Dialog::CastFingerprint);
                    }
                    _ => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
        } else if self.at.elapsed() > Duration::from_millis(900) {
            self.pending = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
fn main() -> eframe::Result {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/casting/ui-review");
    std::fs::create_dir_all(&output).unwrap();
    let base = output.join(format!("profile-{}", std::process::id()));
    let dirs = AppDirs {
        config: base.join("config"),
        state: base.join("state"),
        cache: base.join("cache"),
    };
    dirs.ensure().unwrap();
    let waker = Waker::default();
    let mut app = App::new_subsonic(
        &waker,
        dirs,
        Settings {
            theme: ThemeChoice::Dark,
            ..Default::default()
        },
        AppOptions {
            restore_sign_in: false,
            media_controls: false,
            tray: false,
            autostart: false,
        },
    );
    app.actions.push(Action::Open(Page::Settings));
    app.casting_receivers = vec![
        sonicfast::casting::Receiver {
            kind: sonicfast::casting::Kind::AirPlay,
            name: "Garage".into(),
            address: "127.0.0.1:5000".parse().unwrap(),
            supported: true,
            pairing: false,
            airplay2: false,
            reason: None,
        },
        sonicfast::casting::Receiver {
            kind: sonicfast::casting::Kind::Chromecast,
            name: "Garage Cast".into(),
            address: "127.0.0.1:8009".parse().unwrap(),
            supported: true,
            pairing: false,
            airplay2: false,
            reason: None,
        },
        sonicfast::casting::Receiver {
            kind: sonicfast::casting::Kind::AirPlay,
            name: "Living room Roku".into(),
            address: "192.0.2.12:7000".parse().unwrap(),
            supported: true,
            pairing: true,
            airplay2: true,
            reason: None,
        },
        sonicfast::casting::Receiver {
            kind: sonicfast::casting::Kind::AirPlay,
            name: "Office HomePod".into(),
            address: "192.0.2.13:7000".parse().unwrap(),
            supported: false,
            pairing: false,
            airplay2: true,
            reason: Some("This receiver restricts access to its Apple Home members".into()),
        },
    ];
    app.casting_status = "Ready; press Play".into();
    eframe::run_native(
        "Sonicfast Casting review",
        eframe::NativeOptions {
            persist_window: false,
            persistence_path: Some(output.join("eframe.ron")),
            viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 950.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("settings-filter"), "Casting".to_string())
            });
            app.attach(&cc.egui_ctx);
            waker.attach(&cc.egui_ctx);
            Ok(Box::new(Review {
                app,
                output,
                stage: 0,
                at: Instant::now(),
                pending: false,
            }))
        }),
    )
}
