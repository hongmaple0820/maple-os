"use client";

import { useState, useEffect } from "react";
import { ChatPanel } from "@/components/chat-panel";
import { WorkflowManager } from "@/components/workflow-manager";
import { KnowledgeManager } from "@/components/knowledge-manager";
import { AgentManager } from "@/components/agent-manager";
import { ScaleEngineManager } from "@/components/scale-engine-manager";
import { CommandPalette } from "@/components/command-palette";
import { SettingsPage } from "@/components/settings-page";
import { PluginsPage } from "@/components/plugins-page";
import { Badge, Button } from "@mapleos/ui";
import { rpcCall, mapleApi } from "@/lib/api";

const NAV_ITEMS = [
  { id: "dashboard", label: "工作台", icon: "layout-dashboard" },
  { id: "chat", label: "对话", icon: "message-square" },
  { id: "workflows", label: "工作流", icon: "git-branch" },
  { id: "agents", label: "Agent", icon: "bot" },
  { id: "knowledge", label: "知识库", icon: "book-open" },
  { id: "scale", label: "SCALE 引擎", icon: "shield" },
  { id: "plugins", label: "插件", icon: "puzzle" },
  { id: "settings", label: "设置", icon: "settings" },
] as const;

type NavId = (typeof NAV_ITEMS)[number]["id"];

interface SystemInfo {
  version: string;
  uptime_secs: number;
  agents_count: number;
  workflows_count: number;
  tasks_count: number;
}

interface TaskStats {
  total: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  dead_letter: number;
}

const iconPaths: Record<string, string> = {
  "layout-dashboard": "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
  "message-square": "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
  "git-branch": "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6M18 9c0 3-4 6-8 6",
  "bot": "M12 8V4H8M16 8V4h-4M8 16v4M16 16v4M3 8h18v8H3z",
  "book-open": "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2zM22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z",
  "shield": "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
  "puzzle": "M19.439 7.85c-.049.322.059.648.289.878l1.658 1.658a1.2 1.2 0 0 1 0 1.697l-2.608 2.608a1.2 1.2 0 0 1-1.697 0L15.72 12.45c-.23-.23-.556-.338-.878-.289a3.722 3.722 0 0 1-2.036-.18 1.2 1.2 0 0 0-1.378 1.378 3.722 3.722 0 0 1 .18 2.036c-.049.322.059.648.289.878l1.658 1.658a1.2 1.2 0 0 1 0 1.697l-2.608 2.608a1.2 1.2 0 0 1-1.697 0L7.85 19.439c-.23-.23-.556-.338-.878-.289a3.722 3.722 0 0 1-2.036.18 1.2 1.2 0 0 0-1.378-1.378 3.722 3.722 0 0 1 .18-2.036c.049-.322-.059-.648-.289-.878L3.98 14.497a1.2 1.2 0 0 1 0-1.697l2.608-2.608a1.2 1.2 0 0 1 1.697 0L8.28 11.55c.23.23.556.338.878.289a3.722 3.722 0 0 1 2.036-.18 1.2 1.2 0 0 0 1.378-1.378 3.722 3.722 0 0 1-.18-2.036c-.049-.322.059-.648.289-.878l1.658-1.658a1.2 1.2 0 0 1 0-1.697l2.608-2.608a1.2 1.2 0 0 1 1.697 0l1.658 1.658c.23.23.556.338.878.289a3.722 3.722 0 0 1 2.036.18 1.2 1.2 0 0 0 1.378 1.378 3.722 3.722 0 0 1-.18 2.036z",
  "settings": "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
};

