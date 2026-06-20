//! Microphone capture via cpal (ALSA/PulseAudio on Linux).
//!
//! We open the default input device at its native rate/format, downmix to mono,
//! convert to signed 16-bit little-endian PCM, and push raw bytes into a channel.
//! Soniox is told the actual sample rate, so no resampling is needed.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{InputCallbackInfo, SampleFormat};
use tokio::sync::mpsc::Sender;

fn err_fn(err: cpal::Error) {
    eprintln!("\naudio stream error: {err}");
}

/// Build (but do not start) an input stream. The returned stream must be kept
/// alive and `.play()`-ed by the caller; dropping it stops capture and closes
/// the channel, which signals end-of-audio downstream.
pub fn build_input_stream(tx: Sender<Vec<u8>>) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device (microphone) found"))?;

    let supported = device.default_input_config()?;
    let sample_rate = supported.sample_rate(); // cpal 0.18: SampleRate == u32
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    // cpal 0.18 takes `config` by value; only one match arm runs, so moving it
    // (and the `err_fn` fn item) in each arm is fine.
    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &InputCallbackInfo| {
                let _ = tx.try_send(f32_to_pcm16(data, channels));
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &InputCallbackInfo| {
                let _ = tx.try_send(i16_to_pcm16(data, channels));
            },
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &InputCallbackInfo| {
                let _ = tx.try_send(u16_to_pcm16(data, channels));
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    Ok((stream, sample_rate))
}

fn f32_to_pcm16(data: &[f32], channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / channels * 2);
    for frame in data.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        let mono = (sum / channels as f32).clamp(-1.0, 1.0);
        let v = (mono * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn i16_to_pcm16(data: &[i16], channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / channels * 2);
    for frame in data.chunks(channels) {
        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
        let v = (sum / channels as i32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn u16_to_pcm16(data: &[u16], channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / channels * 2);
    for frame in data.chunks(channels) {
        let sum: i32 = frame.iter().map(|&s| s as i32 - 32768).sum();
        let v = (sum / channels as i32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
