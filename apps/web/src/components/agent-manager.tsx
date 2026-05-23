"use client";

import { useState } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button } from "@mapleos/ui";
import type { AgentInfo } from "@/lib/admin-types";

const MOCK_AGENTS: AgentInfo[] = [
  { id: "agent-1", name: "GPT-4 Assistant", type: "llm", status: "idle", capabilities: ["chat", "code", "analysis"], last_active: Date.now() },
  { id: "agent-2", name: "Code Reviewer", type: "tool", status: "busy", capabilities: ["review", "lint"], current_task: "Reviewing PR #42", last_active: Date.now() - 60000 },
  { id: "agent-3", name: "Report Writer", type: "llm", status: "offline", capabilities: ["write", "summarize"], last_active: Date.now() - 86400000 },
];

const agentStatusVariant: Record<string, "default" | "secondary" | "outline"> = {
  idle: "default",
  busy: "secondary",
  offline: "outline",
};

export function AgentManager() {
  const [agents] = useState<AgentInfo[]>(MOCK_AGENTS);

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Agents</h2>
        <Button>Register Agent</Button>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {agents.map((agent) => (
          <Card key={agent.id} className="hover:shadow-md transition-shadow">
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">{agent.name}</CardTitle>
                <Badge variant={agentStatusVariant[agent.status]}>{agent.status}</Badge>
              </div>
            </CardHeader>
            <CardContent>
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Badge variant="outline" className="text-xs">{agent.type}</Badge>
                <span>&middot; {agent.id}</span>
              </div>
              <div className="flex gap-1 mt-2">
                {agent.capabilities.map((cap) => (
                  <Badge key={cap} variant="secondary" className="text-xs">{cap}</Badge>
                ))}
              </div>
              {agent.current_task && (
                <p className="text-xs text-muted-foreground mt-2">
                  Task: {agent.current_task}
                </p>
              )}
              <div className="flex items-center justify-between mt-2 text-xs text-muted-foreground">
                <span>Last active: {new Date(agent.last_active).toLocaleTimeString()}</span>
              </div>
              <div className="flex gap-2 mt-3">
                <Button size="sm" variant="outline">Configure</Button>
                {agent.status === "idle" && <Button size="sm">Assign Task</Button>}
                {agent.status === "busy" && (
                  <Button size="sm" variant="destructive">Cancel Task</Button>
                )}
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}