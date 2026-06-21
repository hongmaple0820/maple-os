"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import {
  ReactFlow,
  Controls,
  MiniMap,
  Background,
  useNodesState,
  useEdgesState,
  addEdge,
  type Node,
  type Edge,
  type OnConnect,
  type NodeProps,
  Handle,
  Position,
  MarkerType,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall, mapleApi, getAuthState } from "@/lib/api";
import { useTranslation } from "react-i18next";
import { ExecutionTimeline } from "./execution-timeline";

/* ─── types ─── */

interface WorkflowItem {
  id: string;
  name: string;
  version: number;
  status: string;
  created_at: number;
  updated_at: number;
}

interface WFNodeData {
  label: string;
  labelKey?: string;
  nodeType: "llm" | "tool" | "condition" | "human_approval" | "trigger";
  model?: string;
  skillId?: string;
  expression?: string;
  status?: "idle" | "running" | "completed" | "failed" | "waiting";
}

type RFNode = Node<WFNodeData>;
type RFEdge = Edge;

/* ─── constants ─── */

const statusLabel: Record<string, string> = {
  active: "workflow.status.active",
  draft: "workflow.status.draft",
  paused: "workflow.status.paused",
  failed: "workflow.status.failed",
};
const statusVariant: Record<string, "default" | "secondary" | "outline" | "destructive"> = {
  active: "default",
  draft: "secondary",
  paused: "outline",
  failed: "destructive",
};

const nodeTypeLabel: Record<string, string> = {
  llm: "workflow.nodeTypes.llm",
  tool: "workflow.nodeTypes.tool",
  condition: "workflow.nodeTypes.condition",
  human_approval: "workflow.nodeTypes.humanApproval",
  trigger: "workflow.nodeTypes.trigger",
};
const nodeTypeIcon: Record<string, string> = {
  llm: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 14a4 4 0 1 1 4-4 4 4 0 0 1-4 4z",
  tool: "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z",
  condition: "M16 3h5v5M4 20h5v5M21 3l-7 7M3 20l7-7",
  human_approval:
    "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 8 0 4 4 0 0 0-8 0M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75",
  trigger: "M13 2L3 14h9l-1 8 10-12h-9l1-8z",
};
const nodeTypeColor: Record<string, { bg: string; border: string; accent: string }> = {
  llm: { bg: "bg-primary/10", border: "border-primary", accent: "text-primary" },
  tool: { bg: "bg-warning/10", border: "border-warning", accent: "text-warning" },
  condition: { bg: "bg-success/10", border: "border-success", accent: "text-success" },
  human_approval: { bg: "bg-destructive/10", border: "border-destructive", accent: "text-destructive" },
  trigger: { bg: "bg-muted", border: "border-muted-foreground", accent: "text-muted-foreground" },
};

const statusBorder: Record<string, string> = {
  idle: "border",
  running: "border-2 border-warning animate-pulse",
  completed: "border-2 border-success",
  failed: "border-2 border-destructive",
  waiting: "border-2 border-muted-foreground",
};

const PALETTE_ITEMS = [
  { type: "llm" as const, labelKey: "workflow.nodeTypes.llm" },
  { type: "tool" as const, labelKey: "workflow.nodeTypes.tool" },
  { type: "condition" as const, labelKey: "workflow.nodeTypes.condition" },
  { type: "human_approval" as const, labelKey: "workflow.nodeTypes.humanApproval" },
  { type: "trigger" as const, labelKey: "workflow.nodeTypes.trigger" },
];

const defaultEdgeOptions = {
  type: "smoothstep" as const,
  markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
  style: { strokeWidth: 1.5 },
};

/* ─── custom node component ─── */

