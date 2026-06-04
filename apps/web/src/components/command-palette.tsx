"use client";

import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@mapleos/ui";

interface CommandItem {
  id: string;
  label: string;
  category: string;
  action: () => void;
}

const COMMANDS: CommandItem[] = [
  { id: "nav-dashboard", label: "command.commands.gotoDashboard", category: "command.categories.navigation", action: () => {} },
  { id: "nav-workflow", label: "command.commands.gotoWorkflow", category: "command.categories.navigation", action: () => {} },
  { id: "nav-agent", label: "command.commands.gotoAgent", category: "command.categories.navigation", action: () => {} },
  { id: "nav-knowledge", label: "command.commands.gotoKnowledge", category: "command.categories.navigation", action: () => {} },
  { id: "nav-chat", label: "command.commands.startChat", category: "command.categories.navigation", action: () => {} },
  { id: "wf-create", label: "command.commands.createWorkflow", category: "command.categories.action", action: () => {} },
  { id: "wf-run", label: "command.commands.runRecentWorkflow", category: "command.categories.action", action: () => {} },
  { id: "kb-search", label: "command.commands.searchKnowledge", category: "command.categories.action", action: () => {} },
  { id: "kb-index", label: "command.commands.uploadToKnowledge", category: "command.categories.action", action: () => {} },
  { id: "agent-register", label: "command.commands.registerAgent", category: "command.categories.action", action: () => {} },
  { id: "scale-stats", label: "command.commands.viewScaleStats", category: "command.categories.action", action: () => {} },
];

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (id: string) => void;
}

export function CommandPalette({ open, onClose, onNavigate }: CommandPaletteProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        if (open) onClose();
        else onNavigate("open-palette");
      }
      if (e.key === "Escape" && open) onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose, onNavigate]);

  if (!open) return null;

  const filtered = COMMANDS.filter((c) =>
    t(c.label).toLowerCase().includes(query.toLowerCase())
  );

  return (
    <div className="fixed inset-0 z-50 bg-black/50 flex items-start justify-center pt-[20vh]" onClick={onClose}>
      <div className="w-[560px] bg-card border rounded-lg shadow-card overflow-hidden" onClick={(e) => e.stopPropagation()}>
        <div className="p-3 border-b">
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("command.searchPlaceholder")}
            className="text-[15px] h-10"
            autoFocus
          />
        </div>
        <div className="max-h-[320px] overflow-y-auto p-2">
          {filtered.length === 0 && (
            <div className="text-center text-muted-foreground text-sm py-4">{t("command.noResults")}</div>
          )}
          {filtered.map((cmd) => (
            <button
              key={cmd.id}
              onClick={() => { onNavigate(cmd.id); onClose(); setQuery(""); }}
              className="w-full text-left px-3 py-2 rounded-md text-sm hover:bg-accent transition-colors flex items-center justify-between"
            >
              <span>{t(cmd.label)}</span>
              <span className="text-xs text-muted-foreground">{t(cmd.category)}</span>
            </button>
          ))}
        </div>
        <div className="p-2 border-t text-xs text-muted-foreground flex items-center gap-4">
          <span>{t("command.openHint")}</span>
          <span>{t("command.closeHint")}</span>
          <span>{t("command.executeHint")}</span>
        </div>
      </div>
    </div>
  );
}