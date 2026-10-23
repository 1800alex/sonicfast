use super::{Shared, media::Server};
use crate::cast::MediaConnection;
use libresonic::{
    MediaId, MusicService, StreamProvider,
    opensubsonic::Client,
    playback::{Phase, PlaybackError, PlaybackState, Player as Local},
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tokio::sync::watch;

pub enum Player {
    Local { inner: Local, airplay: bool },
    Cast(Remote),
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
        Ok(Self::Cast(Remote::new(client, provider, shared)?))
    }
    pub fn snapshot(&self) -> PlaybackState {
        match self {
            Self::Local { inner, .. } => inner.snapshot(),
            Self::Cast(p) => p.state.borrow().clone(),
        }
    }
    pub fn subscribe(&self) -> watch::Receiver<PlaybackState> {
        match self {
            Self::Local { inner, .. } => inner.subscribe(),
            Self::Cast(p) => p.state.subscribe(),
        }
    }
    pub fn load(&self, id: MediaId, play: bool) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.load(id, play),
            Self::Cast(p) => p.send(Command::Load(id, play)),
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
            Self::Cast(p) => {
                let _ = p.send(Command::Pause);
            }
        }
    }
    pub fn play(&self) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.play(),
            Self::Cast(p) => p.send(Command::Play),
        }
    }
    pub fn seek(&self, ms: u64) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.seek(ms),
            Self::Cast(p) => p.send(Command::Seek(ms)),
        }
    }
    pub fn stop(&self) {
        match self {
            Self::Local { inner, .. } => inner.stop(),
            Self::Cast(p) => {
                let _ = p.send(Command::Stop);
            }
        }
    }
    pub fn set_volume(&self, v: u16) {
        match self {
            Self::Local { inner, .. } => inner.set_volume(v),
            Self::Cast(p) => {
                let _ = p.send(Command::Volume(v));
            }
        }
    }
    pub async fn shutdown(self) -> Result<(), PlaybackError> {
        match self {
            Self::Local { inner, .. } => inner.shutdown().await,
            Self::Cast(p) => p.shutdown().await,
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
pub struct Remote {
    commands: Option<mpsc::SyncSender<(u64, Command)>>,
    epoch: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    state: watch::Sender<PlaybackState>,
    task: Option<std::thread::JoinHandle<()>>,
}
impl Remote {
    fn new(
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
            .name("cast-media".into())
            .spawn(move || {
                run(
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
struct Transport {
    connection: MediaConnection,
    server: Server,
    url: String,
    session: Option<u64>,
}
impl Transport {
    fn stop(&mut self) {
        if let Some(id) = self.session.take() {
            let _ = self
                .connection
                .media(json!({"type":"STOP","mediaSessionId":id}));
        }
        self.server.revoke();
        self.url.clear();
    }
}
struct Control {
    epoch: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
}
fn run(
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
    let mut transport: Option<Transport> = None;
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
            Some((id, Command::Volume(_))) => *id,
            Some((id, _)) => *id,
            None => epoch.load(Ordering::SeqCst),
        };
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
                        t.connection.volume(volume).map_err(|_| ())?;
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
                    if active == epoch.load(Ordering::SeqCst)
                        && let Some(t) = &mut transport
                        && let Some(id) = t.session
                    {
                        let status = t
                            .connection
                            .media(json!({"type":"PAUSE","mediaSessionId":id}))
                            .map_err(|_| ())?;
                        apply_status(&mut state, &status, &t.url)?;
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
                    if active == epoch.load(Ordering::SeqCst)
                        && let Some(t) = &mut transport
                        && let Some(id) = t.session
                    {
                        let status=t.connection.media(json!({"type":"SEEK","mediaSessionId":id,"currentTime":state.position_ms as f64/1000.0,"resumeState":if state.phase==Phase::Paused{"PLAYBACK_PAUSE"}else{"PLAYBACK_START"}})).map_err(|_|())?;
                        apply_status(&mut state, &status, &t.url)?;
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
                        if active == epoch.load(Ordering::SeqCst)
                            && let Some(t) = &mut transport
                            && let Some(id) = t.session
                        {
                            let status = t
                                .connection
                                .media(json!({"type":"PLAY","mediaSessionId":id}))
                                .map_err(|_| ())?;
                            apply_status(&mut state, &status, &t.url)?;
                        }
                    } else {
                        state.phase = Phase::Loading;
                    }
                }
                None => {
                    if loaded && let Some(t) = &mut transport {
                        let status = t
                            .connection
                            .media(json!({"type":"GET_STATUS"}))
                            .map_err(|_| ())?;
                        apply_status(&mut state, &status, &t.url)?;
                    }
                }
            }
            if closed.load(Ordering::SeqCst) || active != epoch.load(Ordering::SeqCst) {
                return Ok(());
            }
            if !loaded && state.phase == Phase::Loading {
                updates.send_replace(state.clone());
                shared.report("Connecting to Chromecast");
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
                    let targets = shared.targets();
                    let until = std::time::Instant::now() + Duration::from_secs(3);
                    let connection = targets
                        .into_iter()
                        .find_map(|target| {
                            MediaConnection::connect(
                                target,
                                &shared.settings().certificate_sha256,
                                until,
                            )
                            .ok()
                        })
                        .ok_or(())?;
                    let local = connection.local_ip().map_err(|_| ())?;
                    let peer = connection.peer_ip().map_err(|_| ())?;
                    let server = runtime
                        .block_on(Server::new(provider.clone(), local, peer))
                        .map_err(|_| ())?;
                    transport = Some(Transport {
                        connection,
                        server,
                        url: String::new(),
                        session: None,
                    });
                }
                if closed.load(Ordering::SeqCst) || active != epoch.load(Ordering::SeqCst) {
                    return Ok(());
                }
                let t = transport.as_mut().unwrap();
                t.url = t.server.issue(track.id.clone(), mime(&track));
                state.duration_ms = track.duration_ms;
                t.connection.volume(state.volume).map_err(|_| ())?;
                let status=t.connection.media(json!({"type":"LOAD","autoplay":true,"currentTime":state.position_ms as f64/1000.0,"media":{"contentId":t.url,"contentType":mime(&track),"streamType":"BUFFERED","metadata":{"metadataType":3,"title":track.title,"artist":track.artists.iter().map(|a|a.name.as_str()).collect::<Vec<_>>().join(", "),"albumName":track.album.as_ref().map(|a|a.title.as_str()).unwrap_or("")}}})).map_err(|_|())?;
                t.session = status["status"]
                    .as_array()
                    .and_then(|v| v.first())
                    .and_then(|s| s["mediaSessionId"].as_u64());
                if t.session.is_none() {
                    return Err(());
                }
                apply_status(&mut state, &status, &t.url)?;
                loaded = true;
            }
            Ok(())
        })();
        if closed.load(Ordering::SeqCst) || active != epoch.load(Ordering::SeqCst) {
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
            shared
                .report("Chromecast stopped; check receiver, codec and firewall, then press Play");
        } else {
            shared.report(match state.phase {
                Phase::Playing => "Playing on Chromecast (receiver decoding; EQ unavailable)",
                Phase::Paused => "Chromecast paused",
                Phase::Ended => "Chromecast track finished",
                Phase::Stopped => "Chromecast stopped",
                _ => "Chromecast buffering",
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
