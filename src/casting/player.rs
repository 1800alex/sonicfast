use super::{Shared, media::Server};
use crate::cast::MediaConnection;
use libresonic::{
    MediaId, MusicService, StreamProvider,
    opensubsonic::Client,
    playback::{Phase, PlaybackError, PlaybackState, Player as Local},
};
use serde_json::{Value, json};
use std::{
    net::IpAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub enum Player {
    Local {
        inner: Local,
        airplay: bool,
    },
    /// A receiver that fetches and decodes the original itself (Cast, MPD).
    Remote(Remote),
}
impl Player {
    pub fn local(inner: Local, airplay: bool) -> Self {
        Self::Local { inner, airplay }
    }
    pub fn cast(
        client: Arc<Client>,
        provider: Arc<dyn StreamProvider>,
        shared: Shared,
    ) -> Result<Self, PlaybackError> {
        Ok(Self::Remote(Remote::new::<Cast>(client, provider, shared)?))
    }
    pub fn mpd(
        client: Arc<Client>,
        provider: Arc<dyn StreamProvider>,
        shared: Shared,
    ) -> Result<Self, PlaybackError> {
        Ok(Self::Remote(Remote::new::<super::mpd::Device>(
            client, provider, shared,
        )?))
    }
    pub fn snapshot(&self) -> PlaybackState {
        match self {
            Self::Local { inner, .. } => inner.snapshot(),
            Self::Remote(p) => p.state.borrow().clone(),
        }
    }
    pub fn subscribe(&self) -> watch::Receiver<PlaybackState> {
        match self {
            Self::Local { inner, .. } => inner.subscribe(),
            Self::Remote(p) => p.state.subscribe(),
        }
    }
    pub fn load(&self, id: MediaId, play: bool) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.load(id, play),
            Self::Remote(p) => p.send(Command::Load(id, play)),
        }
    }
    pub fn pause(&self) {
        match self {
            Self::Local { inner, airplay } => {
                let position = inner.snapshot().position_ms;
                inner.pause();
                if *airplay {
                    let _ = inner.seek(position);
                }
            }
            Self::Remote(p) => {
                let _ = p.send(Command::Pause);
            }
        }
    }
    pub fn play(&self) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.play(),
            Self::Remote(p) => p.send(Command::Play),
        }
    }
    pub fn seek(&self, ms: u64) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.seek(ms),
            Self::Remote(p) => p.send(Command::Seek(ms)),
        }
    }
    pub fn stop(&self) {
        match self {
            Self::Local { inner, .. } => inner.stop(),
            Self::Remote(p) => {
                let _ = p.send(Command::Stop);
            }
        }
    }
    pub fn set_volume(&self, v: u16) {
        match self {
            Self::Local { inner, .. } => inner.set_volume(v),
            Self::Remote(p) => {
                let _ = p.send(Command::Volume(v));
            }
        }
    }
    pub async fn shutdown(self) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.shutdown().await,
            Self::Remote(p) => p.shutdown().await,
        }
    }
}
enum Command {
    Load(MediaId, bool),
    Pause,
    Play,
    Seek(u64),
    Volume(u16),
    Stop,
}

/// A network receiver that plays a media lease URL and reports progress.
/// Every call that returns state applies the receiver's own reply to it;
/// an error means the receiver failed, stopped trusting us or was taken over.
pub(super) trait Device: Sized {
    /// Shown in status messages, e.g. "Playing on MPD".
    const NAME: &'static str;
    fn connect(shared: &Shared, deadline: Instant) -> Result<Self, ()>;
    fn local_ip(&self) -> Result<IpAddr, ()>;
    fn peer_ip(&self) -> Result<IpAddr, ()>;
    fn volume(&mut self, volume: u16) -> Result<(), ()>;
    /// Plays `url` from `state.position_ms`.
    fn load(
        &mut self,
        url: &str,
        mime: &str,
        track: &libresonic::Track,
        state: &mut PlaybackState,
    ) -> Result<(), ()>;
    /// Whether a song is loaded on the receiver for transport commands.
    fn loaded(&self) -> bool;
    fn pause(&mut self, state: &mut PlaybackState) -> Result<(), ()>;
    fn resume(&mut self, state: &mut PlaybackState) -> Result<(), ()>;
    /// Seeks to `state.position_ms`, staying paused when `state.phase` is.
    fn seek(&mut self, state: &mut PlaybackState) -> Result<(), ()>;
    fn status(&mut self, state: &mut PlaybackState) -> Result<(), ()>;
    /// Stops and forgets the loaded song; best effort.
    fn stop(&mut self);
}

