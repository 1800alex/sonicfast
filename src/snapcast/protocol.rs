//! Small bounded implementation of the public Snapcast v2 framing.
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub const MAX_MESSAGE: usize = 1024 * 1024;
pub const MAX_CHUNK_SAMPLES: usize = 384000;
#[derive(Debug)]
pub struct Message {
    pub kind: u16,
    pub refers: u16,
    pub sent: i64,
    pub body: Vec<u8>,
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
pub fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("Truncated Snapcast message")?
            .try_into()
            .unwrap(),
    ))
}
pub fn timestamp(bytes: &[u8], at: usize) -> Result<i64, String> {
    let sec = u32_at(bytes, at)? as i32;
    let us = u32_at(bytes, at + 4)? as i32;
    if !(-999_999..=999_999).contains(&us) {
        return Err("Invalid Snapcast timestamp".into());
    }
    Ok(i64::from(sec) * 1_000_000 + i64::from(us))
}
pub fn bytes_field(bytes: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let len = u32_at(bytes, 0)? as usize;
    if len > MAX_MESSAGE || bytes.len() < 4 + len {
        return Err("Invalid Snapcast field length".into());
    }
    Ok((&bytes[4..4 + len], &bytes[4 + len..]))
}
pub async fn read(reader: &mut (impl AsyncRead + Unpin)) -> Result<Message, String> {
    let mut header = [0; 26];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|_| "Snapcast connection closed")?;
    let len = u32_at(&header, 22)? as usize;
    if len > MAX_MESSAGE {
        return Err("Snapcast message exceeded 1 MiB".into());
    }
    let mut body = vec![0; len];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| "Truncated Snapcast message")?;
    Ok(Message {
        kind: u16_at(&header, 0),
        refers: u16_at(&header, 4),
        sent: timestamp(&header, 6)?,
        body,
    })
}
pub async fn write(
    writer: &mut (impl AsyncWrite + Unpin),
    kind: u16,
    id: u16,
    at: i64,
    body: &[u8],
) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(26 + body.len());
    bytes.extend(kind.to_le_bytes());
    bytes.extend(id.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes.extend(((at / 1_000_000) as i32).to_le_bytes());
    bytes.extend(((at % 1_000_000) as i32).to_le_bytes());
    bytes.extend([0; 8]);
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(body);
    tokio::time::timeout(std::time::Duration::from_secs(1), writer.write_all(&bytes))
        .await
        .map_err(|_| "Snapcast write timed out")?
        .map_err(|_| "Unable to send Snapcast message".into())
}
pub fn json_body(value: serde_json::Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&value).unwrap();
    let mut output = (bytes.len() as u32).to_le_bytes().to_vec();
    output.append(&mut bytes);
    output
}
#[derive(Clone)]
pub struct Clock {
    origin: Instant,
    samples: Vec<(i64, i64)>,
    pub offset: Option<i64>,
}
impl Clock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            samples: Vec::new(),
            offset: None,
        }
    }
    pub fn now(&self) -> i64 {
        self.origin.elapsed().as_micros() as i64
    }
    pub fn sample(
        &mut self,
        sent_local: i64,
        received_local: i64,
        server_sent: i64,
        server_delta: i64,
    ) -> bool {
        let to_client = received_local - server_sent;
        let roundtrip = server_delta + to_client;
        if received_local < sent_local || !(0..=500_000).contains(&roundtrip) {
            return false;
        }
        let offset = (server_delta - to_client) / 2;
        if self.samples.len() == 32 {
            self.samples.remove(0);
        }
        self.samples.push((roundtrip, offset));
        let mut samples = self.samples.clone();
        samples.sort_unstable_by_key(|s| s.0);
        let mut offsets: Vec<_> = samples.iter().take(8).map(|s| s.1).collect();
        offsets.sort_unstable();
        self.offset = Some(offsets[offsets.len() / 2]);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejects_oversized_and_truncated_messages_before_payload_allocation() {
        let mut header = vec![0; 26];
        header[22..26].copy_from_slice(&((MAX_MESSAGE + 1) as u32).to_le_bytes());
        assert!(
            read(&mut header.as_slice())
                .await
                .unwrap_err()
                .contains("1 MiB")
        );
        assert!(bytes_field(&[5, 0, 0, 0, 1]).is_err());
        assert!(timestamp(&[0, 0, 0, 0, 255, 255, 255, 127], 0).is_err());
    }
    #[test]
    fn clock_rejects_bad_rtt_and_uses_low_delay_samples() {
        let mut clock = Clock::new();
        for n in 0..10 {
            assert!(clock.sample(
                n * 1000,
                n * 1000 + 200,
                n * 1000 + 100 + 5_000_000,
                5_000_100
            ));
        }
        assert_eq!(clock.offset, Some(5_000_000));
        assert!(!clock.sample(0, 1_000_000, 0, 1_000_000));
        assert!(clock.sample(0, 80_000, 5_000_100, 5_000_100));
        assert_eq!(clock.offset, Some(5_000_000));
    }
}
