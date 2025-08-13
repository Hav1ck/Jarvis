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

use crate::audio_input::{next_audio_frame, SAMPLE_RATE};
use crate::models::AppContext;
use crate::utils::convert_i16_to_f32;
use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext};

// waits for the wake word to be detected by Porcupine
pub fn wait_for_wakeword(app: &AppContext, is_running: &Arc<AtomicBool>) -> Result<()> {
    println!("[DEBUG] Entered wait_for_wakeword");
    let frame_length_wwd = app.config.frame_length_wwd;
    let mut frame_count = 0;

    loop {
        // Check if we should stop every 100 frames (about 3 seconds at 30ms frame duration)
        if frame_count % 100 == 0 {
            if !is_running.load(Ordering::Relaxed) {
                println!("[DEBUG] Wake word detection stopped by user");
                return Err(anyhow!("Wake word detection stopped"));
            }
        }

        let frame = next_audio_frame(app.audio_buffer.clone(), frame_length_wwd)?;
        match app.porcupine.process(&frame) {
            Ok(keyword_index) if keyword_index >= 0 => break,
            Ok(_) => {
                frame_count += 1;
                continue;
            }
            Err(e) => {
                return Err(anyhow!("Porcupine process error: {:?}", e));
            }
        }
    }
    println!("[DEBUG] Wakeword detected");
    Ok(())
}

// records a segment of audio until the user stops speaking
pub fn record_command(app: &AppContext, is_running: &Arc<AtomicBool>) -> Result<Vec<i16>> {
    println!("[DEBUG] Entered record_command");
    let frame_length_vad = (SAMPLE_RATE / 1000) * app.config.frame_duration_ms;
    let speech_trigger_frames = app.config.speech_trigger_frames;
    // Use ceil for threshold frames and enforce a sensible minimum (e.g., 5 frames)
    let silence_threshold_frames = {
        let frames = ((app.config.silence_threshold_seconds as f32)
            * (1000.0 / app.config.frame_duration_ms as f32))
            .ceil() as i32;
        frames.max(5)
    };

    let mut is_speaking = false;
    let mut silent_frames = 0;
    let mut speech_frames = 0;
    let mut speech_segment = Vec::new();
    let mut recent_frames: VecDeque<Vec<i16>> =
        VecDeque::with_capacity(speech_trigger_frames as usize);

    let mut frame_count = 0;
    loop {
        // Check if we should stop every 100 frames
        if frame_count % 100 == 0 {
            if !is_running.load(Ordering::Relaxed) {
                println!("[DEBUG] Recording stopped by user");
                return Err(anyhow!("Recording stopped"));
            }
        }

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
            if recent_frames.len() > speech_trigger_frames as usize {
                recent_frames.pop_front();
            }

            if speech_frames >= speech_trigger_frames {
                print!("Speech started: .");
                let _ = std::io::stdout().flush();
                is_speaking = true;
                speech_frames = 0;
                silent_frames = 0; // reset silence count on speech start

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
        frame_count += 1;
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
