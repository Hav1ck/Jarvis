use anyhow::Result;
use elevenlabs_rs::Model;
use porcupine::Porcupine;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};
use webrtc_vad::Vad;
use whisper_rs::WhisperContext;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub porcupine_key: String,
    pub gemini_key: String,
    pub elevenlabs_key: String,
    pub whisper_language: String,
    pub wakeword_path: String,
    pub whisper_model_path: String,
    pub context_window_path: String,
    pub context_window_expiration_seconds: u64,
    pub default_microphone_index: usize,

    // advanced settings
    pub gemini_model: String,
    pub elevenlabs_model: String,
    pub voice_id: String,
    pub llm_system_prompt: String,
    pub vad_mode: String,
    pub wwd_sensitivity: f32,
    pub history_folder_path: String,

    pub frame_duration_ms: usize,
    pub silence_threshold_seconds: usize,
    pub speech_trigger_frames: usize,
    pub frame_length_wwd: usize,
}

pub struct AppContext {
    pub config: Config,
    pub audio_player: AudioPlayer,
    pub porcupine: Porcupine,
    pub vad: Mutex<Vad>,
    pub whisper_context: Arc<WhisperContext>,
    pub audio_buffer: Arc<Mutex<VecDeque<i16>>>,
    pub elevenlabs_model: Model,
}

pub struct AudioPlayer {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        println!("[DEBUG] Initializing AudioPlayer");
        let (_stream, stream_handle) = OutputStream::try_default()?;
        Ok(Self {
            _stream,
            stream_handle,
        })
    }
    pub fn play_sound<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        println!("[DEBUG] Playing sound: {}", path.as_ref().display());
        let sink = Sink::try_new(&self.stream_handle)?;
        let file = File::open(path)?;
        let source = Decoder::new(BufReader::new(file))?;
        sink.append(source);
        sink.detach();
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
}
