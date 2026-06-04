"use client";

import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ChatPanel } from "@/components/chat-panel";
import { WorkflowManager } from "@/components/workflow-manager";
import { KnowledgeManager } from "@/components/knowledge-manager";
import { AgentManager } from "@/components/agent-manager";
import { ScaleEngineManager } from "@/components/scale-engine-manager";
import { CommandPalette } from "@/components/command-palette";
import { SettingsPage } from "@/components/settings-page";
import { PluginsPage } from "@/components/plugins-page";
import CollaborationWorkspace from "@/components/collaboration/workspace-page";
import { Badge, Button } from "@mapleos/ui";
import { rpcCall, mapleApi, isAuthenticated, getAuthState, clearAuthState, setAuthState } from "@/lib/api";
import { AuthPage } from "@/components/auth-page";
import { LanguageSwitcher } from "@/components/language-switcher";
import { ModeSelection } from "@/components/mode-selection";

type NavId = "dashboard" | "chat" | "workflows" | "agents" | "knowledge" | "collaboration" | "scale" | "plugins" | "settings";

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
  "users": "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M16 3.128a4 4 0 0 1 0 7.744M22 21v-2a4 4 0 0 0-3-3.87",
  "shield": "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
  "puzzle": "M19.439 7.85c-.049.322.059.648.289.878l1.658 1.658a1.2 1.2 0 0 1 0 1.697l-2.608 2.608a1.2 1.2 0 0 1-1.697 0L15.72 12.45c-.23-.23-.556-.338-.878-.289a3.722 3.722 0 0 1-2.036-.18 1.2 1.2 0 0 0-1.378 1.378 3.722 3.722 0 0 1 .18 2.036c-.049.322.059.648.289.878l1.658 1.658a1.2 1.2 0 0 1 0 1.697l-2.608 2.608a1.2 1.2 0 0 1-1.697 0L7.85 19.439c-.23-.23-.556-.338-.878-.289a3.722 3.722 0 0 1-2.036.18 1.2 1.2 0 0 0-1.378-1.378 3.722 3.722 0 0 1 .18-2.036c.049-.322-.059-.648-.289-.878L3.98 14.497a1.2 1.2 0 0 1 0-1.697l2.608-2.608a1.2 1.2 0 0 1 1.697 0L8.28 11.55c.23.23.556.338.878.289a3.722 3.722 0 0 1 2.036-.18 1.2 1.2 0 0 0 1.378-1.378 3.722 3.722 0 0 1-.18-2.036c.049-.322.059-.648.289-.878l1.658-1.658a1.2 1.2 0 0 1 0-1.697l2.608-2.608a1.2 1.2 0 0 1 1.697 0l1.658 1.658c.23.23.556.338.878.289a3.722 3.722 0 0 1 2.036.18 1.2 1.2 0 0 0 1.378 1.378 3.722 3.722 0 0 1-.18 2.036z",
  "settings": "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
};

