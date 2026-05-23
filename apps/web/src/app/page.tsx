"use client";

import { useState } from "react";
import { ChatPanel } from "@/components/chat-panel";
import { Button, Badge, Card, CardHeader, CardTitle, CardContent } from "@mapleos/ui";

const NAV_ITEMS = [
  { id: "chat", label: "Chat", icon: "message-square" },
  { id: "workflows", label: "Workflows", icon: "git-branch" },
  { id: "knowledge", label: "Knowledge", icon: "book-open" },
  { id: "agents", label: "Agents", icon: "bot" },
];

export default function Home() {
  const [activeNav, setActiveNav] = useState("chat");

  return (
    <div className="flex h-screen">
      {/* Sidebar */}
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
          <Badge variant="secondary" className="w-full justify-center">
            Server: 7788
          </Badge>
          <div className="flex gap-2">
            <Badge variant="outline" className="flex-1 justify-center">Online</Badge>
            <Badge variant="outline" className="flex-1 justify-center">v0.1.0</Badge>
          </div>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 flex flex-col overflow-hidden">
        {activeNav === "chat" && <ChatPanel />}

        {activeNav !== "chat" && (
          <div className="flex-1 flex items-center justify-center">
            <Card className="max-w-sm">
              <CardHeader>
                <CardTitle>{NAV_ITEMS.find((i) => i.id === activeNav)?.label}</CardTitle>
              </CardHeader>
              <CardContent className="text-muted-foreground text-sm">
                This section is under development. Coming soon in the next release.
              </CardContent>
            </Card>
          </div>
        )}
      </main>
    </div>
  );
}