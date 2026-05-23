"use client";

import { useState } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Input } from "@mapleos/ui";
import type { WorkflowItem } from "@/lib/admin-types";

const MOCK_WORKFLOWS: WorkflowItem[] = [
  { id: "wf-1", name: "Daily Report Generator", version: 3, status: "active", created_at: Date.now() - 86400000 * 7, updated_at: Date.now() - 86400000 },
  { id: "wf-2", name: "Customer Support Escalation", version: 2, status: "draft", created_at: Date.now() - 86400000 * 3, updated_at: Date.now() - 3600000 },
  { id: "wf-3", name: "Code Review Pipeline", version: 1, status: "active", created_at: Date.now() - 86400000, updated_at: Date.now() },
];

const statusVariant: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  active: "default",
  draft: "secondary",
  paused: "outline",
  failed: "destructive",
};

export function WorkflowManager() {
  const [workflows] = useState<WorkflowItem[]>(MOCK_WORKFLOWS);
  const [search, setSearch] = useState("");

  const filtered = workflows.filter((wf) =>
    wf.name.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Workflows</h2>
        <div className="flex gap-2">
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search workflows..."
            className="w-48"
          />
          <Button>Create Workflow</Button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {filtered.length === 0 && (
          <div className="text-center text-muted-foreground py-8">
            No workflows found
          </div>
        )}
        {filtered.map((wf) => (
          <Card key={wf.id} className="hover:shadow-md transition-shadow">
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">{wf.name}</CardTitle>
                <Badge variant={statusVariant[wf.status] ?? "outline"}>
                  {wf.status}
                </Badge>
              </div>
            </CardHeader>
            <CardContent>
              <div className="flex items-center justify-between text-sm text-muted-foreground">
                <span>v{wf.version} &middot; {wf.id}</span>
                <span>{new Date(wf.updated_at).toLocaleDateString()}</span>
              </div>
              <div className="flex gap-2 mt-3">
                <Button size="sm" variant="outline">Edit</Button>
                <Button size="sm">Run</Button>
                <Button size="sm" variant="destructive">Delete</Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}