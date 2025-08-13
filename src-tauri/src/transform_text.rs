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

use crate::models::Config;
use crate::tts;
use anyhow::{Context, Result};
use arboard::Clipboard;
use base64::{engine::general_purpose, Engine as _};
use chrono;
use elevenlabs_rs::Model;
use png::{BitDepth, ColorType, Encoder};
use regex::Regex;
use reqwest::Client;
use serde_json::Value;
use std::{str};
use tauri::{Manager, Emitter};
use std::{thread::sleep, time::Duration};
use winapi::um::winuser::{
    keybd_event, KEYEVENTF_KEYUP, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
};
// copies text to clipboard between [[copy]] and [[/copy]] tags
pub fn copy_to_clipboard_function_for_llm(text: &str) -> Result<String> {
    println!("[DEBUG] Entered copy_to_clipboard_function_for_llm");
    let re = Regex::new(r"(?s)\[\[copy\]\](.*?)\[\[/copy\]\]")
        .context("Failed to compile copy regex")?;

    let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;

    if let Some(cap) = re.captures(text) {
        let content = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        clipboard
            .set_text(content.to_string())
            .context("Failed to set clipboard text")?;
        println!("[DEBUG] Finished copy_to_clipboard_function_for_llm");
        return Ok(re.replace_all(text, "").into_owned());
    } else {
        println!("[DEBUG] Finished copy_to_clipboard_function_for_llm");
        return Ok(text.to_string());
    }
}

// pastes clipboard content into the prompt if it contains a paste command
pub fn paste_clipboard_instead_of_text(prompt: &str) -> String {
    println!("[DEBUG] Entered paste_clipboard_instead_of_text");

    let re = match Regex::new(
        r"(?i)\b(
            ctrl\+v|control v|control-v|control\+v|
            strg\+v|steuerung v|steuerung-v|steuerung\+v|
            cmd\+v|command\+v|command v|command-v|
            contrôle\+v|controle\+v|
            контр(ол)?\+v|контр(ол)? v|
            控制\+v|
            シー\+v|
            ctr\+v|
            control\+y|control y|control-y|ctrl\+y|
            ctrl\+ins|control\+ins|control ins|control-ins|
            shift\+insert|shift\+ins|shift insert|shift-ins|
            cmd\+ins|command\+ins|command ins|command-ins
        )\b"
    ) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("[DEBUG] Regex compile error: {}", err);
            println!("[DEBUG] Finished paste_clipboard_instead_of_text (regex error)");
            return prompt.to_string();
        }
    };

    if re.is_match(prompt) {
        let mut clipboard = match Clipboard::new() {
            Ok(cb) => cb,
            Err(err) => {
                eprintln!("[DEBUG] Clipboard init error: {}", err);
                println!("[DEBUG] Finished paste_clipboard_instead_of_text (clipboard error)");
                return prompt.to_string();
            }
        };

        // try to paste text
        if let Ok(txt) = clipboard.get_text() {
            println!("[DEBUG] Finished paste_clipboard_instead_of_text (text)");
            return re.replace_all(prompt, &txt).into_owned();
        }

        // if text failed, try image
        if let Ok(img) = clipboard.get_image() {
            let mut buf = Vec::new();

            let mut encoder = Encoder::new(&mut buf, img.width as u32, img.height as u32);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);

            match encoder.write_header() {
                Ok(mut writer) => {
                    if let Err(e) = writer.write_image_data(&img.bytes) {
                        eprintln!("[DEBUG] Failed to write PNG data: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("[DEBUG] Failed to write PNG header: {}", e);
                }
            }

            let b64 = general_purpose::STANDARD.encode(&buf);
            let data_uri = format!("data:image/png;base64,{}", b64);
            println!("[DEBUG] Finished paste_clipboard_instead_of_text (image)");
            return re.replace_all(prompt, &data_uri).into_owned();
        }

        // neither text nor image
        println!("[DEBUG] Finished paste_clipboard_instead_of_text (no clipboard content)");
        return prompt.to_string();
    }

    println!("[DEBUG] Finished paste_clipboard_instead_of_text (no match)");
    prompt.to_string()
}

