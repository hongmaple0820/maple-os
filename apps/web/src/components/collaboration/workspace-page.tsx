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

export default function CollaborationWorkspace() {
  const [activeTab, setActiveTab] = useState<"overview" | "kanban" | "team" | "discussions">("kanban");
  const [showTaskDetail, setShowTaskDetail] = useState<Task | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [taskCounts, setTaskCounts] = useState({ todo: 0, "in-progress": 0, review: 0, done: 0 });

  useEffect(() => {
    mapleApi<{ tasks: Array<{ status: string }> }>("/api/board/tasks")
      .then((res) => {
        const tasks = res.tasks ?? [];
        const counts = { todo: 0, "in-progress": 0, review: 0, done: 0 };
        for (const t of tasks) {
          if (t.status in counts) counts[t.status as keyof typeof counts]++;
        }
        setTaskCounts(counts);
      })
      .catch(() => {});
  }, [activeTab]);

  const stats = [
    { label: "待办任务", value: taskCounts.todo, icon: Clock, color: "bg-amber-100 text-amber-700" },
    { label: "进行中", value: taskCounts["in-progress"], icon: Activity, color: "bg-blue-100 text-blue-700" },
    { label: "已完成", value: taskCounts.done, icon: CheckCircle2, color: "bg-emerald-100 text-emerald-700" },
    { label: "审核中", value: taskCounts.review, icon: TrendingUp, color: "bg-primary/10 text-primary" },
  ];

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
              <span className="text-muted-foreground">工作空间</span>
              <span className="text-muted-foreground">/</span>
              <span className="font-medium">MapleOS 开发</span>
            </div>
          </div>

          <div className="flex items-center gap-4">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="搜索任务、成员、讨论..."
                className="w-80 pl-9 pr-4 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary"
              />
            </div>
            <button className="relative p-2 text-muted-foreground hover:text-foreground hover:bg-muted rounded-lg transition-colors">
              <Bell className="w-5 h-5" />
              <span className="absolute top-1 right-1 w-2 h-2 bg-destructive rounded-full" />
            </button>
            <div className="flex items-center gap-2 pl-4 border-l border-border">
              <img
                src="https://api.dicebear.com/7.x/avataaars/svg?seed=me"
                alt="当前用户"
                className="w-8 h-8 rounded-full bg-muted"
              />
              <ChevronDown className="w-4 h-4 text-muted-foreground" />
            </div>
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="px-6 flex items-center gap-1 border-t border-border">
          {[
            { id: "overview", label: "概览", icon: LayoutDashboard },
            { id: "kanban", label: "任务看板", icon: KanbanSquare },
            { id: "team", label: "团队", icon: Users },
            { id: "discussions", label: "讨论", icon: MessageSquare },
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
              {/* Recent Activity */}
              <div className="col-span-2 bg-card rounded-xl border border-border shadow-card overflow-hidden">
                <div className="px-6 py-4 border-b border-border">
                  <div className="flex items-center justify-between">
                    <h2 className="text-lg font-semibold">最近动态</h2>
                    <button className="text-sm text-primary hover:underline">查看全部</button>
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
                  <h2 className="text-lg font-semibold">项目概览</h2>
                </div>
                <div className="p-4 space-y-4">
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">项目进度</span>
                      <span className="font-medium">65%</span>
                    </div>
                    <div className="h-2 bg-muted rounded-full overflow-hidden">
                      <div className="h-full w-[65%] bg-primary rounded-full" />
                    </div>
                  </div>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between text-sm">
                      <span className="text-muted-foreground">截止日期</span>
                      <span className="font-medium">2026-06-15</span>
                    </div>
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Clock className="w-4 h-4" />
                      <span>剩余 21 天</span>
                    </div>
                  </div>
                  <div className="pt-4 border-t border-border">
                    <button className="w-full py-2 px-4 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
                      生成进度报告
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
                    筛选
                    <ChevronDown className="w-4 h-4" />
                  </button>
                  <button className="flex items-center gap-2 px-3 py-1.5 border border-border rounded-lg text-sm hover:bg-muted transition-colors">
                    <Calendar className="w-4 h-4" />
                    本周
                    <ChevronDown className="w-4 h-4" />
                  </button>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <button className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
                  <Plus className="w-4 h-4" />
                  新建任务
                </button>
              </div>
            </div>

            {/* Kanban Board */}
            <KanbanBoard
              onTaskMove={(taskId, source, target) => {
                console.log(`Task ${taskId} moved from ${source} to ${target}`);
              }}
              onTaskClick={(task) => setShowTaskDetail(task)}
            />
          </div>
        )}

        {activeTab === "team" && (
          <div className="max-w-md">
            <OnlineStatus />
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
                    {showTaskDetail.description || "暂无描述"}
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
                    <span className="text-sm text-muted-foreground">负责人:</span>
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
                      <span className="text-sm text-muted-foreground">未分配</span>
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-muted-foreground">截止日期:</span>
                    <span className="text-sm font-medium">{showTaskDetail.dueDate || "未设置"}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-sm text-muted-foreground">优先级:</span>
                    <span className="text-sm font-medium capitalize">{showTaskDetail.priority}</span>
                  </div>
                </div>

                {/* Comments Section */}
                <Comments
                  taskId={showTaskDetail.id}
                  title="任务讨论"
                  placeholder="添加评论..."
                />
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
