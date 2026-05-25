"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, Badge, Button, Spinner, Input, Textarea } from "@mapleos/ui";
import { rpcCall, mapleApi } from "@/lib/api";

interface AgentListItem { id: string; name: string; status: string; model?: string; skills?: string[] }
interface ModelInfo { id: string; name: string; provider: string }
interface SkillInfo { id: string; description: string }
interface TaskStats { total: number; pending: number; running: number; completed: number; failed: number; dead_letter: number }
interface MemoryEntry { id: string; content: string; type: string; metadata: Record<string, string>; created_at: number; access_count: number }
interface CollabMessage { role: "user" | "agent" | "system"; agentId?: string; content: string; timestamp: number }

const agentStatusLabel: Record<string, string> = { Idle: "空闲", Busy: "忙碌", Offline: "离线", idle: "空闲", busy: "忙碌", offline: "离线" };
const agentStatusVariant: Record<string, "default" | "secondary" | "outline"> = { Idle: "default", Busy: "secondary", Offline: "outline", idle: "default", busy: "secondary", offline: "outline" };

const AGENT_AVATARS: Record<string, string> = {
  "maple-core": "M12 2L2 7l10 5 10-5-10-5z",
  "maple-coder": "M16 18l2-2-2-2M8 18l-2-2 2-2M14.5 4l-5 16",
  "maple-analyst": "M12 20V10M18 20V4M6 20v-6",
  "maple-writer": "M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z",
  "maple-reviewer": "M9 12l2 2 4-4m6 2a9 9 0 1 1-18 0 9 9 0 0 1 18 0z",
};