// extracts image parts from a prompt if it contains a data URI
pub fn extract_image_parts(prompt: &str) -> Result<(String,), (String, String, Vec<u8>, String)> {
    println!("[DEBUG] Entered extract_image_parts");
    let re = Regex::new(
        r#"(?P<pre>.*?)(?P<uri>data:image/(?P<mime>\w+);base64,(?P<b64>[A-Za-z0-9+/=]+))(?P<post>.*)"#,
    )
    .expect("Failed to compile image extraction regex");

    if let Some(cap) = re.captures(prompt) {
        let pre = cap.name("pre").unwrap().as_str().to_string();
        let mime = cap.name("mime").unwrap().as_str().to_string();
        let b64 = cap.name("b64").unwrap().as_str();
        let post = cap.name("post").unwrap().as_str().to_string();

        let bytes = general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| (pre.clone(), mime.clone(), Vec::new(), post.clone()))?;

        println!("[DEBUG] Finished extract_image_parts (found image)");
        return Err((pre, mime, bytes, post));
    } else {
        println!("[DEBUG] Finished extract_image_parts (no image)");
        return Ok((prompt.to_string(),));
    }
}

// checks if the prompt contains a "forget" command
pub fn contains_forget(prompt: &str, _config: &Config, app: &tauri::AppHandle) -> bool {
    println!("[DEBUG] Entered contains_forget");
    let re = Regex::new(r"(?i)\b(forget|erase memories|erase memory)\b")
        .expect("Failed to compile forget regex");

    let result = re.is_match(prompt);
    println!("[DEBUG] Finished contains_forget: {}", result);
    if result {
        // move all conversation history files to the history folder
        println!("[DEBUG] Detected 'forget' in prompt, moving conversation history files");
        move_all_conversation_history_to_history_folder(app);
    }
    result
}

// moves all conversation history files to the history folder
fn move_all_conversation_history_to_history_folder(app: &tauri::AppHandle) {
    println!("[DEBUG] Entered move_all_conversation_history_to_history_folder");
    let app_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let source_folder = app_dir.join("context");
    let history_folder = app_dir.join("history");

    if !source_folder.as_os_str().is_empty() && !history_folder.as_os_str().is_empty() {
        println!("[DEBUG] Creating history folder: {:?}", history_folder);
        std::fs::create_dir_all(&history_folder).expect("Failed to create history folder");

        println!(
            "[DEBUG] Reading entries from source folder: {:?}",
            source_folder
        );
        let entries = std::fs::read_dir(&source_folder).expect("Failed to read source folder");

        for entry in entries {
            println!("[DEBUG] Got directory entry");
            let entry = entry.expect("Failed to read entry");
            let path = entry.path();
            println!("[DEBUG] Inspecting path: {:?}", path);

            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.eq_ignore_ascii_case("json"))
                .unwrap_or(false);
            println!(
                "[DEBUG] is_file = {}, extension == \"json\" (case‑insensitive) = {}",
                path.is_file(),
                is_json
            );

            if path.is_file() && is_json {
                let file_name = path.file_name().unwrap();
                let new_path = history_folder.join(file_name);
                println!(
                    "[DEBUG] Moving {:?} -> {:?}",
                    path.display(),
                    new_path.display()
                );
                std::fs::rename(&path, &new_path)
                    .expect("Failed to move conversation history file");
                println!("[DEBUG] Moved {} to {}", path.display(), new_path.display());
            }
        }
    } else {
        println!("[DEBUG] Source or destination folder path is empty, skipping move");
    }

    println!("[DEBUG] Finished move_all_conversation_history_to_history_folder");
}

