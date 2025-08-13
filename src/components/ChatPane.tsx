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

import React, { useCallback, useState } from "react";
import { Message, VoiceState, Config } from "../types";
import { startJarvis, stopJarvis } from "../lib/tauri";
import { sendTextPrompt } from "../lib/tauri";

type ChatPaneProps = {
  messages: Message[];
  voiceState: VoiceState;
  onPrimaryToggle: (next: VoiceState) => void; // idle <-> wake_listening
  onStopSpeaking: () => void;
  config?: Config | null;
  onToggleInputMode?: (mode: "audio" | "text") => void;
  whisperProgress?: number | null;
  onSystemMessage?: (text: string) => void;
};

const ChatBubble: React.FC<{ msg: Message }> = ({ msg }) => {
  const isUser = msg.role === "user";
  const isAssistant = msg.role === "assistant";
  return (
    <div className={`w-full flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[75%] rounded-2xl px-4 py-3 border leading-relaxed ${
          isUser
            ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-100 bubble-user"
            : isAssistant
            ? "bg-[#151a24] border-white/10 text-zinc-200 bubble-assistant"
            : "bg-[#0f1115] border-white/10 text-zinc-400 italic"
        }`}
      >
        <div className="text-[15px] whitespace-pre-wrap">{msg.content}</div>
        {isAssistant && msg.meta && (
          <div className="mt-2 text-[11px] text-zinc-500">
            {(() => {
              const parts: string[] = [];
              if (typeof msg.meta!.ttsTokensEst === 'number') {
                parts.push(`~${msg.meta!.ttsTokensEst} tokens`);
              }
              if (typeof msg.meta!.latencyMs === 'number') {
                const ms = msg.meta!.latencyMs;
                const sec = (ms / 1000).toFixed(2);
                parts.push(`Total time: ${sec}s`);
              }
              return parts.length > 0 ? <span>{parts.join(' · ')}</span> : null;
            })()}
          </div>
        )}
      </div>
    </div>
  );
};

const StatusPill: React.FC<{ state: VoiceState; whisperProgress?: number | null }>
  = ({ state, whisperProgress }) => {
  const downloading = typeof whisperProgress === 'number';
  const label = downloading
    ? `Downloading ${whisperProgress}%`
    : state === "idle"
    ? "Idle"
    : state === "loading"
    ? "Loading"
    : state === "wake_listening"
    ? "Listening for wake word (Jarvis)"
    : state === "recording"
    ? "Recording"
    : state === "processing"
    ? "Thinking"
    : state === "speaking"
    ? "Speaking"
    : "Unknown";

  const color =
    state === "idle"
      ? "bg-zinc-700/40 text-zinc-300"
      : state === "loading" || downloading
      ? "bg-amber-500/15 text-amber-300"
      : state === "speaking"
      ? "bg-blue-500/15 text-blue-300"
      : "bg-emerald-500/15 text-emerald-300";

  return (
    <div
      className={`inline-flex items-center gap-2 px-3 py-1 rounded-full border border-white/10 ${color}`}
    >
      <span className="relative flex h-2 w-2">
        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-current opacity-30"></span>
        <span className="relative inline-flex rounded-full h-2 w-2 bg-current"></span>
      </span>
      <span className="text-xs">{label}</span>
    </div>
  );
};

const Visualizer: React.FC<{ active: boolean }> = ({ active }) => {
  return (
    <div className="h-8 flex items-end gap-[3px]">
      {Array.from({ length: 56 }).map((_, i) => {
        const base = 6 + ((i * 37) % 26);
        return (
          <div
            key={i}
            className="w-[3px] rounded-full"
            style={{
              background:
                "linear-gradient(180deg, rgb(var(--theme-accent) / 0.9), rgb(var(--theme-accent) / 0.45))",
              height: active ? `${base}px` : "5px",
              opacity: active ? 1 : 0.35,
              transition: `height 160ms ease ${i * 8}ms, opacity 200ms ease`,
            }}
          />
        );
      })}
    </div>
  );
};

const MicButton: React.FC<{
  state: VoiceState;
  onToggle: (next: VoiceState) => void;
  onBeforeStart?: () => boolean;
}> = ({ state, onToggle, onBeforeStart }) => {
  const nextState = state === "wake_listening" ? "idle" : "wake_listening";
  const isActive = state === "wake_listening";
  const dotColor =
    state !== "idle" ? "bg-[rgb(var(--theme-accent))]" : "bg-transparent";

  const handleToggle = useCallback(async () => {
    if (nextState === "wake_listening") {
      if (onBeforeStart && !onBeforeStart()) {
        return;
      }
      try {
        await startJarvis();
        onToggle(nextState);
      } catch (err) {
        console.error("Failed to start Jarvis:", err);
        // Don't change state if failed
      }
    } else {
      try {
        await stopJarvis();
        onToggle(nextState);
      } catch (err) {
        console.error("Failed to stop Jarvis:", err);
        // Don't change state if failed
      }
    }
  }, [nextState, onToggle, onBeforeStart]);

  return (
    <button
      onClick={handleToggle}
      className={`mic-btn ${isActive ? "mic-active" : "mic-idle"}`}
      title={
        isActive ? "Stop wake-word listening" : "Start wake-word listening"
      }
    >
      <span className="mic-ring" />
      <svg
        width="22"
        height="22"
        viewBox="0 0 24 24"
        className="text-zinc-200 relative"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
      >
        <path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z" />
        <path d="M19 11a7 7 0 0 1-14 0" />
        <path d="M12 19v3" />
      </svg>
      {state !== "idle" && (
        <span
          className={`absolute -right-2 -top-2 h-3 w-3 rounded-full ${dotColor} animate-pulse`}
        />
      )}
    </button>
  );
};

