//! A minimal Music Player Daemon client for the MPD output (ADR 014).
//!
//! MPD cannot receive a live stream, so it plays the same per-track media
//! lease Cast uses: Sonicfast queues the lease URL, MPD fetches and decodes the
//! original, and transport/progress go over MPD's line protocol
//! (<https://mpd.readthedocs.io/en/latest/protocol.html>). Password-protected
//! servers are not supported yet; their refusal is reported, never retried.

use libresonic::playback::{Phase, PlaybackState};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    time::Instant,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// Not reachable, timed out, closed, or not speaking MPD.
    Connection,
    /// The server needs a password Sonicfast does not send.
    Permission,
    /// MPD's own refusal (its ACK message, never the URL we sent).
    Refused(String),
}

/// One MPD control connection. Generic so tests can script the server.
pub(crate) struct Client<S> {
    stream: BufReader<S>,
    version: String,
}
impl Client<TcpStream> {
    pub(crate) fn connect(target: SocketAddr, deadline: Instant) -> Result<Self, Error> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::Connection);
        }
        let stream =
            TcpStream::connect_timeout(&target, remaining).map_err(|_| Error::Connection)?;
        // Each request must answer quickly; MPD replies at once even while buffering.
        let timeout = Some(std::time::Duration::from_secs(3));
        stream
            .set_read_timeout(timeout)
            .map_err(|_| Error::Connection)?;
        stream
            .set_write_timeout(timeout)
            .map_err(|_| Error::Connection)?;
        stream.set_nodelay(true).map_err(|_| Error::Connection)?;
        Self::handshake(stream)
    }
    pub(crate) fn local_ip(&self) -> Option<std::net::IpAddr> {
        self.stream.get_ref().local_addr().ok().map(|a| a.ip())
    }
    pub(crate) fn peer_ip(&self) -> Option<std::net::IpAddr> {
        self.stream.get_ref().peer_addr().ok().map(|a| a.ip())
    }
}
impl<S: Read + Write> Client<S> {
    pub(crate) fn handshake(stream: S) -> Result<Self, Error> {
        let mut client = Self {
            stream: BufReader::new(stream),
            version: String::new(),
        };
        let greeting = client.line()?;
        client.version = greeting
            .strip_prefix("OK MPD ")
            .ok_or(Error::Connection)?
            .to_owned();
        Ok(client)
    }
    #[cfg(test)]
    pub(crate) fn version(&self) -> &str {
        &self.version
    }
    #[cfg(test)]
    fn into_inner(self) -> S {
        self.stream.into_inner()
    }
    fn line(&mut self) -> Result<String, Error> {
        let mut line = String::new();
        // A reply line is short; refuse a peer that streams without newlines.
        let read = (&mut self.stream)
            .take(8192)
            .read_line(&mut line)
            .map_err(|_| Error::Connection)?;
        if read == 0 || !line.ends_with('\n') {
            return Err(Error::Connection);
        }
        line.truncate(line.trim_end_matches(['\r', '\n']).len());
        Ok(line)
    }
    /// Sends one command with quoted arguments and reads its `key: value` reply.
    pub(crate) fn command(
        &mut self,
        name: &str,
        args: &[&str],
    ) -> Result<Vec<(String, String)>, Error> {
        let mut request = name.to_owned();
        for arg in args {
            request.push_str(" \"");
            for c in arg.chars() {
                match c {
                    '"' | '\\' => {
                        request.push('\\');
                        request.push(c);
                    }
                    // A newline would end the command and start another.
                    c if c.is_control() => request.push(' '),
                    c => request.push(c),
                }
            }
            request.push('"');
        }
        request.push('\n');
        let stream = self.stream.get_mut();
        stream
            .write_all(request.as_bytes())
            .and_then(|()| stream.flush())
            .map_err(|_| Error::Connection)?;
        let mut pairs = Vec::new();
        loop {
            let line = self.line()?;
            if line == "OK" {
                return Ok(pairs);
            }
            if let Some(ack) = line.strip_prefix("ACK ") {
                // ACK [error@command_listNum] {current_command} message_text
                let code = ack
                    .strip_prefix('[')
                    .and_then(|v| v.split_once('@'))
                    .map(|(code, _)| code);
                if code == Some("4") {
                    return Err(Error::Permission);
                }
                let message = ack.split_once("} ").map_or("", |(_, m)| m);
                return Err(Error::Refused(message.chars().take(200).collect()));
            }
            let (key, value) = line.split_once(": ").ok_or(Error::Connection)?;
            if pairs.len() < 256 {
                pairs.push((key.to_owned(), value.to_owned()));
            }
        }
    }
    pub(crate) fn status(&mut self) -> Result<Status, Error> {
        Ok(Status::parse(&self.command("status", &[])?))
    }
}

