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

export type ConversationSummary = {
  id: string;
  filename: string;
};

export type Message = {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  createdAt: number;
  meta?: {
    ttsTokensEst?: number;
    ttsChars?: number;
    latencyMs?: number;
  };
};

export type Config = {
  porcupine_key: string;
  gemini_key: string;
  elevenlabs_key: string;

  whisper_language: string;
  default_microphone_index: number;
  default_microphone_name?: string | null;
  default_output_device_name?: string | null;


  gemini_model: string;
  elevenlabs_model: string;
  voice_id: string;

  llm_system_prompt: string;
  vad_mode: string;
  wwd_sensitivity: number;
  context_window_expiration_seconds: number;

  frame_duration_ms: number;
  silence_threshold_seconds: number;
  speech_trigger_frames: number;
  frame_length_wwd: number;

  dock_position?: 'left' | 'right'; // optional, safe to ignore if unused
  input_mode?: 'audio' | 'text';
  theme?: 'emerald' | 'violet' | 'sky' | 'rose' | 'amber';
};

export type VoiceState =
  | 'idle'
  | 'loading'
  | 'wake_listening'
  | 'recording'
  | 'processing'
  | 'speaking';