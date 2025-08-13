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

import React from "react";
import type { VoiceState } from "../types";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Settings } from "lucide-react";

type TopBarProps = {
  onToggleHistory: () => void;
  onToggleSettings: () => void;
  whisperProgress?: number | null;
  voiceState?: VoiceState;
};

const TopBar: React.FC<TopBarProps> = ({
  onToggleHistory,
  onToggleSettings,
  whisperProgress,
  voiceState,
}) => {
  return (
    <div
      className="w-full h-12 flex items-center justify-between px-3 border-b border-white/5 bg-[#0b0e14]/80 backdrop-blur z-20 sticky top-0"
      data-tauri-drag-region
    >
      {/* Left side - history */}
      <div className="flex items-center gap-2">
        <button
          className="ui-icon-button"
          onClick={onToggleHistory}
          title="Toggle History"
        >
          <svg width="20" height="20" fill="none" className="text-zinc-300">
            <rect x="2" y="4" width="6" height="12" rx="2" stroke="currentColor" />
            <rect x="10" y="4" width="8" height="12" rx="2" stroke="currentColor" />
          </svg>
        </button>
      </div>

      {/* Center - title */}
      <div className="font-semibold tracking-wide text-zinc-200">Jarvis</div>

      {/* Right side - settings & window controls */}
      <div className="flex items-center gap-1" data-tauri-drag-region="false">
        {/* Settings button (moved slightly left) */}
        <button
          className="ui-icon-button mr-2"
          onClick={onToggleSettings}
          title="Toggle Settings"
        >
          <Settings size={18} className="text-zinc-300" />
        </button>

        {/* Window controls */}
        <button
          onClick={() => getCurrentWindow().minimize()}
          className="ui-icon-button hover:bg-emerald-700"
        >
          <Minus size={16} />
        </button>
        <button
          onClick={() => getCurrentWindow().toggleMaximize()}
          className="ui-icon-button hover:bg-emerald-700"
        >
          <Square size={14} />
        </button>
        <button
          onClick={() => getCurrentWindow().close()}
          className="ui-icon-button hover:bg-red-600"
        >
          <X size={16} />
        </button>
      </div>


      {/* Progress bar */}
      {
        (typeof whisperProgress === "number" ||
          voiceState === "loading" ||
          voiceState === "processing") && (
          <div className="absolute bottom-0 left-0 right-0 h-[2px] overflow-hidden">
            {typeof whisperProgress === "number" ? (
              <div
                className="h-full bg-emerald-500/70"
                style={{
                  width: `${Math.max(0, Math.min(100, whisperProgress))}%`,
                }}
              />
            ) : (
              <div
                className="h-full"
                style={{
                  background:
                    "linear-gradient(90deg, transparent, rgba(16,185,129,0.7), transparent)",
                  animation: "shimmer 1.2s linear infinite",
                }}
              />
            )}
          </div>
        )
      }
    </div >
  );
};

export default TopBar;
