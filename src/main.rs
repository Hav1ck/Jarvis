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

mod audio_input;
mod get_text;
mod models;
mod send_to_llm;
mod transform_text;
mod tts;
mod utils;

use crate::audio_input::SAMPLE_RATE;
use anyhow::{Context, Result};
use elevenlabs_rs::Model;
use models::{AppContext, AudioPlayer};
use porcupine::PorcupineBuilder;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use utils::{download_file, load_config};
use webrtc_vad::{SampleRate, Vad, VadMode};
use whisper_rs::{WhisperContext, WhisperContextParameters};

const WHISPER_MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin?download=true";

#[tokio::main]
async fn main() {
    println!("[DEBUG] Entered main()");
    if let Err(e) = run_app().await {
        eprintln!(
            "\n\n\n[ERROR] {}\nIf this is your first time running, please check your config.json, model paths, and device setup.\nFor more help, see the README \n",
            e
        );
        print!("Press Enter to exit...");
        io::stdout().flush().unwrap();
        let _ = io::stdin().read_line(&mut String::new());

        std::process::exit(1);
    }
}

async fn run_app() -> Result<()> {
    println!("[DEBUG] Entered run_app()");
    let config = load_config("assets/config.json".as_ref()).with_context(
        || "Failed to load config.json: Please ensure the file exists and is valid JSON.",
    )?;
    println!("[DEBUG] Loaded config: {:?}", config);
    let wakeword_path = PathBuf::from(&config.wakeword_path);
    let whisper_model_path = PathBuf::from(&config.whisper_model_path);

    let audio_player = AudioPlayer::new().with_context(|| "Failed to initialize audio output")?;
    println!("[DEBUG] Initialized AudioPlayer");

    let porcupine = PorcupineBuilder::new_with_keyword_paths(
        &config.porcupine_key,
        &[wakeword_path.to_str().unwrap()],
    )
    .sensitivities(&[config.wwd_sensitivity])
    .model_path("build/porcupine_params.pv")
    .library_path("build/libpv_porcupine.dll")
    .init()
    .with_context(|| "Unable to create Porcupine wake word engine")?;
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
    download_file(WHISPER_MODEL_URL, &whisper_model_path).await?;
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

    audio_input::start_audio_stream(audio_buffer.clone(), config.default_microphone_index)
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
    main_loop(&app).await?;
    Ok(())
}

async fn main_loop(app: &AppContext) -> Result<()> {
    println!("[DEBUG] Entered main_loop()");
    loop {
        // listen for wake word
        println!("[DEBUG] Waiting for wake word...");
        get_text::wait_for_wakeword(app)?;
        println!("\nWake word detected!");
        if let Err(e) = app.audio_player.play_sound("assets/beep.mp3") {
            eprintln!("Failed to play beep sound: {e}");
        }

        // record the user's command
        println!("Listening for command... (Speak now)");
        println!("[DEBUG] Recording command...");
        let speech_segment = get_text::record_command(app)?;

        if speech_segment.is_empty() {
            println!("No speech detected after wake word. Please try again.");
            println!("[DEBUG] No speech detected after wake word");
        } else {
            println!(
                "Processing {} seconds of audio...",
                speech_segment.len() as f32 / SAMPLE_RATE as f32
            );
            println!("[DEBUG] Spawning async task to process command");

            let whisper_ctx = Arc::clone(&app.whisper_context);
            let config = app.config.clone();
            let elevenlabs_model = app.elevenlabs_model.clone();

            tokio::spawn(async move {
                println!("[DEBUG] Async task started for processing command");
                let result: Result<()> = (|| async {
                    // transcribe audio to text
                    println!("[DEBUG] Transcribing audio to text...");
                    let user_prompt = get_text::transcribe(
                        &whisper_ctx,
                        &speech_segment,
                        &config.whisper_language,
                    )?;

                    // transform text if needed
                    println!("[DEBUG] Optionally transforming prompt...");
                    let transformed_prompt =
                        transform_text::paste_clipboard_instead_of_text(&user_prompt)?;

                    // send prompt to LLM
                    println!("[DEBUG] Sending prompt to LLM...");
                    let llm_answer =
                        send_to_llm::query_gemini(&transformed_prompt, &config).await?;

                    // transform LLM response if needed
                    println!("[DEBUG] Optionally transforming LLM response...");
                    let llm_answer =
                        transform_text::copy_to_clipboard_function_for_llm(&llm_answer)?;

                    // play the LLM response using TTS
                    println!("[DEBUG] Speaking LLM response...");
                    tts::speak(
                        &llm_answer,
                        &config.voice_id,
                        elevenlabs_model,
                        &config.elevenlabs_key,
                    )
                    .await?;
                    println!("[DEBUG] Finished speaking response");
                    Ok(())
                })()
                .await;
                if let Err(e) = result {
                    eprintln!("[Error] Task failed: {e}");
                    println!("[DEBUG] Async task failed: {e}");
                }
            });
        }
        println!("\n----------------------------------------\n");
        println!("[DEBUG] End of main loop iteration");
    }
}