function shouldShowHeader(prev: Message | undefined, curr: Message) {
  if (!prev) return true;
  const sameRole = prev.role === curr.role;
  const gap = curr.createdAt - prev.createdAt;
  return !sameRole || gap > 5 * 60 * 1000;
}

const ChatPane: React.FC<ChatPaneProps> = ({
  messages,
  voiceState,
  onPrimaryToggle,
  onStopSpeaking,
  config,
  onToggleInputMode,
  whisperProgress,
  onSystemMessage,
}) => {
  const visualizerActive =
    voiceState === "recording" || voiceState === "speaking";
  const inputMode = config?.input_mode ?? "audio";
  const [text, setText] = useState("");

  const handleSendText = useCallback(async () => {
    const prompt = text.trim();
    if (!prompt) return;
    if (!config?.gemini_key || config.gemini_key.trim().length === 0) {
      onSystemMessage?.(
        "Please enter your Gemini API key in Settings > API Keys."
      );
      return;
    }
    setText("");
    try {
      await sendTextPrompt(prompt);
    } catch (e) {
      console.error("Failed to send text prompt:", e);
      onSystemMessage?.(
        "There was a problem sending your message. Check your Gemini API key in Settings > API Keys."
      );
    }
  }, [text, config, onSystemMessage]);

  const preflightStart = useCallback(() => {
    if (!config?.porcupine_key || config.porcupine_key.trim().length === 0) {
      onSystemMessage?.(
        "Please enter your Picovoice Porcupine key in Settings > API Keys."
      );
      return false;
    }
    if (config?.porcupine_key && config.porcupine_key.trim().length === 1) {
      onSystemMessage?.(
        "Your Picovoice key appears invalid. Please paste the full access key from Picovoice Console."
      );
      return false;
    }
    return true;
  }, [config, onSystemMessage]);

  return (
    <main className="flex-1 h-full px-2 min-w-0">
      <div className="ui-card h-full flex flex-col">
        <div
          className="px-4 py-3 flex items-center justify-between"
          style={{ borderBottom: "1px solid var(--hairline)" }}
        >
          <div className="text-sm text-zinc-400">Chat</div>
          <StatusPill state={voiceState} whisperProgress={whisperProgress} />
        </div>

        {/* No thinking/download bar in chat pane; top bar shows activity */}

        <div className="flex-1 overflow-y-auto overflow-x-hidden p-4 space-y-3 relative">
          {voiceState === "idle" &&
            messages.length === 0 &&
            inputMode === "audio" && (
              <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
                <div className="ui-card p-6 text-center pointer-events-auto">
                  <div className="text-zinc-300 mb-3">Ready when you are</div>
                  <div className="flex items-center justify-center">
                    <MicButton
                      state={voiceState}
                      onToggle={onPrimaryToggle}
                      onBeforeStart={preflightStart}
                    />
                  </div>
                  <div className="text-xs text-zinc-500 mt-2">
                    Press to start wake-word listening
                  </div>
                </div>
              </div>
            )}

          {messages.map((m, idx) => {
            const prev = messages[idx - 1];
            const showHeader = shouldShowHeader(prev, m);
            return (
              <div key={m.id} className="space-y-1">
                {showHeader && (
                  <div
                    className={`text-[11px] text-zinc-500 px-1 ${
                      m.role === "user" ? "text-right" : ""
                    }`}
                  >
                    {m.role === "user"
                      ? "You"
                      : m.role === "assistant"
                      ? "Assistant"
                      : "System"}{" "}
                    · {new Date(m.createdAt).toLocaleTimeString()}
                  </div>
                )}
                <ChatBubble msg={m} />
              </div>
            );
          })}
        </div>

        <div className="p-4">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-3 flex-1">
              {inputMode === "audio" ? (
                <>
                  <MicButton
                    state={voiceState}
                    onToggle={onPrimaryToggle}
                    onBeforeStart={preflightStart}
                  />
                  <div className="flex-1 h-[52px] flex items-center">
                    <Visualizer active={visualizerActive} />
                  </div>
                </>
              ) : (
                <input
                  className="ui-input w-full h-[52px]"
                  placeholder="Type a message…"
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      handleSendText();
                    }
                  }}
                />
              )}
            </div>

            <button
              className="ui-button input-mode-toggle"
              onClick={() => {
                const next = inputMode === "audio" ? "text" : "audio";
                onToggleInputMode?.(next);
              }}
              title="Toggle input mode"
            >
              {inputMode === "audio" ? (
                <svg
                  width="20"
                  height="20"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path d="M3 19h18" />
                  <path d="M5 15h14" />
                  <path d="M7 11h10" />
                  <path d="M9 7h6" />
                </svg>
              ) : (
                <svg
                  width="20"
                  height="20"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z" />
                  <path d="M19 11a7 7 0 0 1-14 0" />
                  <path d="M12 19v3" />
                </svg>
              )}
            </button>

            {inputMode === "text" && (
              <button
                className="ui-button"
                onClick={handleSendText}
                title="Send"
              >
                Send
              </button>
            )}

            {voiceState === "speaking" && inputMode === "audio" && (
              <button
                className="ui-button settings-action-btn"
                onClick={onStopSpeaking}
              >
                Stop
              </button>
            )}
          </div>
        </div>
      </div>
    </main>
  );
};

export default ChatPane;