export function AgentManager() {
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
  const [showRegister, setShowRegister] = useState(false);
  const [dispatchTask, setDispatchTask] = useState({ agentId: "", prompt: "" });
  const [showDispatch, setShowDispatch] = useState(false);

  const loadAll = async () => {
    try { const r = await rpcCall<{ agents: AgentListItem[] }>("agent.list"); setAgents(r.agents ?? []); } catch { setAgents([]); }
    try { const r = await rpcCall<{ models: ModelInfo[] }>("llm.models"); setModels(r.models ?? []); } catch { setModels([]); }
    try { const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list"); setSkills(r.skills ?? []); } catch { setSkills([]); }
    try { const s = await mapleApi<TaskStats>("/api/tasks/stats"); setTaskStats(s); } catch { setTaskStats(null); }
    try { const m = await mapleApi<{ results: MemoryEntry[] }>("/api/memories/search", { method: "POST", body: { keyword: "", memory_type: "working", limit: 10 } }); setMemories((m.results ?? [])); } catch { setMemories([]); }
    setLoading(false);
  };

  useEffect(() => { loadAll(); }, []);

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
        content: result.response ?? "（无响应）",
        timestamp: Date.now(),
      }]);
    } catch (err) {
      setCollabMessages((prev) => [...prev, {
        role: "system",
        content: `请求失败: ${(err as Error).message}`,
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

  const handleRegister = async () => {
    if (!registerName.trim()) return;
    try {
      await rpcCall("agent.register", { name: registerName.trim() });
      setShowRegister(false); setRegisterName("");
      setCollabMessages((prev) => [...prev, { role: "system", content: `Agent "${registerName}" 已注册`, timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: `注册失败: ${(err as Error).message}`, timestamp: Date.now() }]);
    }
  };

  const handleDispatch = async () => {
    if (!dispatchTask.agentId || !dispatchTask.prompt.trim()) return;
    try {
      await rpcCall("task.create", { agent_id: dispatchTask.agentId, prompt: dispatchTask.prompt.trim() });
      setShowDispatch(false); setDispatchTask({ agentId: "", prompt: "" });
      setCollabMessages((prev) => [...prev, { role: "system", content: `任务已派发给 ${dispatchTask.agentId}`, timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: `派发失败: ${(err as Error).message}`, timestamp: Date.now() }]);
    }
  };

  const handleWriteMemory = async () => {
    try {
      const key = `user-note-${Date.now()}`;
      await mapleApi("/api/memories", { method: "POST", body: { content: memoryQuery, memory_type: "note" } });
      setMemoryQuery("");
      setCollabMessages((prev) => [...prev, { role: "system", content: `记忆已写入: ${key}`, timestamp: Date.now() }]);
      await loadAll();
    } catch (err) {
      setCollabMessages((prev) => [...prev, { role: "system", content: `写入失败: ${(err as Error).message}`, timestamp: Date.now() }]);
    }
  };

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <h2 className="text-[15px] font-semibold">Agent 中心</h2>
        <div className="flex gap-2">
          <Button size="sm" onClick={loadAll}>刷新</Button>
          <Button size="sm" onClick={() => setShowRegister(true)}>注册</Button>
          <Button size="sm" variant="outline" onClick={() => setShowDispatch(true)}>派发任务</Button>
        </div>
      </div>

      {showRegister && (
        <div className="h-9 border-b bg-muted/50 flex items-center gap-2 px-4">
          <Input value={registerName} onChange={(e) => setRegisterName(e.target.value)} placeholder="Agent 名称..." className="w-40 h-7 text-xs" />
          <Button size="sm" onClick={handleRegister} disabled={!registerName.trim()}>确认注册</Button>
          <Button size="sm" variant="ghost" onClick={() => { setShowRegister(false); setRegisterName(""); }}>取消</Button>
        </div>
      )}

      {showDispatch && (
        <div className="h-9 border-b bg-muted/50 flex items-center gap-2 px-4">
          <select value={dispatchTask.agentId} onChange={(e) => setDispatchTask({ ...dispatchTask, agentId: e.target.value })} className="h-7 rounded border bg-background text-xs px-2">
            <option value="">选择 Agent</option>
            {agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          </select>
          <Input value={dispatchTask.prompt} onChange={(e) => setDispatchTask({ ...dispatchTask, prompt: e.target.value })} placeholder="任务描述..." className="w-48 h-7 text-xs" />
          <Button size="sm" onClick={handleDispatch} disabled={!dispatchTask.agentId || !dispatchTask.prompt.trim()}>派发</Button>
          <Button size="sm" variant="ghost" onClick={() => { setShowDispatch(false); setDispatchTask({ agentId: "", prompt: "" }); }}>取消</Button>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        <div className="w-56 border-r bg-card overflow-y-auto p-3">
          <div className="text-[11px] text-muted-foreground mb-2">Agent 团队</div>
          {agents.length === 0 && <div className="text-xs text-muted-foreground py-2">暂无 Agent，点击注册添加</div>}
          <div className="space-y-2">
            {agents.map((a) => (
              <button
                key={a.id}
                onClick={() => setSelectedAgent(a.id)}
                className={`w-full rounded-md border p-2.5 transition-all hover:shadow-card ${
                  selectedAgent === a.id ? "ring-2 ring-primary shadow-card" : ""
                }`}
              >
                <div className="flex items-center gap-2">
                  <svg className={`w-4 h-4 text-primary`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d={AGENT_AVATARS[a.id] ?? AGENT_AVATARS["maple-core"]} />
                  </svg>
                  <div className="flex items-center justify-between flex-1">
                    <span className="text-[13px] font-medium">{a.name}</span>
                    <Badge variant={agentStatusVariant[a.status] ?? "outline"} className="text-[10px]">{agentStatusLabel[a.status] ?? a.status}</Badge>
                  </div>
                </div>
                <div className="text-[11px] text-muted-foreground font-mono mt-0.5">{a.id}</div>
                {a.model && <div className="text-[11px] text-muted-foreground mt-0.5">模型: {a.model}</div>}
              </button>
            ))}
          </div>

          <div className="text-[11px] text-muted-foreground mb-2 mt-4">LLM 模型</div>
          <div className="flex flex-wrap gap-1">
            {models.map((m) => <Badge key={m.id} variant="secondary" className="text-[10px]">{m.name ?? m.id} ({m.provider})</Badge>)}
          </div>

          <div className="text-[11px] text-muted-foreground mb-2 mt-4">可用技能</div>
          <div className="flex flex-wrap gap-1">
            {skills.map((s) => <Badge key={s.id} variant="outline" className="text-[10px]">{s.id}</Badge>)}
          </div>
        </div>

        <div className="flex-1 flex flex-col overflow-hidden">
          {taskStats && (
            <div className="border-b bg-card p-3">
              <div className="grid grid-cols-6 gap-2">
                <MiniStat label="等待" value={taskStats.pending} color="text-warning" />
                <MiniStat label="运行" value={taskStats.running} color="text-primary" />
                <MiniStat label="完成" value={taskStats.completed} color="text-success" />
                <MiniStat label="失败" value={taskStats.failed} color="text-destructive" />
                <MiniStat label="死信" value={taskStats.dead_letter} color="text-muted-foreground" />
                <MiniStat label="总计" value={taskStats.total} />
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
                    <div className="text-[10px] font-medium mb-0.5">系统</div>
                  )}
                  <div className="text-[13px] leading-snug">{msg.content}</div>
                </div>
              </div>
            ))}
            {collabMessages.length === 0 && (
              <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                选择 Agent 后开始协作对话
              </div>
            )}
          </div>

          <div className="border-t bg-card p-3 flex gap-2">
            <div className="flex-1 flex gap-2">
              <Input
                value={collabInput}
                onChange={(e) => setCollabInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleCollabSend(); } }}
                placeholder={selectedAgent ? `与 ${agents.find((a) => a.id === selectedAgent)?.name ?? selectedAgent} 对话...` : "输入消息..."}
                className="h-8 text-xs"
                disabled={collabSending}
              />
              <Button size="sm" onClick={handleCollabSend} disabled={!collabInput.trim() || collabSending}>
                {collabSending ? "发送中..." : "发送"}
              </Button>
            </div>
          </div>

          <div className="border-t bg-muted/30 p-3">
            <div className="text-[11px] text-muted-foreground mb-1.5">记忆搜索</div>
            <div className="flex gap-2">
              <Input value={memoryQuery} onChange={(e) => setMemoryQuery(e.target.value)} placeholder="搜索记忆或写入笔记..." className="h-7 text-xs" />
              <Button size="sm" variant="outline" onClick={handleMemorySearch} className="text-xs">搜索</Button>
              <Button size="sm" variant="ghost" onClick={handleWriteMemory} className="text-xs">写入</Button>
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
              {memories.length === 0 && <div className="text-[11px] text-muted-foreground">暂无记忆</div>}
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