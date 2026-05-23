"use client";

import { useState } from "react";
import { ChatPanel } from "@/components/chat-panel";
import { WorkflowManager } from "@/components/workflow-manager";
import { KnowledgeManager } from "@/components/knowledge-manager";
import { AgentManager } from "@/components/agent-manager";
import { Badge } from "@mapleos/ui";

const NAV_ITEMS = [
  { id: "chat", label: "Chat" },
  { id: "workflows", label: "Workflows" },
  { id: "knowledge", label: "Knowledge" },
  { id: "agents", label: "Agents" },
] as const;

type NavId = (typeof NAV_ITEMS)[number]["id"];

export default function Home() {
  const [activeNav, setActiveNav] = useState<NavId>("chat");

  return (
    <div className="flex h-screen">
      <aside className="w-56 border-r bg-card flex flex-col">
        <div className="p-4 border-b">
          <div className="flex items-center gap-2">
            <svg className="w-6 h-6 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
            </svg>
            <span className="font-semibold text-lg">MapleOS</span>
          </div>
          <p className="text-xs text-muted-foreground mt-1">Agent Collaboration Workstation</p>
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
          <Badge variant="secondary" className="w-full justify-center">Server: 7788</Badge>
          <div className="flex gap-2">
            <Badge variant="outline" className="flex-1 justify-center">Online</Badge>
            <Badge variant="outline" className="flex-1 justify-center">v0.1.0</Badge>
          </div>
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