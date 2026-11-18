use rodio::{Decoder, Source};
use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct DecodedAudio {
    pub samples: Arc<Vec<f32>>,
    pub channels: u16,
    pub rate: u32,
    pub duration: f64,
    pub peaks: Vec<f32>,
}
impl DecodedAudio {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let decoder = Decoder::new(Cursor::new(bytes.to_vec()))
            .map_err(|e| format!("Cannot decode music: {e}"))?;
        let channels = decoder.channels();
        let rate = decoder.sample_rate();
        let samples: Vec<f32> = decoder.convert_samples().collect();
        if samples.is_empty() {
            return Err("The music file contains no samples".into());
        }
        let duration = samples.len() as f64 / channels as f64 / rate as f64;
        let mut peaks = vec![0.0_f32; (duration * 200.0).ceil() as usize];
        for (frame, values) in samples.chunks_exact(channels as usize).enumerate() {
            let bucket = frame * 200 / rate as usize;
            for value in values {
                peaks[bucket] = peaks[bucket].max(value.abs());
            }
        }
        Ok(Self {
            samples: Arc::new(samples),
            channels,
            rate,
            duration,
            peaks,
        })
    }
}
/// Zero-copy source over shared PCM; playback position comes from the audio sink.
#[derive(Clone)]
struct SharedPcm {
    audio: DecodedAudio,
    index: usize,
}
impl Iterator for SharedPcm {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.audio.samples.get(self.index).copied().unwrap_or(0.0);
        self.index = self.index.saturating_add(1);
        Some(sample)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}
