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

import React, { useState } from "react";
import { ConversationSummary } from "../types";
import { deleteConversation, renameConversation } from "../lib/tauri";

type HistoryPaneProps = {
  items: ConversationSummary[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  hidden: boolean;
  onNewConversation?: () => void;
};

const HistoryPane: React.FC<HistoryPaneProps> = ({
  items,
  selectedId,
  onSelect,
  hidden,
  onNewConversation,
}) => {
  function displayName(filename: string): string {
    let base = filename.replace(/\.json$/i, "");
    base = base.replace(/ - \d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}$/i, "");
    // If the remaining is just a timestamp, collapse to a generic label
    if (/^\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}$/.test(base)) {
      return "Conversation";
    }
    const trimmed = base.trim();
    return trimmed.length > 0 ? trimmed : "Conversation";
  }
  const [contextAt, setContextAt] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState<string>("");

  async function handleDelete(id: string) {
    try {
      await deleteConversation(id);
      // Fire a custom event to let parent refresh history
      window.dispatchEvent(new CustomEvent("history-changed"));
    } catch (e) {
      console.error("Failed to delete conversation", e);
    }
  }

  async function handleStartRename(id: string, currentLabel: string) {
    setRenamingId(id);
    setRenameValue(currentLabel);
    setContextAt(null);
  }

  async function handleCommitRename(id: string) {
    const title = renameValue.trim();
    if (!title) {
      setRenamingId(null);
      return;
    }
    try {
      const { new_filename: _} = await renameConversation(id, title);
      window.dispatchEvent(new CustomEvent("history-changed"));
      // In case the renamed chat was selected, keep selection coherent by emitting event only
    } catch (e) {
      console.error("Failed to rename conversation", e);
    } finally {
      setRenamingId(null);
    }
  }

  return (
    <aside
      className={`h-full transition-opacity duration-300 min-w-0 ${
        hidden
          ? "overflow-hidden w-0 opacity-0 pointer-events-none shrink-0"
          : "overflow-visible w-1/6 opacity-100 shrink-0"
      }`}
    >
      <div className="h-full pr-2">
        <div className="ui-card h-full p-2 flex flex-col">
          <div className="px-2 py-2 text-xs uppercase tracking-wider text-zinc-400">
            History
          </div>
          <div className="ui-sep my-1" />
          <div className="overflow-auto grow">
            <ul className="space-y-1">
              {items.map((conv) => {
                const active = conv.id === selectedId;
                const label = displayName(conv.filename);
                return (
                  <li key={conv.id} className="relative">
                    <button
                      onClick={() => onSelect(conv.id)}
                      className={`w-full text-left px-3 py-2 rounded-lg border transition focus:outline-none focus:ring-0 focus-visible:outline-none ${active
                        ? "bg-opacity-10 text-[rgb(var(--theme-accent))] conversation-active"
                        : "bg-transparent border-white/5 hover:bg-white/5 text-zinc-300"
                        }`}
                      title={label}
                    >
                      <div className="flex items-center gap-2">
                        <div
                          className="truncate text-sm"
                          onDoubleClick={(e) => {
                            e.stopPropagation();
                            handleStartRename(conv.id, label);
                          }}
                        >
                          {renamingId === conv.id ? (
                            <input
                              className="ui-input no-focus-ring h-7 px-2 outline-none focus:outline-none focus:ring-0 focus-visible:ring-0"
                              autoFocus
                              value={renameValue}
                              onChange={(e) => setRenameValue(e.target.value)}
                              onKeyDown={(e) => {
                                if (e.key === "Enter") handleCommitRename(conv.id);
                                if (e.key === "Escape") setRenamingId(null);
                              }}
                              onBlur={() => handleCommitRename(conv.id)}
                            />
                          ) : (
                            label
                          )}
                        </div>
                        <div
                          className="relative ml-auto text-zinc-500 hover:text-zinc-300"
                          onClick={(e) => {
                            e.stopPropagation();
                            setContextAt(conv.id);
                          }}
                          title="More"
                        >
                          ⋯
                          {contextAt === conv.id && (
                            <div
                              className="absolute right-0 z-50 bg-[#0f1115] border border-white/10 rounded-md shadow-lg mt-1"
                              onMouseLeave={() => setContextAt(null)}
                            >
                              <button
                                className="block w-full text-left px-3 py-2 text-sm hover:bg-white/5"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleStartRename(conv.id, label);
                                }}
                              >
                                Rename
                              </button>
                              <div className="ui-sep my-1" />
                              <button
                                className="block w-full text-left px-3 py-2 text-sm hover:bg-white/5 text-red-400"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setContextAt(null);
                                  handleDelete(conv.id);
                                }}
                              >
                                Delete
                              </button>
                            </div>
                          )}
                        </div>
                      </div>
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
          <div className="ui-sep my-2" />
          <div className="px-2 py-2">
            <button className="ui-button w-full new-conversation-btn" onClick={onNewConversation}>
              New Conversation
            </button>
          </div>
        </div>
      </div>
    </aside>
  );
};

export default HistoryPane;
