//! Built-in Snapcast v2 TCP receiver. No Snapclient executable or linked GPL code.
//! Implements the published framing, NTP-style clock estimate, bounded PCM/FLAC
//! decoding and timestamped playout through the existing fastframe audio backend.
use super::{Settings, Shared, protocol as p};
use libresonic::playback::Cancellation;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_FLAC, CodecParameters, Decoder, DecoderOptions},
    formats::Packet,
};
use tokio::{io::AsyncWriteExt, net::TcpStream};

#[derive(Clone, Copy, Debug)]
struct Format {
    rate: u32,
    channels: usize,
    bits: u32,
}
impl Format {
    fn validate(self) -> Result<Self, String> {
        if !(8000..=192000).contains(&self.rate)
            || !(1..=2).contains(&self.channels)
            || ![16, 24, 32].contains(&self.bits)
        {
            return Err(
                "Snapcast requires mono/stereo PCM or FLAC, 8–192 kHz, 16/24/32 bit".into(),
            );
        }
        Ok(self)
    }
}
struct Codec {
    format: Format,
    flac: Option<Box<dyn Decoder>>,
}
impl Codec {
    fn new(body: &[u8]) -> Result<Self, String> {
        let (codec, rest) = p::bytes_field(body)?;
        let (header, remaining) = p::bytes_field(rest)?;
        if !remaining.is_empty() {
            return Err("Invalid Snapcast codec header".into());
        }
        match codec {
            b"pcm" => {
                if !header.starts_with(b"RIFF") || header.get(8..12) != Some(b"WAVE") {
                    return Err("Invalid Snapcast PCM header".into());
                }
                let mut pos = 12;
                while pos + 8 <= header.len() {
                    let size = p::u32_at(header, pos + 4)? as usize;
                    if header.get(pos..pos + 4) == Some(b"fmt ") {
                        if size < 16
                            || pos + 8 + size > header.len()
                            || header[pos + 8..pos + 10] != [1, 0]
                        {
                            return Err("Unsupported Snapcast PCM format".into());
                        }
                        let channels =
                            u16::from_le_bytes(header[pos + 10..pos + 12].try_into().unwrap())
                                as usize;
                        let rate = p::u32_at(header, pos + 12)?;
                        let bits =
                            u16::from_le_bytes(header[pos + 22..pos + 24].try_into().unwrap())
                                as u32;
                        return Ok(Self {
                            format: Format {
                                rate,
                                channels,
                                bits,
                            }
                            .validate()?,
                            flac: None,
                        });
                    }
                    pos = pos
                        .checked_add(8 + size + (size % 2))
                        .ok_or("Invalid Snapcast PCM header")?;
                }
                Err("Missing Snapcast PCM format".into())
            }
            b"flac" => {
                if !header.starts_with(b"fLaC")
                    || header.len() < 42
                    || header[4] & 0x7f != 0
                    || header[5..8] != [0, 0, 34]
                {
                    return Err("Invalid Snapcast FLAC stream information".into());
                }
                let mut params = CodecParameters::new();
                params.codec = CODEC_TYPE_FLAC;
                params.with_extra_data(header[8..42].to_vec().into_boxed_slice());
                let decoder = symphonia::default::get_codecs()
                    .make(&params, &DecoderOptions::default())
                    .map_err(|_| "Unsupported Snapcast FLAC header")?;
                let params = decoder.codec_params();
                let format = Format {
                    rate: params.sample_rate.ok_or("Missing FLAC rate")?,
                    channels: params.channels.ok_or("Missing FLAC channels")?.count(),
                    bits: params.bits_per_sample.unwrap_or(16),
                }
                .validate()?;
                Ok(Self {
                    format,
                    flac: Some(decoder),
                })
            }
            _ => Err(
                "Unsupported Snapcast codec; configure Snapserver with codec=flac or codec=pcm"
                    .into(),
            ),
        }
    }
    fn decode(&mut self, bytes: &[u8]) -> Result<Vec<f32>, String> {
        if let Some(decoder) = &mut self.flac {
            let packet = Packet::new_from_slice(0, 0, 0, bytes);
            let decoded = decoder
                .decode(&packet)
                .map_err(|_| "Invalid Snapcast FLAC frame")?;
            if decoded
                .frames()
                .saturating_mul(decoded.spec().channels.count())
                > p::MAX_CHUNK_SAMPLES
            {
                return Err("Snapcast decoded chunk exceeded its sample limit".into());
            }
            let mut output = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
            output.copy_interleaved_ref(decoded);
            Ok(output.samples().to_vec())
        } else {
            // Snapcast transports 24-bit integer PCM in a sign-extended
            // 32-bit container, unlike packed 24-bit WAV data.
            let width = if self.format.bits == 16 { 2 } else { 4 };
            if !bytes.len().is_multiple_of(width * self.format.channels)
                || bytes.len() / width > p::MAX_CHUNK_SAMPLES
            {
                return Err("Invalid Snapcast PCM chunk length".into());
            }
            Ok(bytes
                .chunks_exact(width)
                .map(|v| match width {
                    2 => f32::from(i16::from_le_bytes([v[0], v[1]])) / 32768.0,
                    _ if self.format.bits == 24 => {
                        (i32::from_le_bytes(v.try_into().unwrap()) as f32) / 8388608.0
                    }
                    _ => (i32::from_le_bytes(v.try_into().unwrap()) as f32) / 2147483648.0,
                })
                .collect())
        }
    }
}
struct Chunk {
    at: i64,
    samples: Vec<f32>,
    format: Format,
}
impl Chunk {
    fn end(&self) -> i64 {
        self.at
            + (self.samples.len() / self.format.channels) as i64 * 1_000_000
                / i64::from(self.format.rate)
    }
}
struct Buffer {
    chunks: VecDeque<Chunk>,
    samples: usize,
    buffer_us: i64,
    gain: f32,
}
impl Default for Buffer {
    fn default() -> Self {
        Self {
            chunks: VecDeque::with_capacity(256),
            samples: 0,
            buffer_us: 1_000_000,
            gain: 1.0,
        }
    }
}
impl Buffer {
    fn push(&mut self, chunk: Chunk) -> Result<(), String> {
        // A server cannot grow memory indefinitely, even without a functioning
        // output or with maliciously far-future chunk timestamps.
        if self.samples + chunk.samples.len() > 2_000_000 || self.chunks.len() >= 256 {
            return Err("Snapcast receiver buffer exceeded its limit".into());
        }
        if self
            .chunks
            .back()
            .is_some_and(|previous| chunk.at < previous.at)
        {
            return Err("Snapcast chunk timestamps moved backwards".into());
        }
        self.samples += chunk.samples.len();
        self.chunks.push_back(chunk);
        Ok(())
    }
    fn discard_before(&mut self, at: i64) {
        while self.chunks.front().is_some_and(|c| c.end() <= at) {
            self.samples -= self.chunks.pop_front().unwrap().samples.len();
        }
    }
    fn frame(&self, at: f64) -> [f32; 2] {
        let Some(chunk) = self.chunks.front() else {
            return [0.0; 2];
        };
        let index = (at - chunk.at as f64) * f64::from(chunk.format.rate) / 1_000_000.0;
        if index < 0.0 {
            return [0.0; 2];
        }
        let n = index.floor() as usize;
        let channels = chunk.format.channels;
        if n >= chunk.samples.len() / channels {
            return [0.0; 2];
        }
        let mix = (index - index.floor()) as f32;
        std::array::from_fn(|channel| {
            let channel = channel.min(channels - 1);
            let first = chunk.samples[n * channels + channel];
            let second = chunk
                .samples
                .get((n + 1) * channels + channel)
                .copied()
                .unwrap_or(first);
            (first + (second - first) * mix) * self.gain
        })
    }
}
#[derive(Clone)]
struct Playout {
    buffer: Arc<Mutex<Buffer>>,
    offset: Arc<AtomicI64>,
    synced: Arc<AtomicUsize>,
    clock: p::Clock,
    device_clock: Arc<Mutex<Option<fastframe_audio::Clock>>>,
}
impl Playout {
    fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Buffer::default())),
            offset: Arc::new(AtomicI64::new(0)),
            synced: Arc::new(AtomicUsize::new(0)),
            clock: p::Clock::new(),
            device_clock: Arc::new(Mutex::new(None)),
        }
    }
    fn renderer(&self) -> Renderer {
        Renderer {
            state: self.clone(),
            rate: 44100,
            channels: 2,
            next: None,
        }
    }
}
struct Renderer {
    state: Playout,
    rate: u32,
    channels: usize,
    next: Option<f64>,
}
impl Renderer {
    fn render_at(&mut self, now: i64, latency_us: i64, out: &mut [f32]) {
        out.fill(0.0);
        if self.state.synced.load(Ordering::Acquire) < 5 {
            return;
        }
        let Ok(mut buffer) = self.state.buffer.try_lock() else {
            self.next = None;
            return;
        };
        let target = (now + self.state.offset.load(Ordering::Acquire) + latency_us
            - buffer.buffer_us) as f64;
        let mut position = self.next.unwrap_or(target);
        let error = target - position;
        if error.abs() > 50_000.0 {
            position = target;
        }
        // A small resampling adjustment follows server/device clock drift;
        // larger discontinuities discard stale sound instead of replaying it.
        let step =
            1_000_000.0 / f64::from(self.rate) * (1.0 + (error / 5_000_000.0).clamp(-0.001, 0.001));
        for frame in out.chunks_mut(self.channels) {
            buffer.discard_before(position as i64);
            let samples = buffer.frame(position);
            for (channel, sample) in frame.iter_mut().enumerate() {
                *sample = if channel < 2 { samples[channel] } else { 0.0 };
            }
            position += step;
        }
        self.next = Some(position);
    }
}
impl fastframe_audio::Render for Renderer {
    fn configure(&mut self, rate: u32, channels: u16) {
        self.rate = rate;
        self.channels = usize::from(channels).max(1);
        self.next = None;
    }
    fn render(&mut self, out: &mut [f32]) {
        let latency = self
            .state
            .device_clock
            .try_lock()
            .ok()
            .and_then(|clock| clock.as_ref().map(|c| c.latency().as_micros() as i64))
            .unwrap_or(0);
        self.render_at(self.state.clock.now(), latency, out);
    }
}
struct AbortTask<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Run the built-in receiver, reconnecting until cancelled. `capture` is a
/// diagnostic virtual device using exactly the native timestamp renderer.
/// It never changes the app's saved audio device or writes a user library file.
pub async fn receive(
    settings: Settings,
    shared: Shared,
    cancel: Cancellation,
    capture: Option<std::path::PathBuf>,
) -> Result<(), String> {
    settings.validate()?;
    let mut retry = 1;
    while !cancel.is_cancelled() {
        shared.receiver_status("Connecting to Snapserver");
        let began = Instant::now();
        let result = tokio::select! {
            result = connection(&settings, &shared, &cancel, capture.clone()) => result,
            _ = cancel.cancelled() => break,
        };
        if began.elapsed() > Duration::from_secs(5) {
            retry = 1;
        }
        if cancel.is_cancelled() {
            break;
        }
        let message = result
            .err()
            .unwrap_or_else(|| "Snapcast connection closed".into());
        shared.receiver_status(&format!("{message}; reconnecting"));
        let until = Instant::now() + Duration::from_secs(retry);
        while !cancel.is_cancelled() && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        retry = (retry * 2).min(10);
    }
    shared.receiver_status("Disabled");
    Ok(())
}
async fn connection(
    settings: &Settings,
    shared: &Shared,
    cancel: &Cancellation,
    capture: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let state = Playout::new();
    let connect = tokio::time::timeout(
        Duration::from_secs(3),
        TcpStream::connect((settings.host.as_str(), settings.stream_port)),
    );
    let socket = connect
        .await
        .map_err(|_| "Snapcast connection timed out")?
        .map_err(|_| "Cannot connect to Snapserver")?;
    socket
        .set_nodelay(true)
        .map_err(|_| "Cannot configure Snapcast socket")?;
    let (mut reader, mut writer) = socket.into_split();
    let id = if settings.client_id.is_empty() {
        "sonicfast"
    } else {
        &settings.client_id
    };
    let hello = p::json_body(
        serde_json::json!({"ID":id,"MAC":id,"HostName":"Sonicfast","ClientName":"Sonicfast","Instance":15,"OS":std::env::consts::OS,"Arch":std::env::consts::ARCH,"Version":env!("CARGO_PKG_VERSION"),"SnapStreamProtocolVersion":2}),
    );
    p::write(&mut writer, 5, 1, state.clock.now(), &hello).await?;
    let (messages, mut incoming) = tokio::sync::mpsc::channel(32);
    let _read = AbortTask(tokio::spawn(async move {
        loop {
            let result = tokio::time::timeout(Duration::from_secs(5), p::read(&mut reader))
                .await
                .unwrap_or_else(|_| Err("Snapcast read timed out".into()));
            let failed = result.is_err();
            if messages.send(result).await.is_err() || failed {
                break;
            }
        }
    }));
    let mut codec: Option<Codec> = None;
    let mut output: Option<fastframe_audio::Output<Renderer>> = None;
    let mut capture_output: Option<AbortTask<Result<(), String>>> = None;
    let mut times = HashMap::new();
    let mut serial = 1u16;
    let mut clock = state.clock.clone();
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut last_sync = Instant::now() - Duration::from_secs(2);
    let mut last_response = Instant::now();
    let mut server_buffer = 1000_i64;
    let mut server_latency = 0_i64;
    let mut volume = 100_u64;
    let mut muted = false;
    loop {
        tokio::select! {
            message=incoming.recv()=>{
                let message=message.ok_or("Snapcast reader stopped")??;
                match message.kind {
                    1=>{
                        let new=Codec::new(&message.body)?;
                        let description=if new.flac.is_some(){"FLAC"}else{"PCM"};
                        shared.receiver_status(&format!("Receiving {description}; synchronizing clock"));
                        state.buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner).chunks.clear();
                        state.buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner).samples=0;
                        codec=Some(new);
                        if capture.is_some() {
                            if capture_output.is_none(){capture_output=Some(AbortTask(tokio::spawn(capture_device(state.clone(),capture.clone().unwrap()))));}
                        } else if output.is_none() {
                            let renderer=state.renderer(); let device=settings.receiver_device.clone();
                            let opened=tokio::task::spawn_blocking(move|| {
                                let device=if device.is_empty(){fastframe_audio::Device::Default}else{fastframe_audio::Device::Id(fastframe_audio::DeviceId::new(device))};
                                fastframe_audio::Output::open(fastframe_audio::OutputOptions{device,channels:2,sample_rate:Some(44100),buffer:fastframe_audio::Buffer::FixedOnWindows(fastframe_audio::BufferSize::Duration(Duration::from_millis(20))),max_block_frames:Some(441),media_name:Some("Sonicfast Snapcast".into()),..Default::default()},renderer)
                            });
                            let mut opened=tokio::time::timeout(Duration::from_secs(3),opened).await.map_err(|_|"Receiver audio device open timed out")?.map_err(|_|"Receiver audio device failed")?.map_err(|_|"Receiver audio device unavailable")?;
                            *state.device_clock.lock().unwrap_or_else(std::sync::PoisonError::into_inner)=Some(opened.clock());
                            opened.resume();output=Some(opened);
                        }
                    }
                    2=>{
                        let Some(mut decoder)=codec.take() else {return Err("Snapcast audio arrived before its codec header".into());};
                        let at=p::timestamp(&message.body,0)?;
                        let (bytes,remaining)=p::bytes_field(message.body.get(8..).ok_or("Truncated Snapcast audio")?)?;
                        if !remaining.is_empty(){return Err("Invalid Snapcast audio chunk".into());}
                        let bytes=bytes.to_vec();
                        let (decoder,samples)=tokio::task::spawn_blocking(move||{let samples=decoder.decode(&bytes);(decoder,samples)}).await.map_err(|_|"Snapcast decoder stopped")?;
                        let format=decoder.format;codec=Some(decoder);
                        let samples=samples?;
                        if samples.iter().any(|sample| !sample.is_finite() || sample.abs() > 1.001) {return Err("Invalid Snapcast sample data".into());}
                        if state.synced.load(Ordering::Acquire)>=5 && (at-(clock.now()+clock.offset.unwrap_or(0))).abs()>10_000_000 {return Err("Snapcast audio timestamp outside the playback window".into());}
                        state.buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(Chunk{at,samples,format})?;
                    }
                    3=>{
                        let (json,remaining)=p::bytes_field(&message.body)?;
                        if !remaining.is_empty(){return Err("Invalid Snapcast settings".into());}
                        let json:serde_json::Value=serde_json::from_slice(json).map_err(|_|"Invalid Snapcast settings JSON")?;
                        server_buffer=json["bufferMs"].as_i64().unwrap_or(server_buffer);
                        server_latency=json["latency"].as_i64().unwrap_or(server_latency);
                        volume=json["volume"].as_u64().unwrap_or(volume).min(100);
                        muted=json["muted"].as_bool().unwrap_or(muted);
                        if !(10..=5000).contains(&server_buffer)||!(-5000..=5000).contains(&server_latency){return Err("Snapcast server buffer/latency exceeds supported limits".into());}
                        let mut buffer=state.buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        buffer.buffer_us=(server_buffer-server_latency-i64::from(settings.latency_ms)).clamp(0,5000)*1000;
                        buffer.gain=if muted{0.0}else{volume as f32/100.0};
                    }
                    4=>{
                        if let Some(sent)=times.remove(&message.refers) {
                            let received=clock.now();
                            let delta=p::timestamp(&message.body,0)?;
                            if clock.sample(sent,received,message.sent,delta){
                                state.offset.store(clock.offset.unwrap_or(0),Ordering::Release);
                                let n=state.synced.fetch_add(1,Ordering::AcqRel)+1;
                                last_response=Instant::now();
                                if n==5 {shared.receiver_status("Connected; clock synchronized");}
                            }
                        }
                    }
                    8=>return Err("Snapserver rejected this receiver (authentication may be required)".into()),
                    _=>{},
                }
            }
            _=tick.tick()=>{
                if cancel.is_cancelled(){let _=writer.shutdown().await;return Ok(());}
                if last_response.elapsed()>Duration::from_secs(5){return Err("Snapcast clock synchronization timed out".into());}
                let interval=if state.synced.load(Ordering::Acquire)<20{Duration::from_millis(50)}else{Duration::from_secs(1)};
                if last_sync.elapsed()>=interval{
                    serial=serial.wrapping_add(1).max(2);
                    let at=clock.now();times.insert(serial,at);
                    times.retain(|_,sent|at-*sent<5_000_000);
                    p::write(&mut writer,4,serial,at,&[0;8]).await?;
                    last_sync=Instant::now();
                }
                if let Some(output)=&mut output {output.maintain();if output.failed(){return Err("Receiver audio device lost".into());}}
                if capture_output.as_ref().is_some_and(|task|task.0.is_finished()){return Err("Diagnostic capture stopped".into());}
            }
        }
    }
}
async fn capture_device(state: Playout, path: std::path::PathBuf) -> Result<(), String> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|_| "Unable to create diagnostic capture")?;
    file.write_all(b"SFCAP001")
        .await
        .map_err(|_| "Unable to write diagnostic capture")?;
    let mut renderer = state.renderer();
    let mut buffer = vec![0.0_f32; 882];
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let now = state.clock.now();
        let server = now + state.offset.load(Ordering::Acquire);
        renderer.render_at(now, 0, &mut buffer);
        let mut record = server.to_le_bytes().to_vec();
        record.extend((buffer.len() as u32).to_le_bytes());
        for sample in &buffer {
            record.extend(sample.to_le_bytes());
        }
        file.write_all(&record)
            .await
            .map_err(|_| "Unable to write diagnostic capture")?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_playout_waits_drops_stale_audio_and_obeys_mute() {
        let state = Playout::new();
        state.offset.store(0, Ordering::Release);
        state.synced.store(5, Ordering::Release);
        {
            let mut b = state.buffer.lock().unwrap();
            b.buffer_us = 1000;
            b.push(Chunk {
                at: 1000,
                samples: vec![0.25; 882],
                format: Format {
                    rate: 44100,
                    channels: 2,
                    bits: 16,
                },
            })
            .unwrap();
        }
        let mut render = state.renderer();
        let mut samples = [1.0; 20];
        render.render_at(0, 0, &mut samples);
        assert_eq!(samples, [0.0; 20]);
        render.next = None;
        render.render_at(2000, 0, &mut samples);
        assert_eq!(samples, [0.25; 20]);
        state.buffer.lock().unwrap().gain = 0.0;
        render.next = None;
        render.render_at(2000, 0, &mut samples);
        assert_eq!(samples, [0.0; 20]);
        render.next = None;
        render.render_at(50000, 0, &mut samples);
        assert_eq!(samples, [0.0; 20]);
        assert!(state.buffer.lock().unwrap().chunks.is_empty());
    }
    #[test]
    fn integer_pcm_handles_sign_extended_24bit_and_invalid_alignment() {
        let mut codec = Codec {
            format: Format {
                rate: 48000,
                channels: 1,
                bits: 24,
            },
            flac: None,
        };
        let bytes: Vec<_> = [-8388608_i32, 0, 4194304]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect();
        assert_eq!(codec.decode(&bytes).unwrap(), vec![-1.0, 0.0, 0.5]);
        assert!(codec.decode(&[0; 3]).is_err());
        codec.format.bits = 16;
        assert_eq!(
            codec.decode(&(-16384_i16).to_le_bytes()).unwrap(),
            vec![-0.5]
        );
    }
    #[test]
    fn renderer_resamples_mono_and_hard_resync_discards_old_chunks() {
        let state = Playout::new();
        state.synced.store(5, Ordering::Release);
        let source: Vec<_> = (0..48000)
            .map(|i| (std::f32::consts::TAU * 440.0 * i as f32 / 48000.0).sin() * 0.1)
            .collect();
        {
            let mut buffer = state.buffer.lock().unwrap();
            buffer.buffer_us = 0;
            buffer
                .push(Chunk {
                    at: 0,
                    samples: source,
                    format: Format {
                        rate: 48000,
                        channels: 1,
                        bits: 16,
                    },
                })
                .unwrap();
        }
        let mut renderer = state.renderer();
        let mut samples = vec![0.0; 44100 * 2];
        renderer.render_at(0, 0, &mut samples);
        assert!(
            samples
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[0] == frame[1])
        );
        let crossings = samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| frame[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        assert!((crossings as i32 - 440).abs() <= 1);
        renderer.render_at(2_000_000, 0, &mut samples[..100]);
        assert!(samples[..100].iter().all(|v| *v == 0.0));
        assert!(state.buffer.lock().unwrap().chunks.is_empty());
    }

    #[test]
    fn receiver_buffer_and_unknown_codecs_are_bounded() {
        let mut b = Buffer::default();
        for i in 0..256 {
            b.push(Chunk {
                at: i * 1000,
                samples: vec![0.0; 2],
                format: Format {
                    rate: 44100,
                    channels: 2,
                    bits: 16,
                },
            })
            .unwrap();
        }
        assert!(
            b.push(Chunk {
                at: 300000,
                samples: vec![0.0; 2],
                format: Format {
                    rate: 44100,
                    channels: 2,
                    bits: 16
                }
            })
            .is_err()
        );
        let mut header = 4_u32.to_le_bytes().to_vec();
        header.extend(b"opus");
        header.extend(0_u32.to_le_bytes());
        assert!(Codec::new(&header).err().unwrap().contains("codec=flac"));
    }
}
