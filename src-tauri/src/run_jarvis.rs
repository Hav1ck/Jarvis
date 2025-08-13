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

use crate::audio_input::{self, SAMPLE_RATE};
use crate::get_text;
use crate::models;
use crate::send_to_llm;
use crate::transform_text;
use crate::utils;
use crate::JarvisState; // for app state access // to reuse context builder from lib.rs if available

use anyhow::{anyhow, Context, Result};
use elevenlabs_rs::Model;
use models::{AppContext, AudioPlayer};
use porcupine::PorcupineBuilder;
use reqwest::Client;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use tauri::Emitter;
use tauri::Manager;
use tokio::runtime::Handle;
use webrtc_vad::{SampleRate, Vad, VadMode};
use whisper_rs::{WhisperContext, WhisperContextParameters}; // for buffering TTS // to access app.state() and app.path()
use cpal::traits::{DeviceTrait, HostTrait};
use std::time::Instant;

fn estimate_tts_tokens_and_chars(text: &str) -> (usize, usize) {
    // ElevenLabs bills by characters; provide both chars and a rough token estimate (~4 chars/token)
    let chars = text.chars().count();
    let tokens_est = (chars + 3) / 4; // ceil(chars / 4)
    (tokens_est, chars)
}

fn build_ctx_text_from_active(app: &tauri::AppHandle) -> String {
    // Try to read currently active conversation set by the frontend
    let state = app.state::<JarvisState>();
    let current = state.active_conversation.lock().unwrap().clone();
    if let Some(fname) = current {
        // Build context by reading last 12 turns from that conversation file
        if let Ok(history_dir) = (|| -> Result<std::path::PathBuf, String> {
            let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
            let history = dir.join("history");
            std::fs::create_dir_all(&history).map_err(|e| e.to_string())?;
            Ok(history)
        })() {
            let path = history_dir.join(&fname);
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(turns) = serde_json::from_str::<Vec<serde_json::Value>>(&s) {
                    let start = turns.len().saturating_sub(12);
                    let mut buf = String::new();
                    for t in turns.iter().skip(start) {
                        let role = t
                            .get("role")
                            .and_then(|v| v.as_str())
                            .unwrap_or("user")
                            .to_uppercase();
                        let content = t.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        buf.push_str(&format!("{}: {}\n", role, content));
                    }
                    return buf;
                }
            }
        }
    }
    String::new()
}

const WHISPER_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin?download=true";

// Emit periodic progress updates for Whisper model download
async fn download_whisper_with_progress(
    app: &tauri::AppHandle,
    url: &str,
    path: &std::path::Path,
) -> Result<()> {
    use futures_util::StreamExt;
    use reqwest::Client as ReqwestClient;
    use std::cmp::min;
    use std::fs::File;
    use std::io::Write;

    if path.exists() {
        // Notify complete immediately if file already present
        let _ = app.emit(
            "whisper-download-progress",
            serde_json::json!({"downloaded": 1, "total": 1, "percent": 100}),
        );
        let _ = app.emit("whisper-download-complete", serde_json::json!({}));
        return Ok(());
    }

    utils::ensure_parent_directory_exists(path)?;

    let client = ReqwestClient::new();
    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to GET from {}", url))?;

    let total_size = res
        .content_length()
        .ok_or_else(|| anyhow!("failed to get content-length from {}", url))?;

    let mut file =
        File::create(path).with_context(|| format!("failed to create file {}", path.display()))?;

    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();
    // Emit an initial 0% event
    let _ = app.emit(
        "whisper-download-progress",
        serde_json::json!({"downloaded": 0, "total": total_size, "percent": 0}),
    );

    while let Some(item) = stream.next().await {
        let chunk = item.with_context(|| format!("error while downloading chunk from {}", url))?;
        file.write_all(&chunk)
            .with_context(|| format!("failed to write to file {}", path.display()))?;
        downloaded = min(downloaded + chunk.len() as u64, total_size);

        let percent = ((downloaded as f64 / total_size as f64) * 100.0).round() as u64;
        let _ = app.emit(
            "whisper-download-progress",
            serde_json::json!({"downloaded": downloaded, "total": total_size, "percent": percent}),
        );
    }

    let _ = app.emit("whisper-download-complete", serde_json::json!({}));
    Ok(())
}