/// The parts of MPD's `status` reply the output follows.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Status {
    pub state: String,
    pub song_id: Option<u64>,
    pub elapsed_ms: Option<u64>,
    pub error: bool,
}
impl Status {
    pub(crate) fn parse(pairs: &[(String, String)]) -> Self {
        let mut status = Status::default();
        for (key, value) in pairs {
            match key.as_str() {
                "state" => status.state.clone_from(value),
                "songid" => status.song_id = value.parse().ok(),
                "elapsed" => {
                    status.elapsed_ms = value
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite() && (0.0..86400.0).contains(v))
                        .map(|v| (v * 1000.0) as u64);
                }
                "error" => status.error = true,
                _ => {}
            }
        }
        status
    }
    /// Applies this status to the remote song `ours`. Errors on a fetch or
    /// decode failure and on another client's song, which must not count as
    /// our song finishing.
    pub(crate) fn apply(&self, state: &mut PlaybackState, ours: Option<u64>) -> Result<(), ()> {
        if self.error || (self.song_id.is_some() && self.song_id != ours) {
            return Err(());
        }
        if let Some(ms) = self.elapsed_ms {
            state.position_ms = state.duration_ms.map_or(ms, |d| ms.min(d));
        }
        state.phase = match self.state.as_str() {
            "play" => Phase::Playing,
            "pause" => Phase::Paused,
            // Stopping is the end only once our song had started.
            "stop" if matches!(state.phase, Phase::Playing | Phase::Paused | Phase::Ended) => {
                Phase::Ended
            }
            _ => return Err(()),
        };
        if state.phase == Phase::Ended {
            state.position_ms = state.duration_ms.unwrap_or(state.position_ms);
        }
        Ok(())
    }
}