function WFNodeComponent({ data, selected }: NodeProps<RFNode>) {
  const { t } = useTranslation();
  const colors = nodeTypeColor[data.nodeType];

  return (
    <div
      className={`rounded-lg shadow-card transition-shadow hover:shadow-lg p-3 bg-card min-w-[160px] ${
        statusBorder[data.status ?? "idle"] ?? "border"
      } ${selected ? "ring-2 ring-primary" : ""}`}
    >
      <Handle type="target" position={Position.Top} className="!w-3 !h-3 !bg-muted-foreground/30 !border-2 !border-card hover:!bg-primary hover:!border-primary" />

      <div className="flex items-center gap-1.5 mb-1">
        <svg className={`w-4 h-4 ${colors?.accent ?? "text-muted-foreground"}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d={nodeTypeIcon[data.nodeType]} />
        </svg>
        <span className="text-[13px] font-medium truncate">{data.labelKey ? t(data.labelKey) : data.label}</span>
      </div>

      <div className="flex items-center gap-1">
        <Badge variant="outline" className="text-[10px]">{t(nodeTypeLabel[data.nodeType])}</Badge>
        {data.status && data.status !== "idle" && (
          <Badge
            variant={data.status === "running" ? "secondary" : data.status === "completed" ? "default" : data.status === "failed" ? "destructive" : "outline"}
            className="text-[10px]"
          >
            {data.status === "running"
              ? t("workflow.status.running")
              : data.status === "completed"
                ? t("workflow.status.completed")
                : data.status === "failed"
                  ? t("workflow.status.failed")
                  : t("workflow.status.waiting")}
          </Badge>
        )}
      </div>

      {data.nodeType === "llm" && <div className="text-[11px] text-muted-foreground mt-1 font-mono">model: {data.model ?? "auto"}</div>}
      {data.nodeType === "tool" && <div className="text-[11px] text-muted-foreground mt-1 font-mono">skill: {data.skillId ?? "-"}</div>}

      <Handle type="source" position={Position.Bottom} className="!w-3 !h-3 !bg-muted-foreground/30 !border-2 !border-card hover:!bg-primary hover:!border-primary" />
    </div>
  );
}

const nodeTypes = { wfNode: WFNodeComponent };

/* ─── main component ─── */

export function WorkflowManager() {
  const { t, i18n } = useTranslation();

  /* ── workflow list state ── */
  const [workflows, setWorkflows] = useState<WorkflowItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [createName, setCreateName] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [executing, setExecuting] = useState<string | null>(null);
  const [selectedWf, setSelectedWf] = useState<string | null>(null);

  /* ── canvas state (React Flow) ── */
  const [nodes, setNodes, onNodesChange] = useNodesState<RFNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<RFEdge>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  /* ── sidebar state ── */
  const [consoleLogs, setConsoleLogs] = useState<string[]>([]);
  const [rightTab, setRightTab] = useState<"console" | "history" | "config" | "scheduler">("console");
  const [execHistory, setExecHistory] = useState<{ id: string; status: string; started_at: number; completed_at: number | null }[]>([]);
  const [selectedWfName, setSelectedWfName] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // T2-6: validation errors surfaced from backend validate API
  const [validationErrors, setValidationErrors] = useState<string[]>([]);
  // T2-8: execution_id from the last run, for trace view
  const [lastRunExecId, setLastRunExecId] = useState<string | null>(null);
  const [showTrace, setShowTrace] = useState(false);
  const [schedulerJobs, setSchedulerJobs] = useState<{ id: string; workflow_id: string; cron_expr: string; enabled: boolean; next_run_at: number; last_run_at: number | null }[]>([]);
  const [newJobCron, setNewJobCron] = useState("");
  const [showNewJob, setShowNewJob] = useState(false);

  /* ── derived ── */
  const selectedNode = useMemo(() => nodes.find((n) => n.id === selectedNodeId) ?? null, [nodes, selectedNodeId]);
  const selectedData = selectedNode?.data;

  const loadWorkflows = async () => {
    try {
      const result = await rpcCall<{ workflows: WorkflowItem[] }>("workflow.list");
      setWorkflows(result.workflows ?? []);
    } catch {
      setWorkflows([]);
    }
    setLoading(false);
  };

  useEffect(() => {
    loadWorkflows();
  }, []);

  /* ── load workflow definition into canvas ── */
  const loadWorkflowDefinition = useCallback(async (wfId: string) => {
    try {
      const wf = await mapleApi<{ id: string; name: string; yaml_content: string; version: number }>(`/api/workflows/${wfId}`);
      setSelectedWfName(wf.name);

      let definition: { nodes?: Array<{ id: string; name: string; node_type: { type: string; model_route?: string; skill_id?: string; [key: string]: unknown }; depends_on?: string[] }> };
      try {
        definition = JSON.parse(wf.yaml_content);
      } catch {
        setConsoleLogs((prev) => [...prev, t("workflow.log.loadFailed", { id: wfId })]);
        return;
      }

      const parsedNodes: RFNode[] = (definition.nodes ?? []).map((n, i) => ({
        id: n.id,
        type: "wfNode",
        position: { x: 100 + i * 220, y: 100 + (i % 2) * 80 },
        data: {
          label: n.name,
          nodeType: n.node_type.type as WFNodeData["nodeType"],
          model: n.node_type.model_route,
          skillId: n.node_type.skill_id,
          status: "idle",
        },
      }));

      const parsedEdges: RFEdge[] = [];
      for (const n of definition.nodes ?? []) {
        for (const dep of n.depends_on ?? []) {
          parsedEdges.push({
            id: `e-${dep}-${n.id}`,
            source: dep,
            target: n.id,
            type: "smoothstep",
            markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
          });
        }
      }

      setNodes(parsedNodes);
      setEdges(parsedEdges);
      setConsoleLogs((prev) => [...prev, t("workflow.log.loaded", { name: wf.name, count: parsedNodes.length })]);
    } catch (err) {
      setConsoleLogs((prev) => [...prev, t("workflow.log.loadError", { error: (err as Error).message })]);
    }
  }, [setNodes, setEdges, t]);

  /* ── save current canvas as workflow update ── */
  const saveWorkflow = useCallback(async () => {
    if (!selectedWf) return;
    setSaving(true);
    setValidationErrors([]);
    try {
      const yamlNodes = nodes.map((n) => ({
        id: n.id,
        name: n.data.label,
        node_type: {
          type: n.data.nodeType,
          ...(n.data.nodeType === "llm" && { model_route: n.data.model ?? "auto", prompt_ref: "" }),
          ...(n.data.nodeType === "tool" && { skill_id: n.data.skillId ?? "", config: {} }),
        },
        depends_on: edges.filter((e) => e.target === n.id).map((e) => e.source),
      }));
      const definition = JSON.stringify({
        id: selectedWf,
        name: selectedWfName ?? selectedWf,
        version: 1,
        description: "",
        trigger: { type: "webhook", path: `/hook/${selectedWf}`, method: "POST" },
        variables: {},
        nodes: yamlNodes,
        hooks: {},
      });
      await mapleApi(`/api/workflows/${selectedWf}`, {
        method: "PUT",
        body: { yaml_content: definition },
      });

      // T2-6: validate after save — surface errors to UI
      try {
        const { token } = getAuthState();
        const validateResp = await fetch(`/api/maple/api/v3/workflows/${selectedWf}/validate`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
        });
        if (validateResp.ok) {
          const validateBody = await validateResp.json();
          if (validateBody.valid) {
            setConsoleLogs((prev) => [...prev, t("workflow.log.saved", { name: selectedWfName ?? selectedWf }) + " ✓ validate"]);
          } else {
            setValidationErrors(validateBody.errors ?? []);
            setConsoleLogs((prev) => [...prev, `⚠ validate failed: ${(validateBody.errors ?? []).join("; ")}`]);
          }
        }
      } catch (validateErr) {
        // validate is best-effort; save succeeded
        setConsoleLogs((prev) => [...prev, t("workflow.log.saved", { name: selectedWfName ?? selectedWf })]);
      }

      await loadWorkflows();
    } catch (err) {
      setConsoleLogs((prev) => [...prev, t("workflow.log.saveError", { error: (err as Error).message })]);
    } finally {
      setSaving(false);
    }
  }, [selectedWf, selectedWfName, nodes, edges, loadWorkflows, t]);

  /* ── node palette: add new node to canvas ── */
  const addNode = useCallback(
    (template: (typeof PALETTE_ITEMS)[number]) => {
      const id = `${template.type}-${Date.now()}`;
      const rfNode: RFNode = {
        id,
        type: "wfNode",
        position: { x: 100 + nodes.length * 40, y: 80 + nodes.length * 120 },
        data: { label: t(template.labelKey), labelKey: template.labelKey, nodeType: template.type, status: "idle" },
      };
      setNodes((prev) => [...prev, rfNode]);
      const nodeLabel = t(template.labelKey);
      setConsoleLogs((prev) => [...prev, t("workflow.log.addNode", { label: nodeLabel, id })]);

      if (nodes.length > 0) {
        const last = nodes[nodes.length - 1];
        const lastLabel = last.data.labelKey ? t(last.data.labelKey) : last.data.label;
        setEdges((prev) => [
          ...prev,
          { id: `e-${last.id}-${id}`, source: last.id, target: id, type: "smoothstep", markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 } },
        ]);
        setConsoleLogs((prev) => [...prev, t("workflow.log.connect", { from: lastLabel, to: nodeLabel })]);
      }
    },
    [nodes, setNodes, setEdges, t],
  );

  /* ── remove node ── */
  const removeNode = useCallback(
    (nodeId: string) => {
      setNodes((prev) => prev.filter((n) => n.id !== nodeId));
      setEdges((prev) => prev.filter((e) => e.source !== nodeId && e.target !== nodeId));
      if (selectedNodeId === nodeId) setSelectedNodeId(null);
      setConsoleLogs((prev) => [...prev, t("workflow.log.removeNode", { id: nodeId })]);
    },
    [setNodes, setEdges, selectedNodeId, t],
  );

  /* ── connection handler ── */
  const onConnect: OnConnect = useCallback(
    (params) => {
      setEdges((eds) => addEdge({ ...params, type: "smoothstep", markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 } }, eds));
      setConsoleLogs((prev) => [...prev, t("workflow.log.addConnect")]);
    },
    [setEdges, t],
  );

  /* ── node click → select ── */
  const onNodeClick = useCallback((_event: React.MouseEvent, node: RFNode) => {
    setSelectedNodeId(node.id);
  }, []);

  const onPaneClick = useCallback(() => {
    setSelectedNodeId(null);
  }, []);

  /* ── update node data helper ── */
  const updateNodeData = useCallback(
    (nodeId: string, patch: Partial<WFNodeData>) => {
      setNodes((prev) =>
        prev.map((n) => (n.id === nodeId ? { ...n, data: { ...n.data, ...patch } } : n)),
      );
    },
    [setNodes],
  );

  /* ── create workflow from canvas ── */
  const handleCreate = async () => {
    if (!createName.trim()) return;
    try {
      const yamlNodes = nodes.map((n) => ({
        id: n.id,
        name: n.data.label,
        node_type: {
          type: n.data.nodeType,
          ...(n.data.nodeType === "llm" && { model_route: n.data.model ?? "auto", prompt_ref: "" }),
          ...(n.data.nodeType === "tool" && { skill_id: n.data.skillId ?? "", config: {} }),
        },
        depends_on: edges.filter((e) => e.target === n.id).map((e) => e.source),
      }));
      const definition = JSON.stringify({
        id: createName.trim().replace(/\s+/g, "-").toLowerCase(),
        name: createName.trim(),
        version: 1,
        description: "",
        trigger: { type: "webhook", path: `/hook/${createName}`, method: "POST" },
        variables: {},
        nodes: yamlNodes,
        hooks: {},
      });
      await rpcCall("workflow.create", { name: createName.trim(), yaml_content: definition });
      setShowCreate(false);
      setCreateName("");
      setNodes([]);
      setEdges([]);
      setConsoleLogs([]);
      await loadWorkflows();
    } catch (err) {
      alert(`${t("common.failed")}: ${(err as Error).message}`);
    }
  };

  /* ── execute workflow ── */
  const handleExecute = async (workflowId: string) => {
    setExecuting(workflowId);
    setNodes((prev) => prev.map((n) => ({ ...n, data: { ...n.data, status: "idle" as const } })));
    setConsoleLogs((prev) => [...prev, t("workflow.log.execStart", { id: workflowId })]);
    try {
      const result = await rpcCall<{ exec_id: string; status: string; result?: string; error?: string }>("workflow.execute", {
        workflow_id: workflowId,
      });
      if (result.error) {
        setConsoleLogs((prev) => [...prev, t("workflow.log.execFailed", { error: result.error })]);
      } else {
        setConsoleLogs((prev) => [...prev, t("workflow.log.execSubmit", { id: result.exec_id, status: result.status })]);
      }

      // T2-8: also call v3 create-run to get execution_id for unified trace
      try {
        const { token } = getAuthState();
        const runResp = await fetch(`/api/maple/api/v3/workflow-runs`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
          body: JSON.stringify({
            workflow_id: workflowId,
            workflow_version: 1,
            input: "{}",
          }),
        });
        if (runResp.ok) {
          const runBody = await runResp.json();
          if (runBody.execution_id) {
            setLastRunExecId(runBody.execution_id);
            setConsoleLogs((prev) => [...prev, `📋 execution_id: ${runBody.execution_id}`]);
          }
        }
      } catch {
        // v3 run is best-effort; legacy RPC already handled
      }
    } catch (err) {
      setConsoleLogs((prev) => [...prev, t("workflow.log.execError", { error: (err as Error).message })]);
    } finally {
      setExecuting(null);
    }
  };

  /* ── SSE: real-time node status updates ── */
  useEffect(() => {
    let es: EventSource | null = null;
    try {
      es = new EventSource("/api/maple/api/events");

      const updateNodeStatus = (nodeId: string, status: WFNodeData["status"], extra?: Partial<WFNodeData>) => {
        setNodes((prev) => prev.map((n) => (n.id === nodeId ? { ...n, data: { ...n.data, status, ...extra } } : n)));
      };

      es.addEventListener("node.started", (e) => {
        try {
          const d = JSON.parse(e.data);
          updateNodeStatus(d.node_id, "running");
          setConsoleLogs((prev) => [...prev, t("workflow.log.nodeStart", { id: d.node_id })]);
        } catch {
          /* ignore */
        }
      });
      es.addEventListener("node.completed", (e) => {
        try {
          const d = JSON.parse(e.data);
          updateNodeStatus(d.node_id, "completed");
          setConsoleLogs((prev) => [...prev, t("workflow.log.nodeComplete", { id: d.node_id })]);
        } catch {
          /* ignore */
        }
      });
      es.addEventListener("node.failed", (e) => {
        try {
          const d = JSON.parse(e.data);
          updateNodeStatus(d.node_id, "failed");
          setConsoleLogs((prev) => [...prev, t("workflow.log.nodeFail", { id: d.node_id, error: d.error })]);
        } catch {
          /* ignore */
        }
      });
      es.addEventListener("workflow.completed", (e) => {
        try {
          const d = JSON.parse(e.data);
          setConsoleLogs((prev) => [...prev, t("workflow.log.wfComplete", { id: d.workflow_id })]);
        } catch {
          /* ignore */
        }
      });
      es.addEventListener("workflow.failed", (e) => {
        try {
          const d = JSON.parse(e.data);
          setConsoleLogs((prev) => [...prev, t("workflow.log.wfFail", { id: d.workflow_id, error: d.error })]);
        } catch {
          /* ignore */
        }
      });
    } catch {
      /* EventSource unavailable */
    }
    return () => {
      es?.close();
    };
  }, [setNodes, t]);

  /* ── load execution history ── */
  const loadExecHistory = async (wfId: string) => {
    try {
      const res = await mapleApi<{ executions: { id: string; status: string; started_at: number; completed_at: number | null }[] }>(
        "/api/workflows/" + wfId + "/executions",
      );
      setExecHistory(res.executions ?? []);
      setRightTab("history");
    } catch {
      setExecHistory([]);
    }
  };

  /* ── scheduler jobs ── */
  const loadSchedulerJobs = async () => {
    try {
      const res = await mapleApi<{ jobs: { id: string; workflow_id: string; cron_expr: string; enabled: boolean; next_run_at: number; last_run_at: number | null }[] }>("/api/scheduler/jobs");
      setSchedulerJobs(res.jobs ?? []);
    } catch {
      setSchedulerJobs([]);
    }
  };

  const toggleJob = async (jobId: string, enabled: boolean) => {
    try {
      await mapleApi(`/api/scheduler/jobs/${jobId}`, { method: "PUT", body: { enabled } });
      await loadSchedulerJobs();
    } catch { /* ignore */ }
  };

  const deleteJob = async (jobId: string) => {
    try {
      await mapleApi(`/api/scheduler/jobs/${jobId}`, { method: "DELETE" });
      await loadSchedulerJobs();
    } catch { /* ignore */ }
  };

  const createJob = async () => {
    if (!selectedWf || !newJobCron.trim()) return;
    try {
      await mapleApi("/api/scheduler/jobs", { method: "POST", body: { workflow_id: selectedWf, cron_expr: newJobCron.trim(), enabled: true } });
      setNewJobCron("");
      setShowNewJob(false);
      await loadSchedulerJobs();
    } catch { /* ignore */ }
  };

  /* ── filtered workflow list ── */
  const filtered = workflows.filter((wf) => wf.name.toLowerCase().includes(search.toLowerCase()));

  if (loading)
    return (
      <div className="flex items-center justify-center h-full">
        <Spinner className="w-8 h-8" />
      </div>
    );

  return (
    <div className="flex flex-col h-full">
      {/* ── top bar ── */}
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">{t("workflow.title")}</h2>
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("workflow.search")} className="w-36 h-7 text-xs" />
        </div>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => setShowCreate(true)}>
            {t("workflow.create")}
          </Button>
          {selectedWf && (
            <>
              <Button size="sm" onClick={saveWorkflow} disabled={saving}>
                {saving ? t("common.saving") : t("common.save")}
              </Button>
              <Button size="sm" variant="destructive" onClick={() => { setSelectedWf(null); setSelectedWfName(null); setNodes([]); setEdges([]); }}>
                {t("workflow.closeEdit")}
              </Button>
            </>
          )}
        </div>
      </div>

      {/* ── create bar ── */}
      {showCreate && (
        <div className="h-9 border-b bg-muted/50 flex items-center gap-2 px-4">
          <Input value={createName} onChange={(e) => setCreateName(e.target.value)} placeholder={t("workflow.createTitle")} className="w-36 h-7 text-xs" />
          <Button size="sm" onClick={handleCreate} disabled={!createName.trim()}>
            {t("common.create")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              setShowCreate(false);
              setCreateName("");
            }}
          >
            {t("common.cancel")}
          </Button>
        </div>
      )}

      {/* ── main area ── */}
      <div className="flex flex-1 overflow-hidden">
        {/* ── left sidebar ── */}
        <div className="w-52 border-r bg-card flex flex-col">
          {/* palette */}
          <div className="p-2 border-b">
            <div className="text-[11px] text-muted-foreground mb-1.5">{t("workflow.nodeLibrary")}</div>
            <div className="space-y-1">
              {PALETTE_ITEMS.map((item) => (
                <button
                  key={item.type}
                  onClick={() => addNode(item)}
                  className="w-full text-left px-2 py-1.5 rounded text-xs hover:bg-accent transition-colors flex items-center gap-1.5"
                >
                  <svg className={`w-3.5 h-3.5 ${nodeTypeColor[item.type]?.accent}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d={nodeTypeIcon[item.type]} />
                  </svg>
                  {t(item.labelKey)}
                </button>
              ))}
            </div>
          </div>

          {/* workflow list */}
          <div className="p-2 border-b overflow-y-auto">
            <div className="text-[11px] text-muted-foreground mb-1.5">{t("workflow.savedWorkflows")}</div>
            <div className="space-y-1">
              {filtered.map((wf) => (
                <button
                  key={wf.id}
                  onClick={() => {
                    setSelectedWf(wf.id);
                    setConsoleLogs((prev) => [...prev, t("workflow.log.selectWf", { name: wf.name })]);
                    loadWorkflowDefinition(wf.id);
                  }}
                  className={`w-full text-left px-2 py-1.5 rounded text-xs transition-colors ${
                    selectedWf === wf.id ? "bg-primary/10 text-primary font-medium" : "hover:bg-accent"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span>{wf.name}</span>
                    <Badge variant={statusVariant[wf.status] ?? "outline"} className="text-[10px] px-1">
                      {t(statusLabel[wf.status] ?? wf.status)}
                    </Badge>
                  </div>
                </button>
              ))}
              {filtered.length === 0 && <div className="text-xs text-muted-foreground py-2">{t("workflow.noWorkflows")}</div>}
            </div>
          </div>

          {/* workflow actions */}
          {selectedWf && (
            <div className="p-2 space-y-1">
              <Button size="sm" className="w-full" onClick={() => handleExecute(selectedWf)} disabled={executing === selectedWf}>
                {executing === selectedWf ? t("workflow.executing") : t("workflow.runWorkflow")}
              </Button>
              {/* T2-8: trace toggle — show ExecutionTimeline for the last run */}
              {lastRunExecId && (
                <Button
                  size="sm"
                  variant="outline"
                  className="w-full"
                  onClick={() => setShowTrace((s) => !s)}
                >
                  {showTrace ? t("workflow.trace.hide", "Hide trace") : t("workflow.trace.view", "View trace")}
                </Button>
              )}
              <Button size="sm" variant="outline" className="w-full" onClick={() => loadExecHistory(selectedWf)}>
                {t("workflow.execHistory")}
              </Button>
              {/* T2-6: validation errors panel */}
              {validationErrors.length > 0 && (
                <div className="text-[10px] text-red-600 bg-red-50 border border-red-200 rounded p-2 space-y-1">
                  <div className="font-medium">⚠ {t("workflow.validation.failed", "Validation failed")}:</div>
                  {validationErrors.map((err, i) => (
                    <div key={i}>• {err}</div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* ── canvas: React Flow ── */}
        <div className="flex-1 bg-background relative">
          {nodes.length === 0 && !selectedWf && (
            <div className="absolute inset-0 flex items-center justify-center text-muted-foreground text-sm pointer-events-none z-10">
              {t("workflow.canvasHint")}
            </div>
          )}

          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeClick={onNodeClick}
            onPaneClick={onPaneClick}
            nodeTypes={nodeTypes}
            defaultEdgeOptions={defaultEdgeOptions}
            fitView
            snapToGrid
            snapGrid={[20, 20]}
            proOptions={{ hideAttribution: true }}
          >
            <Controls />
            <MiniMap
              nodeStrokeWidth={3}
              zoomable
              pannable
              className="!bg-card !border"
            />
            <Background gap={20} size={1} className="text-muted-foreground/20" />
          </ReactFlow>
          {/* T2-8: trace panel below canvas when toggled */}
          {showTrace && lastRunExecId && (
            <div className="absolute bottom-0 left-0 right-0 max-h-[300px] overflow-y-auto bg-card border-t shadow-lg z-10">
              <ExecutionTimeline executionId={lastRunExecId} compact />
            </div>
          )}
        </div>

        {/* ── right sidebar ── */}
        <div className="w-52 border-l bg-card flex flex-col">
          {/* node config */}
          {selectedNode && selectedData && (
            <div className="p-3 border-b">
              <div className="text-[11px] text-muted-foreground mb-1">{t("workflow.nodeConfig")}</div>
              <div className="text-[13px] font-medium">{selectedData.labelKey ? t(selectedData.labelKey) : selectedData.label}</div>
              <Badge variant="outline" className="text-[10px] mt-1">
                {t(nodeTypeLabel[selectedData.nodeType])}
              </Badge>
              <div className="text-[11px] text-muted-foreground mt-1 font-mono">
                pos: ({Math.round(selectedNode.position.x)}, {Math.round(selectedNode.position.y)})
              </div>

              {/* label */}
              <div className="mt-2 space-y-1">
                <label className="text-[11px] text-muted-foreground">{t("workflow.nodeName")}</label>
                <Input
                  value={selectedData.label}
                  onChange={(e) => updateNodeData(selectedNode.id, { label: e.target.value })}
                  className="h-7 text-xs"
                />
              </div>

              {/* llm model */}
              {selectedData.nodeType === "llm" && (
                <div className="mt-2 space-y-1">
                  <label className="text-[11px] text-muted-foreground">{t("workflow.modelRoute")}</label>
                  <Input value={selectedData.model ?? "auto"} onChange={(e) => updateNodeData(selectedNode.id, { model: e.target.value })} className="h-7 text-xs" />
                </div>
              )}

              {/* tool skillId */}
              {selectedData.nodeType === "tool" && (
                <div className="mt-2 space-y-1">
                  <label className="text-[11px] text-muted-foreground">{t("workflow.skillId")}</label>
                  <Input
                    value={selectedData.skillId ?? ""}
                    onChange={(e) => updateNodeData(selectedNode.id, { skillId: e.target.value })}
                    className="h-7 text-xs"
                    placeholder="skill_id"
                  />
                </div>
              )}

              {/* condition expression */}
              {selectedData.nodeType === "condition" && (
                <div className="mt-2 space-y-1">
                  <label className="text-[11px] text-muted-foreground">{t("workflow.conditionExpr")}</label>
                  <Input
                    value={selectedData.expression ?? ""}
                    onChange={(e) => updateNodeData(selectedNode.id, { expression: e.target.value })}
                    className="h-7 text-xs"
                    placeholder="e.g. result.status == 'ok'"
                  />
                </div>
              )}

              {/* depends on */}
              <div className="mt-2 space-y-1">
                <label className="text-[11px] text-muted-foreground">{t("workflow.dependsOn")}</label>
                <div className="flex flex-wrap gap-1">
                  {edges
                    .filter((e) => e.target === selectedNode.id)
                    .map((e) => {
                      const src = nodes.find((n) => n.id === e.source);
                      return (
                        <Badge key={e.source} variant="secondary" className="text-[10px]">
                          {src?.data.labelKey ? t(src.data.labelKey) : src?.data.label ?? e.source}
                        </Badge>
                      );
                    })}
                  {edges.filter((e) => e.target === selectedNode.id).length === 0 && (
                    <span className="text-[11px] text-muted-foreground">{t("workflow.noDeps")}</span>
                  )}
                </div>
              </div>
            </div>
          )}
          {!selectedNode && nodes.length > 0 && (
            <div className="p-3 border-b text-[11px] text-muted-foreground">{t("workflow.clickNodeConfig")}</div>
          )}

          {/* tabs: console / history / config */}
          <div className="flex-1 overflow-y-auto p-3">
            <div className="flex gap-2 mb-2">
              {(["console", "history", "config", "scheduler"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => { setRightTab(tab); if (tab === "scheduler") loadSchedulerJobs(); }}
                  className={`text-[11px] px-2 py-0.5 rounded ${rightTab === tab ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-accent"}`}
                >
                  {tab === "console" ? t("workflow.tabs.console") : tab === "history" ? t("workflow.tabs.history") : tab === "scheduler" ? t("workflow.tabs.scheduler") : t("workflow.tabs.config")}
                </button>
              ))}
            </div>

            {rightTab === "console" && (
              <div className="space-y-0.5">
                {consoleLogs.map((log, i) => (
                  <div key={i} className="text-[11px] font-mono text-muted-foreground leading-tight">
                    {log}
                  </div>
                ))}
                {consoleLogs.length === 0 && <div className="text-[11px] text-muted-foreground">{t("workflow.noLogs")}</div>}
              </div>
            )}

            {rightTab === "history" && (
              <div className="space-y-1">
                {execHistory.map((exec) => (
                  <div key={exec.id} className="flex items-center gap-2 p-1.5 rounded border text-[11px]">
                    <Badge variant={exec.status === "completed" ? "default" : exec.status === "failed" ? "destructive" : "secondary"} className="text-[10px]">
                      {exec.status}
                    </Badge>
                    <span className="text-muted-foreground">
                      {new Date(exec.started_at * 1000).toLocaleString(i18n.language?.startsWith("zh") ? "zh-CN" : "en-US")}
                    </span>
                    {exec.completed_at && <span className="text-muted-foreground">{(exec.completed_at - exec.started_at).toFixed(1)}s</span>}
                  </div>
                ))}
                {execHistory.length === 0 && <div className="text-[11px] text-muted-foreground">{t("workflow.noExecHistory")}</div>}
              </div>
            )}

            {rightTab === "config" && selectedNode && selectedData && (
              <div className="space-y-1">
                <div className="text-[11px] text-muted-foreground">ID: {selectedNode.id}</div>
                <div className="text-[11px] text-muted-foreground">
                  {t("workflow.nodeType")}: {t(nodeTypeLabel[selectedData.nodeType])}
                </div>
                <div className="text-[11px] font-mono">
                  pos: ({Math.round(selectedNode.position.x)}, {Math.round(selectedNode.position.y)})
                </div>
              </div>
            )}

            {rightTab === "scheduler" && (
              <div className="space-y-2">
                {selectedWf && (
                  <div>
                    {showNewJob ? (
                      <div className="flex gap-1 mb-2">
                        <Input value={newJobCron} onChange={(e) => setNewJobCron(e.target.value)} placeholder="0 */6 * * *" className="h-6 text-[10px] flex-1" />
                        <Button size="sm" className="h-6 text-[10px]" onClick={createJob} disabled={!newJobCron.trim()}>+</Button>
                        <Button size="sm" variant="ghost" className="h-6 text-[10px]" onClick={() => { setShowNewJob(false); setNewJobCron(""); }}>x</Button>
                      </div>
                    ) : (
                      <Button size="sm" variant="outline" className="w-full h-6 text-[10px] mb-2" onClick={() => setShowNewJob(true)}>
                        {t("workflow.scheduler.create")}
                      </Button>
                    )}
                  </div>
                )}
                {schedulerJobs.map((job) => (
                  <div key={job.id} className="rounded border p-1.5 text-[11px]">
                    <div className="flex items-center justify-between">
                      <span className="font-mono">{job.cron_expr}</span>
                      <Badge variant={job.enabled ? "default" : "outline"} className="text-[9px]">
                        {job.enabled ? "ON" : "OFF"}
                      </Badge>
                    </div>
                    <div className="text-muted-foreground mt-0.5">{job.workflow_id}</div>
                    <div className="flex gap-1 mt-1">
                      <Button size="sm" variant="ghost" className="h-5 text-[9px] px-1" onClick={() => toggleJob(job.id, !job.enabled)}>
                        {job.enabled ? t("common.disable") : t("common.enable")}
                      </Button>
                      <Button size="sm" variant="ghost" className="h-5 text-[9px] px-1 text-destructive" onClick={() => deleteJob(job.id)}>
                        {t("common.delete")}
                      </Button>
                    </div>
                  </div>
                ))}
                {schedulerJobs.length === 0 && <div className="text-[11px] text-muted-foreground">{t("workflow.scheduler.noJobs")}</div>}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
