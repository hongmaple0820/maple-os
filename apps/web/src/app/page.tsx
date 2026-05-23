"use client";

import { useState, useEffect } from "react";
import { ChatPanel } from "@/components/chat-panel";
import { WorkflowManager } from "@/components/workflow-manager";
import { KnowledgeManager } from "@/components/knowledge-manager";
import { AgentManager } from "@/components/agent-manager";
import { Badge, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

const NAV_ITEMS = [
  { id: "chat", label: "对话" },
  { id: "workflows", label: "工作流" },
  { id: "knowledge", label: "知识库" },
  { id: "agents", label: "Agent" },
] as const;

type NavId = (typeof NAV_ITEMS)[number]["id"];

interface SystemInfo {
  version: string;
  uptime_secs: number;
  agents_count: number;
  workflows_count: number;
  tasks_count: number;
}

export default function Home() {
  const [activeNav, setActiveNav] = useState<NavId>("chat");
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);
  const [serverOnline, setServerOnline] = useState(false);

  useEffect(() => {
    const poll = async () => {
      try {
        const info = await rpcCall<SystemInfo>("system.info");
        setSysInfo(info);
        setServerOnline(true);
      } catch {
        setServerOnline(false);
      }
    };
    poll();
    const interval = setInterval(poll, 10000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="flex h-screen">
      <aside className="w-56 border-r bg-card flex flex-col">
        <div className="p-4 border-b">
          <div className="flex items-center gap-2">
            <svg className="w-6 h-6 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
            </svg>
            <span className="font-semibold text-lg">枫信工作站</span>
          </div>
          <p className="text-xs text-muted-foreground mt-1">Agent 协作工作站操作系统</p>
        </div>

        <nav className="flex-1 p-2 space-y-1">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              onClick={() => setActiveNav(item.id)}
              className={`w-full text-left px-3 py-2 rounded-md text-sm transition-colors ${
                activeNav === item.id
                  ? "bg-accent text-accent-foreground font-medium"
                  : "text-muted-foreground hover:bg-accent/50"
              }`}
            >
              {item.label}
            </button>
          ))}
        </nav>

        <div className="p-4 border-t space-y-2">
          <Badge variant="secondary" className="w-full justify-center">端口 7788</Badge>
          <div className="flex gap-2">
            <Badge variant={serverOnline ? "default" : "destructive"} className="flex-1 justify-center">
              {serverOnline ? "已连接" : "离线"}
            </Badge>
            {sysInfo && (
              <Badge variant="outline" className="flex-1 justify-center">v{sysInfo.version ?? "0.1.0"}</Badge>
            )}
          </div>
          {sysInfo && (
            <div className="text-xs text-muted-foreground text-center">
              {sysInfo.agents_count} Agent / {sysInfo.workflows_count} 工作流 / {sysInfo.tasks_count} 任务
            </div>
          )}
        </div>
      </aside>

      <main className="flex-1 flex flex-col overflow-hidden">
        {activeNav === "chat" && <ChatPanel />}
        {activeNav === "workflows" && <WorkflowManager />}
        {activeNav === "knowledge" && <KnowledgeManager />}
        {activeNav === "agents" && <AgentManager />}
      </main>
    </div>
  );
}