use crate::audio_input::{SAMPLE_RATE, next_audio_frame};
use crate::models::AppContext;
use crate::utils::convert_i16_to_f32;
use anyhow::{Result, anyhow};
use std::collections::VecDeque;
use std::io::Write;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext};

// waits for the wake word to be detected by Porcupine
pub fn wait_for_wakeword(app: &AppContext) -> Result<()> {
    println!("[DEBUG] Entered wait_for_wakeword");
    let frame_length_wwd = app.config.frame_length_wwd;
    loop {
        let frame = next_audio_frame(app.audio_buffer.clone(), frame_length_wwd)?;
        match app.porcupine.process(&frame) {
            Ok(keyword_index) if keyword_index >= 0 => break,
            Ok(_) => continue,
            Err(e) => {
                return Err(anyhow!("Porcupine process error: {:?}", e));
            }
        }
    }
    println!("[DEBUG] Wakeword detected");
    Ok(())
}

// records a segment of audio until the user stops speaking
pub fn record_command(app: &AppContext) -> Result<Vec<i16>> {
    println!("[DEBUG] Entered record_command");
    let frame_length_vad = (SAMPLE_RATE / 1000) * app.config.frame_duration_ms;
    let speech_trigger_frames = app.config.speech_trigger_frames;
    let silence_threshold_frames =
        app.config.silence_threshold_seconds * (1000 / app.config.frame_duration_ms);

    let mut is_speaking = false;
    let mut silent_frames = 0;
    let mut speech_frames = 0;
    let mut speech_segment = Vec::new();
    let mut recent_frames: VecDeque<Vec<i16>> = VecDeque::with_capacity(speech_trigger_frames);

    loop {
        let frame = next_audio_frame(app.audio_buffer.clone(), frame_length_vad)?;
        let mut vad = match app.vad.lock() {
            Ok(v) => v,
            Err(e) => {
                return Err(anyhow!("Failed to lock VAD mutex: {e}"));
            }
        };
        let is_speech = match vad.is_voice_segment(&frame) {
            Ok(val) => val,
            Err(e) => {
                return Err(anyhow!("VAD processing failed: {:?}", e));
            }
        };
        drop(vad);

        if is_speaking {
            speech_segment.extend_from_slice(&frame);

            if is_speech {
                silent_frames = 0;
                print!(".");
                let _ = std::io::stdout().flush();
            } else {
                silent_frames += 1;
                print!("_");
                let _ = std::io::stdout().flush();

                if silent_frames >= silence_threshold_frames {
                    println!("\nDetected end of speech.");
                    println!("[DEBUG] End of speech detected, returning segment");
                    return Ok(speech_segment);
                }
            }
        } else if is_speech {
            speech_frames += 1;
            recent_frames.push_back(frame.clone());
            if recent_frames.len() > speech_trigger_frames {
                recent_frames.pop_front();
            }

            if speech_frames >= speech_trigger_frames {
                print!("Speech started: .");
                let _ = std::io::stdout().flush();
                is_speaking = true;
                speech_frames = 0;

                for f in recent_frames.iter() {
                    speech_segment.extend_from_slice(f);
                }

                recent_frames.clear();
                println!("[DEBUG] Speech started, collecting frames");
            }
        } else {
            speech_frames = 0;
            recent_frames.clear();
        }
    }
}

// transcribes the audio segment using Whisper
pub fn transcribe(
    ctx: &WhisperContext,
    audio_data_i16: &[i16],
    whisper_language: &str,
) -> Result<String> {
    println!("[DEBUG] Entered transcribe");
    let audio_data_f32 = convert_i16_to_f32(audio_data_i16);

    let mut state = ctx
        .create_state()
        .map_err(|e| anyhow!("Failed to create Whisper state: {}", e))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(whisper_language));
    params.set_initial_prompt("clipboard");

    state
        .full(params, &audio_data_f32[..])
        .map_err(|e| anyhow!("Failed to run Whisper model: {}", e))?;

    let num_segments = state
        .full_n_segments()
        .map_err(|e| anyhow!("Failed to get number of segments: {}", e))?;

    let mut full_transcript = String::new();

    println!("[DEBUG] Beginning transcription output");
    println!("\n--- TRANSCRIPTION ---");
    for i in 0..num_segments {
        if let (Ok(segment), Ok(start), Ok(end)) = (
            state.full_get_segment_text(i),
            state.full_get_segment_t0(i),
            state.full_get_segment_t1(i),
        ) {
            let text = segment.trim();
            println!("[{}ms -> {}ms]: {}", start, end, text);
            full_transcript.push_str(text);
            full_transcript.push(' ');
        }
    }
    println!("---------------------\n");

    println!("[DEBUG] Finished transcription");
    Ok(full_transcript)
}
