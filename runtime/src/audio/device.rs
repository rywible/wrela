//! cpal device / output-stream wrapper with no compiler types.

use super::ring::{SampleProducer, SampleRing, StereoFrame};
use super::worker::fill_output_from_consumer_channels_atomic;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct AudioDeviceConfig {
    pub sample_rate: u32,
    pub block_size: u32,
}

impl Default for AudioDeviceConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            block_size: 256,
        }
    }
}

pub fn default_host() -> cpal::Host {
    cpal::default_host()
}

pub struct AudioOutputStream {
    stream: cpal::Stream,
    producer: Arc<Mutex<SampleProducer>>,
    underruns: Arc<AtomicU64>,
}

impl AudioOutputStream {
    pub fn producer(&self) -> Arc<Mutex<SampleProducer>> {
        Arc::clone(&self.producer)
    }

    pub fn push_block(&self, block: &[StereoFrame]) -> usize {
        let mut producer = self
            .producer
            .lock()
            .expect("AudioOutputStream producer lock");
        producer.push_block(block)
    }

    pub fn underrun_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.underruns)
    }

    pub fn play(&self) -> Result<(), cpal::PlayStreamError> {
        self.stream.play()
    }

    pub fn take_underruns(&self) -> u64 {
        self.underruns.swap(0, Ordering::AcqRel)
    }
}

pub fn build_default_output_stream(config: AudioDeviceConfig) -> Result<AudioOutputStream, String> {
    let host = default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    build_output_stream_for_device(&device, config)
}

pub fn build_output_stream_for_device(
    device: &cpal::Device,
    config: AudioDeviceConfig,
) -> Result<AudioOutputStream, String> {
    let supported = device
        .default_output_config()
        .map_err(|err| format!("default output config: {err}"))?;
    let channels = supported.channels().max(1);
    let stream_config = cpal::StreamConfig {
        channels,
        sample_rate: config.sample_rate,
        buffer_size: cpal::BufferSize::Fixed(config.block_size),
    };
    let capacity = (config.block_size as usize).saturating_mul(8).max(1024);
    let (producer, consumer) = SampleRing::split(capacity);
    let producer = Arc::new(Mutex::new(producer));
    let underruns = Arc::new(AtomicU64::new(0));
    let err_fn = |err| eprintln!("wrela audio stream error: {err}");
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let mut consumer = consumer;
            let underruns = Arc::clone(&underruns);
            device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    fill_output_from_consumer_channels_atomic(
                        data,
                        channels as usize,
                        &mut consumer,
                        &underruns,
                    )
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let mut consumer = consumer;
            let underruns = Arc::clone(&underruns);
            let mut scratch =
                vec![0.0_f32; (config.block_size as usize).saturating_mul(channels as usize)];
            device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    if data.len() > scratch.len() {
                        data.fill(0);
                        underruns.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    let scratch_slice = &mut scratch[..data.len()];
                    fill_output_from_consumer_channels_atomic(
                        scratch_slice,
                        channels as usize,
                        &mut consumer,
                        &underruns,
                    );
                    for (dst, src) in data.iter_mut().zip(scratch_slice.iter()) {
                        *dst = (src.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    }
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut consumer = consumer;
            let underruns = Arc::clone(&underruns);
            let mut scratch =
                vec![0.0_f32; (config.block_size as usize).saturating_mul(channels as usize)];
            device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    if data.len() > scratch.len() {
                        data.fill(u16::MAX / 2);
                        underruns.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    let scratch_slice = &mut scratch[..data.len()];
                    fill_output_from_consumer_channels_atomic(
                        scratch_slice,
                        channels as usize,
                        &mut consumer,
                        &underruns,
                    );
                    for (dst, src) in data.iter_mut().zip(scratch_slice.iter()) {
                        let normalized = (src.clamp(-1.0, 1.0) * 0.5) + 0.5;
                        *dst = (normalized * u16::MAX as f32) as u16;
                    }
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(format!("unsupported output sample format: {other:?}"));
        }
    }
    .map_err(|err| format!("build output stream: {err}"))?;
    Ok(AudioOutputStream {
        stream,
        producer,
        underruns,
    })
}