fn send_media_key(key_code: u8) {
    unsafe {
        // key down
        keybd_event(key_code, 0, 0, 0);
        // brief pause
        sleep(Duration::from_millis(50));
        // key up
        keybd_event(key_code, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn skip_track(prompt: &str) -> bool {
    println!("[DEBUG] Entered skip_track");
    let re = Regex::new(r"(?i)\b(skip track|next music)\b")
        .expect("Failed to compile skip tracking regex");
    let result = re.is_match(prompt);
    if result {
        send_media_key(VK_MEDIA_NEXT_TRACK as u8);
        println!("Next track command sent.");
    }
    println!("[DEBUG] Finished skip_track: {}", result);
    result
}

pub fn pause_music(prompt: &str) -> bool {
    println!("[DEBUG] Entered pause_music");
    let re = Regex::new(r"(?i)\b(pause music|pause)\b").expect("Failed to compile pause regex");
    let result = re.is_match(prompt);
    if result {
        send_media_key(VK_MEDIA_PLAY_PAUSE as u8);
        println!("Pause command sent.");
    }
    println!("[DEBUG] Finished pause_music: {}", result);
    result
}

pub fn play_music(prompt: &str) -> bool {
    println!("[DEBUG] Entered play_music");
    let re = Regex::new(r"(?i)\b(play music|play)\b").expect("Failed to compile play regex");
    let result = re.is_match(prompt);
    if result {
        send_media_key(VK_MEDIA_PLAY_PAUSE as u8);
        println!("Play/Pause command sent.");
    }
    println!("[DEBUG] Finished play_music: {}", result);
    result
}

pub fn previous_track(prompt: &str) -> bool {
    println!("[DEBUG] Entered previous_track");
    let re = Regex::new(r"(?i)\b(previous track|last music|previous music|last track)\b")
        .expect("Failed to compile previous track regex");
    let result = re.is_match(prompt);
    if result {
        send_media_key(VK_MEDIA_PREV_TRACK as u8);
        println!("Previous track command sent.");
    }
    println!("[DEBUG] Finished previous_track: {}", result);
    result
}

fn estimate_tokens_only(text: &str) -> usize {
    let chars = text.chars().count();
    (chars + 3) / 4
}

pub async fn contains_weather(
    prompt: &str,
    config: &Config,
    elevenlabs_model: Model,
    app: &tauri::AppHandle,
    wake_start_ms: i64,
) -> bool {
    println!("[DEBUG] Entered contains_weather (async)");

    let re = Regex::new(r"(?i)\b(weather|what is the weather)\b")
        .expect("Failed to compile weather regex");
    let matched = re.is_match(prompt);

    println!("[DEBUG] Finished regex match: {}", matched);

    if matched {
        println!("[DEBUG] Detected weather trigger, fetching report");
        
        // Emit message to chat that we're fetching weather
        let message = serde_json::json!({
            "role": "assistant",
            "content": "🌤️ Fetching current weather information...",
            "createdAt": chrono::Utc::now().timestamp_millis()
        });
        let _ = app.emit("new-message", message);
        
        let weather_report = get_weather(app).await; // async
        
        // Emit the weather report to chat with meta (tokens)
        let tts_tokens_est = estimate_tokens_only(&weather_report);
        let assistant_created_at = chrono::Utc::now().timestamp_millis();
        let message = serde_json::json!({
            "role": "assistant",
            "content": weather_report.clone(),
            "createdAt": assistant_created_at,
            "meta": { "ttsTokensEst": tts_tokens_est }
        });
        let _ = app.emit("new-message", message);
        
        println!("[DEBUG] Speaking weather report");
        tts::speak(
            &weather_report,
            &config.voice_id,
            elevenlabs_model,
            &config.elevenlabs_key,
        )
        .await
        .expect("Failed to speak weather report");
        println!("[DEBUG] Finished speaking weather report");
        // Emit meta update for latency
        let end_ms = chrono::Utc::now().timestamp_millis();
        let total_ms = (end_ms - wake_start_ms).max(0) as u64;
        let _ = app.emit(
            "message-meta",
            serde_json::json!({
                "createdAtOfAssistant": assistant_created_at,
                "meta": { "latencyMs": total_ms }
            })
        );
    }

    matched
}

pub async fn get_weather(app: &tauri::AppHandle) -> String {
    println!("[DEBUG] Entered get_weather()");
    let client = Client::new();
    let url = "https://wttr.in/?format=j1";
    println!("[DEBUG] Making HTTP request to: {}", url);

    match client.get(url).send().await {
        Ok(resp) => {
            println!("[DEBUG] HTTP request successful, status: {}", resp.status());
            match resp.json::<Value>().await {
                Ok(data) => {
                    println!("[DEBUG] Successfully parsed JSON response");
                    
                    let temp_c = data
                        .get("current_condition")
                        .and_then(|conds| conds.get(0))
                        .and_then(|cond| cond.get("temp_C"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");

                    let weather_desc = data
                        .get("current_condition")
                        .and_then(|conds| conds.get(0))
                        .and_then(|cond| cond.get("weatherDesc"))
                        .and_then(|descs| descs.get(0))
                        .and_then(|d| d.get("value"))
                        .and_then(Value::as_str)
                        .unwrap_or("unavailable");

                    let weather_report = format!(
                        "The current weather is {} with a temperature of {}°C.",
                        weather_desc, temp_c
                    );
                    
                    println!("[DEBUG] Extracted weather data - temp: {}°C, description: {}", temp_c, weather_desc);
                    println!("[DEBUG] Generated weather report: {}", weather_report);
                    weather_report
                }
                Err(e) => {
                    eprintln!("[ERROR] Failed to parse weather JSON response: {:?}", e);
                    let error_msg = "Sorry, I couldn't parse the weather data.";
                    
                    // Emit error to chat
                    let message = serde_json::json!({
                        "role": "assistant",
                        "content": format!("❌ {}", error_msg),
                        "createdAt": chrono::Utc::now().timestamp_millis()
                    });
                    let _ = app.emit("new-message", message);
                    
                    error_msg.into()
                }
            }
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to fetch weather data: {:?}", e);
            let error_msg = "Sorry, I couldn't get the weather right now.";
            
            // Emit error to chat
            let message = serde_json::json!({
                "role": "assistant",
                "content": format!("❌ {}", error_msg),
                "createdAt": chrono::Utc::now().timestamp_millis()
            });
            let _ = app.emit("new-message", message);
            
            error_msg.into()
        }
    }
}

// here are the checks that return true and exit early
pub async fn if_contains_exit(
    prompt: &str,
    config: &Config,
    elevenlabs_model: Model,
    wake_start_ms: i64,
    app: tauri::AppHandle,
) -> bool {
    println!("[DEBUG] Entered do_all_transformations");
    if contains_forget(prompt, config, &app) {
        println!("[DEBUG] Detected forget command, exiting early");
        return true;
    }

    if skip_track(prompt) {
        println!("[DEBUG] Detected skip track command, exiting early");
        return true;
    }
    if pause_music(prompt) {
        println!("[DEBUG] Detected pause music command, exiting early");
        return true;
    }
    if play_music(prompt) {
        println!("[DEBUG] Detected play music command, exiting early");
        return true;
    }
    if previous_track(prompt) {
        println!("[DEBUG] Detected previous track command, exiting early");
        return true;
    }

    if contains_weather(prompt, config, elevenlabs_model, &app, wake_start_ms).await {
        println!("[DEBUG] Detected weather command, exiting early");
        return true;
    }

    println!("[DEBUG] Finished do_all_transformations");
    false
}

// here are the checks that return text for LLM
pub fn if_contains_transform(prompt: &str, _elevenlabs_model: Model) -> String {
    println!("[DEBUG] Entered if_contains_transform");
    let transformed_prompt = paste_clipboard_instead_of_text(&prompt);

    println!(
        "[DEBUG] Finished if_contains_transform: {}",
        transformed_prompt
    );
    transformed_prompt
}

// here are the checks that return text after it has been processed by LLM
pub fn if_contains_transform_post_llm(prompt: &str) -> String {
    println!("[DEBUG] Entered if_contains_transform_post_llm");
    match copy_to_clipboard_function_for_llm(prompt) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("[ERROR] Failed to copy to clipboard: {:?}", e);
            prompt.to_string() // fallback: return original input
        }
    }
}