export default function Home() {
  const [activeNav, setActiveNav] = useState<NavId>("dashboard");
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);
  const [taskStats, setTaskStats] = useState<TaskStats | null>(null);
  const [serverOnline, setServerOnline] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);

  const pollData = async () => {
    try {
      const info = await rpcCall<SystemInfo>("system.info");
      setSysInfo(info);
      const stats = await mapleApi<TaskStats>("/api/tasks/stats");
      setTaskStats(stats);
      setServerOnline(true);
    } catch {
      setServerOnline(false);
    }
  };

  useEffect(() => {
    pollData();
    const interval = setInterval(pollData, 10000);
    return () => clearInterval(interval);
  }, []);

  const handleCommandNavigate = (id: string) => {
    if (id === "open-palette") { setPaletteOpen(true); return; }
    if (id.startsWith("nav-")) {
      const navId = id.replace("nav-", "");
      if (NAV_ITEMS.find((n) => n.id === navId)) setActiveNav(navId as NavId);
    }
    if (id === "wf-create") setActiveNav("workflows");
    if (id === "kb-search" || id === "kb-index") setActiveNav("knowledge");
  };

  return (
    <div className="flex h-screen flex-col">
      {/* Top Navigation */}
      <header className="h-12 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-3">
          <svg className="w-5 h-5 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
          <span className="font-semibold text-[15px]">MapleOS</span>
          <Badge variant="outline" className="text-[10px] font-mono">v{sysInfo?.version ?? "0.1.0"}</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={serverOnline ? "default" : "destructive"} className="text-xs">
            {serverOnline ? "已连接" : "离线"}
          </Badge>
          <Button variant="ghost" size="sm" onClick={() => setPaletteOpen(true)} className="text-xs font-mono">
            &#8984;K 命令
          </Button>
        </div>
      </header>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <aside className="w-44 border-r bg-card flex flex-col">
          <nav className="flex-1 p-2 space-y-0.5">
            {NAV_ITEMS.map((item) => (
              <button
                key={item.id}
                onClick={() => setActiveNav(item.id)}
                className={`w-full text-left px-3 py-1.5 rounded-md text-[13px] transition-colors flex items-center gap-2 ${
                  activeNav === item.id
                    ? "bg-primary/10 text-primary font-medium"
                    : "text-muted-foreground hover:bg-accent"
                }`}
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <path d={iconPaths[item.icon] ?? ""} />
                </svg>
                {item.label}
              </button>
            ))}
          </nav>

          {sysInfo && (
            <div className="p-3 border-t text-[11px] text-muted-foreground">
              {sysInfo.agents_count} Agent / {sysInfo.workflows_count} 工作流
              {taskStats && (
                <span> / {taskStats.total} 任务</span>
              )}
            </div>
          )}
        </aside>

        {/* Main Workspace */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {activeNav === "dashboard" && <DashboardView sysInfo={sysInfo} taskStats={taskStats} serverOnline={serverOnline} />}
          {activeNav === "chat" && <ChatPanel />}
          {activeNav === "workflows" && <WorkflowManager />}
          {activeNav === "agents" && <AgentManager />}
          {activeNav === "knowledge" && <KnowledgeManager />}
          {activeNav === "scale" && <ScaleEngineManager />}
          {activeNav === "plugins" && <PluginsPage />}
          {activeNav === "settings" && <SettingsPage />}
        </main>
      </div>

      {/* Bottom Command Dock */}
      <footer className="h-8 border-t bg-card flex items-center justify-between px-4 text-[11px] text-muted-foreground">
        <div className="flex items-center gap-3">
          {taskStats && (
            <>
              <span className="text-success">{taskStats.completed} 完成</span>
              <span className="text-warning">{taskStats.running} 运行</span>
              <span>{taskStats.pending} 等待</span>
              {taskStats.failed > 0 && <span className="text-destructive">{taskStats.failed} 失败</span>}
            </>
          )}
        </div>
        <div className="flex items-center gap-2 font-mono">
          <span>7788</span>
          <span>&middot;</span>
          <span>&#8984;K 命令面板</span>
        </div>
      </footer>

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} onNavigate={handleCommandNavigate} />
    </div>
  );
}

