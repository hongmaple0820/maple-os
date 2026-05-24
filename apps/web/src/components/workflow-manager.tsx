"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

interface WorkflowItem { id: string; name: string; version: number; status: string; created_at: number; updated_at: number }

interface WFNode {
  id: string;
  type: "llm" | "tool" | "condition" | "human_approval" | "trigger";
  label: string;
  model?: string;
  skillId?: string;
  status?: "idle" | "running" | "completed" | "failed" | "waiting";
  dependsOn: string[];
}

const statusLabel: Record<string, string> = { active: "活跃", draft: "草稿", paused: "暂停", failed: "失败" };
const statusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = { active: "default", draft: "secondary", paused: "outline", failed: "destructive" };

const nodeTypeLabel: Record<string, string> = { llm: "LLM 调用", tool: "工具调用", condition: "条件判断", human_approval: "人工审批", trigger: "触发器" };
const nodeTypeColor: Record<string, string> = { llm: "bg-primary/10 border-primary", tool: "bg-warning/10 border-warning", condition: "bg-success/10 border-success", human_approval: "bg-destructive/10 border-destructive", trigger: "bg-muted border-muted" };

const NODE_PALETTE: WFNode[] = [
  { id: "node-llm", type: "llm", label: "LLM 调用", model: "auto", dependsOn: [] },
  { id: "node-tool", type: "tool", label: "工具调用", skillId: "", dependsOn: [] },
  { id: "node-condition", type: "condition", label: "条件判断", dependsOn: [] },
  { id: "node-human", type: "human_approval", label: "人工审批", dependsOn: [] },
  { id: "node-trigger", type: "trigger", label: "触发器", dependsOn: [] },
];