// Helper function to emit state changes
async fn emit_state(app: &tauri::AppHandle, state: crate::JarvisStateEnum) {
    let label = match state {
        crate::JarvisStateEnum::Idle => "Idle",
        crate::JarvisStateEnum::WakeListening => "WakeListening",
        crate::JarvisStateEnum::Recording => "Recording",
        crate::JarvisStateEnum::Processing => "Processing",
        crate::JarvisStateEnum::Speaking => "Speaking",
        crate::JarvisStateEnum::Loading => "Loading",
    };
    let _ = app.emit("jarvis-state-changed", label);
}

// Helper function to emit messages
async fn emit_message(app: &tauri::AppHandle, role: &str, content: &str) {
    let message = serde_json::json!({
        "role": role,
        "content": content,
        "createdAt": chrono::Utc::now().timestamp_millis()
    });
    let _ = app.emit("new-message", message);
}

pub fn start_jarvis(is_running: Arc<AtomicBool>, config: models::Config, app: tauri::AppHandle) {
    println!("[DEBUG] Starting Jarvis with config");

    // Create a runtime for async operations
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        if let Err(e) = run_jarvis_with_config(config, is_running.clone(), app.clone()).await {
            eprintln!(
                "\n\n\n[ERROR] {}\nIf this is your first time running, please check your config.json, model paths, and device setup.\nFor more help, see the README \n",
                e
            );
            // Try to provide a friendly system message and reset UI state
            let err_text = format!(
                "Porcupine failed to start. Please enter a valid Picovoice access key in Settings > API Keys. (Details: {})",
                e
            );
            emit_message(&app, "system", &err_text).await;
            emit_state(&app, crate::JarvisStateEnum::Idle).await;
        }
    });
    // Ensure running flag is cleared after thread exits (whether error or stop)
    is_running.store(false, Ordering::Relaxed);
}

