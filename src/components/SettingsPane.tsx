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

import React, { useEffect, useRef, useState } from "react";
import { openExternalUrl } from "../lib/tauri";
import { Config } from "../types";
import { saveConfig, listInputDevices, listOutputDevices } from "../lib/tauri";

type SettingsPaneProps = {
  config: Config | null;
  hidden: boolean;
  onConfigChanged?: (cfg: Config) => void;
};

const FieldRow: React.FC<{
  label: React.ReactNode;
  hint?: string;
  children: React.ReactNode;
}> = ({ label, hint, children }) => (
  <div className="flex flex-col gap-1 tooltip">
    <label className="text-xs text-zinc-400">{label}</label>
    {children}
    {hint && <div className="tooltip-content">{hint}</div>}
  </div>
);

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({
  title,
  children,
}) => (
  <div className="space-y-3">
    {title && (
      <div className="text-xs font-semibold text-zinc-300">{title}</div>
    )}
    {children}
  </div>
);

function useBlockWheelOnNumber() {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const target = e.target as HTMLElement | null;
      if (target instanceof HTMLInputElement && target.type === "number") {
        target.blur();
        e.preventDefault();
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel as any);
  }, []);
  return ref;
}

const SettingsPane: React.FC<SettingsPaneProps> = ({
  config,
  hidden,
  onConfigChanged,
}) => {
  const [local, setLocal] = useState<Config | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "ok" | "err">("idle");
  const [reduceMotion, setReduceMotion] = useState<boolean>(false);
  const [inputDevices, setInputDevices] = useState<string[]>([]);
  const [outputDevices, setOutputDevices] = useState<string[]>([]);
  const wheelBlockRef = useBlockWheelOnNumber();

  useEffect(() => {
    if (config) {
      setLocal(config);
      setReduceMotion(document.body.classList.contains("reduce-motion"));
    }
  }, [config]);

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const insRaw = await listInputDevices().catch(() => [] as any);
        const outsRaw = await listOutputDevices().catch(() => [] as any);
        const ins = Array.isArray(insRaw) ? insRaw.filter((x) => typeof x === 'string') : [];
        const outs = Array.isArray(outsRaw) ? outsRaw.filter((x) => typeof x === 'string') : [];
        if (mounted) {
          setInputDevices(ins);
          setOutputDevices(outs);
        }
      } catch (e) {
        console.warn('Device enumeration failed', e);
      }
    })();
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    document.body.classList.toggle("reduce-motion", reduceMotion);
  }, [reduceMotion]);

  const update = <K extends keyof Config>(key: K, val: Config[K]) => {
    setLocal((prev) => (prev ? { ...prev, [key]: val } : prev));
  };

  const onSave = async () => {
    if (!local) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      await saveConfig(local);
      onConfigChanged?.(local); // ensure parent applies theme etc.
      setSaveStatus("ok");
      setTimeout(() => setSaveStatus("idle"), 1200);
    } catch (e) {
      console.error(e);
      setSaveStatus("err");
    } finally {
      setSaving(false);
    }
  };

  const onReset = () => {
    if (config) setLocal(config);
  };

  // Removed path configuration controls. Paths are determined by the app automatically.

  const defaultSection = (
    <div className="space-y-4">
      <Section title="API Keys">
        <FieldRow
          label={(
            <>
              Porcupine Key {" "}
              <button
                type="button"
                onClick={() => openExternalUrl("https://console.picovoice.ai/login")}
                className="text-emerald-400 hover:underline"
              >
                (console.picovoice.ai)
              </button>
            </>
          )}
          hint="Picovoice Porcupine key for wake word."
        >
          <input
            className="ui-input w-full"
            type="password"
            value={local?.porcupine_key ?? ""}
            onChange={(e) => update("porcupine_key", e.target.value)}
          />
        </FieldRow>
        <FieldRow
          label={(
            <>
              Gemini Key {" "}
              <button
                type="button"
                onClick={() => openExternalUrl("https://aistudio.google.com/welcome")}
                className="text-emerald-400 hover:underline"
              >
                (aistudio.google.com)
              </button>
            </>
          )}
          hint="Google AI Studio API key for the LLM."
        >
          <input
            className="ui-input w-full"
            type="password"
            value={local?.gemini_key ?? ""}
            onChange={(e) => update("gemini_key", e.target.value)}
          />
        </FieldRow>
        <FieldRow
          label={(
            <>
              ElevenLabs Key {" "}
              <button
                type="button"
                onClick={() => openExternalUrl("https://elevenlabs.io/app/settings/api-keys")}
                className="text-emerald-400 hover:underline"
              >
                (elevenlabs.io)
              </button>
            </>
          )}
          hint="API key for ElevenLabs text-to-speech."
        >
          <input
            className="ui-input w-full"
            type="password"
            value={local?.elevenlabs_key ?? ""}
            onChange={(e) => update("elevenlabs_key", e.target.value)}
          />
        </FieldRow>
        <FieldRow label="ElevenLabs Voice ID" hint="Voice identifier in ElevenLabs.">
          <input
            className="ui-input w-full"
            value={local?.voice_id ?? ""}
            onChange={(e) => update("voice_id", e.target.value)}
          />
        </FieldRow>
      </Section>

      <div className="ui-sep my-2" />
      <div className="ui-section-divider text-[11px] text-zinc-400 px-1 select-none">
        Default Settings
      </div>

      <Section title="">
        <FieldRow label="Theme" hint="Choose the accent theme.">
          <select
            className="ui-input"
            value={local?.theme ?? "emerald"}
            onChange={(e) => update("theme", e.target.value as any)}
          >
            <option value="emerald">Emerald</option>
            <option value="rosepine">Rose Pine</option>
            <option value="ocean">Ocean</option>
            <option value="velvet">Velvet</option>
            <option value="cobalt">Cobalt</option>
            <option value="honey">Honey</option>
            <option value="crimson">Crimson</option>
            <option value="orchid">Orchid</option>
          </select>
        </FieldRow>

        <FieldRow
          label="Whisper Language"
          hint="Select the language for transcription."
        >
          <select
            className="ui-input w-full"
            value={local?.whisper_language ?? "en"}
            onChange={(e) => update("whisper_language", e.target.value)}
          >
            <option value="nl">Dutch</option>
            <option value="es">Spanish</option>
            <option value="ko">Korean</option>
            <option value="it">Italian</option>
            <option value="de">German</option>
            <option value="th">Thai</option>
            <option value="ru">Russian</option>
            <option value="pt">Portuguese</option>
            <option value="pl">Polish</option>
            <option value="id">Indonesian</option>
            <option value="zh-TW">Mandarin (TW)</option>
            <option value="sv">Swedish</option>
            <option value="cs">Czech</option>
            <option value="en">English</option>
            <option value="ja">Japanese</option>
            <option value="fr">French</option>
          </select>
        </FieldRow>
        <FieldRow
          label="Microphone"
          hint="Choose the input device by name."
        >
          <select
            className="ui-input w-full"
            value={local?.default_microphone_name ?? ""}
            onChange={(e) => update("default_microphone_name", e.target.value)}
          >
            <option value="">System Default</option>
            {inputDevices.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </FieldRow>

        <FieldRow
          label="Headphones / Output"
          hint="Choose the output device for TTS playback."
        >
          <select
            className="ui-input w-full"
            value={local?.default_output_device_name ?? ""}
            onChange={(e) => update("default_output_device_name", e.target.value)}
          >
            <option value="">System Default</option>
            {outputDevices.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </FieldRow>
      </Section>
    </div>
  );

  const advancedSection = (
    <div className="ui-advanced-wrap space-y-4" ref={wheelBlockRef}>
      <div className="ui-section-divider text-[11px] text-zinc-400 px-1 select-none">
        Advanced Settings
      </div>

      <Section title="Models">
        <FieldRow
          label="Gemini Model"
          hint="Model name used for LLM responses."
        >
          <select
            className="ui-input w-full"
            value={local?.gemini_model ?? "gemini-2.5-flash"}
            onChange={(e) => update("gemini_model", e.target.value)}
          >
            <option value="gemini-2.5-pro">Pro V2.5</option>
            <option value="gemini-2.5-flash">Flash V2.5</option>
            <option value="gemini-2.5-flash-lite">Flash Lite V2.5</option>
            <option value="gemini-2.0-flash">Flash V2.0</option>
            <option value="gemini-2.0-flash-lite">Flash Lite V2.0</option>
          </select>
        </FieldRow>
        <FieldRow label="ElevenLabs Model" hint="Voice model used for TTS.">
          <select
            className="ui-input w-full"
            value={local?.elevenlabs_model ?? "eleven_flash_v2_5"}
            onChange={(e) => update("elevenlabs_model", e.target.value)}
          >
            <option value="eleven_multilingual_v2">Multilingual V2</option>
            <option value="eleven_flash_v2_5">Flash V2.5</option>
            <option value="eleven_turbo_v2_5">Turbo V2.5</option>
          </select>
        </FieldRow>
      </Section>

      <div className="ui-sep" />

      <Section title="System Prompt">
        <FieldRow
          label="LLM System Prompt"
          hint="Sets assistant persona and behavior."
        >
          <textarea
            className="ui-input w-full"
            style={{ minHeight: 96, resize: "vertical" }}
            value={local?.llm_system_prompt ?? ""}
            onChange={(e) => update("llm_system_prompt", e.target.value)}
          />
        </FieldRow>
      </Section>
      <div className="ui-sep" />

      <Section title="Audio & Timing">
        <FieldRow label="VAD Mode" hint="Voice activity detection mode.">
          <select
            className="ui-input w-full"
            value={local?.vad_mode ?? "Aggressive"}
            onChange={(e) => update("vad_mode", e.target.value)}
          >
            <option value="Quality">Quality</option>
            <option value="Aggressive">Aggressive</option>
            <option value="VeryAggressive">Very Aggressive</option>
          </select>
        </FieldRow>
        <FieldRow
          label="Wake Word Sensitivity"
          hint="Higher = more triggers; lower = fewer."
        >
          <input
            type="number"
            className="ui-input w-full"
            min={0}
            max={1}
            step={0.01}
            value={local?.wwd_sensitivity ?? 0.9}
            onChange={(e) => update("wwd_sensitivity", Number(e.target.value))}
          />
        </FieldRow>
        <FieldRow
          label="Context Window Expiration (s)"
          hint="Seconds before conversation context expires."
        >
          <input
            className="ui-input w-full"
            type="number"
            value={local?.context_window_expiration_seconds ?? 1800}
            onChange={(e) =>
              update(
                "context_window_expiration_seconds",
                Number(e.target.value)
              )
            }
          />
        </FieldRow>
        <FieldRow
          label="Frame Duration (ms)"
          hint="Audio frame duration for processing."
        >
          <input
            className="ui-input w-full"
            type="number"
            value={local?.frame_duration_ms ?? 30}
            onChange={(e) =>
              update("frame_duration_ms", Number(e.target.value))
            }
          />
        </FieldRow>
        <FieldRow
          label="Silence Threshold (s)"
          hint="Time of silence to end speech capture."
        >
          <input
            className="ui-input w-full"
            type="number"
            value={local?.silence_threshold_seconds ?? 1}
            onChange={(e) =>
              update("silence_threshold_seconds", Number(e.target.value))
            }
          />
        </FieldRow>
        <FieldRow
          label="Speech Trigger Frames"
          hint="Number of frames to confirm speech start."
        >
          <input
            className="ui-input w-full"
            type="number"
            value={local?.speech_trigger_frames ?? 3}
            onChange={(e) =>
              update("speech_trigger_frames", Number(e.target.value))
            }
          />
        </FieldRow>
        <FieldRow
          label="Frame Length WWD"
          hint="Frame length for wake word detection."
        >
          <input
            className="ui-input w-full"
            type="number"
            value={local?.frame_length_wwd ?? 512}
            onChange={(e) => update("frame_length_wwd", Number(e.target.value))}
          />
        </FieldRow>
      </Section>

      <div className="ui-sep" />

      <Section title="Accessibility">
        <div className="tooltip">
          <label className="ui-checkbox-row">
            <input
              type="checkbox"
              checked={reduceMotion}
              onChange={(e) => setReduceMotion(e.target.checked)}
            />
            <span className="text-zinc-300 text-sm">Reduce Motion</span>
          </label>
          <div className="tooltip-content">
            Disable animations and heavy visual effects.
          </div>
        </div>
      </Section>
    </div>
  );

  return (
    <aside
      className={`h-full transition-opacity duration-300 min-w-0 ${
        hidden
          ? "overflow-hidden w-0 opacity-0 pointer-events-none shrink-0"
          : "overflow-visible w-1/4 opacity-100 shrink-0"
      }`}
    >
      <div className="h-full pl-2">
        <div className="ui-card h-full p-3 flex flex-col overflow-hidden">
          <div className="px-1 py-2 text-xs uppercase tracking-wider text-zinc-400">
            Settings
          </div>
          <div className="ui-sep my-2" />
          {!local ? (
            <div className="text-sm text-zinc-500">Loading config...</div>
          ) : (
            <>
              <div className="flex-1 overflow-y-auto overflow-x-hidden space-y-6 px-4">
                <div className="ui-advanced-wrap">{defaultSection}</div>
                {advancedSection}
              </div>

              <div className="ui-sep my-3" />
              <div className="flex items-center gap-2">
                <button
                  className="ui-button settings-action-btn"
                  onClick={onSave}
                  disabled={saving}
                >
                  {saving
                    ? "Saving…"
                    : saveStatus === "ok"
                    ? "Saved"
                    : saveStatus === "err"
                    ? "Retry Save"
                    : "Save"}
                </button>
                <button
                  className="ui-button settings-action-btn"
                  onClick={onReset}
                >
                  Reset
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </aside>
  );
};

export default SettingsPane;
