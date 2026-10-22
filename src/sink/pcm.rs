//! Librespot-independent PCM adapter over the retained native device path.

use libresonic::playback::{AudioOutput, Cancellation, OutputError, PcmBlock};

use super::*;

/// Single-track native output: source conversion -> shared EQ/tap/limiter -> rodio/fastframe.
pub struct PcmOutput {
    sink: RodioSink,
    processing: crate::vis::PcmProcessor,
    gain: Arc<AtomicU64>,
    format: Option<(u32, u16)>,
    resampler: Option<Resampler>,
    paused: bool,
    validated: bool,
    empty_at: Option<Instant>,
}

impl PcmOutput {
    /// Construction does not open a device. Call the AudioOutput methods only on a feeder thread.
    /// An explicit device must exist; the ordinary saved-device fallback remains unchanged.
    pub fn new(
        device: Option<String>,
        eq: crate::eq::SharedEq,
        tap: Arc<crate::vis::AudioTap>,
    ) -> Self {
        let gain = Arc::new(AtomicU64::new(1.0f64.to_bits()));
        let control = AudioControl::new(DEFAULT_BUFFER_MS);
        let sink_gain = Arc::clone(&gain);
        let processing_gain = Arc::clone(&gain);
        let sink = RodioSink::pcm(
            device,
            Arc::new(|_| {}),
            Box::new(move || f64::from_bits(sink_gain.load(Ordering::Relaxed))),
            DEFAULT_BUFFER_MS,
            Arc::clone(&control),
        );
        let processing = crate::vis::PcmProcessor::new(
            control,
            tap,
            Box::new(move || f64::from_bits(processing_gain.load(Ordering::Relaxed))),
            false,
            eq,
            Arc::new(AtomicU64::new(1.0f64.to_bits())),
        );
        Self {
            sink,
            processing,
            gain,
            format: None,
            resampler: None,
            paused: true,
            validated: false,
            empty_at: None,
        }
    }

    fn validate_device(&mut self) -> Result<(), OutputError> {
        if !self.validated {
            if let Some(wanted) = &self.sink.device {
                let devices =
                    fastframe_audio::output_devices().map_err(|_| OutputError::NoDevice)?;
                if !devices
                    .iter()
                    .any(|device| device.id.as_str() == wanted || device.name == *wanted)
                {
                    return Err(OutputError::NoDevice);
                }
            }
            self.validated = true;
        }
        Ok(())
    }

    fn convert(&mut self, block: PcmBlock<'_>) -> Vec<f64> {
        let format = (block.sample_rate(), block.channels());
        if self.format != Some(format) {
            self.resampler = Resampler::new(format.0, SAMPLE_RATE, NUM_CHANNELS as usize);
            self.format = Some(format);
            self.processing.reset();
        }
        let stereo: Vec<f64> = if block.channels() == 1 {
            block
                .samples()
                .iter()
                .flat_map(|sample| [*sample, *sample])
                .collect()
        } else {
            block.samples().to_vec()
        };
        match &mut self.resampler {
            Some(resampler) => resampler
                .process(
                    &stereo
                        .iter()
                        .map(|sample| *sample as f32)
                        .collect::<Vec<_>>(),
                )
                .into_iter()
                .map(f64::from)
                .collect(),
            None => stereo,
        }
    }

    fn write_normalized(
        &mut self,
        mut samples: Vec<f64>,
        cancel: &Cancellation,
    ) -> Result<(), OutputError> {
        if samples.is_empty() {
            return Ok(());
        }
        if cancel.is_cancelled() {
            return Err(OutputError::Cancelled);
        }
        self.processing.process(&mut samples);
        self.empty_at = None;
        let opening = self.sink.output.is_none();
        self.sink
            .write_pcm(
                samples.into_iter().map(|sample| sample as f32).collect(),
                Some(cancel),
            )
            .map_err(|_| {
                if cancel.is_cancelled() {
                    OutputError::Cancelled
                } else if opening {
                    OutputError::NoDevice
                } else {
                    OutputError::DeviceLost
                }
            })
    }
}

fn cubic_gain(volume: u16) -> f64 {
    if volume == 0 {
        0.0
    } else {
        (0.1 + 0.9 * f64::from(volume) / f64::from(u16::MAX)).powi(3)
    }
}

impl AudioOutput for PcmOutput {
    fn reset(&mut self) -> Result<(), OutputError> {
        self.sink.control.seeked();
        self.processing.reset();
        self.format = None;
        self.resampler = None;
        self.validated = false;
        self.empty_at = None;
        self.paused = true;
        Ok(())
    }

    fn write(&mut self, block: PcmBlock<'_>, cancel: &Cancellation) -> Result<(), OutputError> {
        if cancel.is_cancelled() {
            return Err(OutputError::Cancelled);
        }
        self.validate_device()?;
        let samples = self.convert(block);
        self.write_normalized(samples, cancel)
    }

