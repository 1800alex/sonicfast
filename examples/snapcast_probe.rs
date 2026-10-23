//! Synthetic sender and timestamped receiver diagnostic. No private media,
//! credentials, host mixer changes or companion receiver program.
use clap::{Parser, Subcommand};
use libresonic::playback::{AudioOutput, Cancellation, PcmBlock};
use sonicfast::snapcast::{Sender, Settings, Shared};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Mode,
}
#[derive(Subcommand)]
enum Mode {
    Send {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 4953)]
        port: u16,
        #[arg(long, default_value_t = 8)]
        seconds: u32,
        #[arg(long)]
        scripted: bool,
    },
    AppSend {
        #[arg(long)]
        music_url: String,
        #[arg(long, default_value_t = 4953)]
        port: u16,
    },
    Receive {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 1704)]
        port: u16,
        #[arg(long, default_value_t = 15)]
        seconds: u32,
        #[arg(long)]
        capture: Option<PathBuf>,
        #[arg(long, default_value = "sonicfast-probe")]
        id: String,
        #[arg(long, default_value = "")]
        device: String,
    },
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Args::parse().command {
        Mode::Send {
            host,
            port,
            seconds,
            scripted,
        } => {
            let addr = tokio::net::lookup_host((host.as_str(), port))
                .await?
                .next()
                .ok_or_else(|| anyhow::anyhow!("No source address"))?;
            let shared = Shared::resolved(
                Settings {
                    send: true,
                    allow_insecure: true,
                    host,
                    source_port: port,
                    ..Default::default()
                },
                addr,
            )
            .map_err(anyhow::Error::msg)?;
            tokio::task::spawn_blocking(move||->anyhow::Result<()>{
                let mut sender=Sender::new(shared,sonicfast::eq::shared(),sonicfast::vis::AudioTap::new());
                let cancel=Cancellation::default();sender.reset().map_err(safe)?;sender.resume().map_err(safe)?;
                let start=Instant::now();let mut frames=0;
                for chunk in 0..seconds*50 {
                    if scripted && chunk==100 {sender.pause();println!("{}",serde_json::json!({"event":"pause","elapsed_ms":start.elapsed().as_millis()}));std::thread::sleep(Duration::from_secs(2));sender.resume().map_err(safe)?;}
                    if scripted && chunk==200 {sender.set_volume(0);}
                    if scripted && chunk==250 {sender.set_volume(u16::MAX);}
                    let samples:Vec<f64>=(0..882).flat_map(|i|{
                        let n=frames+i;
                        // Distinct channels and a repeating amplitude marker.
                        let amplitude=if (n/4410)%10==0 {0.3}else{0.08};
                        [amplitude*(std::f64::consts::TAU*440.0*n as f64/44100.0).sin(),amplitude*(std::f64::consts::TAU*880.0*n as f64/44100.0).sin()]
                    }).collect();
                    sender.write(PcmBlock::new(&samples,44100,2).map_err(safe)?,&cancel).map_err(safe)?;frames+=882;
                }
                sender.finish(&cancel).map_err(safe)?;
                while !sender.drained().map_err(safe)? {std::thread::sleep(Duration::from_millis(2));}
                sender.stop();cancel.cancel();
                assert!(sender.write(PcmBlock::new(&[0.0;8],44100,2).map_err(safe)?,&cancel).is_err());
                println!("{}",serde_json::json!({"event":"sent","frames":frames,"elapsed_ms":start.elapsed().as_millis(),"cancelled_write_rejected":true}));Ok(())
            }).await??;
        }
        Mode::AppSend { music_url, port } => {
            tokio::task::spawn_blocking(move || app_send(music_url, port)).await??;
        }
        Mode::Receive {
            host,
            port,
            seconds,
            capture,
            id,
            device,
        } => {
            let settings = Settings {
                receive: true,
                allow_insecure: true,
                host,
                stream_port: port,
                client_id: id,
                receiver_device: device,
                ..Default::default()
            };
            let shared = Shared::default();
            let status = shared.clone();
            let cancel = Cancellation::default();
            let token = cancel.clone();
            let task = tokio::spawn(sonicfast::snapcast::native::receive(
                settings, shared, token, capture,
            ));
            let until = Instant::now() + Duration::from_secs(u64::from(seconds));
            let mut previous = String::new();
            while Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let next = status.status().receiver;
                if next != previous {
                    println!("{}", serde_json::json!({"event":"receiver","status":next}));
                    previous = next;
                }
            }
            cancel.cancel();
            tokio::time::timeout(Duration::from_secs(5), task)
                .await??
                .map_err(anyhow::Error::msg)?;
            println!("{}", serde_json::json!({"event":"receiver_stopped"}));
        }
    }
    Ok(())
}

