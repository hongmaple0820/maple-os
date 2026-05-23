"use client";

import { useState, useEffect } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Spinner } from "@mapleos/ui";
import { rpcCall, mapleApi } from "@/lib/api";

interface AgentListItem { id: string; name: string; status: string }
interface ModelInfo { id: string; name: string; provider: string }
interface SkillInfo { id: string; description: string }
interface TaskStats { total: number; pending: number; running: number; completed: number; failed: number; dead_letter: number }

const agentStatusLabel: Record<string, string> = { Idle: "空闲", Busy: "忙碌", Offline: "离线", idle: "空闲", busy: "忙碌", offline: "离线" };
const agentStatusVariant: Record<string, "default" | "secondary" | "outline"> = { Idle: "default", Busy: "secondary", Offline: "outline", idle: "default", busy: "secondary", offline: "outline" };

export function AgentManager() {
  const [agents, setAgents] = useState<AgentListItem[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [taskStats, setTaskStats] = useState<TaskStats | null>(null);
  const [loading, setLoading] = useState(true);

  const loadAll = async () => {
    try { const agentResult = await rpcCall<{ agents: AgentListItem[] }>("agent.list"); setAgents(agentResult.agents ?? []); } catch { setAgents([]); }
    try { const modelResult = await rpcCall<{ models: ModelInfo[] }>("llm.models"); setModels(modelResult.models ?? []); } catch { setModels([]); }
    try { const skillResult = await rpcCall<{ skills: SkillInfo[] }>("skill.list"); setSkills(skillResult.skills ?? []); } catch { setSkills([]); }
    try { const stats = await mapleApi<TaskStats>("/api/tasks/stats"); setTaskStats(stats); } catch { setTaskStats(null); }
    setLoading(false);
  };

  useEffect(() => { loadAll(); }, []);

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">Agent 管理</h2>
        <Button onClick={loadAll}>刷新</Button>
      </div>
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        <section>
          <h3 className="text-sm font-medium mb-3 text-muted-foreground">已注册 Agent</h3>
          {agents.length === 0 && <p className="text-muted-foreground text-sm">暂无已注册的 Agent</p>}
          <div className="space-y-2">
            {agents.map((agent) => (
              <Card key={agent.id} className="hover:shadow-md transition-shadow">
                <CardContent className="p-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2"><Badge variant="outline" className="text-xs">{agent.id}</Badge><span className="font-medium text-sm">{agent.name}</span></div>
                    <Badge variant={agentStatusVariant[agent.status] ?? "outline"}>{agentStatusLabel[agent.status] ?? agent.status}</Badge>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </section>
        <section>
          <h3 className="text-sm font-medium mb-3 text-muted-foreground">可用 LLM 模型</h3>
          {models.length === 0 && <p className="text-muted-foreground text-sm">暂无可用模型</p>}
          <div className="flex flex-wrap gap-2">{models.map((m) => <Badge key={m.id} variant="secondary" className="text-xs">{m.name ?? m.id} ({m.provider})</Badge>)}</div>
        </section>
        <section>
          <h3 className="text-sm font-medium mb-3 text-muted-foreground">可用技能</h3>
          {skills.length === 0 && <p className="text-muted-foreground text-sm">暂无可用技能</p>}
          <div className="flex flex-wrap gap-2">{skills.map((s) => <Badge key={s.id} variant="outline" className="text-xs">{s.id}: {s.description}</Badge>)}</div>
        </section>
        {taskStats && (
          <section>
            <h3 className="text-sm font-medium mb-3 text-muted-foreground">任务队列统计</h3>
            <div className="grid grid-cols-3 gap-2">
              <Badge variant="outline" className="justify-center p-2">总计 {taskStats.total}</Badge>
              <Badge variant="secondary" className="justify-center p-2">等待 {taskStats.pending}</Badge>
              <Badge variant="default" className="justify-center p-2">运行 {taskStats.running}</Badge>
              <Badge variant="default" className="justify-center p-2">完成 {taskStats.completed}</Badge>
              <Badge variant="destructive" className="justify-center p-2">失败 {taskStats.failed}</Badge>
              <Badge variant="outline" className="justify-center p-2">死信 {taskStats.dead_letter}</Badge>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}