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
