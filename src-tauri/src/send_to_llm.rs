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
use crate::transform_text::extract_image_parts;
use anyhow::{anyhow, Result};
use google_ai_rs::{Auth, Client, Part};
use regex::Regex;
// no serde_json imports needed in this module now
use std::io::{stdout, Write};
use url::Url;

fn is_image_content_type(ct: &str) -> bool {
    let ct = ct.to_lowercase();
    ct.starts_with("image/")
}

fn is_image_url_by_ext(url: &str) -> bool {
    let url_lc = url.to_lowercase();
    url_lc.ends_with(".png")
        || url_lc.ends_with(".jpg")
        || url_lc.ends_with(".jpeg")
        || url_lc.ends_with(".gif")
        || url_lc.ends_with(".webp")
}

fn strip_html(input: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    let no_tags = re.replace_all(input, " ");
    let collapsed = Regex::new(r"\s+").unwrap().replace_all(&no_tags, " ");
    collapsed.trim().to_string()
}

async fn resolve_image_url(raw: &str) -> Option<String> {
    if let Ok(parsed) = Url::parse(raw) {
        if parsed
            .domain()
            .map(|d| d.contains("google") || d.contains("bing"))
            .unwrap_or(false)
        {
            // try common redirect param names (e.g., imgurl on Google, mediaurl on Bing)
            for key in ["imgurl", "mediaurl", "imgrefurl", "url"] {
                if let Some(val) = parsed
                    .query_pairs()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.to_string())
                {
                    if Url::parse(&val).is_ok() {
                        return Some(val);
                    }
                }
            }
        }
    }
    Some(raw.to_string())
}

async fn build_parts_with_media(system_prompt: &str, prompt: &str) -> Result<Vec<Part>> {
    let mut parts: Vec<Part> = Vec::new();
    parts.push(Part::text(system_prompt));

    // 1) Embedded data URI support
    match extract_image_parts(prompt) {
        Ok((plain,)) => {
            parts.push(Part::text(&plain));
        }
        Err((pre, mime, bytes, post)) => {
            if !pre.trim().is_empty() {
                parts.push(Part::text(&pre));
            }
            parts.push(Part::blob(&format!("image/{}", mime), bytes));
            if !post.trim().is_empty() {
                parts.push(Part::text(&post));
            }
        }
    }

    // 2) Remote URLs: try to attach images or page text
    let url_re = Regex::new(r"https?://[^\s)]+").unwrap();
    let client = reqwest::Client::new();

    for m in url_re.find_iter(prompt) {
        let raw_url = m.as_str();
        // Resolve redirector URLs (Google Images, Bing, etc.)
        let target_url = resolve_image_url(raw_url)
            .await
            .unwrap_or_else(|| raw_url.to_string());

        // Fetch HEAD/GET to decide type
        let resp = match client.get(&target_url).send().await {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !resp.status().is_success() {
            continue;
        }

        // Prefer content-type header to detect images
        let ct_header = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if let Some(ct) = ct_header {
            if is_image_content_type(&ct) {
                if let Ok(bytes) = resp.bytes().await {
                    parts.push(Part::blob(&ct, bytes.to_vec()));
                }
                continue;
            }
        }

        // If no CT header or not image, but URL looks like image by extension, try as image
        if is_image_url_by_ext(&target_url) {
            if let Ok(bytes) = resp.bytes().await {
                // Guess type from extension
                let guessed = if target_url.ends_with(".png") {
                    "image/png"
                } else if target_url.ends_with(".jpg") || target_url.ends_with(".jpeg") {
                    "image/jpeg"
                } else if target_url.ends_with(".gif") {
                    "image/gif"
                } else if target_url.ends_with(".webp") {
                    "image/webp"
                } else {
                    "application/octet-stream"
                };
                parts.push(Part::blob(guessed, bytes.to_vec()));
            }
            continue;
        }

        // Treat as web page text
        if let Ok(text) = resp.text().await {
            let stripped = strip_html(&text);
            let snippet = if stripped.len() > 10_000 {
                format!("{}…", &stripped[..10_000])
            } else {
                stripped
            };
            parts.push(Part::text(&format!(
                "Web content from {}:\n{}",
                target_url, snippet
            )));
        }
    }

    Ok(parts)
}

// sends a prompt to the Gemini API and returns the response. ctx_text is the
// conversation context derived from the selected chat history.
pub async fn query_gemini(prompt: &str, config: &Config, ctx_text: &str) -> Result<String> {
    println!("[DEBUG] Entered query_gemini with prompt: {}", prompt);
    let system_prompt = format!("{}{}", config.llm_system_prompt, ctx_text);
    let client = Client::new(Auth::ApiKey(config.gemini_key.to_string()))
        .await
        .map_err(|e| anyhow!("Failed to initialize Gemini client: {e}"))?;
    let model = client.generative_model(&config.gemini_model);

    println!("[DEBUG] Gemini client and model initialized");
    println!("[DEBUG] Starting streaming response");

    let parts = build_parts_with_media(&system_prompt, prompt).await?;
    let mut stream = model.stream_generate_content(parts).await?;

    let mut full_response = String::new();
    loop {
        match stream.next().await {
            Ok(Some(chunk)) => {
                let text = chunk.text();
                if !text.is_empty() {
                    print!("{}", text);
                    stdout().flush()?;
                    full_response.push_str(&text);
                }
            }
            Ok(None) => break,
            Err(e) => {
                eprintln!("[ERROR] streaming chunk error: {:?}", e);
                break;
            }
        }
    }
    println!("\n[DEBUG] Streaming complete");
    println!(
        "\n--- LLM RESPONSE ---\n{}\n-----------------------\n",
        full_response
    );
    println!("[DEBUG] Returning Gemini response");
    Ok(full_response)
}

// Generate a short conversation title using Gemini Flash Lite model
pub async fn generate_conversation_title(seed_text: &str, config: &Config) -> Result<String> {
    let client = Client::new(Auth::ApiKey(config.gemini_key.to_string()))
        .await
        .map_err(|e| anyhow!("Failed to initialize Gemini client: {e}"))?;

    // Prefer a lightweight fast model for title generation
    let model = client.generative_model("gemini-2.0-flash-lite");

    let prompt = format!(
        "You are to generate a concise, descriptive chat title (3-6 words) based on the given conversation snippet.\n\
Do not include quotes or punctuation at the end.\n\
Return only the title text.\n\
Snippet:\n{}",
        seed_text
    );

    let parts = vec![Part::text(&prompt)];
    let mut stream = model.stream_generate_content(parts).await?;
    let mut full = String::new();
    while let Ok(Some(chunk)) = stream.next().await {
        let t = chunk.text();
        if !t.is_empty() {
            full.push_str(&t);
        }
    }
    Ok(full.trim().to_string())
}
