//! App-owned LAN audio outputs; music-server authentication stays in Libresonic.
pub mod airplay;
mod discovery;
mod media;
mod player;
pub use discovery::discover;
pub use player::Player;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    #[default]
    Local,
    AirPlay,
    Chromecast,
    Snapcast,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::AirPlay => "AirPlay",
            Self::Chromecast => "Chromecast",
            Self::Snapcast => "Snapcast",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// None migrates the pre-picker Snapcast Send setting.
    pub output: Option<Kind>,
    pub host: String,
    pub port: u16,
    pub name: String,
    pub allow_lan: bool,
    /// Explicit identity pin for a self-hosted Cast receiver. Blank uses Google device authentication.
    pub certificate_sha256: String,
    /// Authenticated AirPlay transport, negotiated by newer audio receivers.
    pub airplay2: bool,
}
impl Settings {
    pub fn kind(&self, snapcast_send: bool) -> Kind {
        self.output.unwrap_or(if snapcast_send {
            Kind::Snapcast
        } else {
            Kind::Local
        })
    }
    pub fn playback_warning(&self, snapcast: &crate::snapcast::Settings) -> Option<&'static str> {
        (snapcast.receive && self.kind(snapcast.send) == Kind::Local).then_some(
            "Snapcast Receive is active. Local songs are paused; enable Send as well, turn off Receive, or choose another output to play your queue.")
    }
    pub fn validate_with_snapcast(
        &self,
        snapcast: &crate::snapcast::Settings,
    ) -> Result<(), String> {
        self.validate()?;
        if self.kind(snapcast.send) == Kind::Snapcast {
            let mut sending = snapcast.clone();
            sending.send = true;
            sending.validate()?;
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.output, Some(Kind::AirPlay | Kind::Chromecast)) {
            return Ok(());
        }
        if !self.allow_lan {
            return Err("Allow casting on your trusted network first".into());
        }
        let host = &self.host;
        if host.is_empty()
            || host.len() > 253
            || host.starts_with('-')
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))
            || (host.contains(':') && host.parse::<std::net::Ipv6Addr>().is_err())
            || self.port == 0
        {
            return Err(
                "Enter a receiver hostname/IP and port without a URL or credentials".into(),
            );
        }
        if self.name.len() > 256 || self.name.chars().any(char::is_control) {
            return Err("Receiver name is invalid".into());
        }
        if !self.certificate_sha256.is_empty()
            && (self.certificate_sha256.len() != 64
                || !self
                    .certificate_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit()))
        {
            return Err(
                "Receiver certificate fingerprint must be 64 hexadecimal characters".into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receiver {
    pub kind: Kind,
    pub name: String,
    pub address: SocketAddr,
    pub supported: bool,
    pub pairing: bool,
    pub airplay2: bool,
    pub reason: Option<String>,
}
/// Track text and cover shown on the receiver's own display.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
}
/// Heard position as of `at`, tagged with the player's load/seek generation.
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub generation: u64,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub at: std::time::Instant,
}
impl Progress {
    pub fn position_now(&self) -> u64 {
        let moved = if self.playing {
            self.at.elapsed().as_millis() as u64
        } else {
            0
        };
        self.position_ms.saturating_add(moved).min(self.duration_ms)
    }
}
/// What an AirPlay session has to mirror; `revision` changes with the text.
#[derive(Clone, Default)]
pub struct Metadata {
    pub revision: u64,
    pub now: NowPlaying,
    /// JPEG for `now.art_url`, keyed by that URL.
    pub artwork: Option<(String, Arc<[u8]>)>,
    pub progress: Option<Progress>,
}
#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    settings: Settings,
    targets: Vec<SocketAddr>,
    status: String,
    pairing: airplay::pairing::State,
    metadata: Metadata,
    art_requested: Option<String>,
}
impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub fn settings(&self) -> Settings {
        self.lock().settings.clone()
    }
    pub fn targets(&self) -> Vec<SocketAddr> {
        self.lock().targets.clone()
    }
    pub fn status(&self) -> String {
        self.lock().status.clone()
    }
    pub fn report(&self, status: &str) {
        self.lock().status = status.into();
    }
    pub fn metadata(&self) -> Metadata {
        self.lock().metadata.clone()
    }
    pub fn set_now_playing(&self, now: NowPlaying) {
        let mut state = self.lock();
        if state.metadata.now != now {
            state.metadata.now = now;
            state.metadata.revision += 1;
        }
    }
    pub fn set_progress(&self, progress: Progress) {
        self.lock().metadata.progress = Some(progress);
    }
    /// The cover URL to fetch for an AirPlay receiver, returned once per URL.
    pub fn wants_artwork(&self) -> Option<String> {
        let mut state = self.lock();
        let url = state.metadata.now.art_url.clone()?;
        let held = state.metadata.artwork.as_ref().map(|(u, _)| u) == Some(&url);
        if state.settings.output != Some(Kind::AirPlay)
            || held
            || state.art_requested.as_ref() == Some(&url)
        {
            return None;
        }
        state.art_requested = Some(url.clone());
        Some(url)
    }
    /// Stores a fetched cover unless the track moved on while it loaded.
    pub fn set_artwork(&self, url: String, jpeg: Arc<[u8]>) {
        let mut state = self.lock();
        if state.metadata.now.art_url.as_ref() == Some(&url) {
            state.metadata.artwork = Some((url, jpeg));
        }
    }
    pub async fn configure(&self, settings: Settings) -> Result<(), String> {
        settings.validate()?;
        let targets = if matches!(settings.output, Some(Kind::AirPlay | Kind::Chromecast)) {
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                tokio::net::lookup_host((settings.host.as_str(), settings.port)),
            )
            .await
            .ok()
            .and_then(Result::ok)
            .map(|v| v.take(8).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut state = self.lock();
        state.settings = settings;
        state.targets = targets;
        // A failed or skipped cover fetch may be retried on the new output.
        state.art_requested = None;
        state.status = if matches!(
            state.settings.output,
            Some(Kind::AirPlay | Kind::Chromecast)
        ) && state.targets.is_empty()
        {
            "Receiver address unavailable; check the address or reconnect"
        } else {
            "Ready"
        }
        .into();
        drop(state);
        self.restore_pairing();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_migrate_snapcast_and_casting_rejects_credentials_without_consent() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.kind(true), Kind::Snapcast);
        assert_eq!(old.kind(false), Kind::Local);
        let mut s = Settings {
            output: Some(Kind::AirPlay),
            host: "garage.local".into(),
            port: 5000,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        s.allow_lan = true;
        assert!(s.validate().is_ok());
        for host in [
            "http://garage.local",
            "user:password@garage",
            "garage.local:5000",
            "garage\r\nAuthorization: secret",
        ] {
            s.host = host.into();
            assert!(s.validate().is_err());
        }
    }

    #[tokio::test]
    async fn now_playing_bumps_revision_and_artwork_is_requested_once_for_airplay() {
        let shared = Shared::default();
        let now = NowPlaying {
            title: "Song".into(),
            art_url: Some("sonic-art:1".into()),
            ..Default::default()
        };
        shared.set_now_playing(now.clone());
        let revision = shared.metadata().revision;
        shared.set_now_playing(now.clone());
        assert_eq!(shared.metadata().revision, revision);
        assert_eq!(shared.wants_artwork(), None, "Local output needs no cover");
        shared
            .configure(Settings {
                output: Some(Kind::AirPlay),
                host: "127.0.0.1".into(),
                port: 7000,
                allow_lan: true,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(shared.wants_artwork().as_deref(), Some("sonic-art:1"));
        assert_eq!(shared.wants_artwork(), None);
        shared.set_artwork("sonic-art:old".into(), Arc::from(&b"stale"[..]));
        assert!(shared.metadata().artwork.is_none());
        shared.set_artwork("sonic-art:1".into(), Arc::from(&b"jpeg"[..]));
        assert_eq!(&*shared.metadata().artwork.unwrap().1, b"jpeg");
        shared.set_now_playing(NowPlaying {
            title: "Next".into(),
            art_url: Some("sonic-art:2".into()),
            ..Default::default()
        });
        assert!(shared.metadata().revision > revision);
        assert_eq!(shared.wants_artwork().as_deref(), Some("sonic-art:2"));
    }
}
