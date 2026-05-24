"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall, mapleApi } from "@/lib/api";

interface WorkflowItem { id: string; name: string; version: number; status: string; created_at: number; updated_at: number }

interface WFNode {
  id: string;
  type: "llm" | "tool" | "condition" | "human_approval" | "trigger";
  label: string;
  model?: string;
  skillId?: string;
  status?: "idle" | "running" | "completed" | "failed" | "waiting";
  dependsOn: string[];
  x: number;
  y: number;
}

interface WFEdge {
  from: string;
  to: string;
}

const statusLabel: Record<string, string> = { active: "活跃", draft: "草稿", paused: "暂停", failed: "失败" };
const statusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = { active: "default", draft: "secondary", paused: "outline", failed: "destructive" };

const nodeTypeLabel: Record<string, string> = { llm: "LLM 调用", tool: "工具调用", condition: "条件判断", human_approval: "人工审批", trigger: "触发器" };
const nodeTypeIcon: Record<string, string> = { llm: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 14a4 4 0 1 1 4-4 4 4 0 0 1-4 4z", tool: "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z", condition: "M16 3h5v5M4 20h5v5M21 3l-7 7M3 20l7-7", human_approval: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 8 0 4 4 0 0 0-8 0M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75", trigger: "M13 2L3 14h9l-1 8 10-12h-9l1-8z" };
const nodeTypeColor: Record<string, { bg: string; border: string; accent: string }> = {
  llm: { bg: "fill-primary/10", border: "stroke-primary", accent: "text-primary" },
  tool: { bg: "fill-warning/10", border: "stroke-warning", accent: "text-warning" },
  condition: { bg: "fill-success/10", border: "stroke-success", accent: "text-success" },
  human_approval: { bg: "fill-destructive/10", border: "stroke-destructive", accent: "text-destructive" },
  trigger: { bg: "fill-muted", border: "stroke-muted-foreground", accent: "text-muted-foreground" },
};

const NODE_PALETTE: WFNode[] = [
  { id: "node-llm", type: "llm", label: "LLM 调用", model: "auto", dependsOn: [], x: 0, y: 0 },
  { id: "node-tool", type: "tool", label: "工具调用", skillId: "", dependsOn: [], x: 0, y: 0 },
  { id: "node-condition", type: "condition", label: "条件判断", dependsOn: [], x: 0, y: 0 },
  { id: "node-human", type: "human_approval", label: "人工审批", dependsOn: [], x: 0, y: 0 },
  { id: "node-trigger", type: "trigger", label: "触发器", dependsOn: [], x: 0, y: 0 },
];

const NODE_W = 160;
const NODE_H = 72;