    fn set_volume(&mut self, volume: u16) {
        self.gain
            .store(cubic_gain(volume).to_bits(), Ordering::Relaxed);
        self.sink.apply_volume();
    }

    fn pause(&mut self) {
        if self.paused {
            return;
        }
        if let Some(output) = &mut self.sink.output {
            output.transport.fade_out();
            let deadline = Instant::now()
                + TRANSPORT_FADE
                + Duration::from_millis(u64::from(self.sink.buffer_ms));
            while !output.transport.silent() && !output.failed() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
            output.sink.pause();
            output.device.pause();
        }
        self.paused = true;
    }

    fn resume(&mut self) -> Result<(), OutputError> {
        if self.paused {
            if let Some(output) = &mut self.sink.output {
                output
                    .run(&self.sink.control)
                    .map_err(|_| OutputError::DeviceLost)?;
                output.transport.fade_in();
                output.sink.play();
            }
            self.paused = false;
        }
        Ok(())
    }

    fn pending_ms(&self) -> u64 {
        self.sink.output.as_ref().map_or(0, |output| {
            let device_delay = u64::from(self.sink.buffer_ms).saturating_sub(
                self.empty_at
                    .map_or(0, |at| at.elapsed().as_millis() as u64),
            );
            output.queued.frames() * 1000 / u64::from(output.sample_rate) + device_delay
        })
    }

    fn finish(&mut self, cancel: &Cancellation) -> Result<(), OutputError> {
        if let Some(resampler) = &mut self.resampler {
            let tail = resampler
                .process(&[0.0; 64 * 2])
                .into_iter()
                .map(f64::from)
                .collect();
            self.write_normalized(tail, cancel)?;
        }
        // Existing sinc/limiter stages retain a bounded lookahead. Flush it,
        // without adding these padding frames to the source position.
        self.write_normalized(vec![0.0; 512 * 2], cancel)
    }

    fn drained(&mut self) -> Result<bool, OutputError> {
        let Some(output) = &mut self.sink.output else {
            return Ok(true);
        };
        output
            .run(&self.sink.control)
            .map_err(|_| OutputError::DeviceLost)?;
        if output.sink.empty() {
            let at = self.empty_at.get_or_insert_with(Instant::now);
            Ok(at.elapsed() >= Duration::from_millis(u64::from(self.sink.buffer_ms)))
        } else {
            self.empty_at = None;
            Ok(false)
        }
    }

    fn stop(&mut self) {
        if self.sink.output.is_some() {
            self.sink.control.interrupt();
            self.sink.control.stopped();
        }
        if let Some(output) = &mut self.sink.output {
            output.sink.pause();
            output.device.pause();
            output.transport.close();
            output.fed = false;
            output.last_write = None;
        }
        self.processing.reset();
        self.paused = true;
        self.empty_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_volume_has_exact_mute_full_scale_and_a_useful_middle() {
        assert_eq!(cubic_gain(0), 0.0);
        assert_eq!(cubic_gain(u16::MAX), 1.0);
        assert!((cubic_gain(u16::MAX / 2) - 0.1664).abs() < 0.001);
    }

    #[test]
    fn cancelled_pcm_does_not_open_a_device() {
        let cancel = Cancellation::default();
        cancel.cancel();
        let mut sink = PcmOutput::new(None, crate::eq::shared(), crate::vis::AudioTap::new());
        let block = PcmBlock::new(&[0.0; 16], SAMPLE_RATE, 2).unwrap();
        assert_eq!(sink.write(block, &cancel), Err(OutputError::Cancelled));
        assert!(sink.sink.output.is_none());
    }

    #[test]
    fn mono_48khz_conversion_keeps_duration_pitch_and_equal_channels_across_blocks() {
        let mut output = PcmOutput::new(None, crate::eq::shared(), crate::vis::AudioTap::new());
        let mut converted = Vec::new();
        for offset in (0..48000).step_by(1024) {
            let samples: Vec<f64> = (offset..(offset + 1024).min(48000))
                .map(|i| (std::f64::consts::TAU * 660.0 * i as f64 / 48000.0).sin() * 0.1)
                .collect();
            converted.extend(output.convert(PcmBlock::new(&samples, 48000, 1).unwrap()));
        }
        assert!((converted.len() as isize / 2 - 44100).abs() < 40);
        assert!(
            converted
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[0] == frame[1])
        );
        let signal: Vec<f64> = converted
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| frame[0])
            .collect();
        let crossings = signal
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        assert!((crossings as isize - 660).abs() <= 2);
        let rms = (signal.iter().map(|v| v * v).sum::<f64>() / signal.len() as f64).sqrt();
        assert!((rms - 0.1 / 2.0f64.sqrt()).abs() < 0.001);
    }
}