/// MPD as a remote output. Loading replaces MPD's queue with the one lease.
pub(crate) struct Device<S = TcpStream> {
    client: Client<S>,
    song: Option<u64>,
}
impl<S: Read + Write> Device<S> {
    /// Queues `url` alone, labels it, and plays from `state.position_ms`.
    fn play(
        &mut self,
        url: &str,
        track: &libresonic::Track,
        state: &mut PlaybackState,
    ) -> Result<(), ()> {
        let mpd = &mut self.client;
        mpd.command("clearerror", &[]).map_err(drop)?;
        mpd.command("clear", &[]).map_err(drop)?;
        let id = mpd
            .command("addid", &[url])
            .map_err(drop)?
            .into_iter()
            .find(|(k, _)| k == "Id")
            .and_then(|(_, v)| v.parse::<u64>().ok())
            .ok_or(())?;
        self.song = Some(id);
        let id = id.to_string();
        // Text for MPD clients showing what plays; older servers may refuse it.
        let artist = track
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        for (tag, value) in [
            ("Title", track.title.as_str()),
            ("Artist", artist.as_str()),
            (
                "Album",
                track.album.as_ref().map_or("", |a| a.title.as_str()),
            ),
        ] {
            if !value.is_empty() {
                let _ = mpd.command("addtagid", &[&id, tag, value]);
            }
        }
        mpd.command("playid", &[&id]).map_err(drop)?;
        if state.position_ms > 0 {
            self.seek_to(state)
        } else {
            self.refresh(state)
        }
    }
    /// Seeks to `state.position_ms`. A source MPD cannot seek (the music
    /// server offers no byte ranges) keeps playing where MPD is.
    fn seek_to(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        let seconds = format!("{:.3}", state.position_ms as f64 / 1000.0);
        match self.client.command("seekcur", &[&seconds]) {
            Ok(_) | Err(Error::Refused(_)) => self.refresh(state),
            Err(_) => Err(()),
        }
    }
    fn refresh(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.client.status().map_err(drop)?.apply(state, self.song)
    }
}
impl super::player::Device for Device {
    const NAME: &'static str = "MPD";
    fn connect(shared: &super::Shared, deadline: Instant) -> Result<Self, ()> {
        let client = shared
            .targets()
            .into_iter()
            .find_map(|target| Client::connect(target, deadline).ok())
            .ok_or(())?;
        Ok(Self { client, song: None })
    }
    fn local_ip(&self) -> Result<std::net::IpAddr, ()> {
        self.client.local_ip().ok_or(())
    }
    fn peer_ip(&self) -> Result<std::net::IpAddr, ()> {
        self.client.peer_ip().ok_or(())
    }
    fn volume(&mut self, volume: u16) -> Result<(), ()> {
        match self
            .client
            .command("setvol", &[&percent(volume).to_string()])
        {
            // An output without a mixer refuses volume; playback still works.
            Ok(_) | Err(Error::Refused(_)) => Ok(()),
            Err(_) => Err(()),
        }
    }
    fn load(
        &mut self,
        url: &str,
        _mime: &str,
        track: &libresonic::Track,
        state: &mut PlaybackState,
    ) -> Result<(), ()> {
        self.play(url, track, state)
    }
    fn loaded(&self) -> bool {
        self.song.is_some()
    }
    fn pause(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.client.command("pause", &["1"]).map_err(drop)?;
        self.refresh(state)
    }
    fn resume(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.client.command("pause", &["0"]).map_err(drop)?;
        self.refresh(state)
    }
    fn seek(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.seek_to(state)
    }
    fn status(&mut self, state: &mut PlaybackState) -> Result<(), ()> {
        self.refresh(state)
    }
    fn stop(&mut self) {
        if let Some(id) = self.song.take() {
            let _ = self.client.command("stop", &[]);
            let _ = self.client.command("deleteid", &[&id.to_string()]);
        }
    }
}

