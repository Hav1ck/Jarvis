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
use elevenlabs_rs::endpoints::genai::tts::{TextToSpeech, TextToSpeechBody};
use elevenlabs_rs::{ElevenLabsClient, Model};
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;

// uses elvenlabs tts api to create and then play the audio
pub async fn speak(text: &str, voice_id: &str, model: Model, api_key: &str) -> Result<()> {
    println!(
        "[DEBUG] Entered speak with text: {}...",
        &text.chars().take(20).collect::<String>()
    );
    if text.trim().is_empty() {
        return Ok(());
    }

    let client = ElevenLabsClient::new(api_key);
    let body = TextToSpeechBody::new(text).with_model_id(model);
    let endpoint = TextToSpeech::new(voice_id, body);

    println!("[DEBUG] Sending TTS request to ElevenLabs");

    let speech_bytes = client
        .hit(endpoint)
        .await
        .map_err(|e| anyhow!("ElevenLabs API error: {}", e))?;

    println!("[DEBUG] Received TTS audio bytes");

    let (_stream, stream_handle) =
        OutputStream::try_default().map_err(|e| anyhow!("Audio output stream error: {}", e))?;

    let sink = Sink::try_new(&stream_handle).map_err(|e| anyhow!("Audio sink error: {}", e))?;

    let cursor = Cursor::new(speech_bytes);
    let source = Decoder::new(cursor).map_err(|e| anyhow!("Audio decode error: {}", e))?;

    sink.append(source);
    sink.sleep_until_end();

    println!("[DEBUG] Finished playing TTS audio");

    Ok(())
}
