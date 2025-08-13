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

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Config } from '../types';

export async function listHistoryFiles(): Promise<string[]> {
  return await invoke<string[]>('cmd_list_history_files');
}

export async function createConversation(): Promise<string> {
  return await invoke<string>('cmd_create_conversation');
}

export async function readConversation(filename: string): Promise<Array<{ role: string; content: string; createdAt: number }>> {
  return await invoke('cmd_read_conversation', { filename });
}

export async function appendTurn(filename: string, role: string, content: string, createdAt: number): Promise<void> {
  await invoke('cmd_append_turn', { filename, turn: { role, content, createdAt } });
}

export async function loadConfig(): Promise<Config> {
  return await invoke<Config>('cmd_load_config');
}

export async function saveConfig(cfg: Config): Promise<void> {
  await invoke('cmd_save_config', { config: cfg });
}

export async function getRoamingDir(): Promise<string> {
  return await invoke<string>('cmd_get_roaming_dir');
}

export async function resolveResourcePath(relative: string): Promise<string> {
  return await invoke<string>('cmd_resolve_resource_path', { relative });
}

export async function listInputDevices(): Promise<string[]> {
  return await invoke<string[]>('cmd_list_input_devices');
}

export async function listOutputDevices(): Promise<string[]> {
  return await invoke<string[]>('cmd_list_output_devices');
}

export async function startJarvis(): Promise<void> {
  await invoke("cmd_start_jarvis");
}

export async function stopJarvis(): Promise<void> {
  await invoke("cmd_stop_jarvis");
}

export async function getJarvisStatus(): Promise<boolean> {
  return await invoke<boolean>("cmd_get_jarvis_status");
}

export async function getJarvisState(): Promise<string> {
  return await invoke<string>("cmd_get_jarvis_state");
}

// Event listeners for state changes and messages
export function listenToStateChanges(callback: (state: string) => void) {
  return listen('jarvis-state-changed', (event) => {
    callback(event.payload as string);
  });
}

export function listenToNewMessages(callback: (message: any) => void) {
  return listen('new-message', (event) => {
    callback(event.payload);
  });
}

// Whisper model download progress events
export function listenToWhisperDownloadProgress(
  callback: (payload: { downloaded: number; total: number; percent: number }) => void
) {
  return listen('whisper-download-progress', (event) => {
    callback(event.payload as any);
  });
}

export function listenToWhisperDownloadComplete(callback: () => void) {
  return listen('whisper-download-complete', () => {
    callback();
  });
}

export async function sendTextPrompt(prompt: string): Promise<string> {
  return await invoke<string>('cmd_send_text', { prompt });
}

export async function setActiveConversation(filename: string): Promise<void> {
  await invoke('cmd_set_active_conversation', { filename });
}

export async function generateAndRenameConversation(filename: string): Promise<{ new_filename: string; title: string }> {
  return await invoke('cmd_generate_and_rename_conversation', { filename });
}

export async function deleteConversation(filename: string): Promise<void> {
  await invoke('cmd_delete_conversation', { filename });
}

export async function renameConversation(filename: string, newTitle: string): Promise<{ new_filename: string; title: string }> {
  return await invoke('cmd_rename_conversation', { filename, newTitle });
}

// Open external URLs in the user's default browser.
// Uses a dynamic import so the opener plugin is only loaded when needed,
// avoiding slowing down initial app load.
export async function openExternalUrl(url: string): Promise<void> {
  const mod = await import('@tauri-apps/plugin-opener');
  if (typeof mod.openUrl === 'function') {
    await mod.openUrl(url);
  }
}