fn safe(error: libresonic::playback::OutputError) -> anyhow::Error {
    anyhow::anyhow!("Audio output: {error:?}")
}

fn app_send(music_url: String, port: u16) -> anyhow::Result<()> {
    use sonicfast::{
        app::{App, AppOptions},
        backend::{AuthStatus, Waker},
        model::Action,
        paths::AppDirs,
        player::Playback,
        settings::Settings as AppSettings,
    };
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/snapcast/app-proof")
        .join(format!("{}-{}", std::process::id(), port));
    let dirs = AppDirs {
        config: base.join("config"),
        state: base.join("state"),
        cache: base.join("cache"),
    };
    dirs.ensure()?;
    let settings = AppSettings {
        volume: u16::MAX,
        snapcast: Settings {
            send: true,
            allow_insecure: true,
            source_port: port,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut app = App::new_subsonic(
        &Waker::default(),
        dirs,
        settings,
        AppOptions {
            restore_sign_in: false,
            media_controls: false,
            tray: false,
            autostart: false,
        },
    );
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let frame = |app: &mut App| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.background_frame(&ctx);
                app.frame_ui(ui);
            },
        );
        output.textures_delta.clear();
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while !matches!(app.auth, AuthStatus::SignedOut) {
        frame(&mut app);
        anyhow::ensure!(Instant::now() < deadline, "App startup timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    app.server_form.profile.url = music_url;
    app.server_form.profile.username = "fixture".into();
    app.server_form.profile.allow_http = true;
    app.server_form.secret = "fixture-only".into();
    app.actions.push(Action::ServerConnect);
    while !app.is_connected() || app.server_form.active.is_none() {
        frame(&mut app);
        anyhow::ensure!(Instant::now() < deadline, "Fixture login timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
    let server = libresonic::ServerId::new(
        app.server_form
            .active
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No active fixture identity"))?,
    )?;
    let id = libresonic::MediaId::new(server, libresonic::MediaKind::Track, "tone")?;
    // Applying the route must carry the latest player volume into its rebuilt
    // output, rather than restoring the volume from application startup.
    app.actions.push(Action::SetVolume(75));
    app.actions.push(Action::SnapcastSettings(Box::new(
        app.settings.snapcast.clone(),
    )));
    frame(&mut app);

    app.actions.push(Action::PlayUris {
        uris: vec![sonicfast::subsonic::adapter::uri(&id)],
        index: 0,
    });
    let mut played = false;
    loop {
        frame(&mut app);
        played |= app.local.playback == Playback::Playing;
        anyhow::ensure!(app.local.error.is_none(), "App playback failed");
        if played && app.local.playback != Playback::Playing && app.local.position_ms >= 1500 {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "App-to-Snapcast playback did not complete: {:?}, {} ms",
            app.local.playback,
            app.local.position_ms
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    anyhow::ensure!(
        app.local.volume == sonicfast::app::percent_to_volume(75),
        "Route apply lost the current volume"
    );
    println!(
        "{}",
        serde_json::json!({"event":"app_sent","position_ms":app.local.position_ms,"output":app.snapcast_status.sender})
    );
    app.backend.shutdown();
    Ok(())
}