function DashboardView({ sysInfo, taskStats, serverOnline }: { sysInfo: SystemInfo | null; taskStats: TaskStats | null; serverOnline: boolean }) {
  const uptimeMin = sysInfo ? Math.floor(sysInfo.uptime_secs / 60) : 0;
  const uptimeHour = Math.floor(uptimeMin / 60);
  const uptimeDisplay = uptimeHour > 0 ? `${uptimeHour}h ${uptimeMin % 60}m` : `${uptimeMin}m`;

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-h2">工作台</h2>
        <div className="flex items-center gap-2">
          <Badge variant={serverOnline ? "default" : "destructive"} className="text-xs">{serverOnline ? "系统正常" : "服务离线"}</Badge>
          {sysInfo && <Badge variant="outline" className="text-[10px] font-mono">运行 {uptimeDisplay}</Badge>}
        </div>
      </div>

      <div className="grid grid-cols-4 gap-3">
        <MetricCard label="Agent" value={sysInfo?.agents_count ?? 0} color="primary" icon="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 14a4 4 0 1 1 4-4 4 4 0 0 1-4 4z" />
        <MetricCard label="工作流" value={sysInfo?.workflows_count ?? 0} color="secondary" icon="M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6M18 9c0 3-4 6-8 6" />
        <MetricCard label="任务总数" value={taskStats?.total ?? 0} color="foreground" icon="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 5a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2m-6 9 2 2 4-4" />
        <MetricCard label="运行中" value={taskStats?.running ?? 0} color="warning" icon="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83" />
      </div>

      {taskStats && (
        <div className="bg-card border rounded-lg p-4 shadow-card">
          <h3 className="text-h3 mb-3">任务队列</h3>
          <div className="grid grid-cols-6 gap-3 text-center">
            <StatusCell label="等待" value={taskStats.pending} color="text-warning" />
            <StatusCell label="运行" value={taskStats.running} color="text-primary" />
            <StatusCell label="完成" value={taskStats.completed} color="text-success" />
            <StatusCell label="失败" value={taskStats.failed} color="text-destructive" />
            <StatusCell label="死信" value={taskStats.dead_letter} color="text-muted-foreground" />
            <StatusCell label="总计" value={taskStats.total} />
          </div>
          {taskStats.total > 0 && (
            <div className="mt-3 flex gap-1 h-3 rounded-full overflow-hidden bg-muted">
              {taskStats.completed > 0 && <div className="bg-success rounded-full" style={{ width: `${(taskStats.completed / taskStats.total) * 100}%` }} />}
              {taskStats.running > 0 && <div className="bg-primary rounded-full" style={{ width: `${(taskStats.running / taskStats.total) * 100}%` }} />}
              {taskStats.pending > 0 && <div className="bg-warning rounded-full" style={{ width: `${(taskStats.pending / taskStats.total) * 100}%` }} />}
              {taskStats.failed > 0 && <div className="bg-destructive rounded-full" style={{ width: `${(taskStats.failed / taskStats.total) * 100}%` }} />}
            </div>
          )}
        </div>
      )}

      <div className="bg-card border rounded-lg p-4 shadow-card">
        <h3 className="text-h3 mb-3">快速操作</h3>
        <div className="grid grid-cols-4 gap-2">
          <QuickAction icon="M6 3v12M18 9a3 3 0 1 0 0-6" label="新建工作流" navId="workflows" />
          <QuickAction icon="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" label="搜索知识库" navId="knowledge" />
          <QuickAction icon="M12 2a10 10 0 1 0 10 10" label="注册 Agent" navId="agents" />
          <QuickAction icon="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" label="开始对话" navId="chat" />
        </div>
      </div>

      {sysInfo && (
        <div className="bg-card border rounded-lg p-4 shadow-card">
          <h3 className="text-h3 mb-2">系统信息</h3>
          <div className="grid grid-cols-2 gap-3 text-[13px]">
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">版本</div>
              <div className="font-mono font-medium">{sysInfo.version}</div>
            </div>
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">运行时长</div>
              <div className="font-mono font-medium">{uptimeDisplay}</div>
            </div>
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">Agent 数量</div>
              <div className="font-mono font-medium">{sysInfo.agents_count}</div>
            </div>
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">工作流数量</div>
              <div className="font-mono font-medium">{sysInfo.workflows_count}</div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function QuickAction({ icon, label, navId }: { icon: string; label: string; navId: string }) {
  return (
    <button
      className="flex items-center gap-2 rounded-md border p-3 hover:bg-accent hover:shadow-card transition-all text-left"
    >
      <svg className="w-4 h-4 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d={icon} /></svg>
      <span className="text-[13px]">{label}</span>
    </button>
  );
}

function MetricCard({ label, value, color, icon }: { label: string; value: number; color: string; icon: string }) {
  return (
    <div className="bg-card border rounded-md p-3 shadow-card">
      <div className="flex items-center gap-1.5 mb-1">
        <svg className={`w-3.5 h-3.5 text-${color}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d={icon} /></svg>
        <div className="text-[11px] text-muted-foreground">{label}</div>
      </div>
      <div className={`text-metric text-${color}`}>{value}</div>
    </div>
  );
}

function StatusCell({ label, value, color }: { label: string; value: number; color?: string }) {
  return (
    <div className="rounded-md bg-muted/50 p-2 text-center">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className={`text-[18px] font-semibold ${color ?? ""}`}>{value}</div>
    </div>
  );
}

function PlaceholderView({ title, desc }: { title: string; desc: string }) {
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-center space-y-2">
        <h2 className="text-h2">{title}</h2>
        <p className="text-muted-foreground text-sm">{desc}</p>
      </div>
    </div>
  );
}