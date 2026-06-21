"use client";

import { useState, useEffect, useCallback } from "react";
import { Card, CardContent, Badge, Button, Spinner, Input, Textarea } from "@mapleos/ui";
import { rpcCall, mapleApi } from "@/lib/api";
import { useTranslation } from "react-i18next";

interface AgentListItem {
  id: string;
  name: string;
  status: string;
  is_online?: boolean;
  last_heartbeat?: number | null;
  description?: string | null;
  tags?: string[] | null;
  model?: string;
  skills?: string[];
}
interface ModelInfo { id: string; name: string; provider: string }
interface SkillInfo { id: string; description: string }
interface TaskStats { total: number; pending: number; running: number; completed: number; failed: number; dead_letter: number }
interface MemoryEntry { id: string; content: string; type: string; metadata: Record<string, string>; created_at: number; access_count: number }
interface CollabMessage { role: "user" | "agent" | "system"; agentId?: string; content: string; timestamp: number }

const agentStatusKey: Record<string, string> = { Online: "agent.status.idle", Busy: "agent.status.busy", Offline: "agent.status.offline", Idle: "agent.status.idle", idle: "agent.status.idle", busy: "agent.status.busy", offline: "agent.status.offline" };
const agentStatusVariant: Record<string, "default" | "secondary" | "outline"> = { Online: "default", Busy: "secondary", Offline: "outline", Idle: "default", idle: "default", busy: "secondary", offline: "outline" };

const AGENT_AVATARS: Record<string, string> = {
  "maple-core": "M12 2L2 7l10 5 10-5-10-5z",
  "maple-coder": "M16 18l2-2-2-2M8 18l-2-2 2-2M14.5 4l-5 16",
  "maple-analyst": "M12 20V10M18 20V4M6 20v-6",
  "maple-writer": "M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z",
  "maple-reviewer": "M9 12l2 2 4-4m6 2a9 9 0 1 1-18 0 9 9 0 0 1 18 0z",
};

/** Format a unix timestamp (seconds) as a relative "X ago" string */
function timeAgo(unixSecs: number | null | undefined, t: (key: string, opts?: Record<string, unknown>) => string): string | null {
  if (!unixSecs) return null;
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unixSecs;
  if (diff < 0) return null;
  if (diff < 10) return t("agent.heartbeat.justNow");
  if (diff < 60) return t("agent.heartbeat.secondsAgo", { count: diff });
  if (diff < 3600) return t("agent.heartbeat.minutesAgo", { count: Math.floor(diff / 60) });
  if (diff < 86400) return t("agent.heartbeat.hoursAgo", { count: Math.floor(diff / 3600) });
  return t("agent.heartbeat.daysAgo", { count: Math.floor(diff / 86400) });
}