/// Player volume as MPD's 0–100 `setvol`.
pub(crate) fn percent(volume: u16) -> u8 {
    ((u32::from(volume) * 100 + 32767) / 65535) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresonic::playback::{Phase, PlaybackState};
    use std::io::{Cursor, Read, Write};

    /// A connection that replies from a script and records what was sent.
    struct Script {
        replies: Cursor<Vec<u8>>,
        sent: Vec<u8>,
    }
    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.replies.read(buf)
        }
    }
    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.sent.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn client(replies: &str) -> Client<Script> {
        Client::handshake(Script {
            replies: Cursor::new(format!("OK MPD 0.23.5\n{replies}").into_bytes()),
            sent: Vec::new(),
        })
        .unwrap()
    }
    fn sent(client: Client<Script>) -> String {
        String::from_utf8(client.into_inner().sent).unwrap()
    }

    #[test]
    fn only_an_mpd_greeting_is_accepted() {
        let other = Script {
            replies: Cursor::new(b"SSH-2.0-OpenSSH\n".to_vec()),
            sent: Vec::new(),
        };
        assert!(Client::handshake(other).is_err());
        assert!(client("").version().starts_with("0.23"));
    }

    #[test]
    fn replies_are_read_to_ok_and_acks_are_errors() {
        let mut mpd = client("Id: 7\nOK\nACK [50@0] {playid} No such song\n");
        assert_eq!(
            mpd.command("addid", &["http://x/y"]).unwrap(),
            [("Id".into(), "7".into())]
        );
        let error = mpd.command("playid", &["9"]).unwrap_err();
        assert_eq!(error, Error::Refused("No such song".into()));
        assert_eq!(sent(mpd), "addid \"http://x/y\"\nplayid \"9\"\n");
    }

    #[test]
    fn arguments_are_quoted_so_they_cannot_add_commands() {
        let mut mpd = client("OK\n");
        mpd.command("addtagid", &["7", "Title", "A \"B\" \\ C\nkill"])
            .unwrap();
        assert_eq!(
            sent(mpd),
            "addtagid \"7\" \"Title\" \"A \\\"B\\\" \\\\ C kill\"\n"
        );
    }

    #[test]
    fn a_password_protected_server_is_reported_not_retried() {
        let mut mpd = client("ACK [4@0] {status} you don't have permission for \"status\"\n");
        assert_eq!(mpd.command("status", &[]).unwrap_err(), Error::Permission);
    }

    #[test]
    fn status_drives_progress_pause_end_and_rejects_takeover_or_errors() {
        let ours = Some(7);
        let status = |text: &str| Status::parse(&pairs(text));
        let mut state = PlaybackState {
            duration_ms: Some(20_000),
            phase: Phase::Loading,
            ..Default::default()
        };
        status("state: play\nsongid: 7\nelapsed: 12.5\n")
            .apply(&mut state, ours)
            .unwrap();
        assert_eq!((state.phase, state.position_ms), (Phase::Playing, 12_500));
        status("state: pause\nsongid: 7\nelapsed: 15.000\n")
            .apply(&mut state, ours)
            .unwrap();
        assert_eq!((state.phase, state.position_ms), (Phase::Paused, 15_000));
        // Another client started its own song: not ours to report as finished.
        assert!(
            status("state: play\nsongid: 9\nelapsed: 1\n")
                .apply(&mut state, ours)
                .is_err()
        );
        // MPD could not fetch or decode the lease.
        assert!(
            status("state: stop\nerror: Failed to decode http://x\n")
                .apply(&mut state, ours)
                .is_err()
        );
        // Stopping after our song played through is the end of the song.
        status("state: stop\n").apply(&mut state, ours).unwrap();
        assert_eq!((state.phase, state.position_ms), (Phase::Ended, 20_000));
    }

    #[test]
    fn an_unseekable_source_keeps_playing_from_where_mpd_is() {
        // Starting at 5 s and seeking to 9 s are both refused; the song plays on.
        let mpd = client(concat!(
            // clearerror, clear, addid, addtagid Title, playid
            "OK\nOK\nId: 3\nOK\nOK\nOK\n",
            "ACK [2@0] {seekcur} Not seekable\n",
            "state: play\nsongid: 3\nelapsed: 0.200\nOK\n",
            "ACK [2@0] {seekcur} Not seekable\n",
            "state: play\nsongid: 3\nelapsed: 1.000\nOK\n",
        ));
        let mut device = Device {
            client: mpd,
            song: None,
        };
        let track = test_track();
        let mut state = PlaybackState {
            position_ms: 5000,
            duration_ms: Some(20_000),
            phase: Phase::Loading,
            ..Default::default()
        };
        device.play("http://lease", &track, &mut state).unwrap();
        assert_eq!((state.phase, state.position_ms), (Phase::Playing, 200));
        state.position_ms = 9000;
        device.seek_to(&mut state).unwrap();
        assert_eq!(state.position_ms, 1000, "the heard position is MPD's");
    }

    fn test_track() -> libresonic::Track {
        let server = libresonic::ServerId::new("fixture").unwrap();
        libresonic::Track {
            id: libresonic::MediaId::new(server, libresonic::MediaKind::Track, "1").unwrap(),
            title: "Song".into(),
            artists: Vec::new(),
            album: None,
            duration_ms: Some(20_000),
            track_number: None,
            disc_number: None,
            content_type: None,
            suffix: None,
            artwork: None,
            favorite: false,
            rating: None,
            file_path: None,
            file_size: None,
            play_count: None,
        }
    }

    #[test]
    fn volume_maps_to_mpd_percent() {
        assert_eq!(percent(0), 0);
        assert_eq!(percent(u16::MAX), 100);
        assert_eq!(percent(u16::MAX / 2), 50);
    }

    fn pairs(text: &str) -> Vec<(String, String)> {
        text.lines()
            .filter_map(|l| l.split_once(": "))
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }
}