/// Google Cast's Default Media Receiver, after device authentication or the pin.
struct Cast {
    connection: MediaConnection,
    url: String,
    session: Option<u64>,
}
impl Device for Cast {
    const NAME: &'static str = "Chromecast";
    fn connect(shared: &Shared, deadline: Instant) -> Result<Self, ()> {
        let pin = shared.settings().certificate_sha256;
        let connection = shared
            .targets()
            .into_iter()
            .find_map(|target| MediaConnection::connect(target, &pin, deadline).ok())
            .ok_or(())?;
        Ok(Self {
            connection,
            url: String::new(),
            session: None,
        })
    }
    fn local_ip(&self) -> Result<IpAddr, ()> {
        self.connection.local_ip().map_err(|_| ())
    }
    fn peer_ip(&self) -> Result<IpAddr, ()> {
        self.connection.peer_ip().map_err(|_| ())
    }
    fn volume(&mut self, volume: u16) -> Result<(), ()> {
        self.connection.volume(volume).map_err(|_| ())
    }
    fn load(
        &mut self,
        url: &str,
        mime: &str,
        track: &libresonic::Track,
        state: &mut PlaybackState,
    ) -> Result<(), ()> {
        self.url = url.to_owned();
        let status=self.connection.media(json!({"type":"LOAD","autoplay":true,"currentTime":state.position_ms as f64/1000.0,"media":{"contentId":url,"contentType":mime,"streamType":"BUFFERED","metadata":{"metadataType":3,"title":track.title,"artist":track.artists.iter().map(|a|a.name.as_str()).collect::<Vec<_>>().join(", "),"albumName":track.album.as_ref().map(|a|a.title.as_str()).unwrap_or("")}}})).map_err(|_|())?;
        self.session = status["status"]
            .as_array()
            .and_then(|v| v.first())
            .and_then(|s| s["mediaSessionId"].as_u64());
        if self.session.is_none() {
            return Err(());
        }
        apply_status(state, &status, &self.url)
    }
    fn loaded(&self) -> bool {
        self.session.is_some()
    }
    fn pause(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.request(state, |id| json!({"type":"PAUSE","mediaSessionId":id}))
    }
    fn resume(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.request(state, |id| json!({"type":"PLAY","mediaSessionId":id}))
    }
    fn seek(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        let seconds = state.position_ms as f64 / 1000.0;
        let resume = if state.phase == Phase::Paused {
            "PLAYBACK_PAUSE"
        } else {
            "PLAYBACK_START"
        };
        self.request(state, |id| json!({"type":"SEEK","mediaSessionId":id,"currentTime":seconds,"resumeState":resume}))
    }
    fn status(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        let status = self
            .connection
            .media(json!({"type":"GET_STATUS"}))
            .map_err(|_| ())?;
        apply_status(state, &status, &self.url)
    }
    fn stop(&mut self) {
        if let Some(id) = self.session.take() {
            let _ = self
                .connection
                .media(json!({"type":"STOP","mediaSessionId":id}));
        }
        self.url.clear();
    }
}
impl Cast {
    fn request(
        &mut self,
        state: &mut PlaybackState,
        message: impl FnOnce(u64) -> Value,
    ) -> Result<(), ()> {
        let Some(id) = self.session else {
            return Ok(());
        };
        let status = self.connection.media(message(id)).map_err(|_| ())?;
        apply_status(state, &status, &self.url)
    }
}