/** Heartbeat indicator dot — green pulse for online, gray for offline */
function HeartbeatDot({ online }: { online: boolean }) {
  return (
    <span className="relative flex h-2.5 w-2.5">
      {online && <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-75" />}
      <span className={`relative inline-flex rounded-full h-2.5 w-2.5 ${online ? "bg-success" : "bg-muted-foreground/40"}`} />
    </span>
  );
}

export function AgentManager() {
  const { t } = useTranslation();
  const [agents, setAgents] = useState<AgentListItem[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [taskStats, setTaskStats] = useState<TaskStats | null>(null);
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [collabMessages, setCollabMessages] = useState<CollabMessage[]>([]);
  const [collabInput, setCollabInput] = useState("");
  const [collabSending, setCollabSending] = useState(false);
  const [memoryQuery, setMemoryQuery] = useState("");
  const [registerName, setRegisterName] = useState("");
  // T3-5: per-agent model override. Empty string means "inherit global
  // default_model from settings". When user selects a specific model
  // here, that model is used for this agent; otherwise the global
  // routing rule (auto) is applied.
  const [registerModel, setRegisterModel] = useState<string>("");
  const [showRegister, setShowRegister] = useState(false);
  const [dispatchTask, setDispatchTask] = useState({ agentId: "", prompt: "" });
  const [showDispatch, setShowDispatch] = useState(false);
  const [summary, setSummary] = useState<{ total: number; online: number; offline: number; busy: number } | null>(null);

  const loadAll = useCallback(async () => {
    // Use the richer /api/agents/status endpoint for agent list with heartbeat data
    try {
      const status = await mapleApi<{ agents: AgentListItem[]; summary: { total: number; online: number; offline: number; busy: number } }>("/api/agents/status");
      setAgents(status.agents ?? []);
      setSummary(status.summary ?? null);
    } catch {
      // Fallback to RPC
      try { const r = await rpcCall<{ agents: AgentListItem[] }>("agent.list"); setAgents(r.agents ?? []); } catch { setAgents([]); }
    }
    try { const r = await rpcCall<{ models: ModelInfo[] }>("llm.models"); setModels(r.models ?? []); } catch { setModels([]); }
    try { const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list"); setSkills(r.skills ?? []); } catch { setSkills([]); }
    try { const s = await mapleApi<TaskStats>("/api/tasks/stats"); setTaskStats(s); } catch { setTaskStats(null); }
    try { const m = await mapleApi<{ results: MemoryEntry[] }>("/api/memories/search", { method: "POST", body: { keyword: "", memory_type: "working", limit: 10 } }); setMemories((m.results ?? [])); } catch { setMemories([]); }
    setLoading(false);
  }, []);

  useEffect(() => { loadAll(); }, [loadAll]);

  // Auto-refresh every 30 seconds
  useEffect(() => {
    const interval = setInterval(loadAll, 30_000);
    return () => clearInterval(interval);
  }, [loadAll]);

  const handleCollabSend = async () => {
    if (!collabInput.trim()) return;
    const msg: CollabMessage = { role: "user", content: collabInput.trim(), timestamp: Date.now() };
    setCollabMessages((prev) => [...prev, msg]);
    setCollabInput("");
    setCollabSending(true);
    try {
      const targetAgent = selectedAgent ?? agents[0]?.id ?? "maple-core";
      const result = await rpcCall<{ response: string; agent_id: string }>("agent.chat", {
        agent_id: targetAgent,
        prompt: msg.content,
      });
      setCollabMessages((prev) => [...prev, {
        role: "agent",
        agentId: result.agent_id ?? targetAgent,
        content: result.response ?? t("agent.messages.noResponse"),
        timestamp: Date.now(),
      }]);
    } catch (err) {
      setCollabMessages((prev) => [...prev, {
        role: "system",
        content: t("agent.errors.requestFailed", { error: (err as Error).message }),
        timestamp: Date.now(),
      }]);
    } finally { setCollabSending(false); }
  };

  const handleMemorySearch = async () => {
    try {
      const m = await mapleApi<{ results: MemoryEntry[] }>("/api/memories/search", { method: "POST", body: { keyword: memoryQuery, memory_type: "working", limit: 10 } });
      setMemories(m.results ?? []);
    } catch { setMemories([]); }
  };

  const handleDeregister = async (agentId: string) => {
    try {
      await rpcCall("agent.deregister", { id: agentId });
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.messages.deregistered", { id: agentId }), timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.errors.deregisterFailed", { error: (err as Error).message }), timestamp: Date.now() }]);
    }
  };

  const handleRegister = async () => {
    if (!registerName.trim()) return;
    try {
      // T3-5: pass model only when user selected a specific one. Empty
      // string means "inherit global default_model" — server-side
      // routing rule (auto) applies.
      const params: Record<string, unknown> = { name: registerName.trim() };
      if (registerModel) params.model = registerModel;
      await rpcCall("agent.register", params);
      setShowRegister(false); setRegisterName(""); setRegisterModel("");
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.messages.registered", { name: registerName }), timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.errors.registerFailed", { error: (err as Error).message }), timestamp: Date.now() }]);
    }
  };

  const handleDispatch = async () => {
    if (!dispatchTask.agentId || !dispatchTask.prompt.trim()) return;
    try {
      await rpcCall("task.create", { agent_id: dispatchTask.agentId, prompt: dispatchTask.prompt.trim() });
      setShowDispatch(false); setDispatchTask({ agentId: "", prompt: "" });
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.messages.dispatched", { agentId: dispatchTask.agentId }), timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.errors.dispatchFailed", { error: (err as Error).message }), timestamp: Date.now() }]);
    }
  };

  const handleWriteMemory = async () => {
    try {
      const key = `user-note-${Date.now()}`;
      await mapleApi("/api/memories", { method: "POST", body: { content: memoryQuery, memory_type: "note" } });
      setMemoryQuery("");
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.messages.memoryWritten", { key }), timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: t("agent.errors.writeFailed", { error: (err as Error).message }), timestamp: Date.now() }]);
    }
  };

  const selectedAgentData = agents.find((a) => a.id === selectedAgent);

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">{t("agent.title")}</h2>
          {summary && (
            <div className="flex items-center gap-2 ml-2">
              <span className="flex items-center gap-1 text-[11px] text-success"><HeartbeatDot online />{summary.online}</span>
              <span className="text-[11px] text-muted-foreground">/</span>
              <span className="text-[11px] text-muted-foreground">{summary.total} {t("agent.totalAgents")}</span>
            </div>
          )}
        </div>
        <div className="flex gap-2">
          <Button size="sm" onClick={loadAll}>{t("agent.refresh")}</Button>
          <Button size="sm" onClick={() => setShowRegister(true)}>{t("agent.register")}</Button>
          <Button size="sm" variant="outline" onClick={() => setShowDispatch(true)}>{t("agent.dispatchTask")}</Button>
        </div>
      </div>

      {showRegister && (
        <div className="h-9 border-b bg-muted/50 flex items-center gap-2 px-4 flex-wrap">
          <Input value={registerName} onChange={(e) => setRegisterName(e.target.value)} placeholder={t("agent.agentName") + "..."} className="w-40 h-7 text-xs" />
          {/* T3-5: model selector with "inherit global" option */}
          <select
            value={registerModel}
            onChange={(e) => setRegisterModel(e.target.value)}
            className="h-7 rounded border bg-background text-xs px-2"
            title={t("agent.modelInheritHint", "Empty = inherit global default_model from settings")}
          >
            <option value="">{t("agent.modelInherit", "Inherit global model")}</option>
            {models.filter((m) => m.registered !== false).map((m) => (
              <option key={m.id} value={m.id}>
                {m.name ?? m.id} ({m.provider}){m.is_local ? " · local" : ""}
              </option>
            ))}
          </select>
          <Button size="sm" onClick={handleRegister} disabled={!registerName.trim()}>{t("agent.confirmRegister")}</Button>
          <Button size="sm" variant="ghost" onClick={() => { setShowRegister(false); setRegisterName(""); setRegisterModel(""); }}>{t("common.cancel")}</Button>
        </div>
      )}

      {showDispatch && (
        <div className="h-9 border-b bg-muted/50 flex items-center gap-2 px-4">
          <select value={dispatchTask.agentId} onChange={(e) => setDispatchTask({ ...dispatchTask, agentId: e.target.value })} className="h-7 rounded border bg-background text-xs px-2">
            <option value="">{t("agent.selectAgent")}</option>
            {agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          </select>
          <Input value={dispatchTask.prompt} onChange={(e) => setDispatchTask({ ...dispatchTask, prompt: e.target.value })} placeholder={t("agent.taskDescription") + "..."} className="w-48 h-7 text-xs" />
          <Button size="sm" onClick={handleDispatch} disabled={!dispatchTask.agentId || !dispatchTask.prompt.trim()}>{t("agent.dispatch")}</Button>
          <Button size="sm" variant="ghost" onClick={() => { setShowDispatch(false); setDispatchTask({ agentId: "", prompt: "" }); }}>{t("common.cancel")}</Button>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        {/* Left sidebar: agent list */}
        <div className="w-56 border-r bg-card overflow-y-auto p-3">
          <div className="text-[11px] text-muted-foreground mb-2">{t("agent.teamTitle")}</div>
          {agents.length === 0 && <div className="text-xs text-muted-foreground py-2">{t("agent.emptyState")}</div>}
          <div className="space-y-2">
            {agents.map((a) => {
              const isOnline = a.is_online ?? (a.status === "Online" || a.status === "Idle");
              const hbText = timeAgo(a.last_heartbeat ?? null, t);
              return (
                <button
                  key={a.id}
                  onClick={() => setSelectedAgent(a.id)}
                  className={`w-full rounded-md border p-2.5 transition-all hover:shadow-card ${
                    selectedAgent === a.id ? "ring-2 ring-primary shadow-card" : ""
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <HeartbeatDot online={isOnline} />
                    <svg className="w-4 h-4 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d={AGENT_AVATARS[a.id] ?? AGENT_AVATARS["maple-core"]} />
                    </svg>
                    <div className="flex items-center justify-between flex-1">
                      <span className="text-[13px] font-medium">{a.name}</span>
                      <Badge variant={agentStatusVariant[a.status] ?? "outline"} className="text-[10px]">{t(agentStatusKey[a.status] ?? a.status)}</Badge>
                    </div>
                  </div>
                  <div className="text-[11px] text-muted-foreground font-mono mt-0.5">{a.id}</div>
                  {hbText && (
                    <div className="text-[10px] text-muted-foreground mt-0.5 flex items-center gap-1">
                      <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
                      {hbText}
                    </div>
                  )}
                  {a.description && <div className="text-[10px] text-muted-foreground mt-0.5 line-clamp-1">{a.description}</div>}
                  {a.model && <div className="text-[11px] text-muted-foreground mt-0.5">{t("agent.model", { model: a.model })}</div>}
                  {a.tags && a.tags.length > 0 && (
                    <div className="flex flex-wrap gap-0.5 mt-0.5">
                      {a.tags.map((tag) => <Badge key={tag} variant="outline" className="text-[9px] px-1">{tag}</Badge>)}
                    </div>
                  )}
                  <Button
                    size="sm"
                    variant="destructive"
                    className="w-full mt-1.5 h-6 text-[10px]"
                    onClick={(e) => { e.stopPropagation(); handleDeregister(a.id); }}
                  >
                    {t("common.delete")}
                  </Button>
                </button>
              );
            })}
          </div>

          <div className="text-[11px] text-muted-foreground mb-2 mt-4">{t("agent.llmModels")}</div>
          <div className="flex flex-wrap gap-1">
            {models.map((m) => <Badge key={m.id} variant="secondary" className="text-[10px]">{m.name ?? m.id} ({m.provider})</Badge>)}
          </div>

          <div className="text-[11px] text-muted-foreground mb-2 mt-4">{t("agent.skills")}</div>
          <div className="flex flex-wrap gap-1">
            {skills.map((s) => <Badge key={s.id} variant="outline" className="text-[10px]">{s.id}</Badge>)}
          </div>
        </div>

        {/* Main content area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Agent health detail panel (when agent selected) */}
          {selectedAgentData && (
            <div className="border-b bg-card p-3">
              <div className="flex items-center gap-3">
                <HeartbeatDot online={selectedAgentData.is_online ?? selectedAgentData.status === "Online"} />
                <div>
                  <div className="text-[13px] font-medium">{selectedAgentData.name}</div>
                  <div className="text-[11px] text-muted-foreground font-mono">{selectedAgentData.id}</div>
                </div>
                <Badge variant={agentStatusVariant[selectedAgentData.status] ?? "outline"} className="text-[10px]">{t(agentStatusKey[selectedAgentData.status] ?? selectedAgentData.status)}</Badge>
                {selectedAgentData.last_heartbeat && (
                  <span className="text-[11px] text-muted-foreground">
                    {t("agent.lastSeen")}: {timeAgo(selectedAgentData.last_heartbeat, t)}
                  </span>
                )}
              </div>
              {selectedAgentData.description && (
                <div className="text-[11px] text-muted-foreground mt-1">{selectedAgentData.description}</div>
              )}
            </div>
          )}

          {taskStats && (
            <div className="border-b bg-card p-3">
              <div className="grid grid-cols-6 gap-2">
                <MiniStat label={t("dashboard.status.pending")} value={taskStats.pending} color="text-warning" />
                <MiniStat label={t("dashboard.status.running")} value={taskStats.running} color="text-primary" />
                <MiniStat label={t("dashboard.status.completed")} value={taskStats.completed} color="text-success" />
                <MiniStat label={t("dashboard.status.failed")} value={taskStats.failed} color="text-destructive" />
                <MiniStat label={t("dashboard.status.deadLetter")} value={taskStats.dead_letter} color="text-muted-foreground" />
                <MiniStat label={t("dashboard.status.total")} value={taskStats.total} />
              </div>
            </div>
          )}

          <div className="flex-1 overflow-y-auto p-4 space-y-3">
            {collabMessages.map((msg, i) => (
              <div key={i} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                <div className={`max-w-[80%] rounded-lg px-3 py-2 ${
                  msg.role === "user" ? "bg-primary text-primary-foreground" :
                  msg.role === "agent" ? "bg-card border shadow-card" :
                  "bg-muted text-muted-foreground"
                }`}>
                  {msg.role === "agent" && msg.agentId && (
                    <div className="text-[10px] font-medium mb-0.5 opacity-70">{msg.agentId}</div>
                  )}
                  {msg.role === "system" && (
                    <div className="text-[10px] font-medium mb-0.5">{t("chat.role.system")}</div>
                  )}
                  <div className="text-[13px] leading-snug">{msg.content}</div>
                </div>
              </div>
            ))}
            {collabMessages.length === 0 && (
              <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                {t("agent.collabEmpty")}
              </div>
            )}
          </div>

          <div className="border-t bg-card p-3 flex gap-2">
            <div className="flex-1 flex gap-2">
              <Input
                value={collabInput}
                onChange={(e) => setCollabInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleCollabSend(); } }}
                placeholder={selectedAgent ? t("chat.placeholder.withAgent", { name: agents.find((a) => a.id === selectedAgent)?.name ?? selectedAgent }) : t("chat.placeholder.default")}
                className="h-8 text-xs"
                disabled={collabSending}
              />
              <Button size="sm" onClick={handleCollabSend} disabled={!collabInput.trim() || collabSending}>
                {collabSending ? t("chat.sending") : t("chat.send")}
              </Button>
            </div>
          </div>

          <div className="border-t bg-muted/30 p-3">
            <div className="text-[11px] text-muted-foreground mb-1.5">{t("agent.memorySearch")}</div>
            <div className="flex gap-2">
              <Input value={memoryQuery} onChange={(e) => setMemoryQuery(e.target.value)} placeholder={t("agent.searchMemory")} className="h-7 text-xs" />
              <Button size="sm" variant="outline" onClick={handleMemorySearch} className="text-xs">{t("common.search")}</Button>
              <Button size="sm" variant="ghost" onClick={handleWriteMemory} className="text-xs">{t("agent.memoryWrite")}</Button>
            </div>
            <div className="mt-2 space-y-1 max-h-32 overflow-y-auto">
              {memories.map((m) => (
                <div key={m.id} className="rounded border p-1.5 bg-card">
                  <div className="flex items-center justify-between">
                    <Badge variant="outline" className="text-[10px]">{m.type}</Badge>
                    <span className="text-[10px] text-muted-foreground font-mono">{m.id.slice(0, 8)}</span>
                  </div>
                  <div className="text-[11px] text-muted-foreground line-clamp-1">{m.content}</div>
                </div>
              ))}
              {memories.length === 0 && <div className="text-[11px] text-muted-foreground">{t("agent.noMemory")}</div>}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function MiniStat({ label, value, color }: { label: string; value: number; color?: string }) {
  return (
    <div className="rounded-md bg-muted/50 p-1.5 text-center">
      <div className="text-[10px] text-muted-foreground">{label}</div>
      <div className={`text-[15px] font-semibold ${color ?? ""}`}>{value}</div>
    </div>
  );
}