async fn run_jarvis_with_config(
    config: models::Config,
    is_running: Arc<AtomicBool>,
    tauri_app: tauri::AppHandle,
) -> Result<()> {
    println!("[DEBUG] Entered run_jarvis_with_config()");
    // Avoid logging secrets in config; print selected devices only
    println!(
        "[DEBUG] Loaded config: mic_name={:?}, mic_index={}, out_name={:?}",
        config.default_microphone_name,
        config.default_microphone_index,
        config.default_output_device_name
    );

    // Let UI know we're loading heavy assets
    emit_state(&tauri_app, crate::JarvisStateEnum::Loading).await;

    let wakeword_path = (|| -> Result<PathBuf> {
        // 1) User override
        if let Ok(roaming) = tauri_app.path().app_config_dir() {
            let user_ppn = roaming.join("assets").join("Jarvis_en_windows_v3_0_0.ppn");
            println!("[DEBUG] Checking user wakeword at {:?}", user_ppn);
            if user_ppn.exists() {
                return Ok(user_ppn);
            }
        }

        // 2) Dev public assets
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let dev_ppn = current_dir
            .join("assets")
            .join("Jarvis_en_windows_v3_0_0.ppn");
        println!("[DEBUG] Checking dev wakeword at {:?}", dev_ppn);
        if dev_ppn.exists() {
            return Ok(dev_ppn);
        }

        // 3) Bundled resource
        if let Ok(p) = tauri_app.path().resolve(
            "assets/Jarvis_en_windows_v3_0_0.ppn",
            tauri::path::BaseDirectory::Resource,
        ) {
            println!("[DEBUG] Checking bundled wakeword at {:?}", p);
            if p.exists() {
                return Ok(p);
            }
        }
        Err(anyhow!(
            "Wakeword .ppn not found in user assets, public/assets, or resources"
        ))
    })()?;

    // Whisper model lives in app data under assets
    let whisper_model_path = (|| {
        let path = tauri_app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("assets")
            .join("ggml-medium-q5_0.bin");
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        path
    })();

    let audio_player = AudioPlayer::new_with_app_handle(
        tauri_app.clone(),
        config.default_output_device_name.clone(),
    )
        .with_context(|| "Failed to initialize audio output")?;
    println!("[DEBUG] Initialized AudioPlayer");

    // Get the current directory to resolve relative paths
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    println!("[DEBUG] Current directory: {:?}", current_dir);

    // Resolve Porcupine model and library paths
    let (porcupine_params_path, porcupine_lib_path) = (|| {
        // Prefer bundled resources first
        let params_res = tauri_app
            .path()
            .resolve(
                "build/porcupine_params.pv",
                tauri::path::BaseDirectory::Resource,
            )
            .ok();
        let lib_res = tauri_app
            .path()
            .resolve(
                "build/libpv_porcupine.dll",
                tauri::path::BaseDirectory::Resource,
            )
            .ok();

        if let (Some(p_params), Some(p_lib)) = (params_res.clone(), lib_res.clone()) {
            if p_params.exists() && p_lib.exists() {
                return (p_params, p_lib);
            }
        }

        // Fallback to local build directory (dev)
        let build_dir = current_dir.join("build");
        (
            build_dir.join("porcupine_params.pv"),
            build_dir.join("libpv_porcupine.dll"),
        )
    })();

    println!("[DEBUG] Porcupine params path: {:?}", porcupine_params_path);
    println!("[DEBUG] Porcupine lib path: {:?}", porcupine_lib_path);
    println!("[DEBUG] Wakeword path: {:?}", wakeword_path);
    println!("[DEBUG] Whisper model path: {:?}", whisper_model_path);
    if let Ok(md) = std::fs::metadata(&wakeword_path) {
        println!("[DEBUG] Wakeword size: {} bytes", md.len());
    }
    if let Ok(md) = std::fs::metadata(&porcupine_params_path) {
        println!("[DEBUG] Porcupine params size: {} bytes", md.len());
    }
    if let Ok(md) = std::fs::metadata(&porcupine_lib_path) {
        println!("[DEBUG] Porcupine lib size: {} bytes", md.len());
    }

    if config.porcupine_key.trim().is_empty() {
        return Err(anyhow!(
            "Missing Porcupine access key. Please set it in Settings > API Keys."
        ));
    }

    let porcupine = {
        // First try with explicit model and library paths
        let attempt = PorcupineBuilder::new_with_keyword_paths(
            &config.porcupine_key,
            &[wakeword_path.to_str().unwrap()],
        )
        .sensitivities(&[config.wwd_sensitivity])
        .model_path(porcupine_params_path.to_str().unwrap())
        .library_path(porcupine_lib_path.to_str().unwrap())
        .init();

        match attempt {
            Ok(pv) => pv,
            Err(e1) => {
                eprintln!(
                    "[DEBUG] Porcupine init with explicit paths failed: {:?}",
                    e1
                );
                // Fallback: let crate resolve embedded defaults
                PorcupineBuilder::new_with_keyword_paths(
                    &config.porcupine_key,
                    &[wakeword_path.to_str().unwrap()],
                )
                .sensitivities(&[config.wwd_sensitivity])
                .init()
                .map_err(|e2| anyhow!(
                    "Unable to create Porcupine wake word engine: explicit paths error: {:?}; fallback error: {:?}",
                    e1, e2
                ))?
            }
        }
    };
    println!(
        "[DEBUG] Initialized Porcupine with wakeword path: {:?}",
        wakeword_path
    );

    let elevenlabs_model = match config.elevenlabs_model.as_str() {
        "eleven_multilingual_v2" => Model::ElevenMultilingualV2,
        "eleven_flash_v2_5" => Model::ElevenFlashV2_5,
        "eleven_turbo_v2_5" => Model::ElevenTurboV2_5,
        _ => Model::ElevenMultilingualV2,
    };

    println!("[DEBUG] Selected ElevenLabs model: {:?}", elevenlabs_model);

    println!("[DEBUG] Downloading Whisper model if needed...");
    download_whisper_with_progress(&tauri_app, WHISPER_MODEL_URL, &whisper_model_path).await?;
    println!("[DEBUG] Whisper model ready at: {:?}", whisper_model_path);

    let whisper_context = WhisperContext::new_with_params(
        whisper_model_path.to_str().unwrap(),
        WhisperContextParameters::default(),
    )
    .with_context(|| "Failed to load Whisper model")?;
    let whisper_context = Arc::new(whisper_context);
    println!("[DEBUG] WhisperContext initialized");

    let audio_buffer = Arc::new(Mutex::new(VecDeque::<i16>::with_capacity(SAMPLE_RATE * 5)));
    println!("[DEBUG] Audio buffer initialized");

    let vad_mode = match config.vad_mode.to_lowercase().as_str() {
        "quality" => VadMode::Quality,
        "aggressive" => VadMode::Aggressive,
        "veryaggressive" | "very_aggressive" | "very-aggressive" => VadMode::VeryAggressive,
        _ => VadMode::Aggressive,
    };
    println!("[DEBUG] VAD mode set to: {}", config.vad_mode);
    let vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, vad_mode);

    audio_input::start_audio_stream(
        audio_buffer.clone(),
        config.default_microphone_name.clone(),
        config.default_microphone_index,
    )
        .with_context(|| "Failed to start audio input stream")?;
    println!("[DEBUG] Audio input stream started");

    let app = AppContext {
        config,
        audio_player,
        porcupine,
        vad: Mutex::new(vad),
        whisper_context,
        audio_buffer,
        elevenlabs_model,
    };
    println!("[DEBUG] AppContext initialized");

    println!("\n--- Prepared environment successfully  ---");
    // Now ready to listen for wake word
    emit_state(&tauri_app, crate::JarvisStateEnum::WakeListening).await;

    main_loop_with_running(&app, is_running, &tauri_app).await?;
    Ok(())
}

