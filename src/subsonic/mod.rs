//! Sonicfast's native OpenSubsonic backend and compatibility boundary.
pub mod adapter;
pub mod downloads;
mod playlists;
pub mod profile;
mod scrobbling;
#[cfg(test)]
mod test_server;
pub mod ui;
mod worker;
use crate::{
    api::models,
    backend,
    player::{LoadSpec, PlayerCommand},
};

pub enum Request {
    Login(profile::Profile, String),
    Disconnect,
    RemoveProfile(String),
    Sync,
    Snapcast(crate::snapcast::Settings),
    Casting(crate::casting::Settings),
    Route {
        output: crate::casting::Settings,
        snapcast: crate::snapcast::Settings,
    },
    DiscoverOutputs,
    PairAirPlay {
        settings: crate::casting::Settings,
        force: bool,
    },
    SubmitAirPlayPin {
        attempt: u64,
        pin: crate::casting::airplay::pairing::Pin,
    },
    CancelAirPlayPairing(u64),
    CheckCast {
        attempt: u64,
        settings: crate::casting::Settings,
    },
    Configure {
        scrobble: libresonic::playback::scrobble::ScrobbleSettings,
        downloads: downloads::Settings,
    },
    Download(Vec<String>),
    ProbeDownloads(Vec<crate::api::models::Track>),
    DownloadControl {
        uri: Option<String>,
        action: String,
    },
    Load(LoadSpec),
    Transport(PlayerCommand),
    Queue(QueueAction),
    CreatePlaylist {
        name: String,
        public: bool,
        uris: Vec<String>,
    },
    RateSong {
        uri: String,
        rating: u8,
    },
    PlaylistRows {
        id: String,
        rows: Vec<(u32, String)>,
        revision: Option<String>,
    },
}
pub enum QueueAction {
    Add(Vec<String>),
    Insert(usize, Vec<String>),
    Remove(Vec<(usize, String)>),
    Move(usize, usize),
    Clear,
    Skip(usize),
}
pub enum Event {
    AirPlayPairing(crate::casting::airplay::pairing::Status),
    CastingStatus(String),
    CastCheck {
        attempt: u64,
        trust: crate::casting::Trust,
    },
    Receivers(Result<Vec<crate::casting::Receiver>, String>),
    Snapcast(crate::snapcast::Status),
    LibraryRefresh,
    ProfileRemoving {
        id: String,
        busy: bool,
    },
    Profiles(profile::Profiles),
    Downloads {
        epoch: u64,
        update: downloads::Update,
    },
    Notice(String),
    PlaylistChanged(String),
    SongRated {
        uri: String,
        rating: u8,
        result: Result<(), String>,
    },
    Queue {
        queue: Box<models::Queue>,
        manual: Vec<String>,
        source: Option<String>,
        current_index: Option<usize>,
    },
}
pub use worker::run;
pub fn enabled() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_stem()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case("sonicfast"))
        })
        .unwrap_or(false)
}

pub fn send(backend: &backend::Backend, request: Request) {
    backend.send(backend::Command::Subsonic(request));
}
