//! Snapserver PCM source and built-in synchronized Snapcast receiver.
//! These transports are independent of OpenSubsonic authentication.
pub mod native;
mod output;
mod protocol;
mod receiver;
pub use output::{RoutedOutput, Sender};
pub use receiver::Manager;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub send: bool,
    pub receive: bool,
    pub host: String,
    pub source_port: u16,
    pub stream_port: u16,
    pub allow_insecure: bool,
    pub receiver_device: String,
    pub latency_ms: i32,
    pub client_id: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            send: false,
            receive: false,
            host: "127.0.0.1".into(),
            source_port: 4953,
            stream_port: 1704,
            allow_insecure: false,
            receiver_device: String::new(),
            latency_ms: 0,
            client_id: String::new(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.send && !self.receive {
            return Ok(());
        }
        if !self.allow_insecure {
            return Err(
                "Allow unencrypted Snapcast on your trusted network before enabling it".into(),
            );
        }
        let host = self.host.as_str();
        if host.is_empty()
            || host.len() > 253
            || host.starts_with('-')
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))
            || (host.contains(':') && host.parse::<std::net::Ipv6Addr>().is_err())
        {
            return Err(
                "Enter a Snapcast hostname or IP address without a URL, port or credentials".into(),
            );
        }
        if self.source_port == 0 || self.stream_port == 0 {
            return Err("Snapcast ports must be between 1 and 65535".into());
        }
        if self.client_id.len() > 64
            || !self
                .client_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err("Receiver ID must be blank or use up to 64 letters, numbers, dots, underscores or hyphens".into());
        }
        if self.receiver_device.len() > 1024 || self.receiver_device.contains(['\n', '\r', '\0']) {
            return Err("Receiver device contains invalid characters or is too long".into());
        }
        if !(-1000..=1000).contains(&self.latency_ms) {
            return Err("Receiver latency correction must be between -1000 and 1000 ms".into());
        }
        Ok(())
    }
    pub fn receiver_url(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("tcp://{host}:{}", self.stream_port)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub sender: String,
    pub receiver: String,
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    settings: Settings,
    targets: Vec<SocketAddr>,
    status: Status,
}
impl Shared {
    pub fn settings(&self) -> Settings {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .settings
            .clone()
    }
    pub fn status(&self) -> Status {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
            .clone()
    }
    fn targets(&self) -> Vec<SocketAddr> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .targets
            .clone()
    }
    fn sender_status(&self, message: &str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
            .sender = message.into();
    }
    fn receiver_status(&self, message: &str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
            .receiver = message.into();
    }
    /// A resolved address, used by diagnostic harnesses and the manager.
    pub fn resolved(settings: Settings, target: SocketAddr) -> Result<Self, String> {
        settings.validate()?;
        Ok(Self(Arc::new(Mutex::new(State {
            settings,
            targets: vec![target],
            status: Status::default(),
        }))))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_are_disabled_and_explicit_consent_is_required() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
        settings.send = true;
        assert!(settings.validate().unwrap_err().contains("unencrypted"));
        settings.allow_insecure = true;
        assert!(settings.validate().is_ok());
    }
    #[test]
    fn endpoints_cannot_contain_urls_credentials_flags_or_zero_ports() {
        for host in [
            "",
            "tcp://host",
            "user:secret@host",
            "host:1704",
            "--help",
            "host\nfoo",
            "x/y",
        ] {
            let settings = Settings {
                send: true,
                allow_insecure: true,
                host: host.into(),
                ..Default::default()
            };
            assert!(settings.validate().is_err(), "{host}");
        }
        let mut settings = Settings {
            receive: true,
            allow_insecure: true,
            host: "::1".into(),
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        assert_eq!(settings.receiver_url(), "tcp://[::1]:1704");
        settings.source_port = 0;
        assert!(settings.validate().is_err());
    }
}
