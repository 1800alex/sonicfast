//! App-owned LAN audio outputs; music-server authentication stays in Libresonic.
pub mod airplay;
mod discovery;
mod media;
mod mpd;
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
    /// A Music Player Daemon that fetches and decodes the original (ADR 014).
    Mpd,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::AirPlay => "AirPlay",
            Self::Chromecast => "Chromecast",
            Self::Snapcast => "Snapcast",
            Self::Mpd => "MPD",
        }
    }
    /// A receiver on the trusted LAN, addressed by host and port.
    pub fn is_lan(self) -> bool {
        matches!(self, Self::AirPlay | Self::Chromecast | Self::Mpd)
    }
    /// The port a receiver of this kind normally listens on.
    pub fn default_port(self) -> u16 {
        match self {
            Self::AirPlay => 5000,
            Self::Mpd => 6600,
            _ => 8009,
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
    /// Fingerprints the user entered for self-hosted Cast receivers, by
    /// [`Settings::receiver_key`], reused when that receiver is chosen again.
    pub cast_fingerprints: std::collections::BTreeMap<String, String>,
    /// Pin a Cast receiver that is not genuine Google hardware to the
    /// certificate it presents the first time. A changed one still asks.
    pub trust_new_cast_receivers: bool,
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
            "Snapcast Receive is active. Local songs are paused; choose Snapcast or another output, or turn off Receive, to play your queue.")
    }
    /// Which remembered fingerprint belongs to this receiver: its name, which
    /// survives a new DHCP address, or its host when it has none. A wrong
    /// match only fails the pin check, so it can never trust a receiver.
    pub fn receiver_key(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            self.host.to_ascii_lowercase()
        } else {
            name.to_lowercase()
        }
    }
    /// The output for choosing `receiver`, with any fingerprint remembered for it.
    pub fn for_receiver(&self, receiver: &Receiver) -> Settings {
        let mut settings = Settings {
            output: Some(receiver.kind),
            host: receiver.address.ip().to_string(),
            port: receiver.address.port(),
            name: receiver.name.clone(),
            allow_lan: self.allow_lan,
            certificate_sha256: String::new(),
            airplay2: receiver.airplay2,
            cast_fingerprints: self.cast_fingerprints.clone(),
            trust_new_cast_receivers: self.trust_new_cast_receivers,
        };
        if receiver.kind == Kind::Chromecast
            && let Some(pin) = self.cast_fingerprints.get(&settings.receiver_key())
        {
            settings.certificate_sha256.clone_from(pin);
        }
        settings
    }
    /// Records (or, when blank, forgets) the current Cast receiver's fingerprint.
    pub fn remember_fingerprint(&mut self) {
        if self.output != Some(Kind::Chromecast) {
            return;
        }
        let key = self.receiver_key();
        if self.certificate_sha256.is_empty() {
            self.cast_fingerprints.remove(&key);
        } else if self.cast_fingerprints.len() < 64 || self.cast_fingerprints.contains_key(&key) {
            self.cast_fingerprints
                .insert(key, self.certificate_sha256.to_ascii_lowercase());
        }
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
        if !self.output.is_some_and(Kind::is_lan) {
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
/// A SHA-256 certificate fingerprint as pasted (any case, with or without
/// `:`/space separators), as 64 lowercase hex digits.
pub fn normalize_fingerprint(input: &str) -> Option<String> {
    let hex: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':')
        .collect::<String>()
        .to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// Whether a Cast receiver proved its identity before it was chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trust {
    /// A genuine Google Cast device, or the pinned certificate.
    Verified,
    /// It answered but did not prove its identity: not genuine without a pin,
    /// or a different certificate than the pinned one. `presented` is the
    /// SHA-256 of the certificate it showed.
    Untrusted {
        pinned: bool,
        presented: Option<String>,
    },
    Unreachable,
}
impl Trust {
    /// One answer for a receiver reachable at several addresses.
    pub fn combine(results: impl IntoIterator<Item = Trust>) -> Trust {
        results
            .into_iter()
            .fold(Trust::Unreachable, |best, next| match (&best, &next) {
                (Trust::Verified, _) | (_, Trust::Verified) => Trust::Verified,
                (Trust::Untrusted { .. }, _) => best,
                _ => next,
            })
    }
}

/// Connects to the chosen Cast receiver and checks its identity without
/// launching anything on it. Blocking; run off the UI thread.
pub fn check_cast(settings: &Settings) -> Trust {
    use std::net::ToSocketAddrs;
    if settings.validate().is_err() || settings.output != Some(Kind::Chromecast) {
        return Trust::Unreachable;
    }
    let Ok(targets) = (settings.host.as_str(), settings.port).to_socket_addrs() else {
        return Trust::Unreachable;
    };
    let pinned = !settings.certificate_sha256.is_empty();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    Trust::combine(targets.take(4).map(|target| {
        match crate::cast::MediaConnection::check(target, &settings.certificate_sha256, deadline) {
            Ok(()) => Trust::Verified,
            Err(error) => match error.downcast_ref::<crate::cast::Untrusted>() {
                Some(untrusted) => Trust::Untrusted {
                    pinned,
                    presented: untrusted.presented.clone(),
                },
                None => Trust::Unreachable,
            },
        }
    }))
}

/// The Cast receiver being checked before it becomes the output.
#[derive(Clone, Debug, Default)]
pub struct CastCheck {
    pub attempt: u64,
    pub settings: Settings,
    pub stage: CheckStage,
    /// The fingerprint the receiver presented when it was not verified.
    pub presented: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckStage {
    #[default]
    Idle,
    Checking,
    Untrusted {
        pinned: bool,
    },
    Unreachable,
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
        let targets = if settings.output.is_some_and(Kind::is_lan) {
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
        state.status =
            if state.settings.output.is_some_and(Kind::is_lan) && state.targets.is_empty() {
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
    fn cast(name: &str, host: &str) -> Receiver {
        Receiver {
            kind: Kind::Chromecast,
            name: name.into(),
            address: SocketAddr::new(host.parse().unwrap(), 8009),
            supported: true,
            pairing: false,
            airplay2: false,
            reason: None,
        }
    }
    #[test]
    fn an_mpd_output_needs_lan_consent_and_a_plain_host() {
        let mut mpd = Settings {
            output: Some(Kind::Mpd),
            host: "pi.local".into(),
            port: Kind::Mpd.default_port(),
            ..Default::default()
        };
        assert_eq!(mpd.port, 6600);
        assert!(mpd.validate().is_err(), "LAN casting must be allowed first");
        mpd.allow_lan = true;
        assert!(mpd.validate().is_ok());
        mpd.host = "pi.local\npassword x".into();
        assert!(mpd.validate().is_err());
    }
    #[test]
    fn pasted_fingerprints_normalize_to_lowercase_hex() {
        let hex = "ab".repeat(32);
        let colons = vec!["AB"; 32].join(":");
        assert_eq!(
            normalize_fingerprint(&colons).as_deref(),
            Some(hex.as_str())
        );
        assert_eq!(
            normalize_fingerprint(&format!("  {} ", "AB ".repeat(32))).as_deref(),
            Some(hex.as_str())
        );
        for bad in ["", "abc", &"zz".repeat(32), &"ab".repeat(33)] {
            assert_eq!(normalize_fingerprint(bad), None, "{bad}");
        }
    }
    #[test]
    fn a_remembered_fingerprint_follows_its_receiver_only() {
        let mut current = Settings {
            allow_lan: true,
            ..Default::default()
        };
        let den = current.for_receiver(&cast("Den Speaker", "192.0.2.5"));
        assert_eq!(den.output, Some(Kind::Chromecast));
        assert!(den.certificate_sha256.is_empty());
        let mut pinned = den.clone();
        pinned.certificate_sha256 = "a".repeat(64);
        pinned.remember_fingerprint();
        current.cast_fingerprints = pinned.cast_fingerprints.clone();
        // A new address (DHCP) keeps the pin; another receiver does not get it.
        let moved = current.for_receiver(&cast("den speaker", "192.0.2.9"));
        assert_eq!(moved.certificate_sha256, "a".repeat(64));
        assert!(
            current
                .for_receiver(&cast("Kitchen", "192.0.2.5"))
                .certificate_sha256
                .is_empty()
        );
        // Clearing the pin for a receiver forgets it.
        let mut cleared = moved;
        cleared.certificate_sha256.clear();
        cleared.remember_fingerprint();
        assert!(cleared.cast_fingerprints.is_empty());
    }
    #[test]
    fn any_verified_target_wins_and_untrusted_beats_unreachable() {
        use Trust::*;
        assert_eq!(Trust::combine([Unreachable, Verified]), Verified);
        let presented = Some("ab".repeat(32));
        assert_eq!(
            Trust::combine([
                Unreachable,
                Untrusted {
                    pinned: true,
                    presented: presented.clone()
                }
            ]),
            Untrusted {
                pinned: true,
                presented
            }
        );
        assert_eq!(Trust::combine([Unreachable]), Unreachable);
        assert_eq!(Trust::combine([]), Unreachable);
    }
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
