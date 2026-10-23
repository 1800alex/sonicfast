use super::Shared;
use crate::{eq::SharedEq, sink::PcmOutput, vis::AudioTap};
use libresonic::playback::{AudioOutput, Cancellation, OutputError, PcmBlock};
use std::{
    io::Write,
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};
const RATE: u64 = 44100;
const CHUNK_FRAMES: usize = 882; // 20 ms; at most one chunk ahead of real time.

/// Real-time raw PCM source for Snapserver's `tcp://...?sampleformat=44100:16:2`.
/// Only the decoder's feeder thread calls this object. No device is opened.
pub struct Sender {
    shared: Shared,
    processing: PcmOutput,
    socket: Option<TcpStream>,
    deadline: Option<Instant>,
    volume: u16,
    frames_in_chunk: usize,
}
impl Sender {
    pub fn new(shared: Shared, eq: SharedEq, tap: Arc<AudioTap>) -> Self {
        Self {
            shared,
            processing: PcmOutput::new(None, eq, tap),
            socket: None,
            deadline: None,
            volume: u16::MAX,
            frames_in_chunk: 0,
        }
    }
    fn connect(&mut self, cancel: &Cancellation) -> Result<(), OutputError> {
        if self.socket.is_some() {
            return Ok(());
        }
        let until = Instant::now() + Duration::from_secs(3);
        let targets = loop {
            if cancel.is_cancelled() {
                return Err(OutputError::Cancelled);
            }
            let targets = self.shared.targets();
            if !targets.is_empty() {
                break targets;
            }
            if Instant::now() >= until {
                self.shared
                    .sender_status("Source address unavailable; check settings and retry playback");
                return Err(OutputError::DeviceLost);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let deadline = Instant::now() + Duration::from_millis(750);
        let mut connected = None;
        for target in targets.into_iter().take(8) {
            if cancel.is_cancelled() {
                return Err(OutputError::Cancelled);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            if let Ok(socket) =
                TcpStream::connect_timeout(&target, remaining.min(Duration::from_millis(200)))
            {
                connected = Some(socket);
                break;
            }
        }
        let socket = connected.ok_or_else(|| {
            self.shared
                .sender_status("Source connection failed; check Snapserver and retry playback");
            OutputError::DeviceLost
        })?;
        socket
            .set_nonblocking(true)
            .map_err(|_| OutputError::DeviceLost)?;
        socket
            .set_nodelay(true)
            .map_err(|_| OutputError::DeviceLost)?;
        self.socket = Some(socket);
        self.deadline = None;
        self.shared.sender_status("Sending audio");
        Ok(())
    }
    fn send(&mut self, samples: &[f64], cancel: &Cancellation) -> Result<(), OutputError> {
        self.connect(cancel)?;
        for chunk in samples.chunks(CHUNK_FRAMES * 2) {
            let deadline = self.deadline.unwrap_or_else(Instant::now);
            while Instant::now() < deadline {
                if cancel.is_cancelled() {
                    return Err(OutputError::Cancelled);
                }
                std::thread::sleep(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(5)),
                );
            }
            let bytes = encode(chunk, self.volume);
            let mut left = bytes.as_slice();
            let expires = Instant::now() + Duration::from_millis(500);
            while !left.is_empty() {
                if cancel.is_cancelled() {
                    return Err(OutputError::Cancelled);
                }
                if Instant::now() >= expires {
                    return Err(OutputError::DeviceLost);
                }
                match self.socket.as_mut().unwrap().write(left) {
                    Ok(0) => return Err(OutputError::DeviceLost),
                    Ok(written) => left = &left[written..],
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return Err(OutputError::DeviceLost),
                }
            }
            self.frames_in_chunk = (self.frames_in_chunk + chunk.len() / 2) % CHUNK_FRAMES;
            // Keep an anchored sample clock: ordinary scheduler jitter must
            // not make the source progressively slower than real time. Limit
            // catch-up after a long stall to two chunks.
            let next = deadline + Duration::from_secs_f64(chunk.len() as f64 / (RATE * 2) as f64);
            let now = Instant::now();
            self.deadline = Some(
                if now.saturating_duration_since(next) > Duration::from_millis(40) {
                    now + Duration::from_secs_f64(chunk.len() as f64 / (RATE * 2) as f64)
                } else {
                    next
                },
            );
        }
        Ok(())
    }
    fn close(&mut self) {
        self.socket = None;
        self.deadline = None;
        self.frames_in_chunk = 0;
    }
}
fn encode(samples: &[f64], volume: u16) -> Vec<u8> {
    let gain = if volume == 0 {
        0.0
    } else {
        (0.1 + 0.9 * f64::from(volume) / f64::from(u16::MAX)).powi(3)
    };
    samples
        .iter()
        .flat_map(|sample| {
            let sample = if sample.is_finite() {
                (*sample * gain).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            (if sample < 0.0 {
                (sample * 32768.0).round() as i16
            } else {
                (sample * 32767.0).round() as i16
            })
            .to_le_bytes()
        })
        .collect()
}
impl AudioOutput for Sender {
    fn reset(&mut self) -> Result<(), OutputError> {
        self.close();
        self.processing.reset()?;
        self.shared.sender_status("Ready; waiting for playback");
        Ok(())
    }
    fn write(&mut self, block: PcmBlock<'_>, cancel: &Cancellation) -> Result<(), OutputError> {
        if cancel.is_cancelled() {
            return Err(OutputError::Cancelled);
        }
        let samples = self.processing.process_for_network(block);
        let result = self.send(&samples, cancel);
        if result.is_err() {
            self.close();
            self.shared.sender_status(if cancel.is_cancelled() {
                "Playback cancelled"
            } else {
                "Source disconnected; retry playback to reconnect"
            });
        }
        result
    }
    fn set_volume(&mut self, volume: u16) {
        self.volume = volume;
    }
    fn pause(&mut self) {
        self.close();
        self.shared.sender_status("Paused");
    }
    fn resume(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn pending_ms(&self) -> u64 {
        self.deadline.map_or(0, |at| {
            at.saturating_duration_since(Instant::now()).as_millis() as u64
        })
    }
    fn finish(&mut self, cancel: &Cancellation) -> Result<(), OutputError> {
        let mut samples = self.processing.finish_for_network();
        // Snapserver reads whole fixed-size source chunks. Complete the final
        // chunk so closing the TCP source cannot discard the song's last samples.
        let partial = (self.frames_in_chunk + samples.len() / 2) % CHUNK_FRAMES;
        if partial != 0 {
            samples.resize(samples.len() + (CHUNK_FRAMES - partial) * 2, 0.0);
        }
        self.send(&samples, cancel)
    }
    fn drained(&mut self) -> Result<bool, OutputError> {
        Ok(self.pending_ms() == 0)
    }
    fn stop(&mut self) {
        self.close();
        self.processing.stop();
        self.shared.sender_status("Stopped");
    }
}

/// Chosen once when a player is created. Changing routing restarts it paused.
pub struct RoutedOutput {
    output: Box<dyn AudioOutput>,
    receive_only: bool,
}
impl RoutedOutput {
    pub fn new(shared: Shared, device: Option<String>, eq: SharedEq, tap: Arc<AudioTap>) -> Self {
        let settings = shared.settings();
        let output: Box<dyn AudioOutput> = if settings.send {
            Box::new(Sender::new(shared, eq, tap))
        } else {
            Box::new(PcmOutput::new(device, eq, tap))
        };
        Self {
            output,
            receive_only: settings.receive && !settings.send,
        }
    }
}
impl AudioOutput for RoutedOutput {
    fn reset(&mut self) -> Result<(), OutputError> {
        self.output.reset()
    }
    fn write(&mut self, block: PcmBlock<'_>, cancel: &Cancellation) -> Result<(), OutputError> {
        if self.receive_only {
            return Err(OutputError::NoDevice);
        }
        self.output.write(block, cancel)
    }
    fn set_volume(&mut self, volume: u16) {
        self.output.set_volume(volume);
    }
    fn pause(&mut self) {
        self.output.pause();
    }
    fn resume(&mut self) -> Result<(), OutputError> {
        self.output.resume()
    }
    fn pending_ms(&self) -> u64 {
        self.output.pending_ms()
    }
    fn finish(&mut self, cancel: &Cancellation) -> Result<(), OutputError> {
        self.output.finish(cancel)
    }
    fn drained(&mut self) -> Result<bool, OutputError> {
        self.output.drained()
    }
    fn stop(&mut self) {
        self.output.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, net::TcpListener};
    #[test]
    fn signed_little_endian_pcm_sanitizes_clips_and_mutes() {
        assert_eq!(
            encode(&[-2.0, -1.0, 0.0, 1.0, 2.0, f64::NAN], u16::MAX),
            [-32768_i16, -32768, 0, 32767, 32767, 0]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert!(encode(&[0.7; 20], 0).iter().all(|b| *b == 0));
    }
    #[test]
    fn unreachable_first_dns_address_falls_back_without_opening_a_local_device() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let unavailable = closed.local_addr().unwrap();
        drop(closed);
        let shared = Shared::resolved(
            super::super::Settings {
                send: true,
                allow_insecure: true,
                ..Default::default()
            },
            listener.local_addr().unwrap(),
        )
        .unwrap();
        shared.0.lock().unwrap().targets.insert(0, unavailable);
        let reader = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            socket.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let mut sender = Sender::new(shared, crate::eq::shared(), AudioTap::new());
        sender
            .write(
                PcmBlock::new(&[0.1; 128], 44100, 2).unwrap(),
                &Cancellation::default(),
            )
            .unwrap();
        sender.stop();
        assert_eq!(reader.join().unwrap().len(), 256);
    }

    #[test]
    fn finish_flushes_a_partial_server_chunk_before_closing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let shared = Shared::resolved(
            super::super::Settings {
                send: true,
                allow_insecure: true,
                ..Default::default()
            },
            listener.local_addr().unwrap(),
        )
        .unwrap();
        let reader = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            socket.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let mut sender = Sender::new(shared, crate::eq::shared(), AudioTap::new());
        let cancel = Cancellation::default();
        sender
            .write(PcmBlock::new(&[0.1; 246], 44100, 2).unwrap(), &cancel)
            .unwrap();
        sender.finish(&cancel).unwrap();
        sender.stop();
        let bytes = reader.join().unwrap();
        assert!(bytes.len() > 246 * 2);
        assert_eq!(bytes.len() % (CHUNK_FRAMES * 4), 0);
    }

    #[test]
    fn sender_is_paced_and_cancelled_without_a_native_device() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let shared = Shared::resolved(
            super::super::Settings {
                send: true,
                allow_insecure: true,
                ..Default::default()
            },
            listener.local_addr().unwrap(),
        )
        .unwrap();
        let reader = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            s.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let mut sender = Sender::new(shared, crate::eq::shared(), AudioTap::new());
        let cancel = Cancellation::default();
        let samples = vec![0.1; 441 * 2];
        let start = Instant::now();
        for _ in 0..10 {
            sender
                .write(PcmBlock::new(&samples, 44100, 2).unwrap(), &cancel)
                .unwrap();
        }
        assert!(start.elapsed() >= Duration::from_millis(75));
        cancel.cancel();
        assert_eq!(
            sender.write(PcmBlock::new(&samples, 44100, 2).unwrap(), &cancel),
            Err(OutputError::Cancelled)
        );
        sender.stop();
        assert_eq!(reader.join().unwrap().len(), samples.len() * 2 * 10);
    }
}
