//! Classic ALAC/RAOP and PIN-paired encrypted PCM, with RTP timing and resends.
//! Packet layouts follow <https://nto.github.io/AirPlay.html>; no receiver code is linked.
use super::Shared;
mod crypto;
mod metadata;
mod modern;
pub mod pairing;
mod wire;
use crate::{eq::SharedEq, sink::PcmOutput, vis::AudioTap};
use libresonic::playback::{AudioOutput, Cancellation, OutputError, PcmBlock};
pub use metadata::artwork_jpeg;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{SocketAddr, TcpStream, UdpSocket},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const FRAMES: usize = 352;
const LATENCY: u32 = 88200;
pub struct Sender {
    shared: Shared,
    processing: PcmOutput,
    session: Option<Session>,
    pending: Vec<f64>,
    volume: u16,
    heard_until: Option<Instant>,
}
impl Sender {
    pub fn new(shared: Shared, eq: SharedEq, tap: Arc<AudioTap>) -> Self {
        Self {
            shared,
            processing: PcmOutput::new(None, eq, tap),
            session: None,
            pending: Vec::new(),
            volume: u16::MAX,
            heard_until: None,
        }
    }
    fn send(&mut self, samples: &[f64], cancel: &Cancellation) -> Result<(), OutputError> {
        if cancel.is_cancelled() {
            return Err(OutputError::Cancelled);
        }
        if self.session.is_none() {
            self.shared.report("Connecting to AirPlay receiver");
            self.session = Some(Session::connect(&self.shared, cancel).inspect_err(|_| {
                if self.shared.status() == "Connecting to AirPlay receiver" {
                    self.shared
                        .report("AirPlay handshake failed; check the receiver and network");
                }
            })?);
        }
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= FRAMES * 2 {
            let pcm: Vec<i16> = self
                .pending
                .drain(..FRAMES * 2)
                .map(|v| {
                    let gain = f64::from(self.volume) / 65535.0;
                    (v.mul_add(gain, 0.0).clamp(-1.0, 1.0) * 32767.0).round() as i16
                })
                .collect();
            let session = self.session.as_mut().unwrap();
            session.audio(&pcm, cancel).inspect_err(|_| {
                self.shared
                    .report("AirPlay output stopped; press Play to reconnect")
            })?;
            self.heard_until = Some(
                session.started
                    + Duration::from_secs(2)
                    + Duration::from_secs_f64(session.frames as f64 / 44100.0),
            );
            self.shared.report("Sending AirPlay audio");
        }
        if let Some(session) = self.session.as_mut().filter(|s| s.frames > 0) {
            session.metadata(&self.shared).inspect_err(|_| {
                self.shared
                    .report("AirPlay output stopped; press Play to reconnect")
            })?;
        }
        Ok(())
    }
    fn close(&mut self) {
        self.session.take();
        self.pending.clear();
        self.heard_until = None;
    }
}
impl AudioOutput for Sender {
    fn reset(&mut self) -> Result<(), OutputError> {
        self.close();
        self.processing.reset()
    }
    fn write(&mut self, block: PcmBlock<'_>, cancel: &Cancellation) -> Result<(), OutputError> {
        let samples = self.processing.process_for_network(block);
        self.send(&samples, cancel)
    }
    fn set_volume(&mut self, v: u16) {
        self.volume = v;
    }
    fn pause(&mut self) {
        self.close();
        self.processing.pause();
        self.shared.report("AirPlay paused");
    }
    fn resume(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn pending_ms(&self) -> u64 {
        self.heard_until.map_or(0, |v| {
            v.saturating_duration_since(Instant::now()).as_millis() as u64
        })
    }
    fn finish(&mut self, cancel: &Cancellation) -> Result<(), OutputError> {
        let mut tail = self.processing.finish_for_network();
        let remaining = (self.pending.len() + tail.len()) % (FRAMES * 2);
        if remaining > 0 {
            tail.resize(tail.len() + FRAMES * 2 - remaining, 0.0);
        }
        self.send(&tail, cancel)
    }
    fn drained(&mut self) -> Result<bool, OutputError> {
        if let Some(s) = &mut self.session {
            s.service()?;
        }
        Ok(self.pending_ms() == 0)
    }
    fn stop(&mut self) {
        self.close();
        self.processing.stop();
        if matches!(
            self.shared.status().as_str(),
            "Sending AirPlay audio" | "AirPlay paused" | "Connecting to AirPlay receiver"
        ) {
            self.shared.report("AirPlay stopped");
        }
    }
}
struct Session {
    tcp: wire::Wire,
    modern: Option<modern::Transport>,
    url: String,
    session: String,
    cseq: u32,
    audio: UdpSocket,
    control: UdpSocket,
    timing: UdpSocket,
    remote: SocketAddr,
    remote_control: Option<SocketAddr>,
    seq: u16,
    stamp: u32,
    ssrc: u32,
    frames: u64,
    started: Instant,
    last_check: Instant,
    cache: VecDeque<(u16, Vec<u8>)>,
    cancel: Cancellation,
    shown: Shown,
}
/// What this session last put on the receiver's display.
#[derive(Default)]
struct Shown {
    checked: Option<Instant>,
    revision: Option<u64>,
    /// None until a cover (or its absence) has been sent.
    art: Option<Option<String>>,
    generation: Option<u64>,
    progress_at: Option<Instant>,
}
impl Session {
    fn connect(shared: &Shared, cancel: &Cancellation) -> Result<Self, OutputError> {
        let until = Instant::now() + Duration::from_secs(3);
        let targets = loop {
            if cancel.is_cancelled() {
                return Err(OutputError::Cancelled);
            }
            let t = shared.targets();
            if !t.is_empty() {
                break t;
            }
            if Instant::now() > until {
                return Err(OutputError::DeviceLost);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut tcp = None;
        for target in targets {
            if cancel.is_cancelled() {
                return Err(OutputError::Cancelled);
            }
            if let Ok(s) = TcpStream::connect_timeout(&target, Duration::from_millis(300)) {
                tcp = Some((s, target));
                break;
            }
        }
        let (tcp, target) = tcp.ok_or(OutputError::DeviceLost)?;
        tcp.set_read_timeout(Some(Duration::from_millis(10)))
            .map_err(out)?;
        tcp.set_write_timeout(Some(Duration::from_millis(700)))
            .map_err(out)?;
        let local = tcp.local_addr().map_err(out)?.ip();
        let udp = || {
            let s = UdpSocket::bind(SocketAddr::new(local, 0)).map_err(out)?;
            s.set_nonblocking(true).map_err(out)?;
            Ok::<_, OutputError>(s)
        };
        let id = rand::random::<u32>();
        let mut s = Self {
            tcp: wire::Wire::new(tcp),
            modern: None,
            url: format!("rtsp://{target}/{id}"),
            session: String::new(),
            cseq: 0,
            audio: udp()?,
            control: udp()?,
            timing: udp()?,
            remote: target,
            remote_control: None,
            seq: rand::random(),
            stamp: rand::random(),
            ssrc: rand::random(),
            frames: 0,
            started: Instant::now(),
            last_check: Instant::now(),
            cache: VecDeque::new(),
            cancel: cancel.clone(),
            shown: Shown::default(),
        };
        if shared.settings().airplay2 {
            s.setup_modern(shared).map_err(|error| {
                shared.report(&error.to_string());
                if matches!(error, pairing::Error::Cancelled) {
                    OutputError::Cancelled
                } else {
                    OutputError::DeviceLost
                }
            })?;
        } else {
            let family = if local.is_ipv4() { "IP4" } else { "IP6" };
            let body = format!(
                "v=0\r\no=iTunes {id} 0 IN {family} {local}\r\ns=iTunes\r\nc=IN {family} {}\r\nt=0 0\r\nm=audio 0 RTP/AVP 96\r\na=rtpmap:96 AppleLossless\r\na=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n",
                target.ip()
            );
            s.request(
                "ANNOUNCE",
                "Content-Type: application/sdp\r\n",
                body.as_bytes(),
            )?;
            let transport = format!(
                "Transport: RTP/AVP/UDP;unicast;mode=record;control_port={};timing_port={}\r\n",
                s.control.local_addr().map_err(out)?.port(),
                s.timing.local_addr().map_err(out)?.port()
            );
            let reply = s.request("SETUP", &transport, &[])?;
            s.session = header(&reply, "Session")
                .unwrap_or_default()
                .split(';')
                .next()
                .unwrap_or_default()
                .to_owned();
            if s.session.len() > 128 || s.session.contains(['\r', '\n']) {
                return Err(OutputError::DeviceLost);
            }
            let port = header(&reply, "Transport")
                .and_then(|v| {
                    v.split(';').find_map(|p| {
                        p.trim()
                            .strip_prefix("server_port=")
                            .and_then(|v| v.parse::<u16>().ok())
                    })
                })
                .filter(|v| *v > 0)
                .ok_or(OutputError::DeviceLost)?;
            s.remote.set_port(port);
            s.remote_control = header(&reply, "Transport")
                .and_then(|v| {
                    v.split(';').find_map(|p| {
                        p.trim()
                            .strip_prefix("control_port=")
                            .and_then(|v| v.parse::<u16>().ok())
                    })
                })
                .filter(|v| *v > 0)
                .map(|port| SocketAddr::new(target.ip(), port));
            if s.remote_control.is_none() {
                return Err(OutputError::DeviceLost);
            }
        }
        s.request(
            "RECORD",
            &format!(
                "Range: npt=0-\r\nRTP-Info: seq={};rtptime={}\r\n",
                s.seq, s.stamp
            ),
            &[],
        )?;
        s.request(
            "SET_PARAMETER",
            "Content-Type: text/parameters\r\n",
            b"volume: 0.0\r\n",
        )?;
        s.started = Instant::now();
        Ok(s)
    }
    fn request(&mut self, method: &str, headers: &str, body: &[u8]) -> Result<String, OutputError> {
        let result = self.request_until(
            method,
            headers,
            body,
            Instant::now() + Duration::from_secs(2),
        );
        if let Err(error) = &result {
            // Method and our typed error are safe; never log arbitrary receiver headers/bodies.
            log::warn!("AirPlay {method} failed: {error:?}");
        }
        result
    }
    fn request_until(
        &mut self,
        method: &str,
        headers: &str,
        body: &[u8],
        deadline: Instant,
    ) -> Result<String, OutputError> {
        self.exchange_until(method, &self.url.clone(), headers, body, deadline)
            .map(|v| v.0)
    }
    fn exchange_until(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        body: &[u8],
        deadline: Instant,
    ) -> Result<(String, Vec<u8>), OutputError> {
        let (status, reply, body) = self.exchange_status(method, path, headers, body, deadline)?;
        if status != 200 {
            log::warn!("AirPlay {method} returned RTSP status {status}");
            return Err(OutputError::DeviceLost);
        }
        Ok((reply, body))
    }
    /// One request/response; the whole reply is consumed whatever its status,
    /// so a refusal leaves the connection usable.
    fn exchange_status(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        body: &[u8],
        deadline: Instant,
    ) -> Result<(u16, String, Vec<u8>), OutputError> {
        self.cseq += 1;
        let session = if self.session.is_empty() {
            String::new()
        } else {
            format!("Session: {}\r\n", self.session)
        };
        let request = format!(
            "{method} {} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: Sonicfast/0.15\r\n{session}{headers}Content-Length: {}\r\n\r\n",
            path,
            self.cseq,
            body.len()
        );
        self.tcp.write_all(request.as_bytes()).map_err(out)?;
        self.tcp.write_all(body).map_err(out)?;
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            if bytes.len() >= 8192 || Instant::now() > deadline {
                return Err(OutputError::DeviceLost);
            }
            let mut b = [0];
            self.read_servicing(&mut b, deadline, method != "TEARDOWN")?;
            bytes.push(b[0]);
        }
        let reply = String::from_utf8(bytes).map_err(|_| OutputError::DeviceLost)?;
        let status = reply
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or(OutputError::DeviceLost)?;
        let length = header(&reply, "Content-Length")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| OutputError::DeviceLost)?;
        if length > 65536 {
            return Err(OutputError::DeviceLost);
        }
        let mut body = vec![0; length];
        self.read_servicing(&mut body, deadline, method != "TEARDOWN")?;
        Ok((status, reply, body))
    }
    /// Mirrors track text, cover and progress onto the receiver's display,
    /// as iTunes and OwnTone do. Receivers that lack a display decline these.
    fn metadata(&mut self, shared: &Shared) -> Result<(), OutputError> {
        if self
            .shown
            .checked
            .is_some_and(|at| at.elapsed() < Duration::from_millis(250))
        {
            return Ok(());
        }
        self.shown.checked = Some(Instant::now());
        let m = shared.metadata();
        if m.revision == 0 {
            return Ok(());
        }
        if self.shown.revision != Some(m.revision) {
            self.shown.revision = Some(m.revision);
            self.shown.progress_at = None;
            self.set_parameter("application/x-dmap-tagged", &metadata::dmap(&m.now))?;
        }
        let art = m
            .artwork
            .filter(|(url, _)| m.now.art_url.as_ref() == Some(url));
        let key = art.as_ref().map(|(url, _)| url.clone());
        if self.shown.art.as_ref() != Some(&key) {
            self.shown.art = Some(key);
            match &art {
                Some((_, jpeg)) => self.set_parameter("image/jpeg", jpeg)?,
                // Clears the previous track's cover while this one loads.
                None => self.set_parameter("image/none", &[])?,
            }
        }
        if let Some(p) = m.progress.filter(|p| p.duration_ms > 0)
            && (self.shown.generation != Some(p.generation)
                || self
                    .shown
                    .progress_at
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(5)))
        {
            self.shown.generation = Some(p.generation);
            self.shown.progress_at = Some(Instant::now());
            let body = metadata::progress(
                self.stamp.wrapping_sub(LATENCY),
                p.position_now(),
                p.duration_ms,
            );
            self.set_parameter("text/parameters", body.as_bytes())?;
        }
        Ok(())
    }
    /// A declined parameter keeps the stream; only a lost connection fails.
    fn set_parameter(&mut self, content_type: &str, body: &[u8]) -> Result<(), OutputError> {
        let headers = format!(
            "Content-Type: {content_type}\r\nRTP-Info: rtptime={}\r\n",
            self.stamp
        );
        let url = self.url.clone();
        let deadline = Instant::now() + Duration::from_secs(3);
        match self.exchange_status("SET_PARAMETER", &url, &headers, body, deadline) {
            Ok((200, ..)) => Ok(()),
            Ok((status, ..)) => {
                log::info!("AirPlay receiver declined {content_type} (RTSP status {status})");
                Ok(())
            }
            Err(error) => {
                log::warn!("AirPlay SET_PARAMETER {content_type} failed: {error:?}");
                Err(error)
            }
        }
    }
    fn read_servicing(
        &mut self,
        bytes: &mut [u8],
        deadline: Instant,
        cancellable: bool,
    ) -> Result<(), OutputError> {
        let mut received = 0;
        while received < bytes.len() {
            if cancellable && self.cancel.is_cancelled() {
                return Err(OutputError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(OutputError::DeviceLost);
            }
            // Real receivers may withhold SETUP/RECORD until their initial NTP
            // exchange succeeds. Service UDP while waiting for every RTSP response.
            self.service_udp()?;
            match self.tcp.read(&mut bytes[received..]) {
                Ok(0) => return Err(OutputError::DeviceLost),
                Ok(n) => received += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(out(e)),
            }
        }
        Ok(())
    }
    fn audio(&mut self, pcm: &[i16], cancel: &Cancellation) -> Result<(), OutputError> {
        // Bound prebuffering to 100 ms; timestamps include the receiver latency.
        let due = self.started + Duration::from_secs_f64(self.frames as f64 / 44100.0);
        while Instant::now() < due {
            if cancel.is_cancelled() {
                return Err(OutputError::Cancelled);
            }
            self.service()?;
            std::thread::sleep(
                due.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(4)),
            );
        }
        if cancel.is_cancelled() {
            return Err(OutputError::Cancelled);
        }
        self.service()?;
        if self.frames.is_multiple_of((FRAMES * 125) as u64) {
            let mut sync = vec![if self.frames == 0 { 0x90 } else { 0x80 }, 0xd4, 0, 7];
            sync.extend_from_slice(&self.stamp.wrapping_sub(LATENCY).to_be_bytes());
            sync.extend_from_slice(&ntp().to_be_bytes());
            sync.extend_from_slice(&self.stamp.to_be_bytes());
            // Sync packets go to the server's control port, supplied by SETUP.
            let _ = self.control.send_to(&sync, self.control_remote());
        }
        let mut packet = vec![0x80, if self.frames == 0 { 0xe0 } else { 0x60 }];
        packet.extend_from_slice(&self.seq.to_be_bytes());
        packet.extend_from_slice(&self.stamp.to_be_bytes());
        packet.extend_from_slice(&self.ssrc.to_be_bytes());
        if let Some(transport) = &mut self.modern {
            transport
                .audio(&mut packet, pcm)
                .map_err(|_| OutputError::DeviceLost)?;
        } else {
            packet.extend(alac(pcm));
        }
        self.audio.send_to(&packet, self.remote).map_err(out)?;
        self.cache.push_back((self.seq, packet));
        if self.cache.len() > 512 {
            self.cache.pop_front();
        }
        self.seq = self.seq.wrapping_add(1);
        self.stamp = self.stamp.wrapping_add(FRAMES as u32);
        self.frames += FRAMES as u64;
        Ok(())
    }
    fn control_remote(&self) -> SocketAddr {
        self.remote_control.unwrap_or(self.remote)
    }
    fn service(&mut self) -> Result<(), OutputError> {
        if self.last_check.elapsed() >= Duration::from_secs(2) {
            if self.modern.is_some() {
                self.exchange_until(
                    "POST",
                    "/feedback",
                    "",
                    &[],
                    Instant::now() + Duration::from_secs(2),
                )?;
            } else {
                self.request("OPTIONS", "", &[])?;
            }
            self.last_check = Instant::now();
        }
        self.service_udp()
    }
    fn service_udp(&mut self) -> Result<(), OutputError> {
        if let Some(transport) = &mut self.modern {
            transport.service().map_err(|_| OutputError::DeviceLost)?;
        }
        let mut buf = [0; 2048];
        for _ in 0..16 {
            match self.timing.recv_from(&mut buf) {
                Ok((n, peer))
                    if n >= 32 && peer.ip() == self.remote.ip() && buf[1] & 0x7f == 0x52 =>
                {
                    let received = ntp();
                    let mut reply = [0u8; 32];
                    reply[..4].copy_from_slice(&[0x80, 0xd3, 0, 7]);
                    reply[8..16].copy_from_slice(&buf[24..32]);
                    reply[16..24].copy_from_slice(&received.to_be_bytes());
                    reply[24..32].copy_from_slice(&ntp().to_be_bytes());
                    let _ = self.timing.send_to(&reply, peer);
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(out(e)),
            }
        }
        for _ in 0..16 {
            match self.control.recv_from(&mut buf) {
                Ok((n, peer))
                    if n >= 8 && peer.ip() == self.remote.ip() && buf[1] & 0x7f == 0x55 =>
                {
                    let start = u16::from_be_bytes([buf[4], buf[5]]);
                    let count = u16::from_be_bytes([buf[6], buf[7]]).min(32);
                    for offset in 0..count {
                        if let Some((_, packet)) = self
                            .cache
                            .iter()
                            .find(|(seq, _)| *seq == start.wrapping_add(offset))
                        {
                            let mut reply = vec![0x80, 0xd6, 0, 1];
                            reply.extend_from_slice(packet);
                            let _ = self.control.send_to(&reply, peer);
                        }
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(out(e)),
            }
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self
            .tcp
            .tcp
            .set_write_timeout(Some(Duration::from_millis(50)));
        let _ = self.request_until(
            "TEARDOWN",
            "",
            &[],
            Instant::now() + Duration::from_millis(50),
        );
    }
}
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        line.split_once(':')
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
    })
}
fn out(_: std::io::Error) -> OutputError {
    OutputError::DeviceLost
}
fn ntp() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    ((now.as_secs() + 2208988800) << 32) | ((u64::from(now.subsec_nanos()) << 32) / 1_000_000_000)
}
/// Verbatim ALAC stereo frame: element tag, flags, signed samples, end marker.
fn alac(samples: &[i16]) -> Vec<u8> {
    let mut bytes = vec![0; (23 + samples.len() * 16 + 3).div_ceil(8)];
    let mut bit = 0;
    let mut put = |value: u32, count: usize| {
        for shift in (0..count).rev() {
            bytes[bit / 8] |= (((value >> shift) & 1) as u8) << (7 - bit % 8);
            bit += 1;
        }
    };
    put(1, 3);
    put(0, 4);
    put(0, 12);
    put(0, 1);
    put(0, 2);
    put(1, 1);
    for s in samples {
        put(u32::from(*s as u16), 16);
    }
    put(7, 3);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        sync::mpsc,
    };

    fn read_request(reader: &mut BufReader<TcpStream>) -> String {
        read_request_body(reader).0
    }

    fn read_request_body(reader: &mut BufReader<TcpStream>) -> (String, Vec<u8>) {
        let mut request = String::new();
        loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).unwrap() > 0,
                "Sender disconnected"
            );
            request.push_str(&line);
            if line == "\r\n" {
                break;
            }
            assert!(request.len() < 8192);
        }
        let length = header(&request, "Content-Length")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!(length < 8192);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        (request, body)
    }

    fn clock_exchange(timing: &UdpSocket, target: SocketAddr, sequence: u16) {
        let mut packet = [0; 32];
        packet[..2].copy_from_slice(&[0x80, 0xd2]);
        packet[2..4].copy_from_slice(&sequence.to_be_bytes());
        let sent = ntp();
        packet[24..32].copy_from_slice(&sent.to_be_bytes());
        timing.send_to(&packet, target).unwrap();
        let mut reply = [0; 64];
        let (n, peer) = timing
            .recv_from(&mut reply)
            .expect("Receiver needs timing before RECORD can finish");
        assert_eq!(peer, target);
        assert_eq!(n, 32);
        assert_eq!(&reply[..2], &[0x80, 0xd3]);
        assert_eq!(&reply[8..16], &sent.to_be_bytes());
        assert!(u64::from_be_bytes(reply[16..24].try_into().unwrap()) >= sent);
        assert!(u64::from_be_bytes(reply[24..32].try_into().unwrap()) >= sent);
    }

    type Streaming = fn(&mut TcpStream, &mut BufReader<TcpStream>, &UdpSocket, &UdpSocket);

    /// Behaves like a hardware receiver that gates RECORD on initial NTP.
    fn receiver(withhold_record: bool) -> (u16, mpsc::Receiver<()>, std::thread::JoinHandle<()>) {
        receiver_with(withhold_record, first_packets)
    }

    /// The first sync and audio packets after RECORD.
    fn first_packets(
        _: &mut TcpStream,
        _: &mut BufReader<TcpStream>,
        audio: &UdpSocket,
        control: &UdpSocket,
    ) {
        control
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        audio
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut sync = [0; 64];
        assert_eq!(control.recv_from(&mut sync).unwrap().0, 20);
        assert_eq!(
            &sync[..4],
            &[0x90, 0xd4, 0, 7],
            "First sync needs the extension marker"
        );
        let mut packet = [0; 2048];
        assert!(audio.recv_from(&mut packet).unwrap().0 > 12);
        assert_eq!(&packet[..2], &[0x80, 0xe0]);
        assert_eq!(&sync[16..20], &packet[4..8]);
        let next = u32::from_be_bytes(sync[16..20].try_into().unwrap());
        let heard = u32::from_be_bytes(sync[4..8].try_into().unwrap());
        assert_eq!(next.wrapping_sub(heard), LATENCY);
    }

    /// Runs the handshake, then `streaming` once RECORD and volume are acknowledged.
    fn receiver_with(
        withhold_record: bool,
        streaming: Streaming,
    ) -> (u16, mpsc::Receiver<()>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (ready, wait) = mpsc::channel();
        let task = std::thread::spawn(move || {
            let (mut tcp, _) = listener.accept().unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
            tcp.set_write_timeout(Some(Duration::from_secs(4))).unwrap();
            let mut reader = BufReader::new(tcp.try_clone().unwrap());
            assert!(read_request(&mut reader).starts_with("ANNOUNCE "));
            tcp.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\n\r\n")
                .unwrap();
            let setup = read_request(&mut reader);
            assert!(setup.starts_with("SETUP "));
            let timing_port = header(&setup, "Transport")
                .unwrap()
                .split(';')
                .find_map(|v| v.strip_prefix("timing_port="))
                .unwrap()
                .parse::<u16>()
                .unwrap();
            let target = SocketAddr::from(([127, 0, 0, 1], timing_port));
            let timing = UdpSocket::bind("127.0.0.1:0").unwrap();
            timing
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let audio = UdpSocket::bind("127.0.0.1:0").unwrap();
            let control = UdpSocket::bind("127.0.0.1:0").unwrap();
            // Gate SETUP too: some receivers start the exchange before replying.
            clock_exchange(&timing, target, 0);
            tcp.write_all(format!("RTSP/1.0 200 OK\r\nCSeq: 2\r\nSession: 1\r\nTransport: RTP/AVP/UDP;server_port={};control_port={};timing_port={}\r\n\r\n", audio.local_addr().unwrap().port(),control.local_addr().unwrap().port(),timing.local_addr().unwrap().port()).as_bytes()).unwrap();
            assert!(read_request(&mut reader).starts_with("RECORD "));
            for sequence in 1..=3 {
                clock_exchange(&timing, target, sequence);
            }
            ready.send(()).unwrap();
            if !withhold_record {
                tcp.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Length: 4\r\n\r\n")
                    .unwrap();
                // Timing must also keep working while reading a response body.
                clock_exchange(&timing, target, 4);
                tcp.write_all(b"test").unwrap();
                assert!(read_request(&mut reader).starts_with("SET_PARAMETER "));
                tcp.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 4\r\n\r\n")
                    .unwrap();
                streaming(&mut tcp, &mut reader, &audio, &control);
            }
            assert!(read_request(&mut reader).starts_with("TEARDOWN "));
            tcp.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 5\r\n\r\n")
                .unwrap();
        });
        (port, wait, task)
    }

    async fn shared(port: u16) -> Shared {
        let shared = Shared::default();
        shared
            .configure(super::super::Settings {
                output: Some(super::super::Kind::AirPlay),
                host: "127.0.0.1".into(),
                port,
                allow_lan: true,
                ..Default::default()
            })
            .await
            .unwrap();
        shared
    }

    #[tokio::test]
    async fn handshake_services_clock_requests_before_setup_record_and_body_complete() {
        let (port, ready, task) = receiver(false);
        let mut session = Session::connect(&shared(port).await, &Cancellation::default()).unwrap();
        session
            .audio(&[0; FRAMES * 2], &Cancellation::default())
            .unwrap();
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(session);
        task.join().unwrap();
    }

    fn now_playing() -> crate::casting::NowPlaying {
        crate::casting::NowPlaying {
            title: "Song".into(),
            artist: "Band".into(),
            album: "Record".into(),
            art_url: Some("sonic-art:1".into()),
        }
    }

    /// Text, a refused cover, then progress: a declined parameter keeps the session.
    fn display_parameters(
        tcp: &mut TcpStream,
        reader: &mut BufReader<TcpStream>,
        _: &UdpSocket,
        _: &UdpSocket,
    ) {
        let (request, body) = read_request_body(reader);
        assert!(request.starts_with("SET_PARAMETER "));
        assert_eq!(
            header(&request, "Content-Type"),
            Some("application/x-dmap-tagged")
        );
        assert!(
            header(&request, "RTP-Info")
                .unwrap()
                .starts_with("rtptime=")
        );
        assert_eq!(body, metadata::dmap(&now_playing()));
        tcp.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 5\r\n\r\n")
            .unwrap();
        let (request, body) = read_request_body(reader);
        assert_eq!(header(&request, "Content-Type"), Some("image/jpeg"));
        assert_eq!(body, b"jpeg");
        tcp.write_all(b"RTSP/1.0 501 Not Implemented\r\nCSeq: 6\r\nContent-Length: 4\r\n\r\nnope")
            .unwrap();
        let (request, body) = read_request_body(reader);
        assert_eq!(header(&request, "Content-Type"), Some("text/parameters"));
        let frames: Vec<u32> = String::from_utf8(body)
            .unwrap()
            .strip_prefix("progress: ")
            .unwrap()
            .trim_end()
            .split('/')
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(frames[1].wrapping_sub(frames[0]), 88_200);
        assert_eq!(frames[2].wrapping_sub(frames[0]), 441_000);
        tcp.write_all(b"RTSP/1.0 200 OK\r\nCSeq: 7\r\n\r\n")
            .unwrap();
    }

    #[tokio::test]
    async fn display_metadata_reaches_the_receiver_and_a_refusal_keeps_streaming() {
        let (port, ready, task) = receiver_with(false, display_parameters);
        let shared = shared(port).await;
        shared.set_now_playing(now_playing());
        shared.set_artwork("sonic-art:1".into(), Arc::from(&b"jpeg"[..]));
        shared.set_progress(crate::casting::Progress {
            generation: 1,
            position_ms: 2_000,
            duration_ms: 10_000,
            playing: false,
            at: Instant::now(),
        });
        let mut session = Session::connect(&shared, &Cancellation::default()).unwrap();
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        session.metadata(&shared).unwrap();
        // Nothing changed, so nothing more is sent before TEARDOWN.
        session.shown.checked = None;
        session.metadata(&shared).unwrap();
        drop(session);
        task.join().unwrap();
    }

    #[tokio::test]
    async fn missing_rtsp_reply_still_has_a_bounded_deadline() {
        let (port, ready, task) = receiver(true);
        let shared = shared(port).await;
        let started = Instant::now();
        assert!(matches!(
            Session::connect(&shared, &Cancellation::default()),
            Err(OutputError::DeviceLost)
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        task.join().unwrap();
    }

    #[tokio::test]
    async fn cancelled_handshake_releases_receiver_without_waiting_for_record_timeout() {
        let (port, ready, task) = receiver(true);
        let shared = shared(port).await;
        let cancel = Cancellation::default();
        let token = cancel.clone();
        let canceller = std::thread::spawn(move || {
            ready.recv_timeout(Duration::from_secs(2)).unwrap();
            token.cancel();
        });
        let started = Instant::now();
        assert!(matches!(
            Session::connect(&shared, &cancel),
            Err(OutputError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
        canceller.join().unwrap();
        task.join().unwrap();
    }
}
