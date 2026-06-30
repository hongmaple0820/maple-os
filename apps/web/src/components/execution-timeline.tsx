"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { Card, CardContent, Badge, Spinner } from "@mapleos/ui";
import {
  getExecution,
  listExecutionEvents,
  subscribeExecutionEvents,
  type Execution,
  type ExecutionEvent,
} from "@/lib/v3-api";
import { useTranslation } from "react-i18next";

// ============================================================
// Source → color mapping (spec §8.1)
// ============================================================
const SOURCE_COLORS: Record<string, string> = {
  chat: "bg-blue-100 text-blue-700 border-blue-200",
  workflow: "bg-purple-100 text-purple-700 border-purple-200",
  task: "bg-indigo-100 text-indigo-700 border-indigo-200",
  approval: "bg-orange-100 text-orange-700 border-orange-200",
  agent: "bg-green-100 text-green-700 border-green-200",
  tool: "bg-yellow-100 text-yellow-700 border-yellow-200",
  scheduler: "bg-gray-100 text-gray-700 border-gray-200",
  system: "bg-gray-100 text-gray-500 border-gray-200",
};

const EVENT_TYPE_ICONS: Record<string, string> = {
  started: "▶",
  delta: "·",
  tool_call: "🔧",
  tool_result: "↩",
  node_started: "◦",
  node_finished: "•",
  artifact: "📦",
  usage: "⚡",
  approval_requested: "❓",
  approval_decided: "✓",
  retry: "↻",
  cancelled: "✗",
  resumed: "→",
  paused: "⏸",
  done: "✓",
  error: "!",
};

const TERMINAL_STATUSES = new Set(["success", "failed", "cancelled"]);

// ============================================================
// Props
// ============================================================

export interface ExecutionTimelineProps {
  executionId: string;
  /** Compact mode: hide actor + payload, show only event_type + timestamp. */
  compact?: boolean;
  /** Auto-scroll to bottom on new events (default: true). */
  autoScroll?: boolean;
  /** Optional className for the outer card. */
  className?: string;
}

// ============================================================
// Component
// ============================================================

