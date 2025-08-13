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

pub mod audio_input;
pub mod get_text;
pub mod models;
pub mod run_jarvis;
pub mod send_to_llm;
pub mod transform_text;
pub mod tts;
pub mod utils;

use elevenlabs_rs::Model as ElevenModel;
use serde::{Deserialize, Serialize};
use std::thread::JoinHandle;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_window_state::Builder as WindowStateBuilder;
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    porcupine_key: String,
    gemini_key: String,
    elevenlabs_key: String,

    whisper_language: String,
    default_microphone_index: i32,
    default_microphone_name: Option<String>,
    default_output_device_name: Option<String>,

    gemini_model: String,
    elevenlabs_model: String,
    voice_id: String,

    llm_system_prompt: String,
    vad_mode: String,
    wwd_sensitivity: f32,
    context_window_expiration_seconds: i32,

    frame_duration_ms: i32,
    silence_threshold_seconds: i32,
    speech_trigger_frames: i32,
    frame_length_wwd: i32,

    dock_position: Option<String>,
    input_mode: Option<String>,
    theme: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum JarvisStateEnum {
    Idle,
    Loading,
    WakeListening,
    Recording,
    Processing,
    Speaking,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TurnDto {
    pub role: String,
    pub content: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

fn config_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

fn copy_bundled_assets(app: &tauri::AppHandle) -> Result<(), String> {
    let roaming_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;

    // Create necessary directories
    let history_dir = roaming_dir.join("history");
    let context_dir = roaming_dir.join("context");
    let assets_dir = roaming_dir.join("assets");

    fs::create_dir_all(&history_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&context_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;

    println!("[DEBUG] Created directories:");
    println!("  - History: {:?}", history_dir);
    println!("  - Context: {:?}", context_dir);
    println!("  - Assets:  {:?}", assets_dir);

    // Try to copy essential assets into roaming if present in resources
    // Wakeword .ppn is no longer copied to roaming; it is always used from bundled resources
    let to_copy = [("assets/beep.wav", assets_dir.join("beep.wav"))];
    for (res_rel, dest) in to_copy {
        if dest.exists() {
            continue;
        }
        if let Ok(src) = app
            .path()
            .resolve(res_rel, tauri::path::BaseDirectory::Resource)
        {
            if src.exists() {
                if let Err(e) = fs::copy(&src, &dest) {
                    println!(
                        "[DEBUG] Failed to copy resource {:?} to {:?}: {}",
                        src, dest, e
                    );
                } else {
                    println!("[DEBUG] Copied resource {:?} to {:?}", src, dest);
                }
            } else {
                println!("[DEBUG] Resource path does not exist: {:?}", src);
            }
        } else {
            println!("[DEBUG] Failed to resolve resource path: {}", res_rel);
        }
    }

    Ok(())
}

#[tauri::command]
fn cmd_load_config(app: tauri::AppHandle) -> Result<Config, String> {
    // Copy bundled assets to roaming directory
    let _ = copy_bundled_assets(&app);

    let cfg_path = config_path(&app).map_err(|e| e.to_string())?;
    println!("[DEBUG] Config path: {:?}", cfg_path);

    if cfg_path.exists() {
        println!("[DEBUG] Loading existing config from: {:?}", cfg_path);
        let s = fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
        let cfg: Config = serde_json::from_str(&s).map_err(|e| e.to_string())?;

        // Compute defaults but do NOT override if user already set values
        let roaming_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
        println!("[DEBUG] Roaming directory: {:?}", roaming_dir);
        // Paths are hard-coded; no longer in config

        // No path migration necessary; runtime will resolve paths

        println!("[DEBUG] Final config loaded (paths managed by runtime)");

        return Ok(cfg);
    }

    // Create default config if none exists
    println!("[DEBUG] No config found in roaming directory, creating default config");
    let _roaming_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;

    // Create a default config with all the necessary fields
    let mut cfg = Config {
        porcupine_key: String::new(),
        gemini_key: String::new(),
        elevenlabs_key: String::new(),
        whisper_language: "en".to_string(),
        default_microphone_index: 0,
        default_microphone_name: None,
        default_output_device_name: None,
        gemini_model: "gemini-2.5-flash".to_string(),
        elevenlabs_model: "eleven_flash_v2_5".to_string(),
        voice_id: "hU1ratPhBTZNviWitzAh".to_string(),
        llm_system_prompt: "You are a specialized voice assistant. Your primary function is to provide concise, accurate, and direct responses to transcribed user speech. You must strictly adhere to the following guidelines.\n\n# Core Mandate: The Voice Environment\n\n- Input is Imperfect: Always assume the user's input is transcribed speech. It may contain transcription errors, misheard words, homophones (e.g., 'right' vs. 'write'), or be missing punctuation. Your primary task is to interpret the user's likely intent despite these potential flaws.\n- Output is Spoken: All your responses must be optimized for text-to-speech (TTS). Use simple, natural sentence structures that are easy to say and understand. Prioritize clarity over complex vocabulary or sentence construction.\n- Be Direct: Get straight to the point. Avoid conversational filler, preambles ('Certainly, here is the information you requested...'), or postambles ('I hope that helps!').\n\n# Interaction Rules\n\n- Greetings: If the user greets you (e.g., 'hello', 'hi'), respond with a simple, appropriate greeting.\n- Ambiguity: If a user's request is too vague or nonsensical to interpret with high confidence (and is not a greeting), ask for clarification. Do not guess or attempt to answer a question you don't understand. A simple 'I'm not sure what you mean. Could you please rephrase that?' is sufficient.\n- Language: Always respond in the same language as the user's input.\n\n# Strict Output Formatting\n\nYour adherence to these formatting rules is critical. Do not deviate.\n\n- Single-Item Answers: When a query has a single, factual answer (e.g., a definition, a capital city), return only the answer itself. Do not wrap it in a sentence.\n- Lists: For requests that require a list of items, return a numbered list.\n - Constraint: The list must contain no more than 5 items.\n- Code: For requests involving code, return only the code block.\n - Constraint: The code must be wrapped in [[copy]] and [[/copy]] tags.\n - Constraint: Do not include language identifiers (like javascript), explanations, or comments inside or outside the tags.\n\n# Examples (Illustrating Tone and Formatting)\n\n<example>\nuser: hello\nmodel: Hello.\n</example>\n\n<example>\nuser: what is the tallest mountain\nmodel: Mount Everest\n</example>\n\n<example>\nuser: name four planets in our solar system\nmodel:\n1. Mercury\n2. Venus\n3. Earth\n4. Mars\n</example>\n\n<example>\nuser: python function to check if a number is even\nmodel:\n[[copy]]\ndef is_even(n):\n return n % 2 == 0\n[[/copy]]\n</example>\n\n<example>\nuser: can you tell me about the um the thing for cars\nmodel: I'm not sure what you mean by 'the thing for cars.' Could you please be more specific?\n</example>\n\n<example>\nuser: comment ça va\nmodel: Bien, merci. Et vous?\n</example>".to_string(),
        vad_mode: "Quality".to_string(),
        wwd_sensitivity: 0.8,
        context_window_expiration_seconds: 1800,
        frame_duration_ms: 30,
        silence_threshold_seconds: 1,
        speech_trigger_frames: 8,
        frame_length_wwd: 512,
        dock_position: Some("right".to_string()),
        input_mode: Some("audio".to_string()),
        theme: Some("emerald".to_string()),
    };

    // Try to load from bundled resource first to get any additional defaults
    let seeded = app
        .path()
        .resolve("config.json", tauri::path::BaseDirectory::Resource);
    println!("[DEBUG] Attempting to resolve bundled config: {:?}", seeded);
    if let Ok(seed_path) = seeded {
        println!("[DEBUG] Bundled config path: {:?}", seed_path);
        if seed_path.exists() {
            println!("[DEBUG] Found bundled config, using it as base");
            match fs::read_to_string(&seed_path) {
                Ok(s) => {
                    match serde_json::from_str::<Config>(&s) {
                        Ok(bundled_cfg) => {
                            println!("[DEBUG] Successfully parsed bundled config");
                            // Merge bundled config with our default, keeping our paths
                            cfg.porcupine_key = bundled_cfg.porcupine_key;
                            cfg.gemini_key = bundled_cfg.gemini_key;
                            cfg.elevenlabs_key = bundled_cfg.elevenlabs_key;
                            cfg.whisper_language = bundled_cfg.whisper_language;
                            cfg.default_microphone_index = bundled_cfg.default_microphone_index;
                            cfg.default_microphone_name = bundled_cfg.default_microphone_name;
                            cfg.default_output_device_name = bundled_cfg.default_output_device_name;
                            cfg.gemini_model = bundled_cfg.gemini_model;
                            cfg.elevenlabs_model = bundled_cfg.elevenlabs_model;
                            cfg.voice_id = bundled_cfg.voice_id;
                            cfg.llm_system_prompt = bundled_cfg.llm_system_prompt;
                            cfg.vad_mode = bundled_cfg.vad_mode;
                            cfg.wwd_sensitivity = bundled_cfg.wwd_sensitivity;
                            cfg.context_window_expiration_seconds =
                                bundled_cfg.context_window_expiration_seconds;
                            cfg.frame_duration_ms = bundled_cfg.frame_duration_ms;
                            cfg.silence_threshold_seconds = bundled_cfg.silence_threshold_seconds;
                            cfg.speech_trigger_frames = bundled_cfg.speech_trigger_frames;
                            cfg.frame_length_wwd = bundled_cfg.frame_length_wwd;
                            cfg.dock_position = bundled_cfg.dock_position;
                            cfg.input_mode = bundled_cfg.input_mode;
                            cfg.theme = bundled_cfg.theme;
                        }
                        Err(e) => println!("[DEBUG] Failed to parse bundled config JSON: {}", e),
                    }
                }
                Err(e) => println!("[DEBUG] Failed to read bundled config file: {}", e),
            }
        } else {
            println!("[DEBUG] Bundled config path does not exist");
        }
    } else {
        println!("[DEBUG] Failed to resolve bundled config path");
    }

    // Paths are resolved at runtime; nothing to set here

    // Save the default config to the roaming directory
    println!("[DEBUG] Saving default config to: {:?}", cfg_path);
    let s = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    fs::write(&cfg_path, s).map_err(|e| e.to_string())?;

    println!("[DEBUG] Created default config (paths managed by runtime)");

    Ok(cfg)
}

#[tauri::command]
fn cmd_save_config(app: tauri::AppHandle, config: Config) -> Result<(), String> {
    let cfg_path = config_path(&app).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(cfg_path, s).map_err(|e| e.to_string())
}

#[tauri::command]
fn cmd_get_roaming_dir(app: tauri::AppHandle) -> Result<String, String> {
    let roaming_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(roaming_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn cmd_resolve_resource_path(app: tauri::AppHandle, relative: String) -> Result<String, String> {
    match app
        .path()
        .resolve(&relative, tauri::path::BaseDirectory::Resource)
    {
        Ok(p) => Ok(p.to_string_lossy().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn history_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let history = dir.join("history");
    std::fs::create_dir_all(&history).map_err(|e| e.to_string())?;
    Ok(history)
}

#[tauri::command]
fn cmd_list_history_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let history = history_dir(&app)?;
    let mut files: Vec<(std::time::SystemTime, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&history) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("json") {
                        let meta = entry.metadata().map_err(|e| e.to_string())?;
                        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let name = path.file_name().unwrap().to_string_lossy().to_string();
                        files.push((modified, name));
                    }
                }
            }
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(files.into_iter().map(|(_, n)| n).collect())
}

#[tauri::command]
fn cmd_create_conversation(app: tauri::AppHandle) -> Result<String, String> {
    let history = history_dir(&app)?;
    let ts = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let filename = format!("New Conversation - {}.json", ts);
    let path = history.join(&filename);
    std::fs::write(&path, "[]").map_err(|e| e.to_string())?;
    Ok(filename)
}

#[tauri::command]
fn cmd_read_conversation(app: tauri::AppHandle, filename: String) -> Result<Vec<TurnDto>, String> {
    let history = history_dir(&app)?;
    let path = history.join(&filename);
    let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let turns: Vec<TurnDto> = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    Ok(turns)
}

#[tauri::command]
fn cmd_append_turn(app: tauri::AppHandle, filename: String, turn: TurnDto) -> Result<(), String> {
    let history = history_dir(&app)?;
    let path = history.join(&filename);
    let mut turns: Vec<TurnDto> = if path.exists() {
        let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).unwrap_or_default()
    } else {
        Vec::new()
    };
    turns.push(turn);
    let s = serde_json::to_string_pretty(&turns).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())
}

#[tauri::command]
fn cmd_delete_conversation(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    let history = history_dir(&app)?;
    let path = history.join(&filename);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Build context window text from selected conversation
fn build_ctx_text_from_conversation(
    app: &tauri::AppHandle,
    filename: &str,
) -> anyhow::Result<String> {
    let history = history_dir(app).map_err(|e| anyhow::anyhow!(e))?;
    let path = history.join(filename);
    if !path.exists() {
        return Ok(String::new());
    }
    let s = std::fs::read_to_string(&path)?;
    let turns: Vec<TurnDto> = serde_json::from_str(&s).unwrap_or_default();
    let start = turns.len().saturating_sub(12);
    let mut out = String::new();
    for t in turns.iter().skip(start) {
        out.push_str(&format!("{}: {}\n", t.role.to_uppercase(), t.content));
    }
    Ok(out)
}

#[tauri::command]
fn cmd_set_active_conversation(
    state: tauri::State<JarvisState>,
    filename: String,
) -> Result<(), String> {
    let mut g = state.active_conversation.lock().unwrap();
    *g = Some(filename);
    Ok(())
}

#[derive(Serialize)]
struct TitleResult {
    new_filename: String,
    title: String,
}

fn sanitize_title_for_filename(title: &str) -> String {
    let mut s = title.trim().to_string();
    // Remove characters invalid on Windows filesystem
    for ch in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        s = s.replace(ch, " ");
    }
    // Collapse whitespace
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

fn extract_timestamp_stem(name: &str) -> String {
    // expects something like "<title> - YYYY-MM-DD_HH-MM-SS.json"
    let no_ext = name.trim_end_matches(".json");
    if let Some(idx) = no_ext.rfind(" - ") {
        return no_ext[idx + 3..].to_string();
    }
    chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

#[tauri::command]
async fn cmd_generate_and_rename_conversation(
    app: tauri::AppHandle,
    filename: String,
) -> Result<TitleResult, String> {
    // Read turns to build seed
    let history = history_dir(&app)?;
    let path = history.join(&filename);
    let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let turns: Vec<TurnDto> = serde_json::from_str(&s).unwrap_or_default();
    let mut seed = String::new();
    for t in turns.iter().take(4) {
        seed.push_str(&t.content);
        seed.push_str("\n");
    }
    if seed.is_empty() {
        seed = "New conversation".to_string();
    }

    // Load config to access API key
    let cfg = cmd_load_config(app.clone()).map_err(|e| e.to_string())?;
    let run_config = crate::models::Config {
        porcupine_key: cfg.porcupine_key,
        gemini_key: cfg.gemini_key,
        elevenlabs_key: cfg.elevenlabs_key,
        whisper_language: cfg.whisper_language,
        context_window_expiration_seconds: cfg.context_window_expiration_seconds as u64,
        default_microphone_index: cfg.default_microphone_index as usize,
        default_microphone_name: cfg.default_microphone_name.clone(),
        default_output_device_name: cfg.default_output_device_name.clone(),
        gemini_model: cfg.gemini_model,
        elevenlabs_model: cfg.elevenlabs_model.clone(),
        voice_id: cfg.voice_id,
        llm_system_prompt: cfg.llm_system_prompt,
        vad_mode: cfg.vad_mode,
        wwd_sensitivity: cfg.wwd_sensitivity,
        frame_duration_ms: cfg.frame_duration_ms as usize,
        silence_threshold_seconds: cfg.silence_threshold_seconds as usize,
        speech_trigger_frames: cfg.speech_trigger_frames as usize,
        frame_length_wwd: cfg.frame_length_wwd as usize,
    };

    let raw_title = crate::send_to_llm::generate_conversation_title(&seed, &run_config)
        .await
        .map_err(|e| e.to_string())?;
    let title = sanitize_title_for_filename(&raw_title);

    // Prevent AI from naming conversations "New Conversation" as it breaks the system
    if title.to_lowercase() == "new conversation" {
        return Err(
            "AI generated title 'New Conversation' is not allowed as it would break the system"
                .into(),
        );
    }
    let ts = extract_timestamp_stem(&filename);
    let new_filename = format!("{} - {}.json", title, ts);
    let new_path = history.join(&new_filename);
    std::fs::rename(&path, &new_path).map_err(|e| e.to_string())?;

    Ok(TitleResult {
        new_filename,
        title,
    })
}

#[tauri::command]
fn cmd_rename_conversation(
    app: tauri::AppHandle,
    filename: String,
    new_title: String,
) -> Result<TitleResult, String> {
    let history = history_dir(&app)?;
    let path = history.join(&filename);
    if !path.exists() {
        return Err("Conversation not found".into());
    }
    let title = sanitize_title_for_filename(&new_title);

    // Prevent manual renaming to "New Conversation" as it breaks the system
    if title.to_lowercase() == "new conversation" {
        return Err("Title 'New Conversation' is not allowed as it would break the system".into());
    }

    let ts = extract_timestamp_stem(&filename);
    let new_filename = format!("{} - {}.json", title, ts);
    let new_path = history.join(&new_filename);
    std::fs::rename(&path, &new_path).map_err(|e| e.to_string())?;
    Ok(TitleResult {
        new_filename,
        title,
    })
}

pub struct JarvisState {
    is_running: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
    active_conversation: Mutex<Option<String>>,
}

#[tauri::command]
fn cmd_start_jarvis(
    state: tauri::State<JarvisState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    // Lock the mutex to get exclusive access to the thread handle.
    let mut handle_guard = state.handle.lock().unwrap();

    // Check if there is already a handle.
    if let Some(handle) = &*handle_guard {
        // If there is a handle, check if the thread is finished.
        if !handle.is_finished() {
            println!("[Tauri] Attempted to start, but Jarvis is already running.");
            return Err("Jarvis process is already running.".into());
        }
    }

    // If we reach here, it means either:
    // 1. No thread was ever started.
    // 2. The previous thread has finished.
    // We are now clear to start a new one.

    // Set the running flag to true.
    state.is_running.store(true, Ordering::Relaxed);
    let is_running_clone = Arc::clone(&state.is_running);

    println!("[Tauri] Starting Jarvis process in a new thread...");

    // Spawn the new thread.
    let new_handle = std::thread::spawn(move || {
        // Load config and start Jarvis
        if let Ok(config) = cmd_load_config(app.clone()) {
            // Convert Config to the format expected by run_jarvis
            let run_config = models::Config {
                porcupine_key: config.porcupine_key,
                gemini_key: config.gemini_key,
                elevenlabs_key: config.elevenlabs_key,
                whisper_language: config.whisper_language,
                context_window_expiration_seconds: config.context_window_expiration_seconds as u64,
                default_microphone_index: config.default_microphone_index as usize,
                default_microphone_name: config.default_microphone_name.clone(),
                default_output_device_name: config.default_output_device_name.clone(),
                gemini_model: config.gemini_model,
                elevenlabs_model: config.elevenlabs_model,
                voice_id: config.voice_id,
                llm_system_prompt: config.llm_system_prompt,
                vad_mode: config.vad_mode,
                wwd_sensitivity: config.wwd_sensitivity,
                frame_duration_ms: config.frame_duration_ms as usize,
                silence_threshold_seconds: config.silence_threshold_seconds as usize,
                speech_trigger_frames: config.speech_trigger_frames as usize,
                frame_length_wwd: config.frame_length_wwd as usize,
            };

            // Start the Jarvis process
            run_jarvis::start_jarvis(is_running_clone.clone(), run_config, app.clone());
        }
        // If anything goes wrong and we return, ensure the running flag is false
        is_running_clone.store(false, Ordering::Relaxed);
    });

    // Store the new handle in our state, replacing any old (finished) one.
    *handle_guard = Some(new_handle);

    Ok("Jarvis process started successfully.".into())
}

#[tauri::command]
fn cmd_stop_jarvis(state: tauri::State<JarvisState>) -> Result<String, String> {
    // We don't need to lock the handle here, because setting the atomic bool
    // is a safe, independent operation. The running thread will see this change
    // and shut down on its own time.
    if state.is_running.load(Ordering::Relaxed) {
        println!("[Tauri] Sending stop signal to Jarvis.");
        state.is_running.store(false, Ordering::Relaxed);
        Ok("Jarvis stop signal sent.".into())
    } else {
        Err("Jarvis is not running.".into())
    }
}

#[tauri::command]
fn cmd_get_jarvis_status(state: tauri::State<JarvisState>) -> bool {
    state.is_running.load(Ordering::Relaxed)
}

#[tauri::command]
fn cmd_get_jarvis_state() -> JarvisStateEnum {
    // This will be updated by the run_jarvis module
    JarvisStateEnum::Idle
}

#[tauri::command]
fn cmd_emit_state_change(app: tauri::AppHandle, state: JarvisStateEnum) {
    let _ = app.emit("jarvis-state-changed", state);
}

#[tauri::command]
fn cmd_emit_message(app: tauri::AppHandle, role: String, content: String) {
    let message = serde_json::json!({
        "role": role,
        "content": content,
        "createdAt": chrono::Utc::now().timestamp_millis()
    });
    let _ = app.emit("new-message", message);
}

#[tauri::command]
async fn cmd_send_text(app: tauri::AppHandle, prompt: String) -> Result<String, String> {
    // Emit user message immediately for snappy UI
    cmd_emit_message(app.clone(), "user".into(), prompt.clone());

    // Load config and map to runtime model
    let cfg = cmd_load_config(app.clone()).map_err(|e| e.to_string())?;
    if cfg.gemini_key.trim().is_empty() {
        cmd_emit_message(
            app.clone(),
            "system".into(),
            "Please enter your Gemini API key in Settings > API Keys.".into(),
        );
        return Err("Missing Gemini API key".into());
    }
    let run_config = crate::models::Config {
        porcupine_key: cfg.porcupine_key,
        gemini_key: cfg.gemini_key,
        elevenlabs_key: cfg.elevenlabs_key,
        whisper_language: cfg.whisper_language,
        context_window_expiration_seconds: cfg.context_window_expiration_seconds as u64,
        default_microphone_index: cfg.default_microphone_index as usize,
        default_microphone_name: cfg.default_microphone_name.clone(),
        default_output_device_name: cfg.default_output_device_name.clone(),
        gemini_model: cfg.gemini_model,
        elevenlabs_model: cfg.elevenlabs_model.clone(),
        voice_id: cfg.voice_id,
        llm_system_prompt: cfg.llm_system_prompt,
        vad_mode: cfg.vad_mode,
        wwd_sensitivity: cfg.wwd_sensitivity,
        frame_duration_ms: cfg.frame_duration_ms as usize,
        silence_threshold_seconds: cfg.silence_threshold_seconds as usize,
        speech_trigger_frames: cfg.speech_trigger_frames as usize,
        frame_length_wwd: cfg.frame_length_wwd as usize,
    };

    // Optional text transforms (clipboard, etc.)
    let eleven_model = match run_config.elevenlabs_model.as_str() {
        "eleven_multilingual_v2" => ElevenModel::ElevenMultilingualV2,
        "eleven_flash_v2_5" => ElevenModel::ElevenFlashV2_5,
        "eleven_turbo_v2_5" => ElevenModel::ElevenTurboV2_5,
        _ => ElevenModel::ElevenMultilingualV2,
    };
    let transformed = crate::transform_text::if_contains_transform(&prompt, eleven_model);

    // Build context from active conversation selection
    let ctx_text = {
        let state = app.state::<JarvisState>();
        let current = state.active_conversation.lock().unwrap().clone();
        if let Some(fname) = current {
            build_ctx_text_from_conversation(&app, &fname).unwrap_or_default()
        } else {
            String::new()
        }
    };

    // Query LLM with selected chat context
    let mut answer = crate::send_to_llm::query_gemini(&transformed, &run_config, &ctx_text)
        .await
        .map_err(|e| e.to_string())?;

    // Post-transform (copy blocks, etc.)
    answer = crate::transform_text::if_contains_transform_post_llm(&answer);
    answer = answer.trim().to_string();

    // Emit assistant message
    cmd_emit_message(app.clone(), "assistant".into(), answer.clone());

    Ok(answer)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(WindowStateBuilder::default().build())
        .plugin(tauri_plugin_opener::init())
        .manage(JarvisState {
            is_running: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
            active_conversation: Mutex::new(None),
        })
        // Intercept window close to hide to tray instead of quitting
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Save window geometry before hiding
                let _ = window.app_handle().save_window_state(StateFlags::all());
                api.prevent_close();
                let _ = window.hide();
            }
        })
        // Handle tray and app menu events
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "start_listening" => {
                let state = app.state::<JarvisState>();
                let _ = cmd_start_jarvis(state, app.clone());
            }
            "stop_listening" => {
                let state = app.state::<JarvisState>();
                let _ = cmd_stop_jarvis(state);
            }
            "quit" => {
                // Save all window states before quitting
                let _ = app.save_window_state(StateFlags::all());
                app.exit(0);
            }
            _ => {}
        })
        // Create the tray icon and menu
        .setup(|app| {
            // Restore window state before showing
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.restore_state(StateFlags::all());
            }

            // Try to set the window/taskbar icon from bundled resources
            if let Some(win) = app.get_webview_window("main") {
                let candidates = ["icons/icon.png", "jarvis_icon_2.png"];
                for name in candidates {
                    if let Ok(icon_path) = app
                        .path()
                        .resolve(name, tauri::path::BaseDirectory::Resource)
                    {
                        if let Ok(bytes) = std::fs::read(icon_path) {
                            if let Ok(img) = image::load_from_memory(&bytes) {
                                let img = img.to_rgba8();
                                let (w, h) = (img.width(), img.height());
                                let _ = win.set_icon(tauri::image::Image::new_owned(
                                    img.to_vec(),
                                    w,
                                    h,
                                ));
                                break;
                            }
                        }
                    }
                }
            }

            // Prepare tray icon image
            let tray_image: Option<tauri::image::Image> = (|| {
                let candidates = [
                    "icons/32x32.png",
                    "jarvis_icon_2_x32.png",
                    "jarvis_icon_2_x64.png",
                    "jarvis_icon_2.png",
                ];
                for name in candidates {
                    if let Ok(p) = app
                        .path()
                        .resolve(name, tauri::path::BaseDirectory::Resource)
                    {
                        if let Ok(bytes) = std::fs::read(p) {
                            if let Ok(img) = image::load_from_memory(&bytes) {
                                let img = img.to_rgba8();
                                let (w, h) = (img.width(), img.height());
                                return Some(tauri::image::Image::new_owned(img.to_vec(), w, h));
                            }
                        }
                    }
                }
                None
            })();

            // Build tray menu
            let title_item = MenuItemBuilder::new("Jarvis")
                .id("title")
                .enabled(false)
                .build(app)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let start_item = MenuItemBuilder::new("Start Wake Word")
                .id("start_listening")
                .build(app)?;
            let stop_item = MenuItemBuilder::new("Stop Wake Word")
                .id("stop_listening")
                .build(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let show_item = MenuItemBuilder::new("Show").id("show").build(app)?;
            let quit_item = MenuItemBuilder::new("Quit").id("quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[
                    &title_item,
                    &sep1,
                    &start_item,
                    &stop_item,
                    &sep2,
                    &show_item,
                    &quit_item,
                ])
                .build()?;

            // Build tray icon
            let mut tray_builder = TrayIconBuilder::new().menu(&menu);
            if let Some(img) = tray_image {
                tray_builder = tray_builder.icon(img);
            }
            let _tray = tray_builder
                .show_menu_on_left_click(true)
                .on_tray_icon_event(|tray, event| match event {
                    tauri::tray::TrayIconEvent::DoubleClick { .. } => {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    tauri::tray::TrayIconEvent::Click { .. } => {
                        // Left click already opens menu
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_load_config,
            cmd_save_config,
            cmd_get_roaming_dir,
            cmd_resolve_resource_path,
            cmd_list_input_devices,
            cmd_list_output_devices,
            cmd_start_jarvis,
            cmd_stop_jarvis,
            cmd_get_jarvis_status,
            cmd_get_jarvis_state,
            cmd_emit_state_change,
            cmd_emit_message,
            cmd_send_text,
            cmd_set_active_conversation,
            cmd_list_history_files,
            cmd_create_conversation,
            cmd_read_conversation,
            cmd_append_turn,
            cmd_delete_conversation,
            cmd_generate_and_rename_conversation,
            cmd_rename_conversation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn cmd_list_input_devices() -> Result<Vec<String>, String> {
    use cpal::traits::HostTrait as _;
    use cpal::traits::DeviceTrait as _;
    let host = cpal::default_host();
    let mut names = Vec::new();
    println!("[DEBUG] Enumerating input devices via host.input_devices()...");
    let mut had_primary_list = false;
    if let Ok(iter) = host.input_devices() {
        had_primary_list = true;
        for d in iter {
            match d.name() {
                Ok(n) => {
                    println!("[DEBUG] input device: {}", n);
                    names.push(n)
                }
                Err(e) => println!("[DEBUG] input device name error: {}", e),
            }
        }
    }
    if names.is_empty() {
        println!("[WARN] input_devices() returned empty{}; falling back to host.devices() filter", if had_primary_list { " (no devices)" } else { " (error)" });
        if let Ok(iter) = host.devices() {
            for d in iter {
                if d.supported_input_configs().is_ok() {
                    if let Ok(n) = d.name() {
                        println!("[DEBUG] input device (fallback): {}", n);
                        names.push(n);
                    }
                }
            }
        }
    }
    Ok(names)
}

#[tauri::command]
fn cmd_list_output_devices() -> Result<Vec<String>, String> {
    use cpal::traits::HostTrait as _;
    use cpal::traits::DeviceTrait as _;
    let host = cpal::default_host();
    let mut names = Vec::new();
    println!("[DEBUG] Enumerating output devices via host.output_devices()...");
    let mut had_primary_list = false;
    if let Ok(iter) = host.output_devices() {
        had_primary_list = true;
        for d in iter {
            match d.name() {
                Ok(n) => {
                    println!("[DEBUG] output device: {}", n);
                    names.push(n)
                }
                Err(e) => println!("[DEBUG] output device name error: {}", e),
            }
        }
    }
    if names.is_empty() {
        println!("[WARN] output_devices() returned empty{}; falling back to host.devices() filter", if had_primary_list { " (no devices)" } else { " (error)" });
        if let Ok(iter) = host.devices() {
            for d in iter {
                if d.supported_output_configs().is_ok() {
                    if let Ok(n) = d.name() {
                        println!("[DEBUG] output device (fallback): {}", n);
                        names.push(n);
                    }
                }
            }
        }
    }
    Ok(names)
}