pub struct Remote {
    commands: Option<mpsc::SyncSender<(u64, Command)>>,
    epoch: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    state: watch::Sender<PlaybackState>,
    task: Option<std::thread::JoinHandle<()>>,
}
impl Remote {
    fn new<D: Device + 'static>(
        client: Arc<Client>,
        provider: Arc<dyn StreamProvider>,
        shared: Shared,
    ) -> Result<Self, PlaybackError> {
        let (commands, rx) = mpsc::sync_channel(32);
        let (state, _) = watch::channel(PlaybackState::default());
        let updates = state.clone();
        let epoch = Arc::new(AtomicU64::new(0));
        let active = epoch.clone();
        let closed = Arc::new(AtomicBool::new(false));
        let closing = closed.clone();
        let runtime = tokio::runtime::Handle::current();
        let task = std::thread::Builder::new()
            .name("remote-media".into())
            .spawn(move || {
                run::<D>(
                    rx,
                    updates,
                    client,
                    provider,
                    shared,
                    runtime,
                    Control {
                        epoch: active,
                        closed: closing,
                    },
                )
            })
            .map_err(|_| PlaybackError::Worker)?;
        Ok(Self {
            commands: Some(commands),
            epoch,
            closed,
            state,
            task: Some(task),
        })
    }
    fn send(&self, command: Command) -> Result<(), PlaybackError> {
        let volume = matches!(command, Command::Volume(_));
        let epoch = if volume {
            self.epoch.load(Ordering::SeqCst)
        } else {
            self.epoch.fetch_add(1, Ordering::SeqCst) + 1
        };
        self.commands
            .as_ref()
            .ok_or(PlaybackError::Worker)?
            .try_send((epoch, command))
            .map_err(|_| PlaybackError::Worker)
    }
    async fn shutdown(mut self) -> Result<(), PlaybackError> {
        self.closed.store(true, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.commands.take();
        if let Some(task) = self.task.take() {
            tokio::task::spawn_blocking(move || task.join())
                .await
                .map_err(|_| PlaybackError::Worker)?
                .map_err(|_| PlaybackError::Worker)?;
        }
        Ok(())
    }
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        self.commands.take();
    }
}
struct Transport<D> {
    device: D,
    server: Server,
}
impl<D: Device> Transport<D> {
    fn stop(&mut self) {
        self.device.stop();
        self.server.revoke();
    }
}
struct Control {
    epoch: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
}
fn run<D: Device>(
    rx: mpsc::Receiver<(u64, Command)>,
    updates: watch::Sender<PlaybackState>,
    client: Arc<Client>,
    provider: Arc<dyn StreamProvider>,
    shared: Shared,
    runtime: tokio::runtime::Handle,
    control: Control,
) {
    let Control { epoch, closed } = control;
    let mut state = PlaybackState::default();
    let mut transport: Option<Transport<D>> = None;
    let mut loaded = false;
    loop {
        if closed.load(Ordering::SeqCst) {
            break;
        }
        let command = match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let active = match &command {
            Some((id, _)) => *id,
            None => epoch.load(Ordering::SeqCst),
        };
        let current = || active == epoch.load(Ordering::SeqCst);
        let command = command.map(|(_, c)| c);
        let result = (|| -> Result<(), ()> {
            match command {
                Some(Command::Load(id, play)) => {
                    if let Some(t) = &mut transport {
                        t.stop();
                    }
                    loaded = false;
                    state.track = Some(id);
                    state.position_ms = 0;
                    state.duration_ms = None;
                    state.generation = state.generation.wrapping_add(1);
                    state.error = None;
                    state.phase = if play { Phase::Loading } else { Phase::Paused };
                }
                Some(Command::Volume(volume)) => {
                    state.volume = volume;
                    if let Some(t) = &mut transport {
                        t.device.volume(volume)?;
                    }
                }
                Some(Command::Stop) => {
                    if let Some(t) = &mut transport {
                        t.stop();
                    }
                    loaded = false;
                    state.generation = state.generation.wrapping_add(1);
                    state.phase = Phase::Stopped;
                    state.position_ms = 0;
                    state.error = None;
                }
                Some(Command::Pause) => {
                    if current()
                        && let Some(t) = &mut transport
                        && t.device.loaded()
                    {
                        t.device.pause(&mut state)?;
                    }
                    state.phase = Phase::Paused;
                }
                Some(Command::Seek(ms)) => {
                    if matches!(state.phase, Phase::Ended | Phase::Error) {
                        if let Some(t) = &mut transport {
                            t.stop();
                        }
                        loaded = false;
                        state.phase = Phase::Loading;
                        state.error = None;
                    }
                    state.position_ms = state.duration_ms.map_or(ms, |d| ms.min(d));
                    state.generation = state.generation.wrapping_add(1);
                    if current()
                        && let Some(t) = &mut transport
                        && t.device.loaded()
                    {
                        t.device.seek(&mut state)?;
                    }
                }
                Some(Command::Play) => {
                    if state.track.is_none() {
                        return Err(());
                    }
                    if matches!(state.phase, Phase::Ended | Phase::Stopped | Phase::Error) {
                        if let Some(t) = &mut transport {
                            t.stop();
                        }
                        loaded = false;
                        state.position_ms = 0;
                        state.generation = state.generation.wrapping_add(1);
                    }
                    state.error = None;
                    if loaded {
                        if current()
                            && let Some(t) = &mut transport
                            && t.device.loaded()
                        {
                            t.device.resume(&mut state)?;
                        }
                    } else {
                        state.phase = Phase::Loading;
                    }
                }
                None => {
                    if loaded && let Some(t) = &mut transport {
                        t.device.status(&mut state)?;
                    }
                }
            }
            if closed.load(Ordering::SeqCst) || !current() {
                return Ok(());
            }
            if !loaded && state.phase == Phase::Loading {
                updates.send_replace(state.clone());
                shared.report(&format!("Connecting to {}", D::NAME));
                let track = runtime.block_on(async {
                    tokio::time::timeout(
                        Duration::from_secs(3),
                        client.song(state.track.as_ref().ok_or(())?),
                    )
                    .await
                    .map_err(|_| ())?
                    .map_err(|_| ())
                })?;
                if transport.is_none() {
                    let device = D::connect(&shared, Instant::now() + Duration::from_secs(3))?;
                    let local = device.local_ip()?;
                    let peer = device.peer_ip()?;
                    let server = runtime
                        .block_on(Server::new(provider.clone(), local, peer))
                        .map_err(|_| ())?;
                    transport = Some(Transport { device, server });
                }
                if closed.load(Ordering::SeqCst) || !current() {
                    return Ok(());
                }
                let t = transport.as_mut().unwrap();
                let mime = mime(&track);
                let url = t.server.issue(track.id.clone(), mime.clone());
                state.duration_ms = track.duration_ms;
                t.device.volume(state.volume)?;
                t.device.load(&url, &mime, &track, &mut state)?;
                loaded = true;
            }
            Ok(())
        })();
        if closed.load(Ordering::SeqCst) || !current() {
            continue;
        }
        if result.is_err() {
            if let Some(t) = &mut transport {
                t.server.revoke();
            }
            transport = None;
            loaded = false;
            state.phase = Phase::Error;
            state.error = Some(PlaybackError::Output(
                libresonic::playback::OutputError::DeviceLost,
            ));
            shared.report(&format!(
                "{} stopped; check receiver, codec and firewall, then press Play",
                D::NAME
            ));
        } else {
            shared.report(&match state.phase {
                Phase::Playing => {
                    format!("Playing on {} (receiver decoding; EQ unavailable)", D::NAME)
                }
                Phase::Paused => format!("{} paused", D::NAME),
                Phase::Ended => format!("{} track finished", D::NAME),
                Phase::Stopped => format!("{} stopped", D::NAME),
                _ => format!("{} buffering", D::NAME),
            });
        }
        updates.send_replace(state.clone());
    }
    if let Some(t) = &mut transport {
        t.stop();
    }
}
fn mime(track: &libresonic::Track) -> String {
    match track
        .suffix
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "mp4" => "audio/mp4",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
    .into()
}
fn apply_status(state: &mut PlaybackState, message: &Value, url: &str) -> Result<(), ()> {
    let s = message["status"]
        .as_array()
        .and_then(|v| v.first())
        .ok_or(())?;
    if s["media"]["contentId"].as_str().is_some_and(|id| id != url) {
        return Err(());
    }
    if let Some(seconds) = s["currentTime"]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0 && *v < 86400.0)
    {
        state.position_ms = (seconds * 1000.0) as u64;
    }
    state.phase = match s["playerState"].as_str() {
        Some("PLAYING") => Phase::Playing,
        Some("PAUSED") => Phase::Paused,
        Some("BUFFERING") => Phase::Buffering,
        Some("IDLE") if s["idleReason"] == "FINISHED" => Phase::Ended,
        _ => return Err(()),
    };
    if state.phase == Phase::Ended {
        state.position_ms = state.duration_ms.unwrap_or(state.position_ms);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_reports_drive_progress_pause_end_and_reject_takeover_or_errors() {
        let mut state = PlaybackState::default();
        let status = |phase: &str, time: f64, reason: &str, url: &str| json!({"status":[{"playerState":phase,"currentTime":time,"idleReason":reason,"media":{"contentId":url}}]});
        apply_status(
            &mut state,
            &status("PLAYING", 12.5, "", "private-lease"),
            "private-lease",
        )
        .unwrap();
        assert_eq!(state.phase, Phase::Playing);
        assert_eq!(state.position_ms, 12500);
        apply_status(
            &mut state,
            &status("PAUSED", 15.0, "", "private-lease"),
            "private-lease",
        )
        .unwrap();
        assert_eq!(state.phase, Phase::Paused);
        assert!(
            apply_status(
                &mut state,
                &status("PLAYING", 0.0, "", "another-app"),
                "private-lease"
            )
            .is_err()
        );
        assert!(
            apply_status(
                &mut state,
                &status("IDLE", 15.0, "ERROR", "private-lease"),
                "private-lease"
            )
            .is_err()
        );
        apply_status(
            &mut state,
            &status("IDLE", 15.0, "FINISHED", "private-lease"),
            "private-lease",
        )
        .unwrap();
        assert_eq!(state.phase, Phase::Ended);
        assert!(apply_status(&mut state, &json!({"status":[]}), "private-lease").is_err());
    }
}