export function ExecutionTimeline({
  executionId,
  compact = false,
  autoScroll = true,
  className,
}: ExecutionTimelineProps) {
  const { t } = useTranslation();
  const [execution, setExecution] = useState<Execution | null>(null);
  const [events, setEvents] = useState<ExecutionEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [streaming, setStreaming] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  // ---- Initial fetch ----
  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      setLoading(true);
      setError(null);
      try {
        const [exec, eventsResp] = await Promise.all([
          getExecution(executionId),
          listExecutionEvents(executionId),
        ]);
        if (cancelled) return;
        setExecution(exec);
        setEvents(eventsResp.events);

        // If execution is still running, subscribe to SSE
        if (!TERMINAL_STATUSES.has(exec.status)) {
          setStreaming(true);
        }
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    bootstrap();
    return () => {
      cancelled = true;
    };
  }, [executionId]);

  // ---- SSE subscription ----
  useEffect(() => {
    if (!streaming) return;

    let unsubscribe: (() => void) | null = null;
    let stopped = false;

    unsubscribe = subscribeExecutionEvents(
      executionId,
      (event) => {
        if (stopped) return;
        setEvents((prev) => {
          // Dedup by event id — SSE may replay on reconnect
          if (prev.some((e) => e.id === event.id)) return prev;
          return [...prev, event];
        });
      },
      (finalStatus) => {
        if (stopped) return;
        setStreaming(false);
        setExecution((prev) =>
          prev
            ? {
                ...prev,
                status: finalStatus as Execution["status"],
                completed_at: Math.floor(Date.now() / 1000),
              }
            : prev
        );
      },
      (err) => {
        if (stopped) return;
        setError(err.message);
        setStreaming(false);
      }
    );

    return () => {
      stopped = true;
      unsubscribe?.();
    };
  }, [executionId, streaming]);

  // ---- Auto-scroll ----
  const scrollToBottom = useCallback(() => {
    if (!autoScroll || !scrollRef.current) return;
    scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [autoScroll]);

  useEffect(() => {
    scrollToBottom();
  }, [events, scrollToBottom]);

  // ============================================================
  // Render
  // ============================================================

  if (loading) {
    return (
      <Card className={className}>
        <CardContent className="flex items-center gap-2 py-3 text-sm text-gray-500">
          <Spinner className="h-4 w-4" />
          {t("execution.loading", "Loading execution...")}
        </CardContent>
      </Card>
    );
  }

  if (error) {
    return (
      <Card className={className}>
        <CardContent className="py-3 text-sm text-red-600">
          {t("execution.error", "Error")}: {error}
        </CardContent>
      </Card>
    );
  }

  if (!execution) {
    return null;
  }

  const statusColor =
    {
      pending: "bg-gray-100 text-gray-700",
      running: "bg-blue-100 text-blue-700",
      paused: "bg-yellow-100 text-yellow-700",
      success: "bg-green-100 text-green-700",
      failed: "bg-red-100 text-red-700",
      cancelled: "bg-gray-100 text-gray-500",
    }[execution.status] ?? "bg-gray-100 text-gray-700";

  return (
    <Card className={className}>
      <CardContent className="space-y-3">
        {/* Header */}
        <div className="flex items-center justify-between border-b pb-2">
          <div className="flex items-center gap-2">
            <Badge className={statusColor}>{execution.status}</Badge>
            <Badge className={SOURCE_COLORS[execution.source] ?? "bg-gray-100 text-gray-700"}>
              {execution.source}
            </Badge>
            {streaming && (
              <span className="flex items-center gap-1 text-xs text-blue-600">
                <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-blue-500" />
                {t("execution.streaming", "streaming")}
              </span>
            )}
          </div>
          <div className="text-xs text-gray-400">
            {execution.event_count} {t("execution.events", "events")}
          </div>
        </div>

        {/* Event list */}
        <div
          ref={scrollRef}
          className="max-h-[400px] space-y-1 overflow-y-auto pr-1 font-mono text-xs"
        >
          {events.length === 0 && (
            <div className="py-4 text-center text-gray-400">
              {t("execution.no_events", "No events yet")}
            </div>
          )}
          {events.map((evt) => (
            <EventRow key={evt.id} event={evt} compact={compact} />
          ))}
        </div>

        {/* Footer */}
        {!compact && (
          <div className="border-t pt-2 text-xs text-gray-500">
            <div className="flex flex-wrap gap-x-4 gap-y-1">
              <span>
                {t("execution.started", "Started")}:{" "}
                {new Date(execution.started_at * 1000).toLocaleString()}
              </span>
              {execution.completed_at && (
                <span>
                  {t("execution.completed", "Completed")}:{" "}
                  {new Date(execution.completed_at * 1000).toLocaleString()}
                </span>
              )}
              {execution.actor && (
                <span>
                  {t("execution.actor", "Actor")}: {execution.actor}
                  {execution.actor_type && ` (${execution.actor_type})`}
                </span>
              )}
              {execution.error && (
                <span className="text-red-600">
                  {t("execution.error_label", "Error")}: {execution.error}
                </span>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================
// Event row
// ============================================================

function EventRow({ event, compact }: { event: ExecutionEvent; compact: boolean }) {
  const time = new Date(event.created_at * 1000).toLocaleTimeString();
  const icon = EVENT_TYPE_ICONS[event.event_type] ?? "•";
  const sourceColor = SOURCE_COLORS[event.source] ?? "bg-gray-100 text-gray-700";

  return (
    <div className="flex gap-2 rounded border border-transparent px-1 py-0.5 hover:border-gray-200 hover:bg-gray-50">
      <span className="w-4 flex-shrink-0 text-gray-400">{icon}</span>
      <span className="w-16 flex-shrink-0 text-gray-400">{time}</span>
      <Badge className={`flex-shrink-0 ${sourceColor}`}>{event.source}</Badge>
      <span className="flex-shrink-0 font-medium text-gray-700">{event.event_type}</span>
      {!compact && (
        <span className="flex-1 truncate text-gray-500">
          {summarizePayload(event)}
        </span>
      )}
    </div>
  );
}

function summarizePayload(event: ExecutionEvent): string {
  const p = event.payload;
  switch (event.event_type) {
    case "started":
      return `entry=${p.entry ?? "?"} trigger=${p.trigger ?? "?"}`;
    case "delta":
      return typeof p.token === "string" ? `"${p.token}"` : "(delta)";
    case "tool_call":
      return `${p.tool_name ?? "?"}(${JSON.stringify(p.input ?? {}).slice(0, 80)})`;
    case "tool_result":
      return `${p.invocation_id ?? "?"} → ${p.error ? `error: ${p.error}` : JSON.stringify(p.output ?? {}).slice(0, 80)}`;
    case "node_started":
      return `node=${p.node_id ?? "?"} type=${p.node_type ?? "?"}`;
    case "node_finished":
      return `node=${p.node_id ?? "?"} status=${p.status ?? "?"} ${p.error ? "err=" + p.error : ""}`;
    case "artifact":
      return `${p.artifact_type ?? "?"} → ${p.target_id ?? "?"}`;
    case "usage":
      return `${p.total_tokens ?? "?"} tokens (${p.model ?? "?"})`;
    case "approval_requested":
      return `${p.action_type ?? "?"} — ${p.description ?? ""}`;
    case "approval_decided":
      return `${p.decision ?? "?"} by ${p.voter_id ?? "?"}`;
    case "paused":
      return `reason=${p.reason ?? "?"}`;
    case "resumed":
      return `reason=${p.reason ?? "?"}`;
    case "done":
      return String(p.output_summary ?? "(done)");
    case "error":
      return `${p.message ?? "?"}${p.recoverable ? " (recoverable)" : ""}`;
    case "cancelled":
      return `by ${p.actor ?? "?"} — ${p.reason ?? ""}`;
    case "retry":
      return `attempt ${p.attempt ?? "?"} — ${p.reason ?? ""}`;
    default:
      return JSON.stringify(p).slice(0, 100);
  }
}
