/*
Copyright (C) 2025  Hav1ck

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as
published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, StreamConfig};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const SAMPLE_RATE: usize = 16_000;

// sets up and runs the audio input stream in a separate thread.
pub fn start_audio_stream(
    buffer: Arc<Mutex<VecDeque<i16>>>,
    microphone_name: Option<String>,
    default_microphone_index: usize,
) -> Result<()> {
    println!("[DEBUG] Spawning audio input thread...");

    thread::spawn(move || {
        let device = match choose_input_device(
            microphone_name.as_deref(),
            default_microphone_index,
        ) {
            Some(d) => d,
            None => {
                eprintln!(
                    "[ERROR] No input device found at index {}. Exiting audio thread.",
                    default_microphone_index
                );
                return;
            }
        };

        let device_name = device.name().unwrap_or_else(|err| {
            eprintln!("[ERROR] Failed to get device name: {}", err);
            "<unknown device>".to_string()
        });
        println!("[INFO] Using input device: {}", device_name);

        let supported_config = match device.supported_input_configs() {
            Ok(mut configs) => configs.find(|c| {
                c.channels() == 1
                    && c.min_sample_rate().0 <= 16_000
                    && c.max_sample_rate().0 >= 16_000
                    && c.sample_format() == SampleFormat::I16
            }),
            Err(e) => {
                eprintln!("[ERROR] Error getting supported configs: {e}");
                return;
            }
        };

        let config = if let Some(c) = supported_config {
            c.with_sample_rate(cpal::SampleRate(16_000))
        } else {
            match device.default_input_config() {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("[ERROR] No default config found: {e}");
                    return;
                }
            }
        };

        println!(
            "[INFO] Using sample rate: {} Hz, channels: {}, format: {:?}",
            config.sample_rate().0,
            config.channels(),
            config.sample_format()
        );

        let stream_config: StreamConfig = config.clone().into();
        let err_fn = |err| eprintln!("[ERROR] Stream error: {}", err);
        let channels = stream_config.channels as usize;

        let input_sample_rate = stream_config.sample_rate.0;
        let resample_factor = if input_sample_rate != SAMPLE_RATE as u32 {
            input_sample_rate as f64 / SAMPLE_RATE as f64
        } else {
            1.0
        };

        let mut resample_pos = 0.0;

        let stream = match device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                let mut buf = match buffer.lock() {
                    Ok(b) => b,
                    Err(_) => return,
                };
                let samples_iterator: Box<dyn Iterator<Item = i16>> = if resample_factor != 1.0 {
                    let mut resampled = Vec::new();
                    let input_samples = data.iter().step_by(channels).cloned();
                    for sample in input_samples {
                        while resample_pos < 1.0 {
                            resampled.push(sample);
                            resample_pos += resample_factor;
                        }
                        resample_pos -= 1.0;
                    }
                    Box::new(resampled.into_iter())
                } else {
                    Box::new(data.iter().step_by(channels).cloned())
                };
                for sample in samples_iterator {
                    if buf.len() >= buf.capacity() {
                        buf.pop_front();
                    }
                    buf.push_back(sample);
                }
            },
            err_fn,
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[ERROR] Failed to build input stream: {e}");
                return;
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("[ERROR] Failed to start input stream: {e}");
            return;
        }

        println!("[DEBUG] Audio input stream is now playing in the background.");
        loop {
            thread::sleep(Duration::from_secs(u64::MAX));
        }
    });

    thread::sleep(Duration::from_millis(500));
    println!("[DEBUG] Audio thread spawned. Continuing main execution.");

    Ok(())
}

// blocks until a full frame of audio is available from the buffer
pub fn next_audio_frame(
    buffer: Arc<Mutex<VecDeque<i16>>>,
    frame_size: usize,
) -> anyhow::Result<Vec<i16>> {
    loop {
        let mut buf = buffer
            .lock()
            .map_err(|e| anyhow!("Failed to lock audio buffer (poisoned): {e}"))?;

        if buf.len() >= frame_size {
            return Ok(buf.drain(..frame_size).collect());
        }
        drop(buf);
        thread::sleep(Duration::from_millis(10));
    }
}

// chooses an input device by name (case-insensitive contains) or falls back to index
fn choose_input_device(name: Option<&str>, index: usize) -> Option<Device> {
    if let Some(name_query) = name {
        println!("[DEBUG] Choosing input device by name: {}", name_query);
    } else {
        println!("[DEBUG] Choosing input device with index: {}", index);
    }
    let host: Host = cpal::default_host();
    let devices: Vec<Device> = match host.input_devices() {
        Ok(list) => list.collect(),
        Err(err) => {
            eprintln!("[ERROR] Error enumerating input devices: {}", err);
            return None;
        }
    };

    if devices.is_empty() {
        eprintln!("[ERROR] No input devices found on this host.");
        return None;
    }

    println!("[INFO] Available input devices:");
    for (i, device) in devices.iter().enumerate() {
        match device.name() {
            Ok(name) => println!("  Device #{}: {}", i, name),
            Err(err) => println!("  Device #{}: <unknown name: {}>", i, err),
        }
    }

    // Try name match first if provided
    if let Some(query) = name {
        let q = query.to_lowercase();
        if let Some(found) = devices.iter().find(|d| {
            d.name()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false)
        }) {
            match found.name() {
                Ok(n) => println!("[INFO] Selected \"{}\" as input by name!", n),
                Err(err) => println!(
                    "[INFO] Selected <unknown device> as input by name (name error: {})",
                    err
                ),
            }
            return Some(found.clone());
        }
        println!(
            "[WARN] Input device with name containing \"{}\" not found. Falling back to index {}.",
            query, index
        );
    }

    let device = match devices.get(index) {
        Some(d) => d.clone(),
        None => {
            eprintln!(
                "[ERROR] Invalid device index {}. You have {} device(s) available. Using default (index 0).",
                index,
                devices.len()
            );
            if let Some(d) = devices.get(0) {
                d.clone()
            } else {
                return None;
            }
        }
    };

    match device.name() {
        Ok(name) => println!("[INFO] Selected \"{}\" as input!", name),
        Err(err) => println!(
            "[INFO] Selected <unknown device> as input (name error: {})",
            err
        ),
    }

    Some(device)
}
