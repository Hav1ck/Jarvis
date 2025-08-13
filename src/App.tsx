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

import React, { useEffect, useMemo, useState } from "react";
import { listen as listenRawEvent } from "@tauri-apps/api/event";
import TopBar from "./components/TopBar";
import HistoryPane from "./components/HistoryPane";
import ChatPane from "./components/ChatPane";
import SettingsPane from "./components/SettingsPane";
import OnboardingOverlay from "./components/OnboardingOverlay";
import { ConversationSummary, Message, Config, VoiceState } from "./types";
import { listHistoryFiles, loadConfig, getJarvisStatus, listenToStateChanges, listenToNewMessages, createConversation, readConversation, appendTurn, setActiveConversation, generateAndRenameConversation, listenToWhisperDownloadProgress, listenToWhisperDownloadComplete } from "./lib/tauri";

const App: React.FC = () => {
  const [historyHidden, setHistoryHidden] = useState(false);
  const [settingsHidden, setSettingsHidden] = useState(false);

  const [history, setHistory] = useState<ConversationSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [config, setConfig] = useState<Config | null>(null);

  const [voiceState, setVoiceState] = useState<VoiceState>("idle");
  const [whisperProgress, setWhisperProgress] = useState<number | null>(null);

  const pushSystemMessage = async (content: string) => {
    const createdAt = Date.now();
    const newMessage: Message = {
      id: `sys_${createdAt}`,
      role: "system",
      content,
      createdAt,
    };
    setMessages((prev) => [...prev, newMessage]);
    if (selectedId) {
      try {
        await appendTurn(selectedId, newMessage.role, newMessage.content, newMessage.createdAt);
      } catch (e) {
        console.error("Failed to persist system message:", e);
      }
    }
  };

  // Helper: refresh history list and ensure at least one conversation exists
  async function ensureHistory() {
    let files = await listHistoryFiles();
    if (files.length === 0) {
      await createConversation();
      files = await listHistoryFiles();
    }
    const items = files.map((f) => ({ id: f, filename: f }));
    setHistory(items);
    if (!selectedId && items[0]) {
      setSelectedId(items[0].id);
      await setActiveConversation(items[0].id);
    }
  }

  async function handleNewConversation() {
    try {
      // Check if there's already an empty conversation
      const files = await listHistoryFiles();
      const items = files.map((f) => ({ id: f, filename: f }));
      
      // Find an empty conversation (one with no messages)
      let emptyConversationId: string | null = null;
      for (const item of items) {
        try {
          const turns = await readConversation(item.id);
          if (turns.length === 0) {
            emptyConversationId = item.id;
            break;
          }
        } catch (e) {
          console.error(`Failed to read conversation ${item.id}:`, e);
        }
      }
      
      if (emptyConversationId) {
        // Use existing empty conversation
        setHistory(items);
        setSelectedId(emptyConversationId);
        await setActiveConversation(emptyConversationId);
        setMessages([]);
      } else {
        // Create new conversation if no empty one exists
        const fname = await createConversation();
        const newFiles = await listHistoryFiles();
        const newItems = newFiles.map((f) => ({ id: f, filename: f }));
        setHistory(newItems);
        setSelectedId(fname);
        await setActiveConversation(fname);
        setMessages([]);
      }
    } catch (e) {
      console.error("Failed to create conversation:", e);
    }
  }

  useEffect(() => {
    ensureHistory().catch(console.error);
    loadConfig().then(setConfig).catch(console.error);
  }, []);

  // Listen for history changes (e.g., delete) and refresh
  useEffect(() => {
    const handler = () => {
      ensureHistory().catch(console.error);
    };
    window.addEventListener('history-changed', handler);
    return () => window.removeEventListener('history-changed', handler);
  }, []);

  // Load messages when a conversation is selected
  useEffect(() => {
    (async () => {
      if (!selectedId) return;
      try {
        const turns = await readConversation(selectedId);
        const msgs: Message[] = turns.map((t, idx) => ({
          id: `${selectedId}-${idx}`,
          role: (t.role as any),
          content: t.content,
          createdAt: t.createdAt,
        }));
        setMessages(msgs);
        await setActiveConversation(selectedId);
      } catch (e) {
        console.error("Failed to read conversation:", e);
        setMessages([]);
      }
    })();
  }, [selectedId]);

  // Listen for state changes from Jarvis
  useEffect(() => {
    const unsubscribeState = listenToStateChanges((state) => {
      console.log("Jarvis state changed:", state);
      switch (state) {
        case "Idle":
          setVoiceState("idle");
          break;
        case "Loading":
          setVoiceState("loading"); // distinct loading state
          break;
        case "WakeListening":
          setVoiceState("wake_listening");
          break;
        case "Recording":
          setVoiceState("recording");
          break;
        case "Processing":
          setVoiceState("processing");
          break;
        case "Speaking":
          setVoiceState("speaking");
          break;
        default:
          console.warn("Unknown state:", state);
      }
    });

    const unsubscribeMessages = listenToNewMessages(async (messageData) => {
      console.log("New message received:", messageData);
      const newMessage: Message = {
        id: (messageData && messageData.id) || `msg_${Date.now()}_${Math.random()}`,
        role: messageData.role as 'user' | 'assistant' | 'system',
        content: messageData.content,
        createdAt: messageData.createdAt,
        meta: messageData.meta,
      };
      setMessages(prev => [...prev, newMessage]);
      // Persist to current conversation
      if (selectedId) {
        try {
          await appendTurn(selectedId, newMessage.role, newMessage.content, newMessage.createdAt);
          // optional: no-op
        } catch (e) {
          console.error("Failed to append turn:", e);
        }
      }

      // If this is a brand-new conversation filename, generate a title after the first user message
      if (selectedId && newMessage.role === 'user' && selectedId.startsWith('New Conversation - ')) {
        try {
          const { new_filename } = await generateAndRenameConversation(selectedId);
          // Refresh history and switch selection to the renamed file
          const files = await listHistoryFiles();
          const items = files.map((f) => ({ id: f, filename: f }));
          setHistory(items);
          setSelectedId(new_filename);
          await setActiveConversation(new_filename);
        } catch (e) {
          console.error('Failed to generate/rename conversation title:', e);
        }
      }
    });

    // Listen for meta updates that attach to previously-sent assistant message
    const unsubscribeMeta = listenRawEvent('message-meta', (event) => {
      const payload = event.payload as any;
      const createdAtOfAssistant = payload?.createdAtOfAssistant as number;
      const meta = payload?.meta as any;
      if (typeof createdAtOfAssistant !== 'number' || !meta) return;
      setMessages((prev) =>
        prev.map((m) =>
          m.role === 'assistant' && m.createdAt === createdAtOfAssistant
            ? { ...m, meta: { ...(m.meta || {}), ...meta } }
            : m
        )
      );
    });

    // Whisper download progress listeners
    const unsubProgPromise = listenToWhisperDownloadProgress(({ percent }) => {
      setWhisperProgress(percent);
    });
    const unsubDonePromise = listenToWhisperDownloadComplete(() => {
      setWhisperProgress(null);
    });

    return () => {
      unsubscribeState.then(unsub => unsub());
      unsubscribeMessages.then(unsub => unsub());
      unsubscribeMeta.then((unsub: any) => unsub());
      unsubProgPromise.then(unsub => unsub());
      unsubDonePromise.then(unsub => unsub());
    };
  }, [selectedId]);

  // Periodically check Jarvis status to keep UI in sync
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const isRunning = await getJarvisStatus();
        if (!isRunning && voiceState !== "idle") {
          setVoiceState("idle");
        }
      } catch (err) {
        console.error("Failed to get Jarvis status:", err);
      }
    }, 1000); // Check every second

    return () => clearInterval(interval);
  }, [voiceState]);

  const handlePrimaryToggle = (next: VoiceState) => {
    if (next === "idle" || next === "wake_listening") {
      setVoiceState(next);
    }
  };

  const handleStopSpeaking = () => setVoiceState("idle");

  const layoutClass = useMemo(() => "h-[calc(100vh-3rem)]", []);
  const themeClass = `theme-${config?.theme ?? "emerald"}`;

  // Apply theme class to body
  useEffect(() => {
    document.body.className = themeClass;
  }, [themeClass]);

  return (
    <div className="h-screen flex flex-col">
      <TopBar
        onToggleHistory={() => setHistoryHidden((v) => !v)}
        onToggleSettings={() => setSettingsHidden((v) => !v)}
        whisperProgress={whisperProgress}
        voiceState={voiceState}
      />
      <div
        className={`flex gap-2 px-2 py-2 ${layoutClass} w-full max-w-full`}
      >
        <HistoryPane
          items={history}
          selectedId={selectedId}
          onSelect={setSelectedId}
          hidden={historyHidden}
          onNewConversation={handleNewConversation}
        />
        <div className="flex-1 min-w-0">
          <ChatPane
            messages={messages}
            voiceState={voiceState}
            onPrimaryToggle={handlePrimaryToggle}
            onStopSpeaking={handleStopSpeaking}
            config={config}
            onToggleInputMode={(mode) =>
              setConfig((c) => (c ? { ...c, input_mode: mode } : c))
            }
            whisperProgress={whisperProgress}
            onSystemMessage={pushSystemMessage}
          />
        </div>
        <SettingsPane
          config={config}
          hidden={settingsHidden}
          onConfigChanged={setConfig}
        />
      </div>
      {/* Onboarding overlay that blocks usage until keys are present */}
      <OnboardingOverlay config={config} onConfigChanged={setConfig} />
    </div>
  );
};

export default App;
