"use client";

import { useState, useEffect } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

interface WorkflowItem {
  id: string;
  name: string;
  version: number;
  status: string;
  created_at: number;
  updated_at: number;
}

const statusLabel: Record<string, string> = {
  active: "活跃",
  draft: "草稿",
  paused: "暂停",
  failed: "失败",
};

const statusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  active: "default",
  draft: "secondary",
  paused: "outline",
  failed: "destructive",
};

export function WorkflowManager() {
  const [workflows, setWorkflows] = useState<WorkflowItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [createName, setCreateName] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [executing, setExecuting] = useState<string | null>(null);

  const loadWorkflows = async () => {
    try {
      const result = await rpcCall<{ workflows: WorkflowItem[] }>("workflow.list");
      setWorkflows(result.workflows ?? []);
    } catch {
      setWorkflows([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadWorkflows(); }, []);

  const handleCreate = async () => {
    if (!createName.trim()) return;
    try {
      await rpcCall("workflow.create", { name: createName.trim(), yaml_content: `name: ${createName}\ntrigger:\n  type: webhook\n  path: /hook/${createName}\nnodes: []` });
      setShowCreate(false);
      setCreateName("");
      await loadWorkflows();
    } catch (err) {
      alert(`创建失败: ${(err as Error).message}`);
    }
  };

  const handleExecute = async (workflowId: string) => {
    setExecuting(workflowId);
    try {
      const result = await rpcCall<{ exec_id: string; status: string; result?: string; error?: string }>("workflow.execute", { workflow_id: workflowId });
      if (result.error) {
        alert(`执行失败: ${result.error}`);
      } else {
        alert(`执行成功! exec_id: ${result.exec_id}, 状态: ${result.status}`);
      }
    } catch (err) {
      alert(`执行出错: ${(err as Error).message}`);
    } finally {
      setExecuting(null);
    }
  };

  const filtered = workflows.filter((wf) => wf.name.toLowerCase().includes(search.toLowerCase()));

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">工作流管理</h2>
        <div className="flex gap-2">
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索工作流..."
            className="w-48"
          />
          <Button onClick={() => setShowCreate(true)}>新建工作流</Button>
        </div>
      </div>

      {showCreate && (
        <div className="border-b p-4 bg-muted/50 flex items-center gap-2">
          <Input
            value={createName}
            onChange={(e) => setCreateName(e.target.value)}
            placeholder="工作流名称..."
            className="w-48"
          />
          <Button onClick={handleCreate} disabled={!createName.trim()}>确认创建</Button>
          <Button variant="outline" onClick={() => { setShowCreate(false); setCreateName(""); }}>取消</Button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {filtered.length === 0 && (
          <div className="text-center text-muted-foreground py-8">暂无工作流数据</div>
        )}
        {filtered.map((wf) => (
          <Card key={wf.id} className="hover:shadow-md transition-shadow">
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">{wf.name}</CardTitle>
                <Badge variant={statusVariant[wf.status] ?? "outline"}>
                  {statusLabel[wf.status] ?? wf.status}
                </Badge>
              </div>
            </CardHeader>
            <CardContent>
              <div className="flex items-center justify-between text-sm text-muted-foreground">
                <span>版本 v{wf.version} &middot; {wf.id}</span>
                <span>更新于 {new Date(wf.updated_at * 1000).toLocaleDateString("zh-CN")}</span>
              </div>
              <div className="flex gap-2 mt-3">
                <Button size="sm" variant="outline">编辑</Button>
                <Button
                  size="sm"
                  onClick={() => handleExecute(wf.id)}
                  disabled={executing === wf.id}
                >
                  {executing === wf.id ? "执行中..." : "运行"}
                </Button>
                <Button size="sm" variant="destructive">删除</Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}