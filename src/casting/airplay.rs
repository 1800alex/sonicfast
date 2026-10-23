//! Classic, unencrypted RAOP with 44.1 kHz stereo ALAC, RTP timing and resends.
//! Packet layouts follow <https://nto.github.io/AirPlay.html>; no receiver code is linked.
use super::Shared;
use crate::{eq::SharedEq, sink::PcmOutput, vis::AudioTap};
use libresonic::playback::{AudioOutput, Cancellation, OutputError, PcmBlock};
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
                self.shared.report(
                    "AirPlay connection failed; pairing/password-only receivers are not supported",
                )
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
        self.shared.report("AirPlay stopped");
    }
}
struct Session {
    tcp: TcpStream,
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
        tcp.set_read_timeout(Some(Duration::from_millis(700)))
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
            tcp,
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
        };
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
        self.cseq += 1;
        let session = if self.session.is_empty() {
            String::new()
        } else {
            format!("Session: {}\r\n", self.session)
        };
        let request = format!(
            "{method} {} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: Sonicfast/0.15\r\n{session}{headers}Content-Length: {}\r\n\r\n",
            self.url,
            self.cseq,
            body.len()
        );
        self.tcp.write_all(request.as_bytes()).map_err(out)?;
        self.tcp.write_all(body).map_err(out)?;
        let mut bytes = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !bytes.ends_with(b"\r\n\r\n") {
            if bytes.len() >= 8192 || Instant::now() > deadline {
                return Err(OutputError::DeviceLost);
            }
            let mut b = [0];
            self.tcp.read_exact(&mut b).map_err(out)?;
            bytes.push(b[0]);
        }
        let reply = String::from_utf8(bytes).map_err(|_| OutputError::DeviceLost)?;
        if reply.split_whitespace().nth(1) != Some("200") {
            return Err(OutputError::DeviceLost);
        }
        let length = header(&reply, "Content-Length")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| OutputError::DeviceLost)?;
        if length > 65536 {
            return Err(OutputError::DeviceLost);
        }
        let mut body = vec![0; length];
        self.tcp.read_exact(&mut body).map_err(out)?;
        Ok(reply)
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
            let mut sync = vec![0x80, 0xd4, 0, 7];
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
        packet.extend(alac(pcm));
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
            self.request("OPTIONS", "", &[])?;
            self.last_check = Instant::now();
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
        let _ = self.tcp.set_read_timeout(Some(Duration::from_millis(50)));
        let _ = self.tcp.set_write_timeout(Some(Duration::from_millis(50)));
        let _ = self.request("TEARDOWN", "", &[]);
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
