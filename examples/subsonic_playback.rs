//! Phase 2 native PCM proof. No Engine, librespot Session or GUI is constructed.
//! Connection secrets are environment-only. See docs/design/phase-2.md.

use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use libresonic::opensubsonic::{Client, ClientOptions, Credentials};
use libresonic::playback::{OutputError, Phase, PlaybackError, PlaybackState, Player};
use libresonic::{AlbumOrder, MediaId, MediaKind, MusicService, PageRequest, ServerId};
use sonicfast::{eq, sink::PcmOutput, vis::AudioTap};

// Limit diagnostics to device/DSP code; HTTP debug output may contain secrets.
struct AudioLog;
impl log::Log for AudioLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target().starts_with("sonicfast::sink")
            || metadata.target().starts_with("fastframe_audio")
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

#[derive(Parser)]
struct Args {
    /// Explicitly allow a trusted plaintext HTTP server.
    #[arg(long)]
    allow_http: bool,
    /// Track identity; otherwise find a track with --format in the first 100 albums.
    #[arg(long)]
    track: Option<String>,
    #[arg(long, default_value = "mp3", value_parser = ["mp3", "flac", "m4a"])]
    format: String,
    /// Output ID or saved name; an absent explicit choice is an error.
    #[arg(long)]
    device: Option<String>,
    #[arg(long)]
    list_devices: bool,
    /// Run deterministic pause/resume/mute/seek/stop/end checks, emitting JSON evidence.
    #[arg(long)]
    scripted: bool,
    /// Also prove an unavailable output fails safely before opening the chosen output.
    #[arg(long, requires = "scripted")]
    check_missing_device: bool,
}

fn required(name: &str) -> anyhow::Result<String> {
    env::var(name).map_err(|_| anyhow::anyhow!("Set the required {name} environment variable"))
}