export default function Home() {
  const { t } = useTranslation();
  const [activeNav, setActiveNav] = useState<NavId>("dashboard");
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);
  const [taskStats, setTaskStats] = useState<TaskStats | null>(null);
  const [serverOnline, setServerOnline] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [authenticated, setAuthenticated] = useState(false);
  const [loading, setLoading] = useState(true);
  const [appMode, setAppMode] = useState<"local" | "cloud" | null>(null);

  useEffect(() => {
    const savedMode = localStorage.getItem("mapleos-mode") as "local" | "cloud" | null;
    if (savedMode === "local") {
      handleLocalMode();
    } else if (savedMode === "cloud") {
      setAppMode("cloud");
      setAuthenticated(isAuthenticated());
    }
    setLoading(false);
    const handleLogout = () => setAuthenticated(false);
    window.addEventListener("auth:logout", handleLogout);
    return () => window.removeEventListener("auth:logout", handleLogout);
  }, []);

  const getOrCreateDeviceId = (): string => {
    let deviceId = localStorage.getItem("mapleos-device-id");
    if (!deviceId) {
      deviceId = `device-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
      localStorage.setItem("mapleos-device-id", deviceId);
    }
    return deviceId;
  };

  const handleLocalMode = async () => {
    localStorage.setItem("mapleos-mode", "local");
    setAppMode("local");
    const deviceId = getOrCreateDeviceId();
    try {
      const data = await mapleApi<{ token?: string; user_id?: string; username?: string; role?: string; error?: string }>(
        "/api/auth/device-login",
        { method: "POST", body: { device_id: deviceId } }
      );
      if (data.token) {
        setAuthState(data.token, "", {
          user_id: data.user_id ?? "",
          username: data.username ?? deviceId.slice(0, 12),
          role: data.role ?? "user",
        });
        setAuthenticated(true);
      } else {
        setAuthState("local-token", "", {
          user_id: deviceId,
          username: deviceId.slice(0, 12),
          role: "user",
        });
        setAuthenticated(true);
      }
    } catch {
      setAuthState("local-token", "", {
        user_id: deviceId,
        username: deviceId.slice(0, 12),
        role: "user",
      });
      setAuthenticated(true);
    }
  };

  const handleCloudMode = () => {
    localStorage.setItem("mapleos-mode", "cloud");
    setAppMode("cloud");
  };

  const pollData = async () => {
    if (!authenticated) return;
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
    if (!authenticated) return;
    pollData();
    const interval = setInterval(pollData, 10000);
    return () => clearInterval(interval);
  }, [authenticated]);

  if (loading) {
    return (
      <div className="flex h-screen items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
      </div>
    );
  }

  if (!appMode) {
    return <ModeSelection onSelectLocal={handleLocalMode} onSelectCloud={handleCloudMode} />;
  }

  if (!authenticated) {
    return <AuthPage onAuth={() => setAuthenticated(true)} />;
  }

  const NAV_ITEMS: { id: NavId; labelKey: string; icon: string }[] = [
    { id: "dashboard", labelKey: "nav.dashboard", icon: "layout-dashboard" },
    { id: "chat", labelKey: "nav.chat", icon: "message-square" },
    { id: "workflows", labelKey: "nav.workflows", icon: "git-branch" },
    { id: "agents", labelKey: "nav.agents", icon: "bot" },
    { id: "knowledge", labelKey: "nav.knowledge", icon: "book-open" },
    { id: "collaboration", labelKey: "nav.collaboration", icon: "users" },
    { id: "scale", labelKey: "nav.scaleEngine", icon: "shield" },
    { id: "plugins", labelKey: "nav.plugins", icon: "puzzle" },
    { id: "settings", labelKey: "nav.settings", icon: "settings" },
  ];

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
          <span className="font-semibold text-[15px]">{t("common.appName")}</span>
          <Badge variant="outline" className="text-[10px] font-mono">v{sysInfo?.version ?? "0.1.0"}</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={serverOnline ? "default" : "destructive"} className="text-xs">
            {serverOnline ? t("common.online") : t("common.offline")}
          </Badge>
          {(() => { const u = getAuthState().user; return u ? <Badge variant="outline" className="text-[10px]">{u.username} ({u.role})</Badge> : null; })()}
          <LanguageSwitcher />
          <Button variant="ghost" size="sm" onClick={() => { clearAuthState(); setAuthenticated(false); }} className="text-xs">
            {t("auth.logout")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setPaletteOpen(true)} className="text-xs font-mono">
            &#8984;K
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
                {t(item.labelKey)}
              </button>
            ))}
          </nav>

          {sysInfo && (
            <div className="p-3 border-t text-[11px] text-muted-foreground">
              {sysInfo.agents_count} {t("dashboard.sidebar.agents")} / {sysInfo.workflows_count} {t("dashboard.sidebar.workflows")}
              {taskStats && (
                <span> / {taskStats.total} {t("dashboard.sidebar.tasks")}</span>
              )}
            </div>
          )}
        </aside>

        {/* Main Workspace */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {activeNav === "dashboard" && <DashboardView sysInfo={sysInfo} taskStats={taskStats} serverOnline={serverOnline} onNavigate={(id) => setActiveNav(id as NavId)} />}
          {activeNav === "chat" && <ChatPanel />}
          {activeNav === "workflows" && <WorkflowManager />}
          {activeNav === "agents" && <AgentManager />}
          {activeNav === "knowledge" && <KnowledgeManager />}
          {activeNav === "collaboration" && <CollaborationWorkspace />}
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
              <span className="text-success">{taskStats.completed} {t("dashboard.status.completed")}</span>
              <span className="text-warning">{taskStats.running} {t("dashboard.status.running")}</span>
              <span>{taskStats.pending} {t("dashboard.status.pending")}</span>
              {taskStats.failed > 0 && <span className="text-destructive">{taskStats.failed} {t("dashboard.status.failed")}</span>}
            </>
          )}
        </div>
        <div className="flex items-center gap-2 font-mono">
          <span>7788</span>
          <span>&middot;</span>
          <span>&#8984;K</span>
        </div>
      </footer>

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} onNavigate={handleCommandNavigate} />
    </div>
  );
}

function DashboardView({ sysInfo, taskStats, serverOnline, onNavigate }: { sysInfo: SystemInfo | null; taskStats: TaskStats | null; serverOnline: boolean; onNavigate?: (id: string) => void }) {
  const { t } = useTranslation();
  const uptimeMin = sysInfo ? Math.floor(sysInfo.uptime_secs / 60) : 0;
  const uptimeHour = Math.floor(uptimeMin / 60);
  const uptimeDisplay = uptimeHour > 0 ? `${uptimeHour}h ${uptimeMin % 60}m` : `${uptimeMin}m`;

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-h2">{t("dashboard.title")}</h2>
        <div className="flex items-center gap-2">
          <Badge variant={serverOnline ? "default" : "destructive"} className="text-xs">{serverOnline ? t("dashboard.systemNormal") : t("dashboard.serviceOffline")}</Badge>
          {sysInfo && <Badge variant="outline" className="text-[10px] font-mono">{t("dashboard.uptime", { time: uptimeDisplay })}</Badge>}
        </div>
      </div>

      <div className="grid grid-cols-4 gap-3">
        <MetricCard label={t("dashboard.metrics.agents")} value={sysInfo?.agents_count ?? 0} color="primary" icon="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 14a4 4 0 1 1 4-4 4 4 0 0 1-4 4z" />
        <MetricCard label={t("dashboard.metrics.workflows")} value={sysInfo?.workflows_count ?? 0} color="secondary" icon="M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6M6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6M18 9c0 3-4 6-8 6" />
        <MetricCard label={t("dashboard.metrics.totalTasks")} value={taskStats?.total ?? 0} color="foreground" icon="M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 5a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2m-6 9 2 2 4-4" />
        <MetricCard label={t("dashboard.metrics.running")} value={taskStats?.running ?? 0} color="warning" icon="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83" />
      </div>

      {taskStats && (
        <div className="bg-card border rounded-lg p-4 shadow-card">
          <h3 className="text-h3 mb-3">{t("dashboard.taskQueue")}</h3>
          <div className="grid grid-cols-6 gap-3 text-center">
            <StatusCell label={t("dashboard.status.pending")} value={taskStats.pending} color="text-warning" />
            <StatusCell label={t("dashboard.status.running")} value={taskStats.running} color="text-primary" />
            <StatusCell label={t("dashboard.status.completed")} value={taskStats.completed} color="text-success" />
            <StatusCell label={t("dashboard.status.failed")} value={taskStats.failed} color="text-destructive" />
            <StatusCell label={t("dashboard.status.deadLetter")} value={taskStats.dead_letter} color="text-muted-foreground" />
            <StatusCell label={t("dashboard.status.total")} value={taskStats.total} />
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
        <h3 className="text-h3 mb-3">{t("dashboard.quickActions")}</h3>
        <div className="grid grid-cols-4 gap-2">
          <QuickAction icon="M6 3v12M18 9a3 3 0 1 0 0-6" label={t("dashboard.actions.newWorkflow")} navId="workflows" onClick={onNavigate} />
          <QuickAction icon="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" label={t("dashboard.actions.searchKnowledge")} navId="knowledge" onClick={onNavigate} />
          <QuickAction icon="M12 2a10 10 0 1 0 10 10" label={t("dashboard.actions.registerAgent")} navId="agents" onClick={onNavigate} />
          <QuickAction icon="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" label={t("dashboard.actions.startChat")} navId="chat" onClick={onNavigate} />
        </div>
      </div>

      {sysInfo && (
        <div className="bg-card border rounded-lg p-4 shadow-card">
          <h3 className="text-h3 mb-2">{t("dashboard.systemInfo")}</h3>
          <div className="grid grid-cols-2 gap-3 text-[13px]">
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">{t("dashboard.info.version")}</div>
              <div className="font-mono font-medium">{sysInfo.version}</div>
            </div>
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">{t("dashboard.info.uptime")}</div>
              <div className="font-mono font-medium">{uptimeDisplay}</div>
            </div>
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">{t("dashboard.info.agentCount")}</div>
              <div className="font-mono font-medium">{sysInfo.agents_count}</div>
            </div>
            <div className="rounded-md bg-muted/50 p-3">
              <div className="text-[11px] text-muted-foreground mb-0.5">{t("dashboard.info.workflowCount")}</div>
              <div className="font-mono font-medium">{sysInfo.workflows_count}</div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function QuickAction({ icon, label, navId, onClick }: { icon: string; label: string; navId: string; onClick?: (id: string) => void }) {
  return (
    <button
      onClick={() => onClick?.(navId)}
      className="flex items-center gap-2 rounded-md border p-3 hover:bg-accent hover:shadow-card transition-all text-left"
    >
      <svg className="w-4 h-4 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d={icon} /></svg>
      <span className="text-[13px]">{label}</span>
    </button>
  );
}

const colorClassMap: Record<string, string> = {
  primary: "text-primary",
  secondary: "text-secondary",
  foreground: "text-foreground",
  warning: "text-warning",
  success: "text-success",
  destructive: "text-destructive",
  muted: "text-muted-foreground",
};

function MetricCard({ label, value, color, icon }: { label: string; value: number; color: string; icon: string }) {
  const cls = colorClassMap[color] ?? "text-foreground";
  return (
    <div className="bg-card border rounded-md p-3 shadow-card">
      <div className="flex items-center gap-1.5 mb-1">
        <svg className={`w-3.5 h-3.5 ${cls}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d={icon} /></svg>
        <div className="text-[11px] text-muted-foreground">{label}</div>
      </div>
      <div className={`text-metric ${cls}`}>{value}</div>
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
