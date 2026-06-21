//! Microphone capture via cpal (ALSA/PulseAudio on Linux).
//!
//! Opens the default input device at its native rate/format, downmixes to mono,
//! converts to signed 16-bit little-endian PCM, and pushes raw bytes into a
//! channel. It also writes a 0..1 peak level into a shared atomic each callback
//! so the UI avatar can react to the speaker's voice.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{InputCallbackInfo, SampleFormat};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

fn err_fn(err: cpal::Error) {
    eprintln!("\naudio stream error: {err}");
}

/// Build (but do not start) an input stream. Keep the returned stream alive and
/// `.play()` it; dropping it stops capture and closes the channel. `level` is
/// updated with the latest 0..1 peak amplitude each callback.
pub fn build_input_stream(
    tx: Sender<Vec<u8>>,
    level: Arc<AtomicU32>,
) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device (microphone) found"))?;

    let supported = device.default_input_config()?;
    let sample_rate = supported.sample_rate(); // cpal 0.18: SampleRate == u32
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &InputCallbackInfo| {
                let (bytes, peak) = f32_to_pcm16(data, channels);
                level.store(peak.to_bits(), Ordering::Relaxed);
                let _ = tx.try_send(bytes);
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &InputCallbackInfo| {
                let (bytes, peak) = i16_to_pcm16(data, channels);
                level.store(peak.to_bits(), Ordering::Relaxed);
                let _ = tx.try_send(bytes);
            },
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &InputCallbackInfo| {
                let (bytes, peak) = u16_to_pcm16(data, channels);
                level.store(peak.to_bits(), Ordering::Relaxed);
                let _ = tx.try_send(bytes);
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    Ok((stream, sample_rate))
}

fn f32_to_pcm16(data: &[f32], channels: usize) -> (Vec<u8>, f32) {
    let mut out = Vec::with_capacity(data.len() / channels * 2);
    let mut peak = 0.0f32;
    for frame in data.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        let mono = (sum / channels as f32).clamp(-1.0, 1.0);
        peak = peak.max(mono.abs());
        out.extend_from_slice(&((mono * 32767.0) as i16).to_le_bytes());
    }
    (out, peak)
}

fn i16_to_pcm16(data: &[i16], channels: usize) -> (Vec<u8>, f32) {
    let mut out = Vec::with_capacity(data.len() / channels * 2);
    let mut peak = 0.0f32;
    for frame in data.chunks(channels) {
        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
        let v = (sum / channels as i32) as i16;
        peak = peak.max((v as f32 / 32768.0).abs());
        out.extend_from_slice(&v.to_le_bytes());
    }
    (out, peak)
}

fn u16_to_pcm16(data: &[u16], channels: usize) -> (Vec<u8>, f32) {
    let mut out = Vec::with_capacity(data.len() / channels * 2);
    let mut peak = 0.0f32;
    for frame in data.chunks(channels) {
        let sum: i32 = frame.iter().map(|&s| s as i32 - 32768).sum();
        let v = (sum / channels as i32) as i16;
        peak = peak.max((v as f32 / 32768.0).abs());
        out.extend_from_slice(&v.to_le_bytes());
    }
    (out, peak)
}
