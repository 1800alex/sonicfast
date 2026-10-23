//! A single paired AirPlay audio stream: encrypted control/events and PCM RTP.
//! Group coordination, PTP and buffered multiroom playback are not implemented.
use super::{
    Session, crypto,
    pairing::{self, Error},
    wire::Wire,
};
use plist::{Dictionary, Value};
use std::{
    io::{Cursor, Read, Write},
    net::SocketAddr,
    time::{Duration, Instant},
};

pub(super) struct Transport {
    event: Wire,
    pending: Vec<u8>,
    key: [u8; 32],
    nonce: u64,
}
fn dict(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Dictionary(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn integer(v: u64) -> Value {
    Value::Integer(v.into())
}
fn port(body: &Dictionary, name: &str) -> Result<u16, Error> {
    body.get(name)
        .and_then(Value::as_unsigned_integer)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| *v != 0)
        .ok_or(Error::Protocol)
}
impl Session {
    fn plist(&mut self, body: Value) -> Result<(String, Value), Error> {
        let mut bytes = Vec::new();
        body.to_writer_binary(&mut bytes)
            .map_err(|_| Error::Protocol)?;
        let (headers, bytes) = self
            .exchange_until(
                "SETUP",
                &self.url.clone(),
                "Content-Type: application/x-apple-binary-plist\r\nX-Apple-ProtocolVersion: 1\r\n",
                &bytes,
                Instant::now() + Duration::from_secs(4),
            )
            .map_err(|_| Error::Network)?;
        let body = Value::from_reader(Cursor::new(bytes)).map_err(|_| Error::Protocol)?;
        Ok((headers, body))
    }
    pub(super) fn setup_modern(&mut self, shared: &crate::casting::Shared) -> Result<(), Error> {
        let credentials = shared.airplay_credentials(&self.cancel)?;
        let secret = pairing::verify_pairing(&mut self.tcp, &credentials, &self.cancel)?;
        self.cseq = 2;
        let uuid = format!("{:032x}", rand::random::<u128>());
        let (headers, response) = self.plist(dict([
            ("deviceID", "02:00:00:00:00:01".into()),
            ("sessionUUID", uuid.into()),
            (
                "timingPort",
                integer(u64::from(self.timing.local_addr()?.port())),
            ),
            ("timingProtocol", "NTP".into()),
            ("isMultiSelectAirPlay", false.into()),
            ("groupContainsGroupLeader", false.into()),
            ("macAddress", "02:00:00:00:00:01".into()),
            ("model", "Sonicfast".into()),
            ("name", "Sonicfast".into()),
            ("sourceVersion", "377.40.00".into()),
            ("senderSupportsRelay", false.into()),
            ("statsCollectionEnabled", false.into()),
        ]))?;
        self.session = super::header(&headers, "Session")
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .to_owned();
        if self.session.len() > 128 || self.session.contains(['\r', '\n']) {
            return Err(Error::Protocol);
        }
        let response = response.as_dictionary().ok_or(Error::Protocol)?;
        let event_port = port(response, "eventPort")?;
        let mut event = Wire::connect(SocketAddr::new(self.remote.ip(), event_port))?;
        event.tcp.set_read_timeout(Some(Duration::from_millis(1)))?;
        event.encrypt(
            crypto::derive(&secret, "Events-Salt", "Events-Read-Encryption-Key")?,
            crypto::derive(&secret, "Events-Salt", "Events-Write-Encryption-Key")?,
        );
        let key: [u8; 32] = rand::random();
        self.modern = Some(Transport {
            event,
            pending: Vec::new(),
            key,
            nonce: 0,
        });
        let (_, response) = self.plist(dict([(
            "streams",
            Value::Array(vec![dict([
                ("audioFormat", integer(0x800)),
                ("audioMode", "default".into()),
                (
                    "controlPort",
                    integer(u64::from(self.control.local_addr()?.port())),
                ),
                ("ct", integer(1)),
                ("isMedia", true.into()),
                ("latencyMax", integer(88200)),
                ("latencyMin", integer(11025)),
                ("shk", Value::Data(key.to_vec())),
                ("spf", integer(352)),
                ("sr", integer(44100)),
                ("type", integer(0x60)),
                ("supportsDynamicStreamID", false.into()),
                ("streamConnectionID", integer(u64::from(self.ssrc))),
            ])]),
        )]))?;
        let stream = response
            .as_dictionary()
            .and_then(|v| v.get("streams"))
            .and_then(Value::as_array)
            .and_then(|v| v.first())
            .and_then(Value::as_dictionary)
            .ok_or(Error::Protocol)?;
        self.remote.set_port(port(stream, "dataPort")?);
        self.remote_control = Some(SocketAddr::new(
            self.remote.ip(),
            port(stream, "controlPort")?,
        ));
        Ok(())
    }
}
impl Transport {
    pub fn audio(&mut self, packet: &mut Vec<u8>, pcm: &[i16]) -> Result<(), Error> {
        let mut audio: Vec<u8> = pcm.iter().flat_map(|v| v.to_be_bytes()).collect();
        let nonce = self.nonce.to_le_bytes();
        self.nonce = self.nonce.checked_add(1).ok_or(Error::Protocol)?;
        crypto::seal(&self.key, &nonce, &packet[4..12], &mut audio)?;
        packet.extend(audio);
        packet.extend(nonce);
        Ok(())
    }
    pub fn service(&mut self) -> Result<(), Error> {
        // Keep receiver event requests flowing without blocking the audio feeder.
        // Framing retains partial data across socket read polls.
        for _ in 0..4 {
            let mut bytes = [0; 2048];
            match self.event.read(&mut bytes) {
                Ok(0) => return Err(Error::Network),
                Ok(n) => self.pending.extend_from_slice(&bytes[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    break;
                }
                Err(_) => return Err(Error::Network),
            }
            if self.pending.len() > 65536 + 8192 {
                return Err(Error::Protocol);
            }
        }
        for _ in 0..4 {
            let Some(header_end) = self
                .pending
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .map(|v| v + 4)
            else {
                if self.pending.len() >= 8192 {
                    return Err(Error::Protocol);
                }
                break;
            };
            let header =
                std::str::from_utf8(&self.pending[..header_end]).map_err(|_| Error::Protocol)?;
            let len = super::header(header, "Content-Length")
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| Error::Protocol)?;
            if len > 65536 {
                return Err(Error::Protocol);
            }
            if self.pending.len() < header_end + len {
                break;
            }
            let sequence = super::header(header, "CSeq")
                .unwrap_or("0")
                .parse::<u32>()
                .map_err(|_| Error::Protocol)?;
            self.event.write_all(
                format!("HTTP/1.1 200 OK\r\nCSeq: {sequence}\r\nContent-Length: 0\r\n\r\n")
                    .as_bytes(),
            )?;
            self.pending.drain(..header_end + len);
        }
        Ok(())
    }
}
