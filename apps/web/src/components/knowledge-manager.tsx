"use client";

import { useState } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Input } from "@mapleos/ui";
import type { KnowledgeBase } from "@/lib/admin-types";

const MOCK_KBS: KnowledgeBase[] = [
  { id: "kb-1", name: "Product Documentation", description: "All product docs and API references", doc_count: 42, status: "ready", created_at: Date.now() - 86400000 * 5 },
  { id: "kb-2", name: "Internal Policies", description: "Company policies and compliance docs", doc_count: 18, status: "indexing", created_at: Date.now() - 3600000 },
  { id: "kb-3", name: "Engineering Wiki", description: "Tech decisions and architecture notes", doc_count: 0, status: "empty", created_at: Date.now() },
];

const kbStatusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  ready: "default",
  indexing: "secondary",
  empty: "outline",
  error: "destructive",
};

export function KnowledgeManager() {
  const [kbs] = useState<KnowledgeBase[]>(MOCK_KBS);
  const [search, setSearch] = useState("");

  const filtered = kbs.filter((kb) =>
    kb.name.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Knowledge Bases</h2>
        <div className="flex gap-2">
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search knowledge bases..."
            className="w-48"
          />
          <Button>Create KB</Button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {filtered.length === 0 && (
          <div className="text-center text-muted-foreground py-8">No knowledge bases found</div>
        )}
        {filtered.map((kb) => (
          <Card key={kb.id} className="hover:shadow-md transition-shadow">
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">{kb.name}</CardTitle>
                <Badge variant={kbStatusVariant[kb.status] ?? "outline"}>{kb.status}</Badge>
              </div>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground">{kb.description}</p>
              <div className="flex items-center justify-between text-sm text-muted-foreground mt-2">
                <span>{kb.doc_count} documents</span>
                <span>{new Date(kb.created_at).toLocaleDateString()}</span>
              </div>
              <div className="flex gap-2 mt-3">
                <Button size="sm" variant="outline">Browse</Button>
                <Button size="sm">Upload Docs</Button>
                <Button size="sm" variant="destructive">Delete</Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}