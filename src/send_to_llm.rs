use crate::models::Config;
use crate::transform_text::extract_image_parts;
use anyhow::Result;
use google_ai_rs::{Auth, Client, Part};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

// sends a prompt to the gemini api and returns the response
pub async fn query_gemini(prompt: &str, config: &Config) -> Result<String> {
    println!("[DEBUG] Entered query_gemini with prompt: {}", prompt);
    let context_window = read_context_window(config)
        .map_err(|e| anyhow::anyhow!("Could not read context window: {e}"))?;

    let ctx_text = context_window
        .iter()
        .filter_map(|entry| {
            let role = entry.get("role")?.as_str()?;
            let content = entry.get("content")?.as_str()?;
            Some(format!("{}: {}\n", role.to_uppercase(), content))
        })
        .collect::<String>();

    println!("[DEBUG] Built system prompt for Gemini");

    let system_prompt = format!("{}{}", config.llm_system_prompt, ctx_text);
    let client = Client::new(Auth::ApiKey(config.gemini_key.to_string()))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize Gemini client: {e}"))?;
    let model = client.generative_model(&config.gemini_model);

    println!("[DEBUG] Gemini client and model initialized");

    let response = match extract_image_parts(prompt) {
        Ok((plain,)) => model.generate_content((system_prompt.clone(), plain)).await,
        Err((pre, mime, bytes, post)) => {
            model
                .generate_content((
                    system_prompt,
                    pre,
                    Part::blob(&format!("image/{}", mime), bytes),
                    post,
                ))
                .await
        }
    };

    println!("[DEBUG] Gemini API request sent");

    let response_text = match response {
        Ok(r) => r.text(),
        Err(e) => {
            return Err(anyhow::anyhow!("Gemini API error: {e}"));
        }
    };

    println!("[DEBUG] Gemini API response received");

    write_context_window(config, prompt, &response_text)
        .map_err(|e| anyhow::anyhow!("Failed to write context window: {e}"))?;

    println!("[DEBUG] Context window updated");

    println!(
        "\n--- LLM RESPONSE ---\n{}\n-----------------------\n",
        response_text
    );
    println!("[DEBUG] Returning Gemini response");
    Ok(response_text)
}

// returns the current epoch time in seconds based on system time
fn current_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

// reads the context window from the json file
fn read_context_window(config: &Config) -> Result<Vec<Value>> {
    let ctx_dir = Path::new(&config.context_window_path);
    let history_dir = Path::new(&config.history_folder_path);
    fs::create_dir_all(ctx_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create context window directory: {e}"))?;
    fs::create_dir_all(history_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create history directory: {e}"))?;

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(ctx_dir)
        .map_err(|e| anyhow::anyhow!("Failed to read context window directory: {e}"))?
    {
        if let Ok(e) = entry {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }

    if files.is_empty() {
        return Ok(vec![]);
    }

    files.sort_by_key(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    });

    for old in files.iter().skip(1) {
        let file_name = old.file_name().unwrap();
        fs::rename(old, history_dir.join(file_name)).map_err(|e| {
            anyhow::anyhow!(
                "Failed to move old context file {}: {e}",
                file_name.to_string_lossy()
            )
        })?;
    }

    let newest = &files[0];
    let file_epoch = newest
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let now = current_epoch_secs();

    if now.saturating_sub(file_epoch) > config.context_window_expiration_seconds {
        let file_name = newest.file_name().unwrap();
        fs::rename(newest, history_dir.join(file_name))?;
        return Ok(vec![]);
    }

    let mut f = File::open(newest)
        .map_err(|e| anyhow::anyhow!("Failed to open newest context window file: {e}"))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)
        .map_err(|e| anyhow::anyhow!("Failed to read context window file: {e}"))?;
    let v: Value = serde_json::from_str(&buf)
        .map_err(|e| anyhow::anyhow!("Failed to parse context window JSON: {e}"))?;

    let messages = v
        .get("context_window")
        .and_then(|cw| cw.as_array())
        .cloned()
        .unwrap_or_else(Vec::new);

    Ok(messages)
}

// writes the question and answer to the context window file
fn write_context_window(config: &Config, question: &str, answer: &str) -> Result<()> {
    let ctx_dir = Path::new(&config.context_window_path);
    let history_dir = Path::new(&config.history_folder_path);
    fs::create_dir_all(ctx_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create context window directory: {e}"))?;
    fs::create_dir_all(history_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create history directory: {e}"))?;

    let now = current_epoch_secs();
    let new_filename = format!("{}.json", now);
    let new_path = ctx_dir.join(&new_filename);

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(ctx_dir)
        .map_err(|e| anyhow::anyhow!("Failed to read context window directory: {e}"))?
    {
        if let Ok(e) = entry {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }

    files.sort_by_key(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    });

    let mut messages: Vec<Value> = vec![];

    if !files.is_empty() {
        let newest = files.pop().unwrap();
        let newest_epoch = newest
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let expired = now.saturating_sub(newest_epoch) > config.context_window_expiration_seconds;

        if expired {
            for f in [vec![newest], files].concat() {
                let name = f.file_name().unwrap();
                fs::rename(&f, history_dir.join(name)).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to move expired context file {}: {e}",
                        name.to_string_lossy()
                    )
                })?;
            }
        } else {
            for old in files {
                let name = old.file_name().unwrap();
                fs::rename(&old, history_dir.join(name)).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to move old context file {}: {e}",
                        name.to_string_lossy()
                    )
                })?;
            }

            fs::rename(&newest, &new_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to move newest context file to {}: {e}",
                    new_path.display()
                )
            })?;

            let mut f = File::open(&new_path)
                .map_err(|e| anyhow::anyhow!("Failed to open new context window file: {e}"))?;
            let mut buf = String::new();
            f.read_to_string(&mut buf)
                .map_err(|e| anyhow::anyhow!("Failed to read new context window file: {e}"))?;
            let v: Value = serde_json::from_str(&buf)
                .map_err(|e| anyhow::anyhow!("Failed to parse new context window JSON: {e}"))?;

            messages = v
                .get("context_window")
                .and_then(|cw| cw.as_array())
                .cloned()
                .unwrap_or_default();
        }
    }

    messages.push(json!({ "role": "user", "content": question }));
    messages.push(json!({ "role": "assistant", "content": answer }));

    let payload = json!({ "context_window": messages });
    let mut f = File::create(&new_path)
        .map_err(|e| anyhow::anyhow!("Failed to create new context window file: {e}"))?;
    f.write_all(serde_json::to_string_pretty(&payload)?.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to write context window JSON: {e}"))?;

    Ok(())
}
