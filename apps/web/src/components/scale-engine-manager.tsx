"use client";

import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall, scaleApi } from "@/lib/api";

interface ScaleTool { name: string; description: string; inputSchema: Record<string, unknown> }
interface ScaleCallResult { ok: boolean; result?: unknown; error?: string }
interface ArtifactSummary { type: string; count: number; states: Record<string, number> }

const ARTIFACT_TYPES = ["spec", "plan", "task", "defect", "test_case", "knowledge"];
const artifactTypeLabel: Record<string, string> = { spec: "scale.artifactTypeLabels.spec", plan: "scale.artifactTypeLabels.plan", task: "scale.artifactTypeLabels.task", defect: "scale.artifactTypeLabels.defect", test_case: "scale.artifactTypeLabels.testCase", knowledge: "scale.artifactTypeLabels.knowledge" };
const FSM_STATES = ["draft", "reviewing", "approved", "in_progress", "completed", "cancelled"];
const fsmStateLabel: Record<string, string> = { draft: "scale.fsmStateLabels.draft", reviewing: "scale.fsmStateLabels.reviewing", approved: "scale.fsmStateLabels.approved", in_progress: "scale.fsmStateLabels.inProgress", completed: "scale.fsmStateLabels.completed", cancelled: "scale.fsmStateLabels.cancelled" };
const fsmStateColor: Record<string, string> = { draft: "bg-muted text-muted-foreground", reviewing: "bg-warning/10 text-warning", approved: "bg-success/10 text-success", in_progress: "bg-primary/10 text-primary", completed: "bg-success text-success-foreground", cancelled: "bg-destructive/10 text-destructive" };

