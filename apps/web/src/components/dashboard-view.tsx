"use client";

import { useTranslation } from "react-i18next";
import { Badge } from "@mapleos/ui";

export interface SystemInfo {
  version: string;
  uptime_secs: number;
  agents_count: number;
  workflows_count: number;
  tasks_count: number;
}

export interface TaskStats {
  total: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  dead_letter: number;
}

export function DashboardView({
  sysInfo,
  taskStats,
  serverOnline,
  onNavigate,
}: {
  sysInfo: SystemInfo | null;
  taskStats: TaskStats | null;
  serverOnline: boolean;
  onNavigate?: (id: string) => void;
}) {
  const { t } = useTranslation();
  const uptimeMin = sysInfo ? Math.floor(sysInfo.uptime_secs / 60) : 0;
  const uptimeHour = Math.floor(uptimeMin / 60);
  const uptimeDisplay = uptimeHour > 0 ? `${uptimeHour}h ${uptimeMin % 60}m` : `${uptimeMin}m`;

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-h2">{t("dashboard.title")}</h2>
        <div className="flex items-center gap-2">
          <Badge variant={serverOnline ? "default" : "destructive"} className="text-xs">
            {serverOnline ? t("dashboard.systemNormal") : t("dashboard.serviceOffline")}
          </Badge>
          {sysInfo && (
            <Badge variant="outline" className="text-[10px] font-mono">
              {t("dashboard.uptime", { time: uptimeDisplay })}
            </Badge>
          )}
        </div>
      </div>

      <div className="grid grid-cols-4 gap-3">
        <MetricCard label={t("dashboard.metrics.agents")} value={sysInfo?.agents_count ?? 0} color="primary" />
        <MetricCard label={t("dashboard.metrics.workflows")} value={sysInfo?.workflows_count ?? 0} color="secondary" />
        <MetricCard label={t("dashboard.metrics.totalTasks")} value={taskStats?.total ?? 0} color="foreground" />
        <MetricCard label={t("dashboard.metrics.running")} value={taskStats?.running ?? 0} color="warning" />
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
          <QuickAction label={t("dashboard.actions.newWorkflow")} navId="workflows" onClick={onNavigate} />
          <QuickAction label={t("dashboard.actions.searchKnowledge")} navId="knowledge" onClick={onNavigate} />
          <QuickAction label={t("dashboard.actions.registerAgent")} navId="agents" onClick={onNavigate} />
          <QuickAction label={t("dashboard.actions.startChat")} navId="chat" onClick={onNavigate} />
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

function QuickAction({ label, navId, onClick }: { label: string; navId: string; onClick?: (id: string) => void }) {
  return (
    <button
      onClick={() => onClick?.(navId)}
      className="flex items-center gap-2 rounded-md border p-3 hover:bg-accent hover:shadow-card transition-all text-left"
    >
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

function MetricCard({ label, value, color }: { label: string; value: number; color: string }) {
  const cls = colorClassMap[color] ?? "text-foreground";
  return (
    <div className="bg-card border rounded-md p-3 shadow-card">
      <div className="text-[11px] text-muted-foreground mb-1">{label}</div>
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