fn event(action: &str, player: &Player, tap: &AudioTap) {
    let state = player.snapshot();
    let samples = tap.window(1024, 0);
    let rms =
        (samples.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    println!(
        "{}",
        serde_json::json!({
            "unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "action": action, "phase": format!("{:?}", state.phase),
            "position_ms": state.position_ms, "duration_ms": state.duration_ms,
            "generation": state.generation, "volume": state.volume,
            "startup_ms": state.startup_ms, "encoded_bytes": state.encoded_bytes,
            "peak_input_bytes": state.peak_input_bytes, "tap_rms": rms,
        })
    );
    let _ = io::stdout().flush();
}

async fn wait(player: &Player, predicate: impl Fn(&PlaybackState) -> bool) -> anyhow::Result<()> {
    let mut updates = player.subscribe();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let state = updates.borrow().clone();
            if predicate(&state) {
                return Ok(());
            }
            if let Some(error) = state.error {
                return Err(anyhow::anyhow!(error));
            }
            updates
                .changed()
                .await
                .map_err(|_| anyhow::anyhow!("Player worker stopped"))?;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Playback transition timed out"))?
}

async fn scripted(player: &Player, tap: &AudioTap) -> anyhow::Result<()> {
    wait(player, |s| {
        s.phase == Phase::Playing && s.position_ms >= 300
    })
    .await?;
    event("playing", player, tap);
    player.pause();
    tokio::time::sleep(Duration::from_millis(350)).await;
    let frozen = player.snapshot().position_ms;
    event("paused", player, tap);
    tokio::time::sleep(Duration::from_millis(400)).await;
    anyhow::ensure!(
        player.snapshot().position_ms == frozen,
        "Paused position advanced"
    );
    player.play()?;
    event("resume", player, tap);
    tokio::time::sleep(Duration::from_millis(450)).await;
    player.set_volume(0);
    // Allow the bounded feeder/output queue to observe the new gain.
    tokio::time::sleep(Duration::from_millis(300)).await;
    event("muted", player, tap);
    anyhow::ensure!(
        tap.window(1024, 0).iter().any(|v| v.abs() > 0.001),
        "Pre-volume tap is silent at mute"
    );
    tokio::time::sleep(Duration::from_millis(350)).await;
    player.set_volume(u16::MAX);
    event("unmute", player, tap);
    player.seek(1500)?;
    let generation = player.snapshot().generation;
    wait(player, |s| {
        s.generation == generation && s.phase == Phase::Playing
    })
    .await?;
    event("seeked", player, tap);
    tokio::time::sleep(Duration::from_millis(450)).await;
    let at = Instant::now();
    player.stop();
    let command_ms = at.elapsed().as_millis();
    tokio::time::sleep(Duration::from_millis(350)).await;
    anyhow::ensure!(
        player.snapshot().phase == Phase::Stopped,
        "Stop was superseded"
    );
    event("stopped", player, tap);
    anyhow::ensure!(command_ms < 1000, "Stop command exceeded one second");
    tokio::time::sleep(Duration::from_millis(350)).await;
    player.play()?;
    wait(player, |s| s.phase == Phase::Ended).await?;
    event("ended", player, tap);
    Ok(())
}

async fn run(args: Args) -> anyhow::Result<()> {
    if args.list_devices {
        for device in fastframe_audio::output_devices()
            .map_err(|_| anyhow::anyhow!("Unable to enumerate outputs"))?
        {
            println!("{}\t{}", device.id.as_str(), device.name);
        }
        return Ok(());
    }
    let server = ServerId::new("playback-probe")?;
    let credentials = match env::var("LIBRESONIC_API_KEY") {
        Ok(key) => Credentials::api_key(key),
        Err(env::VarError::NotPresent) => Credentials::token(
            required("LIBRESONIC_USERNAME")?,
            required("LIBRESONIC_PASSWORD")?,
        ),
        Err(_) => anyhow::bail!("Invalid API-key environment variable"),
    };
    let client = Arc::new(Client::new(
        server.clone(),
        &required("LIBRESONIC_URL")?,
        credentials,
        ClientOptions::default().allow_http(args.allow_http),
    )?);
    client.connect().await?;
    let id = if let Some(track) = args.track {
        MediaId::new(server, MediaKind::Track, track)?
    } else {
        let albums = client
            .albums(AlbumOrder::Title, PageRequest::new(0, 100)?)
            .await?;
        let mut found = None;
        for album in albums.items {
            let album = client.album(&album.id).await?;
            if let Some(track) = album
                .tracks
                .into_iter()
                .find(|t| t.suffix.as_deref() == Some(&args.format))
            {
                found = Some(track.id);
                break;
            }
        }
        found.ok_or_else(|| anyhow::anyhow!("No requested codec found in the first 100 albums"))?
    };
    let runtime = tokio::runtime::Handle::current();
    if args.check_missing_device {
        let missing = Player::spawn(
            client.clone(),
            Box::new(PcmOutput::new(
                Some("libresonic-intentionally-absent-device".into()),
                eq::shared(),
                AudioTap::new(),
            )),
            runtime.clone(),
        )?;
        missing.load(id.clone(), true)?;
        wait(&missing, |s| s.phase == Phase::Error).await?;
        anyhow::ensure!(
            missing.snapshot().error == Some(PlaybackError::Output(OutputError::NoDevice)),
            "Missing device did not return NoDevice"
        );
        println!("{{\"action\":\"missing_device_recoverable\"}}");
        missing.shutdown().await?;
    }
    let tap = AudioTap::new();
    let player = Player::spawn(
        client,
        Box::new(PcmOutput::new(args.device, eq::shared(), tap.clone())),
        runtime,
    )?;
    player.load(id, true)?;
    let result = if args.scripted {
        scripted(&player, &tap).await
    } else {
        eprintln!(
            "Commands: pause, play, stop, volume 0..65535, seek <milliseconds>, status, quit"
        );
        let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if sender.blocking_send(line).is_err() {
                    break;
                }
            }
        });
        while let Some(line) = receiver.recv().await {
            let mut words = line.split_whitespace();
            match words.next() {
                Some("pause") => player.pause(),
                Some("play") => player.play()?,
                Some("stop") => player.stop(),
                Some("volume") => {
                    if let Some(value) = words.next().and_then(|v| v.parse().ok()) {
                        player.set_volume(value);
                    }
                }
                Some("seek") => {
                    if let Some(value) = words.next().and_then(|v| v.parse().ok()) {
                        player.seek(value)?;
                    }
                }
                Some("quit") => break,
                _ => {}
            }
            event("status", &player, &tap);
            if let Some(error) = player.snapshot().error {
                eprintln!("{error}");
            }
        }
        Ok(())
    };
    player.shutdown().await?;
    result
}

#[tokio::main]
async fn main() -> ExitCode {
    let _ = log::set_logger(&AudioLog);
    log::set_max_level(log::LevelFilter::Info);
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
