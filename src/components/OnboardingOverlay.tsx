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
import { openExternalUrl } from "../lib/tauri";
import type { Config } from "../types";
import { resolveResourcePath, saveConfig } from "../lib/tauri";

type OnboardingOverlayProps = {
  config: Config | null;
  onConfigChanged: (cfg: Config) => void;
};

// Utility to check if a key is non-empty after trim
function isFilled(val?: string | null): boolean {
  return !!val && val.trim().length > 0;
}

const OnboardingOverlay: React.FC<OnboardingOverlayProps> = ({ config, onConfigChanged }) => {
  const [local, setLocal] = useState<Config | null>(config);
  const [gifPaths, setGifPaths] = useState<{
    picovoice?: string;
    gemini?: string;
    elevenlabs?: string;
  }>({});

  useEffect(() => setLocal(config), [config]);

  // Resolve resource paths for GIFs (these are placeholders that the user will provide)
  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const [p1, p2, p3] = await Promise.all([
          resolveResourcePath("assets/get_picovoice.gif").catch(() => ""),
          resolveResourcePath("assets/get_gemini.gif").catch(() => ""),
          resolveResourcePath("assets/get_elevenlabs.gif").catch(() => ""),
        ]);
        if (mounted) setGifPaths({ picovoice: p1, gemini: p2, elevenlabs: p3 });
      } catch {
        // ignore, paths may not exist during development until assets are added
      }
    })();
    return () => {
      mounted = false;
    };
  }, []);

  const missing = useMemo(() => {
    const cfg = config;
    return {
      porcupine: !isFilled(cfg?.porcupine_key),
      gemini: !isFilled(cfg?.gemini_key),
      elevenlabs: !isFilled(cfg?.elevenlabs_key),
    };
  }, [config]);

  const allSet = useMemo(() => !missing.porcupine && !missing.gemini && !missing.elevenlabs, [missing]);

  const update = <K extends keyof Config>(key: K, value: Config[K]) => {
    setLocal((prev) => (prev ? { ...prev, [key]: value } as Config : prev));
  };

  const handleSave = async () => {
    if (!local) return;
    await saveConfig(local);
    onConfigChanged(local);
  };

  // Only render when there is a config and at least one key is missing
  if (!local || allSet) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/70 backdrop-blur" />

      {/* Modal */}
      <div className="relative max-w-4xl w-[92vw] bg-[#0b0e14] border border-white/10 rounded-lg shadow-xl p-6 overflow-y-auto max-h-[90vh]">
        <div className="mb-4">
          <h2 className="text-xl font-semibold text-zinc-100">Set up your API keys</h2>
          <p className="text-zinc-400 text-sm mt-1">
            Add the required keys below to continue. You cannot use the app until all required keys are provided.
          </p>
        </div>

        {/* Picovoice / Porcupine */}
        <div className="ui-card p-4 mb-4 border border-white/10 rounded-md">
          <div className="flex flex-col md:flex-row gap-4">
            <div className="md:w-1/2">
              <div className="text-zinc-200 font-medium mb-1">
                Picovoice Porcupine Key
                {" "}
                <button
                  type="button"
                  onClick={() => openExternalUrl("https://console.picovoice.ai/login")}
                  className="text-emerald-400 hover:underline"
                >
                  (console.picovoice.ai)
                </button>
              </div>
              <div className="text-zinc-400 text-sm mb-3">Required for wake word detection.</div>
              <input
                className="ui-input w-full"
                type="password"
                placeholder="Paste Picovoice key"
                value={local.porcupine_key ?? ""}
                onChange={(e) => update("porcupine_key", e.target.value)}
              />
              {missing.porcupine && (
                <div className="text-[12px] text-amber-400 mt-1">Missing key</div>
              )}
            </div>
            <div className="md:w-1/2">
              {gifPaths.picovoice ? (
                <img
                  src={gifPaths.picovoice}
                  alt="How to get Picovoice Porcupine key"
                  className="w-full h-auto rounded border border-white/10"
                />
              ) : (
                <div className="w-full h-40 flex items-center justify-center text-zinc-500 text-sm border border-dashed border-white/10 rounded">
                  Place get_picovoice.gif in src-tauri/assets
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Gemini */}
        <div className="ui-card p-4 mb-4 border border-white/10 rounded-md">
          <div className="flex flex-col md:flex-row gap-4">
            <div className="md:w-1/2">
              <div className="text-zinc-200 font-medium mb-1">
                Gemini API Key
                {" "}
                <button
                  type="button"
                  onClick={() => openExternalUrl("https://aistudio.google.com/welcome")}
                  className="text-emerald-400 hover:underline"
                >
                  (aistudio.google.com)
                </button>
              </div>
              <div className="text-zinc-400 text-sm mb-3">Required for the LLM.</div>
              <input
                className="ui-input w-full"
                type="password"
                placeholder="Paste Gemini key"
                value={local.gemini_key ?? ""}
                onChange={(e) => update("gemini_key", e.target.value)}
              />
              {missing.gemini && (
                <div className="text-[12px] text-amber-400 mt-1">Missing key</div>
              )}
            </div>
            <div className="md:w-1/2">
              {gifPaths.gemini ? (
                <img
                  src={gifPaths.gemini}
                  alt="How to get Gemini key"
                  className="w-full h-auto rounded border border-white/10"
                />
              ) : (
                <div className="w-full h-40 flex items-center justify-center text-zinc-500 text-sm border border-dashed border-white/10 rounded">
                  Place get_gemini.gif in src-tauri/assets
                </div>
              )}
            </div>
          </div>
        </div>

        {/* ElevenLabs */}
        <div className="ui-card p-4 mb-4 border border-white/10 rounded-md">
          <div className="flex flex-col md:flex-row gap-4">
            <div className="md:w-1/2">
              <div className="text-zinc-200 font-medium mb-1">
                ElevenLabs API Key
                {" "}
                <button
                  type="button"
                  onClick={() => openExternalUrl("https://elevenlabs.io/app/settings/api-keys")}
                  className="text-emerald-400 hover:underline"
                >
                  (elevenlabs.io)
                </button>
              </div>
              <div className="text-zinc-400 text-sm mb-3">Required for text-to-speech.</div>
              <input
                className="ui-input w-full"
                type="password"
                placeholder="Paste ElevenLabs key"
                value={local.elevenlabs_key ?? ""}
                onChange={(e) => update("elevenlabs_key", e.target.value)}
              />
              {missing.elevenlabs && (
                <div className="text-[12px] text-amber-400 mt-1">Missing key</div>
              )}
            </div>
            <div className="md:w-1/2">
              {gifPaths.elevenlabs ? (
                <img
                  src={gifPaths.elevenlabs}
                  alt="How to get ElevenLabs key"
                  className="w-full h-auto rounded border border-white/10"
                />
              ) : (
                <div className="w-full h-40 flex items-center justify-center text-zinc-500 text-sm border border-dashed border-white/10 rounded">
                  Place get_elevenlabs.gif in src-tauri/assets
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Actions: Save only; no skip */}
        <div className="flex items-center justify-between mt-2">
          <div className="text-[12px] text-zinc-500">
            Tip: You can also edit keys later in Settings → API Keys.
          </div>
          <button
            className="ui-button px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white rounded disabled:opacity-50 disabled:cursor-not-allowed"
            onClick={handleSave}
          >
            Save
          </button>
        </div>

        {/* Blocker note when some keys are still missing after save attempt */}
        {!allSet && (
          <div className="mt-3 text-[12px] text-amber-400">
            All keys are required to continue.
          </div>
        )}
      </div>
    </div>
  );
};

export default OnboardingOverlay;


