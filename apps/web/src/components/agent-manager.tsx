"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, Badge, Button, Spinner } from "@mapleos/ui";
import { rpcCall, mapleApi } from "@/lib/api";

interface AgentListItem { id: string; name: string; status: string }
interface ModelInfo { id: string; name: string; provider: string }
interface SkillInfo { id: string; description: string }
interface TaskStats { total: number; pending: number; running: number; completed: number; failed: number; dead_letter: number }
interface MemoryEntry { id: string; key: string; value: string; type: string; created_at: number }

const agentStatusLabel: Record<string, string> = { Idle: "空闲", Busy: "忙碌", Offline: "离线", idle: "空闲", busy: "忙碌", offline: "离线" };
const agentStatusVariant: Record<string, "default" | "secondary" | "outline"> = { Idle: "default", Busy: "secondary", Offline: "outline", idle: "default", busy: "secondary", offline: "outline" };

export function AgentManager() {
  const [agents, setAgents] = useState<AgentListItem[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [taskStats, setTaskStats] = useState<TaskStats | null>(null);
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [loading, setLoading] = useState(true);

  const loadAll = async () => {
    try { const r = await rpcCall<{ agents: AgentListItem[] }>("agent.list"); setAgents(r.agents ?? []); } catch { setAgents([]); }
    try { const r = await rpcCall<{ models: ModelInfo[] }>("llm.models"); setModels(r.models ?? []); } catch { setModels([]); }
    try { const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list"); setSkills(r.skills ?? []); } catch { setSkills([]); }
    try { const s = await mapleApi<TaskStats>("/api/tasks/stats"); setTaskStats(s); } catch { setTaskStats(null); }
    try { const m = await mapleApi<MemoryEntry[]>("/api/memories/search?query=&limit=10"); setMemories(m ?? []); } catch { setMemories([]); }
    setLoading(false);
  };

  useEffect(() => { loadAll(); }, []);

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <h2 className="text-[15px] font-semibold">Agent 中心</h2>
        <Button size="sm" onClick={loadAll}>刷新</Button>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Agent Members Panel */}
        <div className="w-56 border-r bg-card overflow-y-auto p-3">
          <div className="text-[11px] text-muted-foreground mb-2">已注册 Agent</div>
          {agents.length === 0 && <div className="text-xs text-muted-foreground py-2">暂无 Agent</div>}
          <div className="space-y-2">
            {agents.map((a) => (
              <div key={a.id} className="rounded-md border p-2 hover:shadow-card transition-shadow">
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-medium">{a.name}</span>
                  <Badge variant={agentStatusVariant[a.status] ?? "outline"} className="text-[10px]">{agentStatusLabel[a.status] ?? a.status}</Badge>
                </div>
                <div className="text-[11px] text-muted-foreground font-mono mt-0.5">{a.id}</div>
              </div>
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

        {/* Main: Task Timeline + Task Queue */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {taskStats && (
            <Card className="shadow-card">
              <CardContent className="p-3">
                <div className="text-[13px] font-medium mb-2">任务队列</div>
                <div className="grid grid-cols-6 gap-2 text-center">
                  <MiniStat label="等待" value={taskStats.pending} />
                  <MiniStat label="运行" value={taskStats.running} />
                  <MiniStat label="完成" value={taskStats.completed} />
                  <MiniStat label="失败" value={taskStats.failed} />
                  <MiniStat label="死信" value={taskStats.dead_letter} />
                  <MiniStat label="总计" value={taskStats.total} />
                </div>
              </CardContent>
            </Card>
          )}

          {/* Memory Timeline */}
          <Card className="shadow-card">
            <CardContent className="p-3">
              <div className="text-[13px] font-medium mb-2">最近记忆</div>
              {memories.length === 0 && <div className="text-xs text-muted-foreground">暂无记忆记录</div>}
              <div className="space-y-1.5">
                {memories.map((m) => (
                  <div key={m.id} className="rounded border p-2">
                    <div className="flex items-center justify-between">
                      <span className="text-[13px]">{m.key}</span>
                      <Badge variant="outline" className="text-[10px]">{m.type}</Badge>
                    </div>
                    <div className="text-[11px] text-muted-foreground mt-0.5 line-clamp-2">{m.value}</div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Right: Quick Actions */}
        <div className="w-44 border-l bg-card overflow-y-auto p-3">
          <div className="text-[11px] text-muted-foreground mb-2">快速操作</div>
          <div className="space-y-1.5">
            <Button size="sm" className="w-full text-xs">注册新 Agent</Button>
            <Button size="sm" variant="outline" className="w-full text-xs">分配任务</Button>
            <Button size="sm" variant="outline" className="w-full text-xs">查看死信队列</Button>
            <Button size="sm" variant="outline" className="w-full text-xs">写入记忆</Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function MiniStat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md bg-muted/50 p-1.5">
      <div className="text-[10px] text-muted-foreground">{label}</div>
      <div className="text-[15px] font-semibold">{value}</div>
    </div>
  );
}