export function ScaleEngineManager() {
  const { t } = useTranslation();
  const [tools, setTools] = useState<ScaleTool[]>([]);
  const [loading, setLoading] = useState(true);
  const [callResult, setCallResult] = useState<string | null>(null);
  const [callLoading, setCallLoading] = useState(false);
  const [selectedTool, setSelectedTool] = useState<string | null>(null);
  const [argsInput, setArgsInput] = useState("{}");
  const [bridgeOnline, setBridgeOnline] = useState(false);
  const [activeTab, setActiveTab] = useState<"tools" | "call" | "artifacts">("tools");
  const [createType, setCreateType] = useState("spec");
  const [createTitle, setCreateTitle] = useState("");
  const [callLog, setCallLog] = useState<string[]>([]);

  const loadTools = async () => {
    try {
      const res = await scaleApi<{ tools: ScaleTool[] }>("/tools");
      setTools(res.tools ?? []);
      setBridgeOnline(true);
    } catch {
      try {
        const result = await rpcCall<{ raw: string }>("scale.tools");
        const parsed = JSON.parse(result.raw);
        setTools(parsed.tools ?? []);
        setBridgeOnline(true);
      } catch {
        setBridgeOnline(false);
        setTools([]);
      }
    }
    setLoading(false);
  };

  useEffect(() => { loadTools(); }, []);

  const handleCall = async () => {
    if (!selectedTool) return;
    setCallLoading(true);
    setCallResult(null);
    setCallLog((prev) => [...prev, `[Call] ${selectedTool} ${argsInput}`]);
    try {
      const args = JSON.parse(argsInput);
      const res = await scaleApi<ScaleCallResult>("/call", { method: "POST", body: { name: selectedTool, arguments: args } });
      setCallResult(JSON.stringify(res, null, 2));
      setCallLog((prev) => [...prev, `[Result] ok=${res.ok}`]);
    } catch {
      try {
        const args = JSON.parse(argsInput);
        const rpcRes = await rpcCall<{ raw: string }>("scale.call", { tool_name: selectedTool, arguments: args });
        setCallResult(rpcRes.raw);
        setCallLog((prev) => [...prev, `[Result] RPC fallback ok`]);
      } catch (rpcErr) {
        setCallResult(`${t("common.failed")}: ${(rpcErr as Error).message}`);
        setCallLog((prev) => [...prev, `[Error] ${(rpcErr as Error).message}`]);
      }
    } finally { setCallLoading(false); }
  };

  const handleCreateArtifact = async () => {
    if (!createTitle.trim()) return;
    setCallLog((prev) => [...prev, `[Create] ${createType}: ${createTitle}`]);
    try {
      const res = await scaleApi<ScaleCallResult>("/call", {
        method: "POST",
        body: { name: "scale_create", arguments: { artifact_type: createType, title: createTitle.trim() } },
      });
      setCallLog((prev) => [...prev, `[Create] ok=${res.ok}, result=${JSON.stringify(res.result)}`]);
      setCreateTitle("");
    } catch (err) {
      setCallLog((prev) => [...prev, `[Create Failed] ${(err as Error).message}`]);
    }
  };

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  const toolCategories = [...new Set(tools.map((tool) => {
    if (tool.name.startsWith("scale_")) return "SCALE";
    return t("common.noData");
  }))];

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">{t("scale.title")}</h2>
          <Badge variant={bridgeOnline ? "default" : "destructive"} className="text-[10px]">{bridgeOnline ? t("scale.bridgeOnline") : t("scale.bridgeOffline")}</Badge>
          <Badge variant="outline" className="text-[10px]">{t("scale.toolCount", { count: tools.length })}</Badge>
        </div>
        <Button size="sm" onClick={loadTools}>{t("common.refresh")}</Button>
      </div>

      <div className="h-8 border-b bg-muted/30 flex items-center gap-2 px-4">
        {(["tools", "call", "artifacts"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={`px-2 py-1 rounded text-[11px] transition-colors ${
              activeTab === tab ? "bg-primary text-primary-foreground font-medium" : "text-muted-foreground hover:bg-accent"
            }`}
          >
            {tab === "tools" ? t("scale.tabs.tools") : tab === "call" ? t("scale.tabs.call") : t("scale.tabs.artifacts")}
          </button>
        ))}
      </div>

      <div className="flex flex-1 overflow-hidden">
        <div className="flex-1 overflow-y-auto p-4 space-y-2">
          {activeTab === "tools" && (
            <>
              {tools.length === 0 && !bridgeOnline && (
                <Card className="shadow-card">
                  <CardContent className="p-4 text-[12px] text-muted-foreground">
                    <p>{t("scale.bridgeOfflineHint")}</p>
                    <code className="block bg-muted p-2 rounded text-[11px] mt-2">{t("scale.bridgeCommand")}</code>
                    <p className="mt-2">{t("scale.bridgePorts")}</p>
                  </CardContent>
                </Card>
              )}
              {tools.map((tool) => (
                <button
                  key={tool.name}
                  onClick={() => { setSelectedTool(tool.name); setArgsInput("{}"); setActiveTab("call"); }}
                  className="w-full"
                >
                  <Card className={`shadow-card hover:shadow-lg transition-shadow ${selectedTool === tool.name ? "ring-2 ring-primary" : ""}`}>
                    <CardContent className="p-3">
                      <div className="flex items-center justify-between">
                        <span className="text-[13px] font-medium">{tool.name}</span>
                        <Badge variant={tool.name.startsWith("scale_") ? "default" : "outline"} className="text-[10px]">
                          {tool.name.startsWith("scale_") ? "SCALE" : t("common.noData")}
                        </Badge>
                      </div>
                      <div className="text-[11px] text-muted-foreground mt-0.5">{tool.description}</div>
                    </CardContent>
                  </Card>
                </button>
              ))}
            </>
          )}

          {activeTab === "call" && (
            <>
              {selectedTool ? (
                <Card className="shadow-card border-primary">
                  <CardContent className="p-3 space-y-2">
                    <div className="text-[13px] font-medium">{t("scale.callTitle", { name: selectedTool })}</div>
                    <div className="text-[11px] text-muted-foreground">{t("scale.paramsLabel")}</div>
                    <textarea
                      value={argsInput}
                      onChange={(e) => setArgsInput(e.target.value)}
                      className="w-full h-16 rounded-md border bg-transparent px-3 py-2 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                      placeholder={t("scale.paramsPlaceholder")}
                    />
                    <Button size="sm" onClick={handleCall} disabled={callLoading}>
                      {callLoading ? t("scale.executing") : t("scale.execute")}
                    </Button>
                    {callResult && (
                      <pre className="bg-muted p-3 rounded text-[11px] overflow-x-auto max-h-32 overflow-y-auto whitespace-pre-wrap">{callResult}</pre>
                    )}
                  </CardContent>
                </Card>
              ) : (
                <div className="text-center text-muted-foreground py-8">{t("scale.selectToolHint")}</div>
              )}
            </>
          )}

          {activeTab === "artifacts" && (
            <div className="space-y-3">
              <Card className="shadow-card">
                <CardContent className="p-3 space-y-2">
                  <div className="text-[13px] font-medium">{t("scale.createArtifact")}</div>
                  <div className="flex gap-2">
                    <select value={createType} onChange={(e) => setCreateType(e.target.value)} className="h-7 rounded border bg-background text-xs px-2">
                      {ARTIFACT_TYPES.map((type) => <option key={type} value={type}>{t(artifactTypeLabel[type])} ({type})</option>)}
                    </select>
                    <Input value={createTitle} onChange={(e) => setCreateTitle(e.target.value)} placeholder={t("scale.titlePlaceholder")} className="h-7 text-xs w-40" />
                    <Button size="sm" onClick={handleCreateArtifact} disabled={!createTitle.trim()}>{t("common.create")}</Button>
                  </div>
                </CardContent>
              </Card>

              <Card className="shadow-card">
                <CardContent className="p-3">
                  <div className="text-[13px] font-medium mb-2">{t("scale.fsmStates")}</div>
                  <div className="flex items-center gap-1 overflow-x-auto">
                    {FSM_STATES.map((state, i) => (
                      <div key={state} className="flex items-center gap-1">
                        <Badge className={`text-[10px] ${fsmStateColor[state]}`}>{t(fsmStateLabel[state])}</Badge>
                        {i < FSM_STATES.length - 1 && <span className="text-muted-foreground text-[10px]">→</span>}
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>

              <Card className="shadow-card">
                <CardContent className="p-3">
                  <div className="text-[13px] font-medium mb-2">{t("scale.artifactTypes")}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {ARTIFACT_TYPES.map((type) => (
                      <Badge key={type} variant="outline" className="text-[11px]">{t(artifactTypeLabel[type])} ({type})</Badge>
                    ))}
                  </div>
                </CardContent>
              </Card>
            </div>
          )}
        </div>

        <div className="w-48 border-l bg-card overflow-y-auto p-3">
          <div className="text-[11px] text-muted-foreground mb-1">{t("scale.callLog")}</div>
          <div className="space-y-0.5">
            {callLog.map((log, i) => (
              <div key={i} className="text-[11px] font-mono text-muted-foreground leading-tight">{log}</div>
            ))}
            {callLog.length === 0 && <div className="text-[11px] text-muted-foreground">{t("scale.noLogs")}</div>}
          </div>

          <div className="mt-4 text-[11px] text-muted-foreground mb-1">{t("scale.toolCategories")}</div>
          <div className="space-y-1">
            {toolCategories.map((cat) => (
              <div key={cat} className="flex items-center justify-between">
                <span className="text-[12px]">{cat}</span>
                <Badge variant="outline" className="text-[10px]">{tools.filter((tool) => (tool.name.startsWith("scale_") ? t("scale.title") : t("common.noData")) === cat).length}</Badge>
              </div>
            ))}
          </div>

          <div className="mt-4 text-[11px] text-muted-foreground">
            <div>{t("scale.bridgePort")}</div>
            <div>{t("scale.backendPort")}</div>
            <div>{t("scale.protocol")}</div>
          </div>
        </div>
      </div>
    </div>
  );
}