export function WorkflowManager() {
  const [workflows, setWorkflows] = useState<WorkflowItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [createName, setCreateName] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [executing, setExecuting] = useState<string | null>(null);
  const [selectedWf, setSelectedWf] = useState<string | null>(null);
  const [canvasNodes, setCanvasNodes] = useState<WFNode[]>([]);
  const [edges, setEdges] = useState<WFEdge[]>([]);
  const [consoleLogs, setConsoleLogs] = useState<string[]>([]);
  const [rightTab, setRightTab] = useState<"console" | "history" | "config">("console");
  const [execHistory, setExecHistory] = useState<{ id: string; status: string; started_at: number; completed_at: number | null }[]>([]);
  const [selectedNode, setSelectedNode] = useState<WFNode | null>(null);
  const [connectingFrom, setConnectingFrom] = useState<string | null>(null);
  const [draggingNode, setDraggingNode] = useState<string | null>(null);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isPanning, setIsPanning] = useState(false);
  const [panStart, setPanStart] = useState({ x: 0, y: 0 });
  const canvasRef = useRef<HTMLDivElement>(null);

  const loadWorkflows = async () => {
    try {
      const result = await rpcCall<{ workflows: WorkflowItem[] }>("workflow.list");
      setWorkflows(result.workflows ?? []);
    } catch { setWorkflows([]); }
    setLoading(false);
  };

  useEffect(() => { loadWorkflows(); }, []);

  const nextPos = useCallback(() => {
    const baseX = 80 + pan.x;
    const baseY = 40 + pan.y;
    const offsetY = canvasNodes.length * (NODE_H + 40);
    return { x: baseX, y: baseY + offsetY };
  }, [canvasNodes, pan]);

  const addNode = (template: WFNode) => {
    const pos = nextPos();
    const node: WFNode = { ...template, id: `${template.type}-${Date.now()}`, status: "idle", dependsOn: [], x: pos.x, y: pos.y };
    setCanvasNodes((prev) => [...prev, node]);
    setConsoleLogs((prev) => [...prev, `[编辑器] 添加节点: ${node.label} (${node.id})`]);
    if (canvasNodes.length > 0) {
      const lastNode = canvasNodes[canvasNodes.length - 1];
      setEdges((prev) => [...prev, { from: lastNode.id, to: node.id }]);
      setConsoleLogs((prev) => [...prev, `[连线] ${lastNode.label} → ${node.label}`]);
    }
  };

  const removeNode = (nodeId: string) => {
    setCanvasNodes((prev) => prev.filter((n) => n.id !== nodeId));
    setEdges((prev) => prev.filter((e) => e.from !== nodeId && e.to !== nodeId));
    if (selectedNode?.id === nodeId) setSelectedNode(null);
    if (connectingFrom === nodeId) setConnectingFrom(null);
    setConsoleLogs((prev) => [...prev, `[编辑器] 移除节点: ${nodeId}`]);
  };

  const toggleConnection = (fromId: string, toId: string) => {
    const exists = edges.some((e) => e.from === fromId && e.to === toId);
    if (exists) {
      setEdges((prev) => prev.filter((e) => !(e.from === fromId && e.to === toId)));
      setConsoleLogs((prev) => [...prev, `[连线] 移除连接`]);
    } else {
      setEdges((prev) => [...prev, { from: fromId, to: toId }]);
      setConsoleLogs((prev) => [...prev, `[连线] 添加连接`]);
    }
  };

  const handleNodeMouseDown = (e: React.MouseEvent, nodeId: string) => {
    e.stopPropagation();
    if (connectingFrom) {
      if (connectingFrom !== nodeId) {
        toggleConnection(connectingFrom, nodeId);
      }
      setConnectingFrom(null);
      return;
    }
    const node = canvasNodes.find((n) => n.id === nodeId);
    if (!node) return;
    setSelectedNode(node);
    setDraggingNode(nodeId);
    setDragOffset({ x: e.clientX - node.x, y: e.clientY - node.y });
  };

  const handleCanvasMouseMove = useCallback((e: React.MouseEvent) => {
    if (draggingNode) {
      const newX = e.clientX - dragOffset.x;
      const newY = e.clientY - dragOffset.y;
      setCanvasNodes((prev) => prev.map((n) => n.id === draggingNode ? { ...n, x: newX, y: newY } : n));
    }
    if (isPanning) {
      const dx = e.clientX - panStart.x;
      const dy = e.clientY - panStart.y;
      setPan((prev) => ({ x: prev.x + dx, y: prev.y + dy }));
      setPanStart({ x: e.clientX, y: e.clientY });
      setCanvasNodes((prev) => prev.map((n) => ({ ...n, x: n.x + dx, y: n.y + dy })));
    }
  }, [draggingNode, dragOffset, isPanning, panStart]);

  const handleCanvasMouseUp = useCallback(() => {
    setDraggingNode(null);
    setIsPanning(false);
  }, []);

  const handleCanvasMouseDown = (e: React.MouseEvent) => {
    if (e.button === 1 || (e.button === 0 && e.altKey)) {
      setIsPanning(true);
      setPanStart({ x: e.clientX, y: e.clientY });
      return;
    }
    if (connectingFrom) {
      setConnectingFrom(null);
      return;
    }
    setSelectedNode(null);
  };

  const startConnecting = (nodeId: string) => {
    setConnectingFrom(nodeId);
    setConsoleLogs((prev) => [...prev, `[连线] 点击目标节点完成连接`]);
  };

  const handleCreate = async () => {
    if (!createName.trim()) return;
    try {
      const yamlNodes = canvasNodes.map((n) => ({
        id: n.id, type: n.type, label: n.label,
        ...(n.type === "llm" && { model_route: n.model ?? "auto" }),
        ...(n.type === "tool" && { skill_id: n.skillId ?? "" }),
        depends_on: edges.filter((e) => e.to === n.id).map((e) => e.from),
      }));
      const yaml = JSON.stringify({ name: createName.trim(), nodes: yamlNodes, trigger: { type: "webhook", path: `/hook/${createName}` } });
      await rpcCall("workflow.create", { name: createName.trim(), yaml_content: yaml });
      setShowCreate(false); setCreateName(""); setCanvasNodes([]); setEdges([]); setConsoleLogs([]);
      await loadWorkflows();
    } catch (err) { alert(`创建失败: ${(err as Error).message}`); }
  };

  const handleExecute = async (workflowId: string) => {
    setExecuting(workflowId);
    setCanvasNodes((prev) => prev.map((n) => ({ ...n, status: "idle" as WFNode["status"] })));
    setConsoleLogs((prev) => [...prev, `[执行] 开始执行工作流 ${workflowId}`]);
    try {
      const result = await rpcCall<{ exec_id: string; status: string; result?: string; error?: string }>("workflow.execute", { workflow_id: workflowId });
      if (result.error) {
        setConsoleLogs((prev) => [...prev, `[执行] 失败: ${result.error}`]);
      } else {
        setConsoleLogs((prev) => [...prev, `[执行] 已提交! exec_id=${result.exec_id}, 状态=${result.status}`]);
      }
    } catch (err) {
      setConsoleLogs((prev) => [...prev, `[执行] 出错: ${(err as Error).message}`]);
    } finally { setExecuting(null); }
  };

  useEffect(() => {
    let es: EventSource | null = null;
    try {
      es = new EventSource("/api/maple/api/events");
      es.addEventListener("node.started", (e) => {
        try {
          const d = JSON.parse(e.data);
          setCanvasNodes((prev) => prev.map((n) => n.id === d.node_id ? { ...n, status: "running" } : n));
          setConsoleLogs((prev) => [...prev, `[SSE] 节点开始: ${d.node_id}`]);
        } catch { /* ignore */ }
      });
      es.addEventListener("node.completed", (e) => {
        try {
          const d = JSON.parse(e.data);
          setCanvasNodes((prev) => prev.map((n) => n.id === d.node_id ? { ...n, status: "completed" } : n));
          setConsoleLogs((prev) => [...prev, `[SSE] 节点完成: ${d.node_id}`]);
        } catch { /* ignore */ }
      });
      es.addEventListener("node.failed", (e) => {
        try {
          const d = JSON.parse(e.data);
          setCanvasNodes((prev) => prev.map((n) => n.id === d.node_id ? { ...n, status: "failed" } : n));
          setConsoleLogs((prev) => [...prev, `[SSE] 节点失败: ${d.node_id} - ${d.error}`]);
        } catch { /* ignore */ }
      });
      es.addEventListener("workflow.completed", (e) => {
        try {
          const d = JSON.parse(e.data);
          setConsoleLogs((prev) => [...prev, `[SSE] 工作流完成: ${d.workflow_id}`]);
        } catch { /* ignore */ }
      });
      es.addEventListener("workflow.failed", (e) => {
        try {
          const d = JSON.parse(e.data);
          setConsoleLogs((prev) => [...prev, `[SSE] 工作流失败: ${d.workflow_id} - ${d.error}`]);
        } catch { /* ignore */ }
      });
    } catch { /* EventSource unavailable */ }
    return () => { es?.close(); };
  }, []);

  const loadExecHistory = async (wfId: string) => {
    try {
      const res = await mapleApi<{ executions: { id: string; status: string; started_at: number; completed_at: number | null }[] }>("/api/maple/api/workflows/" + wfId + "/executions");
      setExecHistory(res.executions ?? []);
      setRightTab("history");
    } catch { setExecHistory([]); }
  };

  const filtered = workflows.filter((wf) => wf.name.toLowerCase().includes(search.toLowerCase()));
  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  const renderEdge = (edge: WFEdge) => {
    const fromNode = canvasNodes.find((n) => n.id === edge.from);
    const toNode = canvasNodes.find((n) => n.id === edge.to);
    if (!fromNode || !toNode) return null;
    const x1 = fromNode.x + NODE_W / 2;
    const y1 = fromNode.y + NODE_H;
    const x2 = toNode.x + NODE_W / 2;
    const y2 = toNode.y;
    const midY = (y1 + y2) / 2;
    return (
      <path
        key={`${edge.from}-${edge.to}`}
        d={`M${x1},${y1} C${x1},${midY} ${x2},${midY} ${x2},${y2}`}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        className="text-muted-foreground/50"
        markerEnd="url(#arrowhead)"
      />
    );
  };

  const statusBorder: Record<string, string> = { idle: "border", running: "border-2 border-warning animate-pulse", completed: "border-2 border-success", failed: "border-2 border-destructive", waiting: "border-2 border-muted-foreground" };
  const nodeColors = nodeTypeColor;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">工作流编辑器</h2>
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="搜索..." className="w-36 h-7 text-xs" />
          {connectingFrom && <Badge variant="secondary" className="text-[11px]">连线模式 — 点击目标节点</Badge>}
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
        <div className="w-52 border-r bg-card flex flex-col">
          <div className="p-2 border-b">
            <div className="text-[11px] text-muted-foreground mb-1.5">节点库 — 点击添加</div>
            <div className="space-y-1">
              {NODE_PALETTE.map((template) => (
                <button
                  key={template.id}
                  onClick={() => addNode(template)}
                  className="w-full text-left px-2 py-1.5 rounded text-xs hover:bg-accent transition-colors flex items-center gap-1.5"
                >
                  <svg className={`w-3.5 h-3.5 ${nodeColors[template.type]?.accent}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d={nodeTypeIcon[template.type]} /></svg>
                  {nodeTypeLabel[template.type]}
                </button>
              ))}
            </div>
          </div>
          <div className="p-2 border-b overflow-y-auto">
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
              <Button size="sm" variant="outline" className="w-full" onClick={() => loadExecHistory(selectedWf)}>执行历史</Button>
            </div>
          )}
          <div className="p-2 text-[10px] text-muted-foreground">
            <div>Alt+拖拽 = 平移画布</div>
            <div>点击节点出口 = 连线模式</div>
          </div>
        </div>

        <div
          ref={canvasRef}
          className="flex-1 bg-background overflow-hidden relative"
          onMouseDown={handleCanvasMouseDown}
          onMouseMove={handleCanvasMouseMove}
          onMouseUp={handleCanvasMouseUp}
          style={{ cursor: isPanning ? "grabbing" : draggingNode ? "grabbing" : connectingFrom ? "crosshair" : "default" }}
        >
          <svg className="absolute inset-0 w-full h-full" style={{ pointerEvents: "none" }}>
            <defs>
              <pattern id="grid" width="20" height="20" patternUnits="userSpaceOnUse">
                <path d="M 20 0 L 0 0 0 20" fill="none" stroke="currentColor" strokeWidth="0.3" className="text-muted-foreground/20" />
              </pattern>
              <marker id="arrowhead" markerWidth="8" markerHeight="6" refX="8" refY="3" orient="auto">
                <polygon points="0 0, 8 3, 0 6" fill="currentColor" className="text-muted-foreground/50" />
              </marker>
            </defs>
            <rect width="100%" height="100%" fill="url(#grid)" />
            {edges.map(renderEdge)}
          </svg>

          {canvasNodes.length === 0 && !selectedWf && (
            <div className="absolute inset-0 flex items-center justify-center text-muted-foreground text-sm pointer-events-none">
              点击左侧节点库添加节点，开始编排工作流
            </div>
          )}

          {canvasNodes.map((node) => {
            const colors = nodeColors[node.type];
            return (
              <div
                key={node.id}
                onMouseDown={(e) => handleNodeMouseDown(e, node.id)}
                onClick={(e) => e.stopPropagation()}
                className={`absolute group cursor-grab active:cursor-grabbing select-none ${
                  selectedNode?.id === node.id ? "ring-2 ring-primary z-10" : ""
                } ${connectingFrom === node.id ? "ring-2 ring-warning z-10" : ""}`}
                style={{ left: node.x, top: node.y, width: NODE_W }}
              >
                <div className={`rounded-lg shadow-card transition-shadow hover:shadow-lg p-3 ${
                  statusBorder[node.status ?? "idle"] ?? "border"
                }`}>
                  <div className="flex items-center gap-1.5 mb-1">
                    <svg className={`w-4 h-4 ${colors?.accent ?? "text-muted-foreground"}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d={nodeTypeIcon[node.type]} /></svg>
                    <span className="text-[13px] font-medium truncate">{node.label}</span>
                  </div>
                  <div className="flex items-center gap-1">
                    <Badge variant="outline" className="text-[10px]">{nodeTypeLabel[node.type]}</Badge>
                    {node.status && node.status !== "idle" && (
                      <Badge variant={node.status === "running" ? "secondary" : node.status === "completed" ? "default" : node.status === "failed" ? "destructive" : "outline"} className="text-[10px]">
                        {node.status === "running" ? "运行中" : node.status === "completed" ? "已完成" : node.status === "failed" ? "失败" : "等待"}
                      </Badge>
                    )}
                  </div>
                  {node.type === "llm" && <div className="text-[11px] text-muted-foreground mt-1 font-mono">model: {node.model ?? "auto"}</div>}
                  {node.type === "tool" && <div className="text-[11px] text-muted-foreground mt-1 font-mono">skill: {node.skillId ?? "-"}</div>}
                </div>

                <div className="absolute -bottom-2 left-1/2 -translate-x-1/2 w-4 h-4 rounded-full border-2 border-card bg-muted-foreground/30 hover:bg-primary hover:border-primary cursor-crosshair transition-colors z-20 opacity-0 group-hover:opacity-100"
                  onMouseDown={(e) => { e.stopPropagation(); startConnecting(node.id); }}
                />

                <button
                  onClick={(e) => { e.stopPropagation(); removeNode(node.id); }}
                  className="absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-destructive text-white text-[10px] flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity z-20"
                >
                  x
                </button>
              </div>
            );
          })}
        </div>

        <div className="w-52 border-l bg-card flex flex-col">
          {selectedNode && (
            <div className="p-3 border-b">
              <div className="text-[11px] text-muted-foreground mb-1">节点配置</div>
              <div className="text-[13px] font-medium">{selectedNode.label}</div>
              <Badge variant="outline" className="text-[10px] mt-1">{nodeTypeLabel[selectedNode.type]}</Badge>
              <div className="text-[11px] text-muted-foreground mt-1 font-mono">pos: ({Math.round(selectedNode.x)}, {Math.round(selectedNode.y)})</div>
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
              {selectedNode.type === "condition" && (
                <div className="mt-2 space-y-1">
                  <label className="text-[11px] text-muted-foreground">条件表达式</label>
                  <Input defaultValue="" className="h-7 text-xs" placeholder="e.g. result.status == 'ok'" />
                </div>
              )}
              <div className="mt-2 space-y-1">
                <label className="text-[11px] text-muted-foreground">依赖节点</label>
                <div className="flex flex-wrap gap-1">
                  {edges.filter((e) => e.to === selectedNode.id).map((e) => {
                    const src = canvasNodes.find((n) => n.id === e.from);
                    return <Badge key={e.from} variant="secondary" className="text-[10px]">{src?.label ?? e.from}</Badge>;
                  })}
                  {edges.filter((e) => e.to === selectedNode.id).length === 0 && <span className="text-[11px] text-muted-foreground">无依赖</span>}
                </div>
              </div>
            </div>
          )}
          {!selectedNode && canvasNodes.length > 0 && (
            <div className="p-3 border-b text-[11px] text-muted-foreground">点击节点查看配置</div>
          )}
          <div className="flex-1 overflow-y-auto p-3">
            <div className="flex gap-2 mb-2">
              {(["console", "history", "config"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setRightTab(tab)}
                  className={`text-[11px] px-2 py-0.5 rounded ${rightTab === tab ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-accent"}`}
                >
                  {tab === "console" ? "控制台" : tab === "history" ? "历史" : "配置"}
                </button>
              ))}
            </div>
            {rightTab === "console" && (
              <div className="space-y-0.5">
                {consoleLogs.map((log, i) => (
                  <div key={i} className="text-[11px] font-mono text-muted-foreground leading-tight">{log}</div>
                ))}
                {consoleLogs.length === 0 && <div className="text-[11px] text-muted-foreground">暂无日志</div>}
              </div>
            )}
            {rightTab === "history" && (
              <div className="space-y-1">
                {execHistory.map((exec) => (
                  <div key={exec.id} className="flex items-center gap-2 p-1.5 rounded border text-[11px]">
                    <Badge variant={exec.status === "completed" ? "default" : exec.status === "failed" ? "destructive" : "secondary"} className="text-[10px]">{exec.status}</Badge>
                    <span className="text-muted-foreground">{new Date(exec.started_at * 1000).toLocaleString("zh-CN")}</span>
                    {exec.completed_at && <span className="text-muted-foreground">{((exec.completed_at - exec.started_at)).toFixed(1)}s</span>}
                  </div>
                ))}
                {execHistory.length === 0 && <div className="text-[11px] text-muted-foreground">暂无执行历史</div>}
              </div>
            )}
            {rightTab === "config" && selectedNode && (
              <div className="space-y-1">
                <div className="text-[11px] text-muted-foreground">ID: {selectedNode.id}</div>
                <div className="text-[11px] text-muted-foreground">类型: {nodeTypeLabel[selectedNode.type]}</div>
                <div className="text-[11px] font-mono">pos: ({Math.round(selectedNode.x)}, {Math.round(selectedNode.y)})</div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}