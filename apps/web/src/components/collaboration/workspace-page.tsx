"use client";

import React, { useState, useEffect } from "react";
import {
  LayoutDashboard,
  KanbanSquare,
  Users,
  MessageSquare,
  Settings,
  Bell,
  Search,
  Filter,
  Calendar,
  ChevronDown,
  Plus,
  MoreHorizontal,
  Clock,
  CheckCircle2,
  AlertCircle,
  Activity,
  TrendingUp,
  FileText
} from "lucide-react";
import KanbanBoard, { Task } from "./kanban-board";
import OnlineStatus from "./online-status";
import Comments from "./comments";
import { mapleApi } from "@/lib/api";
import { useTranslation } from "react-i18next";

export default function CollaborationWorkspace() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"overview" | "kanban" | "team" | "discussions" | "settings">("kanban");
  const [showTaskDetail, setShowTaskDetail] = useState<Task | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [taskCounts, setTaskCounts] = useState({ todo: 0, "in-progress": 0, review: 0, done: 0 });
  const [workspaceName, setWorkspaceName] = useState(t("collab.projectName"));
  const [workspaceId, setWorkspaceId] = useState<string | null>(null);
  const [members, setMembers] = useState<Array<{ id: string; name: string; member_type: string; role: string }>>([]);
  const [showAddMember, setShowAddMember] = useState(false);
  const [newMemberName, setNewMemberName] = useState("");
  const [newMemberType, setNewMemberType] = useState("human");
  const [wsDescription, setWsDescription] = useState("");
  const [wsMaxAgents, setWsMaxAgents] = useState(5);
  const [wsAutoApprove, setWsAutoApprove] = useState(false);
  const [wsKbEnabled, setWsKbEnabled] = useState(true);
  const [showNotifications, setShowNotifications] = useState(false);
  const [notifications, setNotifications] = useState<Array<{ id: string; actor: string; action: string; target: string; time: string; read: boolean }>>([]);

  useEffect(() => {
    mapleApi<{ tasks: Array<{ status: string }> }>("/api/board/tasks")
      .then((res) => {
        const tasks = res.tasks ?? [];
        const counts = { todo: 0, "in-progress": 0, review: 0, done: 0 };
        for (const task of tasks) {
          if (task.status in counts) counts[task.status as keyof typeof counts]++;
        }
        setTaskCounts(counts);
      })
      .catch(() => {});
  }, [activeTab]);

  useEffect(() => {
    mapleApi<{ workspaces: Array<{ id: string; name: string }> }>("/api/workspaces")
      .then((res) => {
        if (res.workspaces?.length > 0) {
          setWorkspaceName(res.workspaces[0].name);
          setWorkspaceId(res.workspaces[0].id);
        }
      })
      .then(() => {
        // Load full workspace settings
        mapleApi<{ workspace: { name: string; description?: string; max_agents?: number; auto_approve?: boolean; knowledge_base_enabled?: boolean } }>("/api/workspaces/default")
          .then((res) => {
            if (res.workspace) {
              setWorkspaceName(res.workspace.name);
              setWsDescription(res.workspace.description || "");
              setWsMaxAgents(res.workspace.max_agents ?? 5);
              setWsAutoApprove(res.workspace.auto_approve ?? false);
              setWsKbEnabled(res.workspace.knowledge_base_enabled ?? true);
            }
          })
          .catch(() => {});
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!workspaceId) return;
    mapleApi<{ members: Array<{ id: string; name: string; member_type: string; role: string }> }>(`/api/workspaces/${workspaceId}/members`)
      .then((res) => setMembers(res.members ?? []))
      .catch(() => {});
  }, [workspaceId]);

  const stats = [
    { label: t("collab.stats.todo"), value: taskCounts.todo, icon: Clock, color: "bg-amber-100 text-amber-700" },
    { label: t("collab.stats.inProgress"), value: taskCounts["in-progress"], icon: Activity, color: "bg-blue-100 text-blue-700" },
    { label: t("collab.stats.done"), value: taskCounts.done, icon: CheckCircle2, color: "bg-emerald-100 text-emerald-700" },
    { label: t("collab.stats.review"), value: taskCounts.review, icon: TrendingUp, color: "bg-primary/10 text-primary" },
  ];

  const [recentActivities, setRecentActivities] = useState<Array<{ user: string; action: string; target: string; time: string }>>([]);
  const [refreshKey, setRefreshKey] = useState(0);

  const loadActivities = () => {
    mapleApi<{ activities: Array<{ actor: string; action: string; target?: string; created_at: number }> }>("/api/activity")
      .then((res) => {
        const mapped = (res.activities || []).map((a) => ({
          user: a.actor,
          action: a.action,
          target: a.target || "",
          time: new Date(a.created_at * 1000).toLocaleString(),
        }));
        setRecentActivities(mapped);
      })
      .catch(() => {});
  };

  useEffect(() => { loadActivities(); loadNotifications(); }, []);

  const loadNotifications = () => {
    mapleApi<{ activities: Array<{ id?: number; actor: string; action: string; target?: string; created_at: number }> }>("/api/activity")
      .then((res) => {
        const mapped = (res.activities || []).slice(0, 20).map((a, i) => ({
          id: String(a.id ?? i),
          actor: a.actor,
          action: a.action,
          target: a.target || "",
          time: new Date(a.created_at * 1000).toLocaleString(),
          read: i > 4,
        }));
        setNotifications(mapped);
      })
      .catch(() => {});
  };

  // SSE listener for real-time collaboration events
  useEffect(() => {
    const es = new EventSource("/api/maple/api/events");
    const refresh = () => { setRefreshKey((k) => k + 1); loadActivities(); loadNotifications(); };
    for (const evt of ["task.created", "task.updated", "task.deleted", "comment.created", "activity.logged"]) {
      es.addEventListener(evt, refresh);
    }
    return () => { es.close(); };
  }, []);

  return (
    <div className="min-h-screen bg-background">
      {/* Top Navigation */}
      <header className="sticky top-0 z-50 bg-card border-b border-border">
        <div className="px-6 py-3 flex items-center justify-between">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-2">
              <div className="w-8 h-8 bg-primary rounded-lg flex items-center justify-center">
                <LayoutDashboard className="w-5 h-5 text-primary-foreground" />
              </div>
              <span className="font-semibold text-lg">MapleOS</span>
            </div>
            <div className="flex items-center gap-1 text-sm">
              <span className="text-muted-foreground">{t("collab.workspace")}</span>
              <span className="text-muted-foreground">/</span>
              <span className="font-medium">{workspaceName}</span>
            </div>
          </div>

          <div className="flex items-center gap-4">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("collab.searchPlaceholder")}
                className="w-80 pl-9 pr-4 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary"
              />
            </div>
            <div className="relative">
              <button onClick={() => { setShowNotifications(!showNotifications); if (!showNotifications) loadNotifications(); }} className="relative p-2 text-muted-foreground hover:text-foreground hover:bg-muted rounded-lg transition-colors">
                <Bell className="w-5 h-5" />
                {notifications.filter((n) => !n.read).length > 0 && (
                  <span className="absolute top-1 right-1 w-2 h-2 bg-destructive rounded-full" />
                )}
              </button>
              {showNotifications && (
                <div className="absolute right-0 top-full mt-2 w-80 bg-card border border-border rounded-xl shadow-xl z-50 max-h-96 overflow-y-auto">
                  <div className="px-4 py-3 border-b border-border flex items-center justify-between">
                    <span className="text-sm font-semibold">{t("collab.notifications.title")}</span>
                    <button onClick={() => setNotifications((prev) => prev.map((n) => ({ ...n, read: true })))} className="text-xs text-primary hover:underline">{t("collab.notifications.markAllRead")}</button>
                  </div>
                  {notifications.length === 0 && <div className="p-4 text-center text-xs text-muted-foreground">{t("collab.notifications.empty")}</div>}
                  {notifications.map((n) => (
                    <div key={n.id} className={`px-4 py-3 border-b border-border hover:bg-muted/50 transition-colors ${!n.read ? "bg-primary/5" : ""}`}>
                      <div className="flex items-start gap-2">
                        {!n.read && <div className="w-1.5 h-1.5 rounded-full bg-primary mt-1.5 flex-shrink-0" />}
                        <div className="flex-1 min-w-0">
                          <p className="text-xs"><span className="font-medium">{n.actor}</span> {n.action} <span className="font-medium">{n.target}</span></p>
                          <p className="text-[10px] text-muted-foreground mt-0.5">{n.time}</p>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
            <div className="flex items-center gap-2 pl-4 border-l border-border">
              <img
                src="https://api.dicebear.com/7.x/avataaars/svg?seed=me"
                alt="User"
                className="w-8 h-8 rounded-full bg-muted"
              />
              <ChevronDown className="w-4 h-4 text-muted-foreground" />
            </div>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="px-6 flex items-center gap-1 border-t border-border">
          {[
            { id: "overview", label: t("collab.tabs.overview"), icon: LayoutDashboard },
            { id: "kanban", label: t("collab.tabs.kanban"), icon: KanbanSquare },
            { id: "team", label: t("collab.tabs.team"), icon: Users },
            { id: "discussions", label: t("collab.tabs.discussions"), icon: MessageSquare },
            { id: "settings", label: t("collab.tabs.settings"), icon: Settings },
          ].map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as typeof activeTab)}
              className={`flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
                activeTab === tab.id
                  ? "border-primary text-primary"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              <tab.icon className="w-4 h-4" />
              {tab.label}
            </button>
          ))}
        </div>
      </header>

      {/* Main Content */}
      <main className="p-6">
        {activeTab === "overview" && (
          <div className="space-y-6">
            {/* Stats Grid */}
            <div className="grid grid-cols-4 gap-4">
              {stats.map((stat, idx) => (
                <div
                  key={idx}
                  className="bg-card p-5 rounded-xl border border-border shadow-card"
                >
                  <div className="flex items-start justify-between">
                    <div>
                      <p className="text-sm text-muted-foreground mb-1">{stat.label}</p>
                      <p className="text-2xl font-bold">{stat.value}</p>
                    </div>
                    <div className={`p-2 rounded-lg ${stat.color}`}>
                      <stat.icon className="w-5 h-5" />
                    </div>
                  </div>
                </div>
              ))}
            </div>

            <div className="grid grid-cols-3 gap-6">
              {/* Task Distribution Chart */}
              <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
                <div className="px-6 py-4 border-b border-border">
                  <h2 className="text-lg font-semibold">{t("collab.chart.taskDistribution")}</h2>
                </div>
                <div className="p-4 flex flex-col items-center">
                  {(() => {
                    const total = taskCounts.todo + taskCounts["in-progress"] + taskCounts.review + taskCounts.done;
                    if (total === 0) return <div className="text-xs text-muted-foreground py-8">{t("collab.chart.noData")}</div>;
                    const segments = [
                      { label: t("collab.stats.todo"), value: taskCounts.todo, color: "#f59e0b" },
                      { label: t("collab.stats.inProgress"), value: taskCounts["in-progress"], color: "#3b82f6" },
                      { label: t("collab.stats.review"), value: taskCounts.review, color: "#8b5cf6" },
                      { label: t("collab.stats.done"), value: taskCounts.done, color: "#10b981" },
                    ];
                    let acc = 0;
                    const gradientParts = segments.map((s) => {
                      const start = acc;
                      acc += (s.value / total) * 360;
                      return `${s.color} ${start}deg ${acc}deg`;
                    });
                    return (
                      <>
                        <div className="w-32 h-32 rounded-full mb-4" style={{ background: `conic-gradient(${gradientParts.join(", ")})` }} />
                        <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 w-full">
                          {segments.map((s) => (
                            <div key={s.label} className="flex items-center gap-2 text-xs">
                              <div className="w-2.5 h-2.5 rounded-full flex-shrink-0" style={{ backgroundColor: s.color }} />
                              <span className="text-muted-foreground">{s.label}</span>
                              <span className="font-medium ml-auto">{s.value}</span>
                            </div>
                          ))}
                        </div>
                      </>
                    );
                  })()}
                </div>
              </div>

              {/* Task Status Bar Chart */}
              <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
                <div className="px-6 py-4 border-b border-border">
                  <h2 className="text-lg font-semibold">任务进度</h2>
                </div>
                <div className="p-4">
                  {(() => {
                    const bars = [
                      { label: t("collab.stats.todo"), value: taskCounts.todo, color: "bg-amber-400" },
                      { label: t("collab.stats.inProgress"), value: taskCounts["in-progress"], color: "bg-blue-400" },
                      { label: t("collab.stats.review"), value: taskCounts.review, color: "bg-violet-400" },
                      { label: t("collab.stats.done"), value: taskCounts.done, color: "bg-emerald-400" },
                    ];
                    const max = Math.max(...bars.map((b) => b.value), 1);
                    return (
                      <div className="space-y-3">
                        {bars.map((bar) => (
                          <div key={bar.label}>
                            <div className="flex items-center justify-between text-xs mb-1">
                              <span className="text-muted-foreground">{bar.label}</span>
                              <span className="font-medium">{bar.value}</span>
                            </div>
                            <div className="h-3 bg-muted rounded-full overflow-hidden">
                              <div className={`h-full rounded-full transition-all ${bar.color}`} style={{ width: `${(bar.value / max) * 100}%` }} />
                            </div>
                          </div>
                        ))}
                      </div>
                    );
                  })()}
                </div>
              </div>

              {/* Recent Activity */}
              <div className="col-span-2 bg-card rounded-xl border border-border shadow-card overflow-hidden">
                <div className="px-6 py-4 border-b border-border">
                  <div className="flex items-center justify-between">
                    <h2 className="text-lg font-semibold">{t("collab.overview.recentActivity")}</h2>
                    <button className="text-sm text-primary hover:underline">{t("collab.overview.viewAll")}</button>
                  </div>
                </div>
                <div className="p-4">
                  <div className="space-y-4">
                    {recentActivities.map((activity, idx) => (
                      <div key={idx} className="flex items-center gap-3 p-3 bg-muted/30 rounded-lg">
                        <div className="w-2 h-2 rounded-full bg-primary" />
                        <div className="flex-1">
                          <div className="flex items-center gap-2">
                            <span className="font-medium text-sm">{activity.user}</span>
                            <span className="text-sm text-muted-foreground">{activity.action}</span>
                            <span className="text-sm font-medium">{activity.target}</span>
                          </div>
                          <p className="text-xs text-muted-foreground mt-1">{activity.time}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>

              {/* Quick Stats */}
              <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
                <div className="px-6 py-4 border-b border-border">
                  <h2 className="text-lg font-semibold">{t("collab.overview.projectOverview")}</h2>
                </div>
                <div className="p-4 space-y-4">
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">{t("collab.overview.progress")}</span>
                      <span className="font-medium">
                        {(() => {
                          const total = taskCounts.todo + taskCounts["in-progress"] + taskCounts.review + taskCounts.done;
                          return total > 0 ? Math.round((taskCounts.done / total) * 100) : 0;
                        })()}%
                      </span>
                    </div>
                    <div className="h-2 bg-muted rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary rounded-full transition-all"
                        style={{ width: `${(() => {
                          const total = taskCounts.todo + taskCounts["in-progress"] + taskCounts.review + taskCounts.done;
                          return total > 0 ? Math.round((taskCounts.done / total) * 100) : 0;
                        })()}%` }}
                      />
                    </div>
                  </div>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">{t("collab.overview.dueDate")}</span>
                      <span className="font-medium">2026-06-15</span>
                    </div>
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Clock className="w-4 h-4" />
                      <span>{t("collab.overview.daysLeft", { days: 21 })}</span>
                    </div>
                  </div>
                  <div className="pt-4 border-t border-border">
                    <button className="w-full py-2 px-4 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
                      {t("collab.overview.generateReport")}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}

        {activeTab === "kanban" && (
          <div className="space-y-6">
            {/* Kanban Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-4">
                <div className="flex items-center gap-2">
                  <button className="flex items-center gap-2 px-3 py-1.5 border border-border rounded-lg text-sm hover:bg-muted transition-colors">
                    <Filter className="w-4 h-4" />
                    {t("collab.kanban.filter")}
                    <ChevronDown className="w-4 h-4" />
                  </button>
                  <button className="flex items-center gap-2 px-3 py-1.5 border border-border rounded-lg text-sm hover:bg-muted transition-colors">
                    <Calendar className="w-4 h-4" />
                    {t("collab.kanban.thisWeek")}
                    <ChevronDown className="w-4 h-4" />
                  </button>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <button className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
                  <Plus className="w-4 h-4" />
                  {t("collab.kanban.newTask")}
                </button>
              </div>
            </div>

            {/* Kanban Board */}
            <KanbanBoard
              key={refreshKey}
              filterQuery={searchQuery}
              onTaskMove={(taskId, source, target) => {
                console.log(`Task ${taskId} moved from ${source} to ${target}`);
              }}
              onTaskClick={(task) => setShowTaskDetail(task)}
            />
          </div>
        )}

        {activeTab === "team" && (
          <div className="max-w-2xl space-y-6">
            <OnlineStatus />

            {/* Workspace Members */}
            <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
              <div className="px-6 py-4 border-b border-border flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <h2 className="text-lg font-semibold">{t("collab.members.title")}</h2>
                  <span className="px-2.5 py-1 bg-muted rounded-full text-xs font-medium text-muted-foreground">{t("collab.members.count", { count: members.length })}</span>
                </div>
                <button
                  onClick={() => setShowAddMember(!showAddMember)}
                  className="flex items-center gap-1.5 px-3 py-1.5 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90 transition-opacity"
                >{t("collab.members.addMember")}</button>
              </div>
              <div className="p-4 space-y-2">
                {showAddMember && (
                  <div className="flex gap-2 mb-3 p-3 bg-muted/30 rounded-lg border border-border">
                    <input
                      type="text" value={newMemberName} onChange={(e) => setNewMemberName(e.target.value)}
                      placeholder={t("collab.members.memberName")} className="flex-1 px-3 py-1.5 bg-background border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary"
                    />
                    <select value={newMemberType} onChange={(e) => setNewMemberType(e.target.value)}
                      className="px-3 py-1.5 bg-background border border-border rounded-lg text-sm">
                      <option value="human">{t("collab.members.human")}</option>
                      <option value="agent">{t("collab.members.agent")}</option>
                    </select>
                    <button
                      onClick={async () => {
                        if (!newMemberName.trim() || !workspaceId) return;
                        await mapleApi(`/api/workspaces/${workspaceId}/members`, {
                          method: "POST",
                          body: { member_id: `member-${Date.now()}`, name: newMemberName.trim(), member_type: newMemberType, role: "member" },
                        });
                        setNewMemberName(""); setShowAddMember(false);
                        const res = await mapleApi<{ members: Array<{ id: string; name: string; member_type: string; role: string }> }>(`/api/workspaces/${workspaceId}/members`);
                        setMembers(res.members ?? []);
                      }}
                      className="px-3 py-1.5 bg-primary text-primary-foreground rounded-lg text-sm hover:opacity-90"
                    >{t("collab.members.add")}</button>
                  </div>
                )}
                {members.map((m) => (
                  <div key={m.id} className="flex items-center justify-between p-2.5 rounded-lg hover:bg-muted/50 transition-colors">
                    <div className="flex items-center gap-3">
                      <div className="w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center text-xs font-medium">
                        {m.name.charAt(0)}
                      </div>
                      <div>
                        <div className="text-sm font-medium">{m.name}</div>
                        <div className="text-xs text-muted-foreground">{m.member_type === "agent" ? t("collab.members.agent") : t("collab.members.human")} | {m.role}</div>
                      </div>
                    </div>
                    {m.role !== "owner" && (
                      <button
                        onClick={async () => {
                          if (!workspaceId) return;
                          await mapleApi(`/api/workspaces/${workspaceId}/members/${m.id}`, { method: "DELETE" });
                          setMembers((prev) => prev.filter((x) => x.id !== m.id));
                        }}
                        className="px-2 py-1 text-xs text-destructive hover:bg-destructive/10 border border-border rounded transition-colors"
                      >{t("collab.members.remove")}</button>
                    )}
                  </div>
                ))}
                {members.length === 0 && <div className="text-xs text-muted-foreground py-4 text-center">{t("collab.members.empty")}</div>}
              </div>
            </div>
          </div>
        )}

        {activeTab === "discussions" && (
          <div className="max-w-3xl">
            <Comments
              onSendComment={(content) => console.log("New comment:", content)}
              onReply={(commentId, content) => console.log("Reply to", commentId, ":", content)}
            />
          </div>
        )}

        {activeTab === "settings" && (
          <div className="max-w-2xl space-y-6">
            <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
              <div className="px-6 py-4 border-b border-border">
                <h2 className="text-lg font-semibold">{t("collab.settings.title")}</h2>
              </div>
              <div className="p-6 space-y-5">
                <div>
                  <label className="block text-sm font-medium mb-1">{t("collab.settings.name")}</label>
                  <input type="text" value={workspaceName} onChange={(e) => setWorkspaceName(e.target.value)}
                    className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary" />
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1">{t("collab.settings.description")}</label>
                  <textarea value={wsDescription} onChange={(e) => setWsDescription(e.target.value)}
                    className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary resize-none" rows={3} />
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="block text-sm font-medium mb-1">{t("collab.settings.maxAgents")}</label>
                    <input type="number" value={wsMaxAgents} onChange={(e) => setWsMaxAgents(Number(e.target.value))}
                      min={1} max={50}
                      className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary" />
                  </div>
                  <div className="flex items-center gap-3 pt-6">
                    <input type="checkbox" id="autoApprove" checked={wsAutoApprove} onChange={(e) => setWsAutoApprove(e.target.checked)}
                      className="w-4 h-4 rounded border-border" />
                    <label htmlFor="autoApprove" className="text-sm font-medium">{t("collab.settings.autoApprove")}</label>
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <input type="checkbox" id="kbEnabled" checked={wsKbEnabled} onChange={(e) => setWsKbEnabled(e.target.checked)}
                    className="w-4 h-4 rounded border-border" />
                  <label htmlFor="kbEnabled" className="text-sm font-medium">{t("collab.settings.kbEnabled")}</label>
                </div>
                <div className="pt-4 border-t border-border flex justify-end gap-2">
                  <button onClick={async () => {
                    if (!workspaceId) return;
                    const updates: Record<string, unknown> = { name: workspaceName, description: wsDescription, max_agents: wsMaxAgents, auto_approve: wsAutoApprove, knowledge_base_enabled: wsKbEnabled };
                    await mapleApi(`/api/workspaces/${workspaceId}`, { method: "PUT", body: updates });
                  }} className="px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">{t("collab.settings.save")}</button>
                </div>
              </div>
            </div>

            <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
              <div className="px-6 py-4 border-b border-border">
                <h2 className="text-lg font-semibold text-destructive">{t("collab.settings.dangerZone")}</h2>
              </div>
              <div className="p-6">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm font-medium">{t("collab.settings.deleteWorkspace")}</p>
                    <p className="text-xs text-muted-foreground mt-1">{t("collab.settings.deleteWarning")}</p>
                  </div>
                  <button onClick={async () => {
                    if (!workspaceId || !confirm(t("collab.settings.deleteConfirm"))) return;
                    await mapleApi(`/api/workspaces/${workspaceId}`, { method: "DELETE" });
                  }} className="px-4 py-2 text-sm text-destructive border border-destructive rounded-lg hover:bg-destructive/10 transition-colors">{t("collab.settings.deleteWorkspace")}</button>
                </div>
              </div>
            </div>
          </div>
        )}
      </main>

      {/* Task Detail Modal */}
      {showTaskDetail && (
        <div
          className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4"
          onClick={() => setShowTaskDetail(null)}
        >
          <div
            className="bg-card rounded-xl border border-border shadow-2xl w-full max-w-2xl max-h-[90vh] overflow-y-auto"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="p-6">
              <div className="flex items-start justify-between mb-4">
                <div>
                  <h2 className="text-xl font-bold">{showTaskDetail.title}</h2>
                  <p className="text-sm text-muted-foreground mt-1">
                    {showTaskDetail.description || t("collab.taskDetail.noDescription")}
                  </p>
                </div>
                <button
                  onClick={() => setShowTaskDetail(null)}
                  className="p-1 text-muted-foreground hover:text-foreground"
                >
                  <AlertCircle className="w-5 h-5" />
                </button>
              </div>

              <div className="space-y-4">
                <div className="flex items-center gap-6 py-4 border-y border-border">
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-muted-foreground">{t("collab.taskDetail.assignee")}</span>
                    {showTaskDetail.assignee ? (
                      <div className="flex items-center gap-1.5">
                        <img
                          src={showTaskDetail.assignee.avatar}
                          alt={showTaskDetail.assignee.name}
                          className="w-6 h-6 rounded-full"
                        />
                        <span className="text-sm font-medium">{showTaskDetail.assignee.name}</span>
                      </div>
                    ) : (
                      <span className="text-sm text-muted-foreground">{t("collab.taskDetail.unassigned")}</span>
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-muted-foreground">{t("collab.taskDetail.dueDate")}</span>
                    <span className="text-sm font-medium">{showTaskDetail.dueDate || t("collab.taskDetail.notSet")}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-muted-foreground">{t("collab.taskDetail.priority")}</span>
                    <span className="text-sm font-medium capitalize">{showTaskDetail.priority}</span>
                  </div>
                </div>

                {/* Comments Section */}
                <Comments
                  taskId={showTaskDetail.id}
                  title={t("collab.taskDetail.taskDiscussion")}
                  placeholder={t("collab.taskDetail.addComment")}
                />
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
