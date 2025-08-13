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

use anyhow::Result;
use elevenlabs_rs::Model;
use porcupine::Porcupine;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use webrtc_vad::Vad;
use whisper_rs::WhisperContext;
use tauri::Manager;
use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub porcupine_key: String,
    pub gemini_key: String,
    pub elevenlabs_key: String,
    pub whisper_language: String,
    pub context_window_expiration_seconds: u64,
    pub default_microphone_index: usize,
    pub default_microphone_name: Option<String>,
    pub default_output_device_name: Option<String>,

    // advanced settings
    pub gemini_model: String,
    pub elevenlabs_model: String,
    pub voice_id: String,
    pub llm_system_prompt: String,
    pub vad_mode: String,
    pub wwd_sensitivity: f32,
    // Paths are hard-coded by the app (wakeword in resources/public; history/context in app data)

    pub frame_duration_ms: usize,
    pub silence_threshold_seconds: usize,
    pub speech_trigger_frames: usize,
    pub frame_length_wwd: usize,
}

pub struct AppContext {
    pub config: Config,
    pub audio_player: AudioPlayer,
    pub porcupine: Porcupine,
    pub vad: Mutex<Vad>,
    pub whisper_context: Arc<WhisperContext>,
    pub audio_buffer: Arc<Mutex<VecDeque<i16>>>,
    pub elevenlabs_model: Model,
}

pub struct AudioPlayer {
    _stream: OutputStream,
    app_handle: Option<tauri::AppHandle>,
}

impl AudioPlayer {
    pub fn new(_assets_dir: std::path::PathBuf, output_device_name: Option<String>) -> Result<Self> {
        println!(
            "[DEBUG] Initializing AudioPlayer with assets dir: {:?}",
            _assets_dir
        );
        let stream = if let Some(name) = output_device_name.as_deref() {
            // Try to open specific output device by name
            let host = cpal::default_host();
            if let Ok(mut devs) = host.output_devices() {
                let name_lower = name.to_lowercase();
                if let Some(device) = devs.find(|d| d.name().map(|n| n.to_lowercase().contains(&name_lower)).unwrap_or(false)) {
                    println!("[INFO] Using output device by name: {}", device.name().unwrap_or_else(|_| "<unknown>".into()));
                    rodio::OutputStreamBuilder::from_device(device)?.open_stream()?
                } else {
                    println!("[WARN] Output device '{}' not found. Falling back to default.", name);
                    OutputStreamBuilder::from_default_device()?.open_stream()?
                }
            } else {
                println!("[WARN] Failed to enumerate output devices. Falling back to default output.");
                OutputStreamBuilder::from_default_device()?.open_stream()?
            }
        } else {
            OutputStreamBuilder::from_default_device()?.open_stream()?
        };
        Ok(Self {
            _stream: stream,
            app_handle: None,
        })
    }

    pub fn new_with_app_handle(app_handle: tauri::AppHandle, output_device_name: Option<String>) -> Result<Self> {
        println!("[DEBUG] Initializing AudioPlayer with app handle");
        let stream = if let Some(name) = output_device_name.as_deref() {
            let host = cpal::default_host();
            if let Ok(mut devs) = host.output_devices() {
                let name_lower = name.to_lowercase();
                if let Some(device) = devs.find(|d| d.name().map(|n| n.to_lowercase().contains(&name_lower)).unwrap_or(false)) {
                    println!("[INFO] Using output device by name: {}", device.name().unwrap_or_else(|_| "<unknown>".into()));
                    rodio::OutputStreamBuilder::from_device(device)?.open_stream()?
                } else {
                    println!("[WARN] Output device '{}' not found. Falling back to default.", name);
                    OutputStreamBuilder::from_default_device()?.open_stream()?
                }
            } else {
                println!("[WARN] Failed to enumerate output devices. Falling back to default output.");
                OutputStreamBuilder::from_default_device()?.open_stream()?
            }
        } else {
            OutputStreamBuilder::from_default_device()?.open_stream()?
        };
        Ok(Self {
            _stream: stream,
            app_handle: Some(app_handle),
        })
    }
    pub fn play_sound<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let requested_path = PathBuf::from(path.as_ref());
        let sound_path = if let Some(app_handle) = &self.app_handle {
            // 1) Prefer user-overridden asset in roaming dir: <AppData>/assets/<file>
            let assets_dir = app_handle
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("assets");

            // If the requested path starts with the "assets" component, strip that for user dir
            use std::path::Component;
            let user_rel: PathBuf = match requested_path.components().next() {
                Some(Component::Normal(first)) if first == std::ffi::OsStr::new("assets") => {
                    requested_path.components().skip(1).collect()
                }
                _ => requested_path.clone(),
            };
            let user_override = assets_dir.join(user_rel);
            if user_override.exists() {
                println!(
                    "[DEBUG] Playing sound from user assets override: {}",
                    user_override.display()
                );
                user_override
            } else {
                // 2) Fallback to bundled resource inside the app
                let resource_path = app_handle
                    .path()
                    .resolve(&requested_path, tauri::path::BaseDirectory::Resource);
                if let Ok(resolved_path) = resource_path {
                    if resolved_path.exists() {
                        println!(
                            "[DEBUG] Playing sound from bundled resource: {}",
                            resolved_path.display()
                        );
                        resolved_path
                    } else {
                        // 3) Dev public assets (when running in dev)
                        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                        if let Some(parent) = current_dir.parent() {
                            let dev_path = parent.join("public").join(&requested_path);
                            if dev_path.exists() {
                                println!(
                                    "[DEBUG] Playing sound from dev public assets: {}",
                                    dev_path.display()
                                );
                                dev_path
                            } else {
                                println!(
                                    "[DEBUG] Neither user override, bundled resource, nor dev asset exists; falling back to requested path: {}",
                                    requested_path.display()
                                );
                                requested_path.clone()
                            }
                        } else {
                            println!(
                                "[DEBUG] No parent dir to resolve dev assets; falling back to requested path: {}",
                                requested_path.display()
                            );
                            requested_path.clone()
                        }
                    }
                } else {
                    // If resolve fails entirely, also try dev public assets
                    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    if let Some(parent) = current_dir.parent() {
                        let dev_path = parent.join("public").join(&requested_path);
                        if dev_path.exists() {
                            println!(
                                "[DEBUG] Playing sound from dev public assets: {}",
                                dev_path.display()
                            );
                            dev_path
                        } else {
                            println!(
                                "[DEBUG] Failed to resolve bundled resource and dev asset missing; falling back to requested path: {}",
                                requested_path.display()
                            );
                            requested_path.clone()
                        }
                    } else {
                        println!(
                            "[DEBUG] Failed to resolve bundled resource and no parent dir; falling back to requested path: {}",
                            requested_path.display()
                        );
                        requested_path.clone()
                    }
                }
            }
        } else {
            // Legacy fallback - try to find the sound file in the current directory
            println!(
                "[DEBUG] Playing sound from legacy path: {}",
                requested_path.display()
            );
            requested_path.clone()
        };

        let sink = Sink::connect_new(&self._stream.mixer());
        let file = File::open(sound_path)?;
        let source = Decoder::new(BufReader::new(file))?;
        sink.append(source);
        sink.detach();
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
}
