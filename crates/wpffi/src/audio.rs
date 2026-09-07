//! Audio device enumeration module.
//!
//! This module provides functionality for listing available audio
//! input (microphone) and output (speaker) devices on the system.

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use tracing::info;

/// List all available audio input and output devices.
pub fn list_devices() -> Result<()> {
    let host = cpal::default_host();

    info!("Available input devices:");
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
            if let Ok(config) = device.default_input_config() {
                info!("      Default input config: {:?}", config);
            }
        }
    }

    info!("Available output devices:");
    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            info!("  - {}", name);
            if let Ok(config) = device.default_output_config() {
                info!("      Default output config: {:?}", config);
            }
        }
    }

    Ok(())
}

/// Find an input audio device by name or substring, or return default input device if None.
pub fn find_input_device(host: &cpal::Host, name_opt: Option<&str>) -> Result<cpal::Device> {
    if let Some(name) = name_opt {
        let name_lower = name.to_lowercase();
        let devices = host.input_devices()?;
        for device in devices {
            if let Ok(dev_name) = device.name()
                && dev_name.to_lowercase().contains(&name_lower)
            {
                return Ok(device);
            }
        }
        anyhow::bail!("Input audio device containing '{}' not found", name);
    } else {
        host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input audio device available"))
    }
}

/// Find an output audio device by name or substring, or return default output device if None.
pub fn find_output_device(host: &cpal::Host, name_opt: Option<&str>) -> Result<cpal::Device> {
    if let Some(name) = name_opt {
        let name_lower = name.to_lowercase();
        let devices = host.output_devices()?;
        for device in devices {
            if let Ok(dev_name) = device.name()
                && dev_name.to_lowercase().contains(&name_lower)
            {
                return Ok(device);
            }
        }
        anyhow::bail!("Output audio device containing '{}' not found", name);
    } else {
        host.default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output audio device available"))
    }
}

use crate::config::Configuration;
use cpal::traits::StreamTrait;
use cpal::{SampleRate, Stream, StreamConfig};
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::mpsc;
use tracing::error;

/// Manages CPAL audio input (microphone) and output (speaker) streams and resampling pipeline.
pub struct AudioEngine {
    _input_stream: Stream,
    _output_stream: Stream,
}

impl AudioEngine {
    /// Start microphone input and speaker output streams based on the provided `Configuration`.
    pub fn start(
        config: &Configuration,
        tx_audio: mpsc::Sender<Vec<u8>>,
        audio_buffer: &'static Mutex<VecDeque<f32>>,
    ) -> Result<Self> {
        let host = cpal::default_host();

        let input_device = find_input_device(&host, config.input_device.as_deref())?;
        info!("Using input device: {}", input_device.name()?);

        let output_device = find_output_device(&host, config.output_device.as_deref())?;
        info!("Using output device: {}", output_device.name()?);

        let target_input_config = StreamConfig {
            channels: config.channels,
            sample_rate: SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let net_sample_rate = config.sample_rate;

        // Build Input Stream (Microphone -> Resampler -> tx_audio)
        let (input_stream, _actual_input_rate, _actual_input_channels) = {
            let tx_audio_clone = tx_audio.clone();
            let build = |cfg: &StreamConfig, rate: u32, ch: u16| {
                let mut resampler = crate::resample::Resampler::new(rate, net_sample_rate);
                let tx = tx_audio_clone.clone();
                input_device.build_input_stream(
                    cfg,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let channels = ch as usize;
                        let num_frames = data.len() / channels;
                        if num_frames == 0 {
                            return;
                        }
                        let mut mono_samples = Vec::with_capacity(num_frames);
                        for f in 0..num_frames {
                            let sum: f32 = (0..channels).map(|c| data[f * channels + c]).sum();
                            mono_samples.push(sum / channels as f32);
                        }
                        let resampled = resampler.process(&mono_samples);
                        let bytes: Vec<u8> = resampled
                            .iter()
                            .flat_map(|&sample| sample.to_le_bytes())
                            .collect();
                        let _ = tx.blocking_send(bytes);
                    },
                    |err| error!("Input stream error: {}", err),
                    None,
                )
            };

            match build(&target_input_config, config.sample_rate, config.channels) {
                Ok(stream) => (stream, config.sample_rate, config.channels),
                Err(err) => {
                    info!(
                        "Requested input stream config ({:?}) not supported ({}), falling back to device default config...",
                        target_input_config, err
                    );
                    let def_cfg = input_device.default_input_config()?;
                    let def_stream_config: StreamConfig = def_cfg.config();
                    let rate = def_stream_config.sample_rate.0;
                    let ch = def_stream_config.channels;
                    info!(
                        "Using input device default config: {} Hz, {} channels",
                        rate, ch
                    );
                    let stream = build(&def_stream_config, rate, ch)?;
                    (stream, rate, ch)
                }
            }
        };

        // Build Output Stream (audio_buffer -> Resampler -> Speaker)
        let target_output_config = StreamConfig {
            channels: config.channels,
            sample_rate: SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let (output_stream, _actual_output_rate, _actual_output_channels) = {
            let build = |cfg: &StreamConfig, rate: u32, ch: u16| {
                let mut output_resampler = crate::resample::Resampler::new(net_sample_rate, rate);
                let mut leftover: Vec<f32> = Vec::new();
                let channels = ch as usize;

                output_device.build_output_stream(
                    cfg,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let frames_needed = data.len() / channels;
                        if frames_needed == 0 {
                            return;
                        }

                        if leftover.len() < frames_needed {
                            let needed_resampled = frames_needed - leftover.len();
                            let net_needed = ((needed_resampled as f64
                                * (net_sample_rate as f64 / rate as f64))
                                .ceil() as usize)
                                + 4;
                            let mut net_samples = Vec::with_capacity(net_needed);
                            {
                                let mut buffer = audio_buffer.lock().unwrap();
                                let drain_count = net_needed.min(buffer.len());
                                for _ in 0..drain_count {
                                    if let Some(s) = buffer.pop_front() {
                                        net_samples.push(s);
                                    }
                                }
                            }
                            let mut resampled = output_resampler.process(&net_samples);
                            leftover.append(&mut resampled);
                        }

                        for f in 0..frames_needed {
                            let sample = leftover.get(f).copied().unwrap_or(0.0);
                            for c in 0..channels {
                                data[f * channels + c] = sample;
                            }
                        }

                        let drain_len = frames_needed.min(leftover.len());
                        leftover.drain(0..drain_len);
                    },
                    |err| error!("Output stream error: {}", err),
                    None,
                )
            };

            match build(&target_output_config, config.sample_rate, config.channels) {
                Ok(stream) => (stream, config.sample_rate, config.channels),
                Err(err) => {
                    info!(
                        "Requested output stream config ({:?}) not supported ({}), falling back to device default config...",
                        target_output_config, err
                    );
                    let def_cfg = output_device.default_output_config()?;
                    let def_stream_config: StreamConfig = def_cfg.config();
                    let rate = def_stream_config.sample_rate.0;
                    let ch = def_stream_config.channels;
                    info!(
                        "Using output device default config: {} Hz, {} channels",
                        rate, ch
                    );
                    let stream = build(&def_stream_config, rate, ch)?;
                    (stream, rate, ch)
                }
            }
        };

        input_stream.play()?;
        output_stream.play()?;

        info!("Audio streams started. Speaking now will transmit audio over WebRTC.");

        Ok(Self {
            _input_stream: input_stream,
            _output_stream: output_stream,
        })
    }
}
