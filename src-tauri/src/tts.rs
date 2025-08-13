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
use elevenlabs_rs::Model;
use reqwest::Client;
use rodio::{Decoder, OutputStreamBuilder, Sink};
use serde_json::json;
use std::io::Cursor;
use tokio::task;

pub async fn speak(text: &str, voice_id: &str, model: Model, api_key: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }

    // 1) Send the streaming request
    let url = format!(
        "https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream?output_format=mp3_44100_128",
        voice_id = voice_id
    );
    let client = Client::new();
    let resp = client
        .post(&url)
        .header("xi-api-key", api_key)
        .json(&json!({
            "text": text,
            "model_id": String::from(model),
        }))
        .send()
        .await
        .map_err(|e| anyhow!("HTTP request error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("ElevenLabs API returned {}: {}", status, body));
    }

    // 2) Buffer the full audio payload into a Vec<u8>
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow!("Error reading TTS body: {}", e))?;
    let audio_bytes = bytes.to_vec();

    // 3) Spawn a blocking task for playback
    task::spawn_blocking(move || -> Result<()> {
        // Everything here is on a blocking thread: OutputStream is OK
        let cursor = Cursor::new(audio_bytes);
        let stream = OutputStreamBuilder::from_default_device()?
            .open_stream()
            .map_err(|e| anyhow!("Audio init error: {}", e))?;
        let sink = Sink::connect_new(&stream.mixer());
        let decoder = Decoder::new(cursor).map_err(|e| anyhow!("Decode error: {}", e))?;
        sink.append(decoder);
        sink.sleep_until_end();
        Ok(())
    })
    .await
    .map_err(|e| anyhow!("Playback thread panic: {}", e))??;

    Ok(())
}