export function WorkflowManager() {
  const [workflows, setWorkflows] = useState<WorkflowItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [createName, setCreateName] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [executing, setExecuting] = useState<string | null>(null);
  const [selectedWf, setSelectedWf] = useState<string | null>(null);
  const [canvasNodes, setCanvasNodes] = useState<WFNode[]>([]);
  const [consoleLogs, setConsoleLogs] = useState<string[]>([]);
  const [selectedNode, setSelectedNode] = useState<WFNode | null>(null);
  const canvasRef = useRef<HTMLDivElement>(null);

  const loadWorkflows = async () => {
    try {
      const result = await rpcCall<{ workflows: WorkflowItem[] }>("workflow.list");
      setWorkflows(result.workflows ?? []);
    } catch { setWorkflows([]); }
    setLoading(false);
  };

  useEffect(() => { loadWorkflows(); }, []);

  const addNode = (template: WFNode) => {
    const node: WFNode = { ...template, id: `${template.type}-${Date.now()}`, status: "idle", dependsOn: [] };
    setCanvasNodes((prev) => [...prev, node]);
    setConsoleLogs((prev) => [...prev, `[编辑器] 添加节点: ${node.label} (${node.id})`]);
  };

  const removeNode = (nodeId: string) => {
    setCanvasNodes((prev) => prev.filter((n) => n.id !== nodeId));
    setSelectedNode(null);
    setConsoleLogs((prev) => [...prev, `[编辑器] 移除节点: ${nodeId}`]);
  };

  const handleCreate = async () => {
    if (!createName.trim()) return;
    try {
      const yamlNodes = canvasNodes.map((n) => ({
        id: n.id, type: n.type, label: n.label,
        ...(n.type === "llm" && { model_route: n.model ?? "auto" }),
        ...(n.type === "tool" && { skill_id: n.skillId ?? "" }),
        depends_on: n.dependsOn,
      }));
      const yaml = JSON.stringify({ name: createName.trim(), nodes: yamlNodes, trigger: { type: "webhook", path: `/hook/${createName}` } });
      await rpcCall("workflow.create", { name: createName.trim(), yaml_content: yaml });
      setShowCreate(false); setCreateName(""); setCanvasNodes([]); setConsoleLogs([]);
      await loadWorkflows();
    } catch (err) { alert(`创建失败: ${(err as Error).message}`); }
  };

  const handleExecute = async (workflowId: string) => {
    setExecuting(workflowId);
    setConsoleLogs((prev) => [...prev, `[执行] 开始执行工作流 ${workflowId}`]);
    try {
      const result = await rpcCall<{ exec_id: string; status: string; result?: string; error?: string }>("workflow.execute", { workflow_id: workflowId });
      if (result.error) {
        setConsoleLogs((prev) => [...prev, `[执行] 失败: ${result.error}`]);
      } else {
        setConsoleLogs((prev) => [...prev, `[执行] 成功! exec_id=${result.exec_id}, 状态=${result.status}`]);
        if (result.result) setConsoleLogs((prev) => [...prev, `[结果] ${result.result}`]);
      }
    } catch (err) {
      setConsoleLogs((prev) => [...prev, `[执行] 出错: ${(err as Error).message}`]);
    } finally { setExecuting(null); }
  };

  const filtered = workflows.filter((wf) => wf.name.toLowerCase().includes(search.toLowerCase()));
  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">工作流编辑器</h2>
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="搜索..." className="w-36 h-7 text-xs" />
        </div>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => setShowCreate(true)}>新建</Button>
          {selectedWf && <Button size="sm" variant="destructive" onClick={() => setSelectedWf(null)}>关闭编辑</Button>}
        </div>
      </div>

      {showCreate && (
        <div className="h-9 border-b bg-muted/50 flex items-center gap-2 px-4">
          <Input value={createName} onChange={(e) => setCreateName(e.target.value)} placeholder="工作流名称..." className="w-36 h-7 text-xs" />
          <Button size="sm" onClick={handleCreate} disabled={!createName.trim()}>创建</Button>
          <Button size="sm" variant="ghost" onClick={() => { setShowCreate(false); setCreateName(""); }}>取消</Button>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar: Node Library + Workflow List */}
        <div className="w-52 border-r bg-card flex flex-col">
          <div className="p-2 border-b">
            <div className="text-[11px] text-muted-foreground mb-1.5">节点库</div>
            <div className="space-y-1">
              {NODE_PALETTE.map((template) => (
                <button
                  key={template.id}
                  onClick={() => addNode(template)}
                  className="w-full text-left px-2 py-1.5 rounded text-xs hover:bg-accent transition-colors"
                >
                  {nodeTypeLabel[template.type]}
                </button>
              ))}
            </div>
          </div>
          <div className="p-2 border-b">
            <div className="text-[11px] text-muted-foreground mb-1.5">已保存工作流</div>
            <div className="space-y-1">
              {filtered.map((wf) => (
                <button
                  key={wf.id}
                  onClick={() => { setSelectedWf(wf.id); setConsoleLogs((prev) => [...prev, `[导航] 选择工作流: ${wf.name}`]); }}
                  className={`w-full text-left px-2 py-1.5 rounded text-xs transition-colors ${
                    selectedWf === wf.id ? "bg-primary/10 text-primary font-medium" : "hover:bg-accent"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span>{wf.name}</span>
                    <Badge variant={statusVariant[wf.status] ?? "outline"} className="text-[10px] px-1">{statusLabel[wf.status] ?? wf.status}</Badge>
                  </div>
                </button>
              ))}
              {filtered.length === 0 && <div className="text-xs text-muted-foreground py-2">暂无工作流</div>}
            </div>
          </div>
          {selectedWf && (
            <div className="p-2 space-y-1">
              <Button size="sm" className="w-full" onClick={() => handleExecute(selectedWf)} disabled={executing === selectedWf}>
                {executing === selectedWf ? "执行中..." : "运行此工作流"}
              </Button>
            </div>
          )}
        </div>

        {/* Canvas */}
        <div ref={canvasRef} className="flex-1 bg-background overflow-auto relative p-4">
          {canvasNodes.length === 0 && !selectedWf && (
            <div className="absolute inset-0 flex items-center justify-center text-muted-foreground text-sm">
              从左侧节点库拖入节点，或选择已保存工作流查看
            </div>
          )}
          <div className="space-y-3">
            {canvasNodes.map((node, idx) => (
              <div
                key={node.id}
                onClick={() => setSelectedNode(node)}
                className={`cursor-pointer rounded-md p-3 border shadow-card transition-all ${
                  selectedNode?.id === node.id ? "ring-2 ring-primary" : ""
                } ${nodeTypeColor[node.type] ?? "bg-card border"}`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="text-[11px] font-mono text-muted-foreground">#${idx + 1}</span>
                    <span className="text-[13px] font-medium">{node.label}</span>
                    <Badge variant="outline" className="text-[10px]">{nodeTypeLabel[node.type]}</Badge>
                  </div>
                  <button onClick={(e) => { e.stopPropagation(); removeNode(node.id); }} className="text-xs text-muted-foreground hover:text-destructive">&times;</button>
                </div>
                {node.type === "llm" && <div className="text-[11px] text-muted-foreground mt-1">模型: {node.model ?? "auto"}</div>}
                {node.type === "tool" && <div className="text-[11px] text-muted-foreground mt-1">技能: {node.skillId ?? "未指定"}</div>}
                {node.dependsOn.length > 0 && (
                  <div className="text-[11px] text-muted-foreground mt-1">依赖: {node.dependsOn.join(", ")}</div>
                )}
                {node.status && node.status !== "idle" && (
                  <Badge variant={node.status === "running" ? "secondary" : node.status === "completed" ? "default" : node.status === "failed" ? "destructive" : "outline"} className="text-[10px] mt-1">
                    {node.status === "running" ? "运行中" : node.status === "completed" ? "已完成" : node.status === "failed" ? "失败" : "等待审批"}
                  </Badge>
                )}
                {idx < canvasNodes.length - 1 && (
                  <div className="flex justify-center my-2">
                    <svg className="w-4 h-4 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M12 5v14M5 12l7 7 7-7" /></svg>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Right Panel: Console + Config */}
        <div className="w-48 border-l bg-card flex flex-col">
          {selectedNode && (
            <div className="p-3 border-b">
              <div className="text-[11px] text-muted-foreground mb-1">节点配置</div>
              <div className="text-[13px] font-medium">{selectedNode.label}</div>
              <Badge variant="outline" className="text-[10px] mt-1">{nodeTypeLabel[selectedNode.type]}</Badge>
              {selectedNode.type === "llm" && (
                <div className="mt-2 space-y-1">
                  <label className="text-[11px] text-muted-foreground">模型路由</label>
                  <Input defaultValue={selectedNode.model ?? "auto"} className="h-7 text-xs" />
                </div>
              )}
              {selectedNode.type === "tool" && (
                <div className="mt-2 space-y-1">
                  <label className="text-[11px] text-muted-foreground">技能 ID</label>
                  <Input defaultValue={selectedNode.skillId ?? ""} className="h-7 text-xs" placeholder="skill_id" />
                </div>
              )}
            </div>
          )}
          {!selectedNode && canvasNodes.length > 0 && (
            <div className="p-3 border-b text-[11px] text-muted-foreground">点击节点查看配置</div>
          )}
          <div className="flex-1 overflow-y-auto p-3">
            <div className="text-[11px] text-muted-foreground mb-1">控制台</div>
            <div className="space-y-0.5">
              {consoleLogs.map((log, i) => (
                <div key={i} className="text-[11px] font-mono text-muted-foreground leading-tight">{log}</div>
              ))}
              {consoleLogs.length === 0 && <div className="text-[11px] text-muted-foreground">暂无日志</div>}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}