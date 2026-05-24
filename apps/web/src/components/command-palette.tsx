"use client";

import { useState, useEffect } from "react";
import { Input } from "@mapleos/ui";

interface CommandItem {
  id: string;
  label: string;
  category: string;
  action: () => void;
}

const COMMANDS: CommandItem[] = [
  { id: "nav-dashboard", label: "前往工作台", category: "导航", action: () => {} },
  { id: "nav-workflow", label: "前往工作流", category: "导航", action: () => {} },
  { id: "nav-agent", label: "前往 Agent 中心", category: "导航", action: () => {} },
  { id: "nav-knowledge", label: "前往知识库", category: "导航", action: () => {} },
  { id: "nav-chat", label: "开始对话", category: "导航", action: () => {} },
  { id: "wf-create", label: "新建工作流", category: "操作", action: () => {} },
  { id: "wf-run", label: "运行最近工作流", category: "操作", action: () => {} },
  { id: "kb-search", label: "搜索知识库", category: "操作", action: () => {} },
  { id: "kb-index", label: "上传文档到知识库", category: "操作", action: () => {} },
  { id: "agent-register", label: "注册新 Agent", category: "操作", action: () => {} },
  { id: "scale-stats", label: "查看 SCALE 统计", category: "操作", action: () => {} },
];

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (id: string) => void;
}

export function CommandPalette({ open, onClose, onNavigate }: CommandPaletteProps) {
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
    c.label.toLowerCase().includes(query.toLowerCase())
  );

  return (
    <div className="fixed inset-0 z-50 bg-black/50 flex items-start justify-center pt-[20vh]" onClick={onClose}>
      <div className="w-[560px] bg-card border rounded-lg shadow-card overflow-hidden" onClick={(e) => e.stopPropagation()}>
        <div className="p-3 border-b">
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="输入命令或搜索..."
            className="text-[15px] h-10"
            autoFocus
          />
        </div>
        <div className="max-h-[320px] overflow-y-auto p-2">
          {filtered.length === 0 && (
            <div className="text-center text-muted-foreground text-sm py-4">未找到匹配命令</div>
          )}
          {filtered.map((cmd) => (
            <button
              key={cmd.id}
              onClick={() => { onNavigate(cmd.id); onClose(); setQuery(""); }}
              className="w-full text-left px-3 py-2 rounded-md text-sm hover:bg-accent transition-colors flex items-center justify-between"
            >
              <span>{cmd.label}</span>
              <span className="text-xs text-muted-foreground">{cmd.category}</span>
            </button>
          ))}
        </div>
        <div className="p-2 border-t text-xs text-muted-foreground flex items-center gap-4">
          <span>&#8984;K 打开</span>
          <span>ESC 关闭</span>
          <span>Enter 执行</span>
        </div>
      </div>
    </div>
  );
}