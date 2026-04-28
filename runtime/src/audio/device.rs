//! cpal device / output-stream wrapper with no compiler types.

use super::ring::{SampleProducer, SampleRing, StereoFrame};
use super::voice::{VoiceLedger, VoiceRenderer};
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

pub struct VoiceOutputStream {
    stream: cpal::Stream,
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

impl VoiceOutputStream {
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

pub fn build_default_voice_output_stream(
    config: AudioDeviceConfig,
    ledger: Arc<VoiceLedger>,
) -> Result<VoiceOutputStream, String> {
    let host = default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;
    build_voice_output_stream_for_device(&device, config, ledger)
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

pub fn build_voice_output_stream_for_device(
    device: &cpal::Device,
    config: AudioDeviceConfig,
    ledger: Arc<VoiceLedger>,
) -> Result<VoiceOutputStream, String> {
    let supported = device
        .default_output_config()
        .map_err(|err| format!("default output config: {err}"))?;
    let channels = supported.channels().max(1) as usize;
    let stream_config = cpal::StreamConfig {
        channels: supported.channels().max(1),
        sample_rate: config.sample_rate,
        buffer_size: cpal::BufferSize::Fixed(config.block_size),
    };
    let scratch_len = (config.block_size as usize).max(1);
    let underruns = Arc::new(AtomicU64::new(0));
    let err_fn = |err| eprintln!("wrela audio stream error: {err}");
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let ledger = Arc::clone(&ledger);
            let underruns = Arc::clone(&underruns);
            let mut renderer = VoiceRenderer::new(config.sample_rate);
            let mut scratch = vec![StereoFrame::SILENCE; scratch_len];
            device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    render_voice_ledger_to_f32_channels(
                        data,
                        channels,
                        &ledger,
                        &mut renderer,
                        &mut scratch,
                        &underruns,
                    )
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let ledger = Arc::clone(&ledger);
            let underruns = Arc::clone(&underruns);
            let mut renderer = VoiceRenderer::new(config.sample_rate);
            let mut scratch = vec![StereoFrame::SILENCE; scratch_len];
            device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    render_voice_ledger_to_i16_channels(
                        data,
                        channels,
                        &ledger,
                        &mut renderer,
                        &mut scratch,
                        &underruns,
                    )
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let ledger = Arc::clone(&ledger);
            let underruns = Arc::clone(&underruns);
            let mut renderer = VoiceRenderer::new(config.sample_rate);
            let mut scratch = vec![StereoFrame::SILENCE; scratch_len];
            device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    render_voice_ledger_to_u16_channels(
                        data,
                        channels,
                        &ledger,
                        &mut renderer,
                        &mut scratch,
                        &underruns,
                    )
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
    Ok(VoiceOutputStream { stream, underruns })
}

fn render_voice_ledger_to_f32_channels(
    output: &mut [f32],
    channels: usize,
    ledger: &VoiceLedger,
    renderer: &mut VoiceRenderer,
    scratch: &mut [StereoFrame],
    underruns: &AtomicU64,
) {
    let frames = prepare_voice_output(output, channels, ledger, renderer, scratch, underruns);
    for frame_index in 0..frames {
        let start = frame_index * channels;
        write_f32_channels(scratch[frame_index], &mut output[start..start + channels]);
    }
    output[frames * channels..].fill(0.0);
}

fn render_voice_ledger_to_i16_channels(
    output: &mut [i16],
    channels: usize,
    ledger: &VoiceLedger,
    renderer: &mut VoiceRenderer,
    scratch: &mut [StereoFrame],
    underruns: &AtomicU64,
) {
    let frames =
        prepare_voice_output_for_len(output.len(), channels, ledger, renderer, scratch, underruns);
    for frame_index in 0..frames {
        let start = frame_index * channels;
        write_i16_channels(scratch[frame_index], &mut output[start..start + channels]);
    }
    let tail = &mut output[frames * channels..];
    tail.fill(0);
}

fn render_voice_ledger_to_u16_channels(
    output: &mut [u16],
    channels: usize,
    ledger: &VoiceLedger,
    renderer: &mut VoiceRenderer,
    scratch: &mut [StereoFrame],
    underruns: &AtomicU64,
) {
    let frames =
        prepare_voice_output_for_len(output.len(), channels, ledger, renderer, scratch, underruns);
    for frame_index in 0..frames {
        let start = frame_index * channels;
        write_u16_channels(scratch[frame_index], &mut output[start..start + channels]);
    }
    let tail = &mut output[frames * channels..];
    tail.fill(u16::MAX / 2);
}

fn prepare_voice_output(
    output: &mut [f32],
    channels: usize,
    ledger: &VoiceLedger,
    renderer: &mut VoiceRenderer,
    scratch: &mut [StereoFrame],
    underruns: &AtomicU64,
) -> usize {
    prepare_voice_output_for_len(output.len(), channels, ledger, renderer, scratch, underruns)
}

fn prepare_voice_output_for_len(
    output_len: usize,
    channels: usize,
    ledger: &VoiceLedger,
    renderer: &mut VoiceRenderer,
    scratch: &mut [StereoFrame],
    underruns: &AtomicU64,
) -> usize {
    let channels = channels.max(1);
    let frames = output_len / channels;
    if frames == 0 {
        return 0;
    }
    if frames > scratch.len() {
        underruns.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    let snapshot = ledger.load();
    renderer.render_block(&snapshot.voices, &mut scratch[..frames])
}

fn write_f32_channels(frame: StereoFrame, output: &mut [f32]) {
    match output {
        [] => {}
        [mono] => *mono = (frame.left + frame.right) * 0.5,
        [left, right, rest @ ..] => {
            *left = frame.left;
            *right = frame.right;
            rest.fill(0.0);
        }
    }
}

fn write_i16_channels(frame: StereoFrame, output: &mut [i16]) {
    match output {
        [] => {}
        [mono] => {
            *mono = (((frame.left + frame.right) * 0.5).clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        }
        [left, right, rest @ ..] => {
            *left = (frame.left.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            *right = (frame.right.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            rest.fill(0);
        }
    }
}

fn write_u16_channels(frame: StereoFrame, output: &mut [u16]) {
    fn convert(sample: f32) -> u16 {
        ((sample.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
    }
    match output {
        [] => {}
        [mono] => *mono = convert((frame.left + frame.right) * 0.5),
        [left, right, rest @ ..] => {
            *left = convert(frame.left);
            *right = convert(frame.right);
            rest.fill(u16::MAX / 2);
        }
    }
}
