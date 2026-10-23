//! Bounded HTTP/RTSP and HAP record framing, including partial timed reads.
use super::{crypto, pairing::Error};
use libresonic::playback::Cancellation;
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
pub(super) struct Wire {
    pub tcp: TcpStream,
    keys: Option<Keys>,
    received: Vec<u8>,
    plain: Vec<u8>,
}
struct Keys {
    write: [u8; 32],
    read: [u8; 32],
    tx: u64,
    rx: u64,
}
impl Wire {
    pub fn new(tcp: TcpStream) -> Self {
        Self {
            tcp,
            keys: None,
            received: Vec::new(),
            plain: Vec::new(),
        }
    }
    pub fn connect(address: SocketAddr) -> Result<Self, Error> {
        let tcp = TcpStream::connect_timeout(&address, Duration::from_millis(700))?;
        tcp.set_read_timeout(Some(Duration::from_millis(10)))?;
        tcp.set_write_timeout(Some(Duration::from_millis(700)))?;
        tcp.set_nodelay(true)?;
        Ok(Self::new(tcp))
    }
    pub fn encrypt(&mut self, write: [u8; 32], read: [u8; 32]) {
        self.keys = Some(Keys {
            write,
            read,
            tx: 0,
            rx: 0,
        });
    }
    pub fn post(
        &mut self,
        path: &str,
        body: &[u8],
        sequence: u32,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, Error> {
        self.write_all(format!("POST {path} HTTP/1.1\r\nHost: {}\r\nCSeq: {sequence}\r\nUser-Agent: AirPlay/320.20\r\nConnection: keep-alive\r\nX-Apple-HKP: 3\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n", self.tcp.peer_addr()?, body.len()).as_bytes())?;
        self.write_all(body)?;
        // A sleeping television may take several seconds to display its code.
        let until =
            Instant::now() + Duration::from_secs(if path == "/pair-pin-start" { 10 } else { 4 });
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            if header.len() >= 8192 {
                return Err(Error::Protocol);
            }
            let mut b = [0];
            self.read_until(&mut b, until, cancel)?;
            header.push(b[0]);
        }
        let header = String::from_utf8(header).map_err(|_| Error::Protocol)?;
        match header.split_whitespace().nth(1) {
            Some("200") => {}
            Some("401" | "403" | "470") => return Err(Error::Authentication),
            Some("429" | "453") => return Err(Error::Backoff),
            _ => return Err(Error::Protocol),
        }
        let len = super::header(&header, "Content-Length")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| Error::Protocol)?;
        if len > 65536 {
            return Err(Error::Protocol);
        }
        let mut body = vec![0; len];
        self.read_until(&mut body, until, cancel)?;
        Ok(body)
    }
    fn read_until(
        &mut self,
        bytes: &mut [u8],
        until: Instant,
        cancel: &Cancellation,
    ) -> Result<(), Error> {
        let mut pos = 0;
        while pos < bytes.len() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if Instant::now() >= until {
                return Err(Error::Network);
            }
            match self.read(&mut bytes[pos..]) {
                Ok(0) => return Err(Error::Network),
                Ok(n) => pos += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) => {}
                Err(_) => return Err(Error::Network),
            }
        }
        Ok(())
    }
}
impl Read for Wire {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let Some(keys) = &mut self.keys else {
            return self.tcp.read(buf);
        };
        while self.plain.is_empty() {
            if self.received.len() >= 2 {
                let len = usize::from(u16::from_le_bytes(self.received[..2].try_into().unwrap()));
                if len == 0 || len > 1024 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                if self.received.len() >= len + 18 {
                    self.plain = crypto::open(
                        &keys.read,
                        &keys.rx.to_le_bytes(),
                        &self.received[..2],
                        &self.received[2..len + 18],
                    )
                    .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
                    keys.rx = keys.rx.checked_add(1).ok_or(io::ErrorKind::InvalidData)?;
                    self.received.drain(..len + 18);
                    break;
                }
            }
            let mut chunk = [0; 1042];
            let n = self.tcp.read(&mut chunk)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            self.received.extend_from_slice(&chunk[..n]);
        }
        let n = buf.len().min(self.plain.len());
        buf[..n].copy_from_slice(&self.plain[..n]);
        self.plain.drain(..n);
        Ok(n)
    }
}
impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let Some(keys) = &mut self.keys else {
            return self.tcp.write(buf);
        };
        let n = buf.len().min(1024);
        let len = (n as u16).to_le_bytes();
        let mut bytes = buf[..n].to_vec();
        crypto::seal(&keys.write, &keys.tx.to_le_bytes(), &len, &mut bytes)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        keys.tx = keys.tx.checked_add(1).ok_or(io::ErrorKind::InvalidData)?;
        self.tcp.write_all(&len)?;
        self.tcp.write_all(&bytes)?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn encrypted_records_survive_fragmentation_timeouts_and_reject_tampering() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = Wire::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        client.encrypt([3; 32], [4; 32]);
        let bytes = vec![42; 3000];
        client.write_all(&bytes).unwrap();
        let mut incoming = Wire::new(server.try_clone().unwrap());
        incoming.encrypt([4; 32], [3; 32]);
        let mut got = vec![0; 3000];
        incoming.read_exact(&mut got).unwrap();
        assert_eq!(got, bytes);
        let mut frame = b"fragmented".to_vec();
        let len = (frame.len() as u16).to_le_bytes();
        crypto::seal(&[4; 32], &0u64.to_le_bytes(), &len, &mut frame).unwrap();
        server.write_all(&len[..1]).unwrap();
        let mut got = [0; 10];
        assert!(client.read(&mut got).is_err());
        server.write_all(&len[1..]).unwrap();
        server.write_all(&frame[..3]).unwrap();
        assert!(client.read(&mut got).is_err());
        server.write_all(&frame[3..]).unwrap();
        client.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"fragmented");
        server.write_all(&len).unwrap();
        server.write_all(&frame).unwrap();
        // Replaying nonce zero is rejected; the receiver now expects nonce one.
        assert_eq!(
            client.read(&mut got).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