async fn main_loop_with_running(
    app: &AppContext,
    is_running: Arc<AtomicBool>,
    tauri_app: &tauri::AppHandle,
) -> Result<()> {
    println!("[DEBUG] Entered main_loop_with_running()");
    let http_client = Client::new();

    while is_running.load(Ordering::Relaxed) {
        // 1) Wake‐word detection
        println!("[DEBUG] Waiting for wake word...");
        emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
        get_text::wait_for_wakeword(app, &is_running)?;
        let perf_start = Instant::now();
        let wake_start_ms = chrono::Utc::now().timestamp_millis();
        println!("\nWake word detected!");
        if let Err(e) = app.audio_player.play_sound("assets/beep.wav") {
            eprintln!("Failed to play beep sound: {e}");
        }

        // Check if we should stop
        if !is_running.load(Ordering::Relaxed) {
            break;
        }

        // 2) Record user command
        println!("Listening for command... (Speak now)");
        println!("[DEBUG] Recording command...");
        emit_state(tauri_app, crate::JarvisStateEnum::Recording).await;
        let speech_segment = get_text::record_command(app, &is_running)?;

        if speech_segment.is_empty() {
            println!("No speech detected after wake word. Please try again.");
            println!("[DEBUG] No speech detected after wake word");
            emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
        } else {
            println!(
                "Processing {} seconds of audio...",
                speech_segment.len() as f32 / SAMPLE_RATE as f32
            );
            println!("[DEBUG] Processing command inline (no spawn)");
            emit_state(tauri_app, crate::JarvisStateEnum::Processing).await;

            let whisper_ctx = Arc::clone(&app.whisper_context);
            let config = app.config.clone();
            let elevenlabs_model = app.elevenlabs_model.clone();
            let speech_segment = speech_segment.clone();
            let client_clone = http_client.clone();

            // Ensure at least 1s of audio (Whisper needs >= ~1000 ms)
            let mut audio_for_transcribe = speech_segment.clone();
            // Use ~1.2s to comfortably exceed Whisper's 1s minimum
            let min_samples = (SAMPLE_RATE * 12) / 10; // 1.2s at 16kHz
            if audio_for_transcribe.len() < min_samples {
                audio_for_transcribe.reserve(min_samples - audio_for_transcribe.len());
                audio_for_transcribe
                    .extend(std::iter::repeat(0).take(min_samples - audio_for_transcribe.len()));
            }

            // a) Transcribe
            println!("[DEBUG] Transcribing audio to text...");
            let mut user_prompt = get_text::transcribe(
                &whisper_ctx,
                &audio_for_transcribe,
                &config.whisper_language,
            )?;
            user_prompt = user_prompt.trim().to_string();

            // If transcription is empty, still emit a placeholder so UI shows the user message
            let transcription_was_empty = user_prompt.is_empty();
            if transcription_was_empty {
                user_prompt = String::from("(couldn't understand)");
            }

            // Emit user message
            emit_message(tauri_app, "user", &user_prompt).await;

            // If we couldn't understand, do not send to LLM; go back to listening
            if transcription_was_empty {
                emit_message(
                    tauri_app,
                    "assistant",
                    "Sorry, I didn't catch that. Please repeat.",
                )
                .await;
                emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
                continue;
            }

            // b) Pre-transform / exit
            println!("[DEBUG] Optionally transforming prompt...");
            if transform_text::if_contains_exit(
                &user_prompt,
                &config,
                elevenlabs_model.clone(),
                wake_start_ms,
                tauri_app.clone(),
            )
            .await
            {
                // Exit early (no more processing)
                continue;
            }
            let transformed_prompt =
                transform_text::if_contains_transform(&user_prompt, elevenlabs_model.clone());

            // c) Query LLM with context from the currently selected conversation
            println!("[DEBUG] Sending prompt to LLM...");
            if config.gemini_key.trim().is_empty() {
                emit_message(
                    tauri_app,
                    "system",
                    "Please enter your Gemini API key in Settings > API Keys.",
                )
                .await;
                emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
                continue;
            }
            let ctx_text = build_ctx_text_from_active(tauri_app);
            let llm_answer =
                send_to_llm::query_gemini(&transformed_prompt, &config, &ctx_text).await?;

            // Emit assistant message with initial meta (TTS usage estimate)
            let (tts_tokens_est, _tts_chars) = estimate_tts_tokens_and_chars(&llm_answer);
            let assistant_created_at = chrono::Utc::now().timestamp_millis();
            let assistant_payload = serde_json::json!({
                "role": "assistant",
                "content": llm_answer,
                "createdAt": assistant_created_at,
                "meta": {
                    "ttsTokensEst": tts_tokens_est
                }
            });
            let _ = tauri_app.emit("new-message", assistant_payload);

            // d) Post-transform
            println!("[DEBUG] Optionally transforming LLM response...");
            let llm_answer = transform_text::if_contains_transform_post_llm(&llm_answer);
            let llm_answer = llm_answer.trim().to_string();

            // If post-transform result is empty, skip TTS and return to listening
            if llm_answer.is_empty() {
                println!(
                    "[DEBUG] Post-LLM transform produced empty output; skipping TTS and returning to WakeListening"
                );
                emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
                continue;
            }

            // e) Buffer TTS audio
            println!("[DEBUG] Buffering TTS response...");
            if config.elevenlabs_key.trim().is_empty() {
                emit_message(
                    tauri_app,
                    "system",
                    "Please enter your ElevenLabs API key in Settings > API Keys.",
                )
                .await;
                emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
                continue;
            }
            if config.voice_id.trim().is_empty() {
                emit_message(
                    tauri_app,
                    "system",
                    "Please enter your ElevenLabs Voice ID in Settings > API Keys.",
                )
                .await;
                emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
                continue;
            }
            emit_state(tauri_app, crate::JarvisStateEnum::Speaking).await;
            let url = format!(
                "https://api.elevenlabs.io/v1/text-to-speech/{}/stream?output_format=mp3_44100_128",
                &config.voice_id
            );
            let resp = client_clone
                .post(&url)
                .header("xi-api-key", &config.elevenlabs_key)
                .json(&serde_json::json!({
                    "text": llm_answer,
                    "model_id": String::from(elevenlabs_model),
                }))
                .send()
                .await
                .map_err(|e| anyhow!("TTS HTTP error: {}", e))?;

            if !resp.status().is_success() {
                let s = resp.status();
                let b = resp.text().await.unwrap_or_default();
                return Err(anyhow!("TTS API returned {}: {}", s, b));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| anyhow!("Error reading TTS body: {}", e))?;
            let audio_bytes = bytes.to_vec();

            // f) Play audio on a dedicated thread
            println!("[DEBUG] Playing buffered audio...");
            let tokio_handle = Handle::current();
            let output_device_name = config.default_output_device_name.clone();
            let join = thread::spawn(move || -> Result<(), anyhow::Error> {
                tokio_handle.block_on(async {
                    let cursor = std::io::Cursor::new(audio_bytes);
                    let stream = if let Some(name) = output_device_name.as_deref() {
                        let host = cpal::default_host();
                        if let Ok(mut devs) = host.output_devices() {
                            let name_lower = name.to_lowercase();
                            if let Some(device) = devs.find(|d| d
                                .name()
                                .map(|n| n.to_lowercase().contains(&name_lower))
                                .unwrap_or(false))
                            {
                                rodio::OutputStreamBuilder::from_device(device)?
                                    .open_stream()
                                    .map_err(|e| anyhow!("Audio init error: {}", e))?
                            } else {
                                rodio::OutputStreamBuilder::from_default_device()?
                                    .open_stream()
                                    .map_err(|e| anyhow!("Audio init error: {}", e))?
                            }
                        } else {
                            rodio::OutputStreamBuilder::from_default_device()?
                                .open_stream()
                                .map_err(|e| anyhow!("Audio init error: {}", e))?
                        }
                    } else {
                        rodio::OutputStreamBuilder::from_default_device()?
                            .open_stream()
                            .map_err(|e| anyhow!("Audio init error: {}", e))?
                    };
                    let sink = rodio::Sink::connect_new(&stream.mixer());
                    let decoder =
                        rodio::Decoder::new(cursor).map_err(|e| anyhow!("Decode error: {}", e))?;
                    sink.append(decoder);
                    sink.sleep_until_end();
                    Ok(())
                })
            });

            // 1) Catch thread panic or return
            let thread_res = join.join().map_err(|_| anyhow!("Audio thread panicked"))?;
            // 2) Propagate any playback error
            thread_res?;

            println!("[DEBUG] Finished speaking response");
            // Emit meta update with total latency (wake -> end of speech)
            let total_ms = perf_start.elapsed().as_millis() as u64;
            let _ = tauri_app.emit(
                "message-meta",
                serde_json::json!({
                    "createdAtOfAssistant": assistant_created_at,
                    "meta": { "latencyMs": total_ms }
                })
            );
            emit_state(tauri_app, crate::JarvisStateEnum::WakeListening).await;
        }

        println!("\n----------------------------------------\n");
        println!("[DEBUG] End of main loop iteration");
    }

    println!("[DEBUG] Jarvis stopped");
    Ok(())
}
