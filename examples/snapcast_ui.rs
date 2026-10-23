//! Native visual review of Snapcast controls with a disposable app profile.
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
        self.app.auth = if (2..=4).contains(&self.stage) {
            AuthStatus::Connected {
                username: "Visual fixture".into(),
            }
        } else {
            AuthStatus::SignedOut
        };
        self.app.user = if (2..=4).contains(&self.stage) {
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
                    .save(self.output.join(format!("snapcast-{}.png", self.stage)))
                    .unwrap();
                self.stage += 1;
                self.pending = false;
                self.at = Instant::now();
                match self.stage {
                    1 => ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("snapcast-signed-out-open"), true);
                    }),
                    2 => {
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("snapcast-signed-out-open"), false);
                            d.insert_temp(egui::Id::new("settings-filter"), "Snapcast".to_string());
                        });
                        self.app.actions.push(Action::Open(Page::Settings));
                    }
                    3 => ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("settings-filter"), "receiver".to_string());
                    }),
                    4 => self.app.actions.push(Action::SetTheme(ThemeChoice::Light)),
                    5 => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            680.0, 850.0,
                        )));
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("snapcast-signed-out-open"), true);
                        });
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
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/snapcast/ui-review");
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
    eframe::run_native(
        "Sonicfast Snapcast review",
        eframe::NativeOptions {
            persist_window: false,
            persistence_path: Some(output.join("eframe.ron")),
            viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 950.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
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
