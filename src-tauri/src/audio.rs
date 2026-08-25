use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Records from the default input device into a shared buffer at the device's
/// native rate; `stop()` returns the whole utterance as 16 kHz mono f32.
///
/// Owns a `cpal::Stream`, which is !Send — a `Recorder` must be created and
/// used on a single thread (the pipeline thread).
pub struct Recorder {
    stream: Option<cpal::Stream>,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            buf: Arc::new(Mutex::new(Vec::new())),
            rate: 0,
        }
    }

    pub fn start(&mut self, app: AppHandle) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("no input device available"))?;
        let config = device.default_input_config()?;
        self.rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        log::info!(
            "recording: {} @ {} Hz, {} ch, {:?}",
            device.name().unwrap_or_default(),
            self.rate,
            channels,
            config.sample_format()
        );
        self.buf.lock().unwrap().clear();

        let err_fn = |e| log::error!("audio stream error: {e}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                self.build_stream::<f32>(&device, &config.into(), channels, app, err_fn)?
            }
            cpal::SampleFormat::I16 => {
                self.build_stream::<i16>(&device, &config.into(), channels, app, err_fn)?
            }
            cpal::SampleFormat::U16 => {
                self.build_stream::<u16>(&device, &config.into(), channels, app, err_fn)?
            }
            other => return Err(anyhow!("unsupported sample format {other:?}")),
        };
        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn build_stream<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        app: AppHandle,
        err_fn: fn(cpal::StreamError),
    ) -> Result<cpal::Stream>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let buf = self.buf.clone();
        let rate = self.rate as usize;
        let mut since_emit = 0usize;
        let mut level_acc = 0f32;
        let stream = device.build_input_stream(
            config,
            move |data: &[T], _| {
                let mut buf = buf.lock().unwrap();
                for frame in data.chunks(channels) {
                    let mono =
                        frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
                    buf.push(mono);
                    level_acc += mono * mono;
                    since_emit += 1;
                }
                // Emit an RMS level ~20x/sec for the HUD waveform.
                if since_emit >= rate / 20 {
                    let rms = (level_acc / since_emit as f32).sqrt();
                    let _ = app.emit("audio-level", rms);
                    since_emit = 0;
                    level_acc = 0.0;
                }
            },
            err_fn,
            None,
        )?;
        Ok(stream)
    }

    /// Stop recording and return the utterance as 16 kHz mono samples.
    pub fn stop(&mut self) -> Result<Vec<f32>> {
        drop(self.stream.take());
        let samples = std::mem::take(&mut *self.buf.lock().unwrap());
        log::info!(
            "captured {:.2}s of audio",
            samples.len() as f32 / self.rate.max(1) as f32
        );
        resample_to_16k(&samples, self.rate)
    }
}

pub const TARGET_RATE: u32 = 16_000;

fn resample_to_16k(input: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    if from_rate == TARGET_RATE || input.is_empty() {
        return Ok(input.to_vec());
    }
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };
    const CHUNK: usize = 1024;
    let mut rs =
        SincFixedIn::<f32>::new(TARGET_RATE as f64 / from_rate as f64, 2.0, params, CHUNK, 1)
            .map_err(|e| anyhow!("resampler init: {e}"))?;
    let mut out =
        Vec::with_capacity(input.len() * TARGET_RATE as usize / from_rate as usize + CHUNK);
    for chunk in input.chunks(CHUNK) {
        let waves = if chunk.len() == CHUNK {
            rs.process(&[chunk], None)
        } else {
            rs.process_partial(Some(&[chunk]), None)
        }
        .map_err(|e| anyhow!("resample: {e}"))?;
        out.extend_from_slice(&waves[0]);
    }
    // Flush the resampler's internal delay line.
    let tail = rs
        .process_partial::<&[f32]>(None, None)
        .map_err(|e| anyhow!("resample flush: {e}"))?;
    out.extend_from_slice(&tail[0]);
    Ok(out)
}