impl Source for SharedPcm {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.audio.channels
    }
    fn sample_rate(&self) -> u32 {
        self.audio.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.index = ((pos.as_secs_f64() * self.audio.rate as f64).round() as usize
            * self.audio.channels as usize)
            .min(self.audio.samples.len());
        Ok(())
    }
}
use rodio::cpal::{
    self,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
#[derive(Default)]
struct SampleClock {
    sequence: AtomicU64,
    position: AtomicU64,
    observed: AtomicU64,
    ready: AtomicBool,
}
impl SampleClock {
    fn publish(&self, position: f64, observed: u64) {
        self.sequence.fetch_add(1, Ordering::SeqCst);
        self.position.store(position.to_bits(), Ordering::SeqCst);
        self.observed.store(observed, Ordering::SeqCst);
        self.ready.store(true, Ordering::SeqCst);
        self.sequence.fetch_add(1, Ordering::SeqCst);
    }
    fn read(&self) -> Option<(f64, u64)> {
        loop {
            let a = self.sequence.load(Ordering::SeqCst);
            if a & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let ready = self.ready.load(Ordering::SeqCst);
            let position = self.position.load(Ordering::SeqCst);
            let observed = self.observed.load(Ordering::SeqCst);
            if a == self.sequence.load(Ordering::SeqCst) {
                return ready.then_some((f64::from_bits(position), observed));
            }
        }
    }
}
pub struct Transport {
    stream: Option<cpal::Stream>,
    pub audio: Option<DecodedAudio>,
    pub playing: bool,
    start: f64,
    clock: Instant,
    sample_clock: Arc<SampleClock>,
    errors: Option<Receiver<String>>,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            stream: None,
            audio: None,
            playing: false,
            start: 0.0,
            clock: Instant::now(),
            sample_clock: Arc::default(),
            errors: None,
        }
    }
}
fn output<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: DecodedAudio,
    start: f64,
    origin: Instant,
    clock: Arc<SampleClock>,
    errors: mpsc::Sender<String>,
) -> Result<cpal::Stream, String> {
    let index = (start * audio.rate as f64).round() as usize * audio.channels as usize;
    let mut source = rodio::source::UniformSourceIterator::<_, f32>::new(
        SharedPcm { audio, index },
        config.channels,
        config.sample_rate.0,
    );
    let channels = config.channels as usize;
    let rate = config.sample_rate.0 as f64;
    let mut frames = 0_u64;
    device
        .build_output_stream(
            config,
            move |buffer: &mut [T], info: &cpal::OutputCallbackInfo| {
                let stamp = info.timestamp();
                let latency = stamp
                    .playback
                    .duration_since(&stamp.callback)
                    .unwrap_or_default()
                    .as_secs_f64();
                // Schedule the first sample of this buffer on the device's audible timeline.
                // The UI interpolates only between successive device-clock observations.
                clock.publish(
                    frames as f64 / rate - latency,
                    origin.elapsed().as_nanos() as u64,
                );
                for sample in buffer.iter_mut() {
                    *sample = T::from_sample(source.next().unwrap_or(0.0));
                }
                frames += (buffer.len() / channels) as u64;
            },
            move |error| {
                let _ = errors.send(format!("Audio output interrupted: {error}"));
            },
            None,
        )
        .map_err(|e| e.to_string())
}
impl Transport {
    pub fn play(&mut self, time: f64) -> Result<(), String> {
        self.stop();
        self.start = time;
        self.clock = Instant::now();
        self.sample_clock = Arc::default();
        if let Some(audio) = &self.audio {
            let device = cpal::default_host()
                .default_output_device()
                .ok_or("No audio output device is available")?;
            let default = device.default_output_config().map_err(|e| e.to_string())?;
            let chosen = device
                .supported_output_configs()
                .ok()
                .and_then(|mut configs| {
                    configs.find(|c| {
                        c.channels() == default.channels()
                            && c.sample_format() == default.sample_format()
                            && c.min_sample_rate().0 <= audio.rate
                            && c.max_sample_rate().0 >= audio.rate
                    })
                })
                .map_or(default, |c| {
                    c.with_sample_rate(cpal::SampleRate(audio.rate))
                });
            let config = chosen.config();
            let format = chosen.sample_format();
            let (tx, rx) = mpsc::channel();
            macro_rules! build {
                ($ty:ty) => {
                    output::<$ty>(
                        &device,
                        &config,
                        audio.clone(),
                        time,
                        self.clock,
                        self.sample_clock.clone(),
                        tx,
                    )
                };
            }
            let stream = match format {
                cpal::SampleFormat::F32 => build!(f32),
                cpal::SampleFormat::F64 => build!(f64),
                cpal::SampleFormat::I8 => build!(i8),
                cpal::SampleFormat::I16 => build!(i16),
                cpal::SampleFormat::I32 => build!(i32),
                cpal::SampleFormat::I64 => build!(i64),
                cpal::SampleFormat::U8 => build!(u8),
                cpal::SampleFormat::U16 => build!(u16),
                cpal::SampleFormat::U32 => build!(u32),
                cpal::SampleFormat::U64 => build!(u64),
                _ => Err("Unsupported audio output sample format".into()),
            }?;
            stream.play().map_err(|e| e.to_string())?;
            self.stream = Some(stream);
            self.errors = Some(rx);
        }
        self.playing = true;
        Ok(())
    }
    pub fn position(&self) -> f64 {
        if !self.playing {
            return self.start;
        }
        if self.stream.is_some() {
            self.sample_clock
                .read()
                .map_or(self.start, |(position, observed)| {
                    self.start
                        + (position
                            + (self.clock.elapsed().as_nanos() as u64).saturating_sub(observed)
                                as f64
                                / 1e9)
                            .max(0.0)
                })
        } else {
            self.start + self.clock.elapsed().as_secs_f64()
        }
    }
    pub fn stop(&mut self) {
        if self.playing {
            self.start = self.position();
        }
        self.playing = false;
        self.stream = None;
        self.errors = None;
    }
    pub fn take_error(&mut self) -> Option<String> {
        let error = self.errors.as_ref().and_then(|rx| rx.try_recv().ok());
        if error.is_some() {
            self.stop();
        }
        error
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_clock_snapshot_preserves_output_latency() {
        let clock = SampleClock::default();
        assert!(clock.read().is_none());
        clock.publish(4800.0 / 48000.0 - 0.012, 15_000_000);
        let (position, observed) = clock.read().unwrap();
        assert!((position - 0.088).abs() < 1e-12);
        assert_eq!(observed, 15_000_000);
    }
}
