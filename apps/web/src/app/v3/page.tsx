"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { Button, Input, Badge, Card, CardContent, Spinner } from "@mapleos/ui";
import { getAuthState } from "@/lib/api";
import {
  createGroup, listGroups, listMembers,
  listMessages, sendMessage,
  editMessage, deleteMessage,
  searchMessages,
  addReaction, pinMessage,
  listTasks, transitionTask,
  voteApproval, listPendingApprovals,
  searchMemory, getMemoryStats,
  createDm, listDms, listGrants, grantTool, revokeTool,
  listDelegations, createDelegation, interveneDelegation,
  uploadMessageAttachment, linkAttachment, deleteAttachment, getAttachmentDownloadUrl,
  listRules, createRule, updateRule, deleteRule,
  listCronJobs, createCronJob, updateCronJob, deleteCronJob,
  getThread,
  listHooks, createHook, updateHook, deleteHook, listHookLogs,
  listWorkflows, createWorkflow, deleteWorkflow,
  listWorkflowRuns, createWorkflowRun, updateWorkflowRunStatus,
  listCheckpoints,
} from "@/lib/v3-api";
import { useGroupWebSocket, type GroupEvent } from "@/lib/v3-ws";
import type {
  Group, GroupMember, GroupMessage, MessagePage,
  TaskV3, ApprovalRequest, AgentMemory,
  DmListItem, DmToolGrant, A2ADelegation,
  GroupRule, GroupCronJob,
  AgentHook, HookLog,
  WorkflowDef, WorkflowRun, RunCheckpoint,
} from "@/lib/v3-types";

// ─── MembersPanel ──────────────────────────────────────

function MembersPanel({ members }: { members: GroupMember[] }) {
  const typeIcon = (t: GroupMember["member_type"]) => t === "agent" ? "🤖" : "👤";
  const roleColor: Record<string, string> = {
    owner: "bg-red-100 text-red-700",
    admin: "bg-orange-100 text-orange-700",
    member: "bg-blue-100 text-blue-700",
    viewer: "bg-gray-100 text-gray-600",
  };
  return (
    <div className="w-64 bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold">Members ({members.length})</h2>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-1">
        {members.map((m) => (
          <div key={m.member_id} className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-accent">
            <span className="text-sm">{typeIcon(m.member_type)}</span>
            <div className="flex-1 min-w-0">
              <div className="text-xs font-medium truncate">{m.member_id}</div>
              <div className="text-[9px] text-muted-foreground">{m.member_type}</div>
            </div>
            <Badge className={`text-[8px] px-1 py-0 ${roleColor[m.role] ?? ""}`}>{m.role}</Badge>
            {m.can_approve && <span className="text-[9px]" title="Can approve">✓</span>}
          </div>
        ))}
        {members.length === 0 && (
          <div className="text-center text-xs text-muted-foreground py-4">No members</div>
        )}
      </div>
    </div>
  );
}

// ─── Helpers ────────────────────────────────────────────

function tsToDate(ts: number) {
  return new Date(ts * 1000);
}

function formatTime(ts: number) {
  return tsToDate(ts).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
}

const PRIORITY_COLORS: Record<string, string> = {
  low: "bg-blue-100 text-blue-700",
  medium: "bg-yellow-100 text-yellow-700",
  high: "bg-orange-100 text-orange-700",
  urgent: "bg-red-100 text-red-700",
  critical: "bg-red-200 text-red-900",
};

const STATUS_LABELS: Record<string, string> = {
  backlog: "Backlog",
  todo: "To Do",
  in_progress: "In Progress",
  review: "Review",
  done: "Done",
  cancelled: "Cancelled",
};

// ─── GroupList Sidebar ──────────────────────────────────

function GroupList({
  groups, selectedId, onSelect, onCreate,
}: {
  groups: Group[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: (name: string) => void;
}) {
  const [newName, setNewName] = useState("");

  const handleCreate = () => {
    const name = newName.trim();
    if (!name) return;
    onCreate(name);
    setNewName("");
  };

  return (
    <div className="flex-1 flex flex-col">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Groups</h2>
        <div className="flex gap-1">
          <Input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="New group..."
            className="h-8 text-xs"
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
          />
          <Button size="sm" className="h-8 px-2 text-xs" onClick={handleCreate}>+</Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto">
        {groups.map((g) => (
          <button
            key={g.id}
            onClick={() => onSelect(g.id)}
            className={`w-full text-left px-3 py-2.5 text-sm hover:bg-accent transition-colors ${
              selectedId === g.id ? "bg-accent font-medium" : ""
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="text-base">
                {g.group_type === "dm" ? "@" : g.group_type === "project" ? "#" : "💬"}
              </span>
              <span className="truncate">{g.name}</span>
              <span className="ml-auto text-[10px] text-muted-foreground">{g.member_count}</span>
            </div>
          </button>
        ))}
        {groups.length === 0 && (
          <div className="p-4 text-center text-xs text-muted-foreground">No groups yet</div>
        )}
      </div>
    </div>
  );
}

// ─── DmList Sidebar ─────────────────────────────────────

function DmList({
  dms, selectedId, onSelect, onStartDm,
}: {
  dms: DmListItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onStartDm: (targetUserId: string) => void;
}) {
  const [targetId, setTargetId] = useState("");

  const handleStart = () => {
    const id = targetId.trim();
    if (!id) return;
    onStartDm(id);
    setTargetId("");
  };

  const dmTypeIcon = (t: DmListItem["dm_type"]) =>
    t === "human_agent" ? "🤖" : t === "agent_agent" ? "⚡" : "👤";

  return (
    <div className="flex-1 flex flex-col">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Direct Messages</h2>
        <div className="flex gap-1">
          <Input
            value={targetId}
            onChange={(e) => setTargetId(e.target.value)}
            placeholder="User ID..."
            className="h-8 text-xs"
            onKeyDown={(e) => e.key === "Enter" && handleStart()}
          />
          <Button size="sm" className="h-8 px-2 text-xs" onClick={handleStart}>+</Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto">
        {dms.map((dm) => (
          <button
            key={dm.group_id}
            onClick={() => onSelect(dm.group_id)}
            className={`w-full text-left px-3 py-2.5 text-sm hover:bg-accent transition-colors ${
              selectedId === dm.group_id ? "bg-accent font-medium" : ""
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="text-base">{dmTypeIcon(dm.dm_type)}</span>
              <span className="truncate">{dm.other_member_id}</span>
              {dm.dm_type !== "human_human" && (
                <Badge className="text-[8px] px-1 py-0 ml-auto bg-purple-100 text-purple-700">
                  {dm.dm_type.replace("_", "→")}
                </Badge>
              )}
            </div>
          </button>
        ))}
        {dms.length === 0 && (
          <div className="p-4 text-center text-xs text-muted-foreground">No DMs yet</div>
        )}
      </div>
    </div>
  );
}

// ─── MessageBubble ──────────────────────────────────────

// ─── Message type renderers ─────────────────────────────

function ToolCallContent({ content }: { content: string }) {
  let parsed: { name?: string; arguments?: unknown; result?: unknown } = {};
  try { parsed = JSON.parse(content); } catch { /* plain text fallback */ }
  return (
    <div className="space-y-1">
      {parsed.name && (
        <div className="flex items-center gap-1.5">
          <Badge className="text-[9px] px-1 py-0 bg-purple-100 text-purple-700">tool</Badge>
          <span className="text-[12px] font-mono font-medium">{parsed.name}</span>
        </div>
      )}
      {parsed.arguments !== undefined && (
        <pre className="text-[11px] bg-muted/50 rounded p-1.5 overflow-x-auto max-h-32">
          {typeof parsed.arguments === "string" ? parsed.arguments : JSON.stringify(parsed.arguments, null, 2)}
        </pre>
      )}
      {parsed.result !== undefined && (
        <div className="text-[11px] text-green-600 bg-green-50 rounded p-1.5">
          {typeof parsed.result === "string" ? parsed.result : JSON.stringify(parsed.result)}
        </div>
      )}
      {!parsed.name && <div className="text-[13px] whitespace-pre-wrap">{content}</div>}
    </div>
  );
}

function ThinkingContent({ content }: { content: string }) {
  return (
    <details className="text-[12px]">
      <summary className="cursor-pointer text-muted-foreground text-[11px] flex items-center gap-1">
        <span>Thinking...</span>
      </summary>
      <div className="mt-1 pl-2 border-l-2 border-muted-foreground/20 text-muted-foreground whitespace-pre-wrap">
        {content}
      </div>
    </details>
  );
}

function WorkflowContent({ content, msgType }: { content: string; msgType: string }) {
  let parsed: { workflow_id?: string; step_name?: string; status?: string; error?: string } = {};
  try { parsed = JSON.parse(content); } catch { /* plain text */ }
  const isFailed = msgType === "workflow_failed";
  const isComplete = msgType === "workflow_complete";
  const icon = isFailed ? "✗" : isComplete ? "✓" : "⟳";
  const color = isFailed ? "text-red-500" : isComplete ? "text-green-500" : "text-blue-500";
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-1.5">
        <span className={`text-[12px] ${color}`}>{icon}</span>
        <Badge className={`text-[9px] px-1 py-0 ${isFailed ? "bg-red-100 text-red-700" : isComplete ? "bg-green-100 text-green-700" : "bg-blue-100 text-blue-700"}`}>
          {msgType.replace("workflow_", "")}
        </Badge>
        {parsed.step_name && <span className="text-[11px] font-mono">{parsed.step_name}</span>}
      </div>
      {parsed.workflow_id && <div className="text-[10px] text-muted-foreground">ID: {parsed.workflow_id.slice(0, 8)}</div>}
      {parsed.error && <div className="text-[11px] text-red-500">{parsed.error}</div>}
      {!parsed.workflow_id && <div className="text-[13px] whitespace-pre-wrap">{content}</div>}
    </div>
  );
}

function SkillContent({ content, msgType }: { content: string; msgType: string }) {
  let parsed: { skill_name?: string; input?: unknown; output?: unknown } = {};
  try { parsed = JSON.parse(content); } catch { /* plain text */ }
  const isResult = msgType === "skill_result";
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-1.5">
        <Badge className="text-[9px] px-1 py-0 bg-cyan-100 text-cyan-700">{isResult ? "skill result" : "skill"}</Badge>
        {parsed.skill_name && <span className="text-[12px] font-mono font-medium">{parsed.skill_name}</span>}
      </div>
      {parsed.input !== undefined && (
        <pre className="text-[11px] bg-muted/50 rounded p-1.5 overflow-x-auto max-h-24">
          {typeof parsed.input === "string" ? parsed.input : JSON.stringify(parsed.input, null, 2)}
        </pre>
      )}
      {parsed.output !== undefined && (
        <div className="text-[11px] bg-green-50 rounded p-1.5">
          {typeof parsed.output === "string" ? parsed.output : JSON.stringify(parsed.output)}
        </div>
      )}
      {!parsed.skill_name && <div className="text-[13px] whitespace-pre-wrap">{content}</div>}
    </div>
  );
}

function TaskEventContent({ content }: { content: string }) {
  let parsed: { task_id?: string; title?: string; status?: string } = {};
  try { parsed = JSON.parse(content); } catch { /* plain text */ }
  return (
    <div className="flex items-center gap-2">
      <Badge className="text-[9px] px-1 py-0 bg-amber-100 text-amber-700">task</Badge>
      {parsed.title && <span className="text-[12px]">{parsed.title}</span>}
      {parsed.status && (
        <Badge variant="outline" className="text-[9px] px-1 py-0">{parsed.status}</Badge>
      )}
      {!parsed.title && <span className="text-[13px]">{content}</span>}
    </div>
  );
}

function MessageContent({ msg }: { msg: GroupMessage }) {
  switch (msg.message_type) {
    case "tool_call":
    case "tool_result":
      return <ToolCallContent content={msg.content} />;
    case "thinking":
      return <ThinkingContent content={msg.content} />;
    case "workflow_run":
    case "workflow_step":
    case "workflow_complete":
    case "workflow_failed":
      return <WorkflowContent content={msg.content} msgType={msg.message_type} />;
    case "skill_call":
    case "skill_result":
      return <SkillContent content={msg.content} msgType={msg.message_type} />;
    case "task_created":
    case "task_updated":
    case "task_completed":
      return <TaskEventContent content={msg.content} />;
    case "markdown":
      return <div className="text-[13px] leading-snug whitespace-pre-wrap">{msg.content}</div>;
    case "image": {
      const imgAttId = msg.attachment_id;
      if (imgAttId) {
        return (
          <div className="space-y-1">
            <img
              src={getAttachmentDownloadUrl(imgAttId)}
              alt={msg.content}
              className="max-w-xs max-h-64 rounded border object-contain"
            />
            <div className="text-[11px] text-muted-foreground">{msg.content}</div>
          </div>
        );
      }
      return <div className="text-[13px] text-blue-500">🖼️ {msg.content}</div>;
    }
    case "file": {
      const fileAttId = msg.attachment_id;
      if (fileAttId) {
        return (
          <a
            href={getAttachmentDownloadUrl(fileAttId)}
            download
            className="flex items-center gap-2 text-[13px] text-blue-500 hover:underline"
          >
            <span>📎</span>
            <span>{msg.content}</span>
          </a>
        );
      }
      return <div className="text-[13px] text-blue-500">📎 {msg.content}</div>;
    }
    case "voice":
      return <div className="text-[13px] text-blue-500">[Voice] {msg.content}</div>;
    default:
      return <div className="text-[13px] leading-snug whitespace-pre-wrap">{msg.content}</div>;
  }
}

// ─── MessageBubble ──────────────────────────────────────

function MessageBubble({
  msg, onReply, onPin, onReact, onEdit, onDelete, onThread, isOwn,
}: {
  msg: GroupMessage;
  onReply: (id: string) => void;
  onPin: (id: string) => void;
  onReact: (id: string, emoji: string) => void;
  onEdit: (id: string, content: string) => void;
  onDelete: (id: string) => void;
  onThread: (id: string) => void;
  isOwn: boolean;
}) {
  const isSystem = msg.sender_type === "system";
  const isAgent = msg.sender_type === "agent";

  if (isSystem || msg.message_type === "system" || msg.message_type === "member_join" || msg.message_type === "member_leave") {
    const icon = msg.message_type === "member_join" ? "→" : msg.message_type === "member_leave" ? "←" : "•";
    return (
      <div className="text-center text-xs text-muted-foreground py-1.5">
        <span className="mr-1">{icon}</span> {msg.content}
      </div>
    );
  }

  const typeBadgeColors: Record<string, string> = {
    tool_call: "bg-purple-100 text-purple-700",
    tool_result: "bg-purple-100 text-purple-700",
    thinking: "bg-gray-100 text-gray-600",
    workflow_run: "bg-blue-100 text-blue-700",
    workflow_step: "bg-blue-100 text-blue-700",
    workflow_complete: "bg-green-100 text-green-700",
    workflow_failed: "bg-red-100 text-red-700",
    skill_call: "bg-cyan-100 text-cyan-700",
    skill_result: "bg-cyan-100 text-cyan-700",
    task_created: "bg-amber-100 text-amber-700",
    task_updated: "bg-amber-100 text-amber-700",
    task_completed: "bg-green-100 text-green-700",
    approval_request: "bg-orange-100 text-orange-700",
    approval_response: "bg-orange-100 text-orange-700",
    external_message: "bg-slate-100 text-slate-600",
    cron_trigger: "bg-indigo-100 text-indigo-700",
  };

  return (
    <div className={`flex ${isAgent ? "justify-start" : "justify-end"} mb-3 group`}>
      <div className={`max-w-[75%] rounded-lg px-3 py-2 ${
        isAgent ? "bg-card border shadow-sm" : "bg-primary text-primary-foreground"
      }`}>
        <div className="flex items-center gap-2 mb-1">
          <span className="text-[11px] font-medium">{msg.sender_id}</span>
          {msg.message_type !== "text" && (
            <Badge className={`text-[9px] px-1 py-0 ${typeBadgeColors[msg.message_type] ?? "bg-muted text-muted-foreground"}`}>
              {msg.message_type.replace(/_/g, " ")}
            </Badge>
          )}
          <span className="text-[10px] opacity-50">{formatTime(msg.created_at)}</span>
          {msg.edited_at && <span className="text-[9px] opacity-40">(edited)</span>}
          {msg.pinned && <span className="text-[10px]">📌</span>}
        </div>
        <MessageContent msg={msg} />
        {msg.thread_reply_count > 0 && (
          <button onClick={() => onThread(msg.id)} className="text-[10px] text-blue-500 mt-1 hover:underline">
            {msg.thread_reply_count} replies
          </button>
        )}
        <div className="opacity-0 group-hover:opacity-100 flex gap-1 mt-1 transition-opacity">
          <button onClick={() => onReply(msg.id)} className="text-[10px] text-muted-foreground hover:text-foreground px-1">Reply</button>
          <button onClick={() => onThread(msg.id)} className="text-[10px] text-muted-foreground hover:text-foreground px-1">Thread</button>
          <button onClick={() => onPin(msg.id)} className="text-[10px] text-muted-foreground hover:text-foreground px-1">Pin</button>
          <button onClick={() => onReact(msg.id, "👍")} className="text-[10px] px-1">👍</button>
          <button onClick={() => onReact(msg.id, "❤️")} className="text-[10px] px-1">❤️</button>
          {isOwn && (
            <>
              <button onClick={() => onEdit(msg.id, msg.content)} className="text-[10px] text-muted-foreground hover:text-foreground px-1">Edit</button>
              <button onClick={() => onDelete(msg.id)} className="text-[10px] text-red-400 hover:text-red-600 px-1">Del</button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── MessageInput ───────────────────────────────────────

function MessageInput({
  onSend, replyTo, onCancelReply, onAttach,
}: {
  onSend: (content: string, replyToId?: string) => void;
  replyTo: string | null;
  onCancelReply: () => void;
  onAttach: (file: File) => void;
}) {
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const handleSend = () => {
    const content = text.trim();
    if (!content) return;
    onSend(content, replyTo ?? undefined);
    setText("");
    onCancelReply();
  };

  return (
    <div className="border-t p-3">
      {replyTo && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground mb-1">
          <span>Replying to {replyTo.slice(0, 8)}</span>
          <button onClick={onCancelReply} className="text-[10px] hover:text-foreground">✕</button>
        </div>
      )}
      <div className="flex gap-2">
        <input
          ref={fileRef}
          type="file"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) { onAttach(file); e.target.value = ""; }
          }}
        />
        <button
          onClick={() => fileRef.current?.click()}
          className="h-9 px-2 text-muted-foreground hover:text-foreground border rounded-md text-sm"
          title="Attach file"
        >
          📎
        </button>
        <textarea
          ref={inputRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              handleSend();
            }
          }}
          placeholder="Type a message..."
          className="flex-1 min-h-[36px] max-h-[120px] resize-none rounded-md border bg-background px-3 py-2 text-sm"
          rows={1}
        />
        <Button onClick={handleSend} disabled={!text.trim()} className="h-9 px-4 text-sm">
          Send
        </Button>
      </div>
    </div>
  );
}

// ─── TaskSidebar ────────────────────────────────────────

function TaskSidebar({ tasks }: { tasks: TaskV3[] }) {
  return (
    <div className="w-64 bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold">Tasks</h2>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {tasks.map((task) => (
          <Card key={task.id} className="p-2">
            <div className="text-xs font-medium mb-1">{task.title}</div>
            <div className="flex items-center gap-1">
              <Badge className={`text-[9px] px-1 py-0 ${PRIORITY_COLORS[task.priority] ?? ""}`}>
                {task.priority}
              </Badge>
              <Badge variant="outline" className="text-[9px] px-1 py-0">
                {STATUS_LABELS[task.status] ?? task.status}
              </Badge>
            </div>
            {task.assignee_id && (
              <div className="text-[10px] text-muted-foreground mt-1">
                → {task.assignee_id}
              </div>
            )}
          </Card>
        ))}
        {tasks.length === 0 && (
          <div className="text-center text-xs text-muted-foreground py-4">No tasks</div>
        )}
      </div>
    </div>
  );
}

// ─── ApprovalCard ───────────────────────────────────────

function ApprovalCard({
  approval, onVote,
}: {
  approval: ApprovalRequest;
  onVote: (id: string, decision: string) => void;
}) {
  return (
    <Card className="p-3 mb-3 border-l-4 border-l-amber-400">
      <div className="flex items-center gap-2 mb-1">
        <Badge variant="outline" className="text-[10px]">Approval</Badge>
        <Badge className={`text-[9px] ${approval.urgency === "critical" ? "bg-red-100 text-red-700" : "bg-amber-100 text-amber-700"}`}>
          {approval.urgency}
        </Badge>
      </div>
      <div className="text-sm font-medium mb-1">{approval.title}</div>
      {approval.description && (
        <div className="text-xs text-muted-foreground mb-2">{approval.description}</div>
      )}
      <div className="flex items-center gap-2 text-[10px] text-muted-foreground mb-2">
        <span>Quorum: {approval.quorum_type} ({approval.required_count})</span>
        <span>•</span>
        <span>Status: {approval.execution_status}</span>
      </div>
      {approval.execution_status === "pending" && (
        <div className="flex gap-2">
          <Button size="sm" className="h-7 text-xs bg-green-600 hover:bg-green-700" onClick={() => onVote(approval.id, "approve")}>
            Approve
          </Button>
          <Button size="sm" variant="destructive" className="h-7 text-xs" onClick={() => onVote(approval.id, "reject")}>
            Reject
          </Button>
        </div>
      )}
    </Card>
  );
}

// ─── TaskBoard ──────────────────────────────────────────

const TASK_COLUMNS: { status: string; label: string; color: string }[] = [
  { status: "backlog", label: "Backlog", color: "bg-gray-100" },
  { status: "todo", label: "To Do", color: "bg-blue-50" },
  { status: "in_progress", label: "In Progress", color: "bg-yellow-50" },
  { status: "review", label: "Review", color: "bg-purple-50" },
  { status: "done", label: "Done", color: "bg-green-50" },
];

const VALID_TRANSITIONS: Record<string, string[]> = {
  backlog: ["todo"],
  todo: ["in_progress", "cancelled"],
  in_progress: ["review", "cancelled"],
  review: ["done", "in_progress"],
  done: [],
  cancelled: ["backlog"],
};

function TaskBoard({ tasks, onTransition }: {
  tasks: TaskV3[];
  onTransition: (taskId: string, newStatus: string) => void;
}) {
  const [expandedTask, setExpandedTask] = useState<string | null>(null);

  const tasksByStatus = TASK_COLUMNS.reduce<Record<string, TaskV3[]>>((acc, col) => {
    acc[col.status] = tasks.filter((t) => t.status === col.status);
    return acc;
  }, {});

  return (
    <div className="flex gap-2 overflow-x-auto h-full p-2">
      {TASK_COLUMNS.map((col) => (
        <div key={col.status} className={`flex-shrink-0 w-52 ${col.color} rounded-lg p-2 flex flex-col`}>
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold">{col.label}</span>
            <Badge variant="outline" className="text-[9px] px-1 py-0">
              {tasksByStatus[col.status]?.length ?? 0}
            </Badge>
          </div>
          <div className="flex-1 space-y-1.5 overflow-y-auto">
            {(tasksByStatus[col.status] ?? []).map((task) => {
              const nextStatuses = VALID_TRANSITIONS[task.status] ?? [];
              const isExpanded = expandedTask === task.id;
              return (
                <Card
                  key={task.id}
                  className="p-2 cursor-pointer hover:shadow-md transition-shadow"
                  onClick={() => setExpandedTask(isExpanded ? null : task.id)}
                >
                  <div className="text-[11px] font-medium leading-tight">{task.title}</div>
                  <div className="flex items-center gap-1 mt-1">
                    <Badge className={`text-[8px] px-1 py-0 ${PRIORITY_COLORS[task.priority] ?? ""}`}>
                      {task.priority}
                    </Badge>
                    {task.assignee_id && (
                      <span className="text-[9px] text-muted-foreground truncate">{task.assignee_id}</span>
                    )}
                  </div>
                  {isExpanded && nextStatuses.length > 0 && (
                    <div className="flex gap-1 mt-2 pt-1 border-t">
                      {nextStatuses.map((s) => (
                        <button
                          key={s}
                          onClick={(e) => { e.stopPropagation(); onTransition(task.id, s); }}
                          className="text-[9px] px-1.5 py-0.5 rounded bg-primary/10 hover:bg-primary/20 text-primary"
                        >
                          → {STATUS_LABELS[s] ?? s}
                        </button>
                      ))}
                    </div>
                  )}
                </Card>
              );
            })}
            {(tasksByStatus[col.status]?.length ?? 0) === 0 && (
              <div className="text-[10px] text-muted-foreground text-center py-4">Empty</div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

// ─── MemoryPanel ────────────────────────────────────────

function MemoryPanel({ agentId }: { agentId: string }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Array<{ memory: AgentMemory; score: number }>>([]);
  const [stats, setStats] = useState<{ working_count: number; episodic_count: number; semantic_count: number; total_count: number } | null>(null);
  const [searching, setSearching] = useState(false);
  const [memoryType, setMemoryType] = useState<string>("");

  useEffect(() => {
    if (!agentId) return;
    getMemoryStats(agentId).then((r) => setStats(r.stats)).catch(console.error);
  }, [agentId]);

  const handleSearch = async () => {
    if (!agentId) return;
    setSearching(true);
    try {
      const res = await searchMemory({
        agent_id: agentId,
        query_text: query || undefined,
        memory_type: memoryType || undefined,
        limit: 20,
      });
      setResults(res.results);
    } catch (err) {
      console.error("Memory search failed:", err);
    } finally {
      setSearching(false);
    }
  };

  const layerColors: Record<string, string> = {
    working: "bg-yellow-100 text-yellow-700",
    episodic: "bg-blue-100 text-blue-700",
    semantic: "bg-green-100 text-green-700",
  };

  return (
    <div className="w-72 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Agent Memory</h2>
        {stats && (
          <div className="flex gap-2 text-[10px] mb-2">
            <span className="text-yellow-600">W:{stats.working_count}</span>
            <span className="text-blue-600">E:{stats.episodic_count}</span>
            <span className="text-green-600">S:{stats.semantic_count}</span>
            <span className="text-muted-foreground">Total:{stats.total_count}</span>
          </div>
        )}
        <div className="flex gap-1 mb-1">
          <select
            value={memoryType}
            onChange={(e) => setMemoryType(e.target.value)}
            className="h-7 text-[10px] rounded border bg-background px-1"
          >
            <option value="">All layers</option>
            <option value="working">Working</option>
            <option value="episodic">Episodic</option>
            <option value="semantic">Semantic</option>
          </select>
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSearch()}
            placeholder="Search memories..."
            className="h-7 text-xs flex-1"
          />
          <Button size="sm" className="h-7 px-2 text-xs" onClick={handleSearch} disabled={searching}>
            {searching ? "..." : "Go"}
          </Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {results.map(({ memory, score }) => (
          <Card key={memory.id} className="p-2">
            <div className="flex items-center gap-1 mb-1">
              <Badge className={`text-[8px] px-1 py-0 ${layerColors[memory.memory_type] ?? ""}`}>
                {memory.memory_type}
              </Badge>
              <span className="text-[9px] text-muted-foreground ml-auto">
                score: {score.toFixed(2)} | hits: {memory.access_count}
              </span>
            </div>
            <div className="text-[11px] leading-snug line-clamp-3">{memory.content}</div>
            {memory.summary && (
              <div className="text-[10px] text-muted-foreground mt-1 line-clamp-1">{memory.summary}</div>
            )}
            {memory.group_id && (
              <div className="text-[9px] text-muted-foreground mt-1">group: {memory.group_id.slice(0, 8)}</div>
            )}
          </Card>
        ))}
        {results.length === 0 && !searching && (
          <div className="text-center text-xs text-muted-foreground py-4">
            {query ? "No results" : "Search agent memories"}
          </div>
        )}
      </div>
    </div>
  );
}

// ─── RulesPanel ──────────────────────────────────────────

function RulesPanel({ groupId }: { groupId: string }) {
  const [rules, setRules] = useState<GroupRule[]>([]);
  const [loading, setLoading] = useState(false);
  const [ruleType, setRuleType] = useState("auto_assign");
  const [configText, setConfigText] = useState("{}");
  const [priority, setPriority] = useState("10");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listRules(groupId);
      setRules(res.rules);
    } catch (err) {
      console.error("Failed to load rules:", err);
    } finally {
      setLoading(false);
    }
  }, [groupId]);

  useEffect(() => { load(); }, [load]);

  const handleCreate = async () => {
    try {
      const config = JSON.parse(configText);
      await createRule(groupId, { rule_type: ruleType, config, priority: parseInt(priority) || 10 });
      setConfigText("{}");
      await load();
    } catch (err) {
      console.error("Failed to create rule:", err);
    }
  };

  const handleToggle = async (rule: GroupRule) => {
    try {
      await updateRule(groupId, rule.id, { enabled: !rule.enabled });
      await load();
    } catch (err) {
      console.error("Failed to toggle rule:", err);
    }
  };

  const handleDelete = async (ruleId: string) => {
    try {
      await deleteRule(groupId, ruleId);
      await load();
    } catch (err) {
      console.error("Failed to delete rule:", err);
    }
  };

  const ruleTypeColors: Record<string, string> = {
    auto_assign: "bg-blue-100 text-blue-700",
    auto_approve: "bg-green-100 text-green-700",
    rate_limit: "bg-orange-100 text-orange-700",
    time_window: "bg-purple-100 text-purple-700",
  };

  return (
    <div className="w-72 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Group Rules</h2>
        <div className="space-y-1">
          <select
            value={ruleType}
            onChange={(e) => setRuleType(e.target.value)}
            className="h-7 w-full text-[10px] rounded border bg-background px-1"
          >
            <option value="auto_assign">Auto Assign</option>
            <option value="auto_approve">Auto Approve</option>
            <option value="rate_limit">Rate Limit</option>
            <option value="time_window">Time Window</option>
          </select>
          <textarea
            value={configText}
            onChange={(e) => setConfigText(e.target.value)}
            placeholder='{"key":"value"}'
            className="w-full h-14 text-[10px] rounded border bg-background px-2 py-1 font-mono resize-none"
          />
          <div className="flex gap-1">
            <Input
              value={priority}
              onChange={(e) => setPriority(e.target.value)}
              placeholder="Priority"
              className="h-7 text-xs w-16"
            />
            <Button size="sm" className="h-7 px-2 text-xs flex-1" onClick={handleCreate}>Add Rule</Button>
          </div>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {loading && <div className="text-center text-xs text-muted-foreground py-4">Loading...</div>}
        {rules.map((rule) => (
          <Card key={rule.id} className={`p-2 ${!rule.enabled ? "opacity-50" : ""}`}>
            <div className="flex items-center gap-1 mb-1">
              <Badge className={`text-[8px] px-1 py-0 ${ruleTypeColors[rule.rule_type] ?? ""}`}>
                {rule.rule_type}
              </Badge>
              <span className="text-[9px] text-muted-foreground">P{rule.priority}</span>
              <div className="ml-auto flex gap-1">
                <button
                  onClick={() => handleToggle(rule)}
                  className="text-[9px] px-1 rounded hover:bg-accent"
                  title={rule.enabled ? "Disable" : "Enable"}
                >
                  {rule.enabled ? "⏸" : "▶"}
                </button>
                <button
                  onClick={() => handleDelete(rule.id)}
                  className="text-[9px] px-1 rounded hover:bg-accent text-red-400"
                  title="Delete"
                >
                  ✕
                </button>
              </div>
            </div>
            <pre className="text-[9px] text-muted-foreground overflow-x-auto max-h-16 font-mono">
              {JSON.stringify(rule.config, null, 2)}
            </pre>
          </Card>
        ))}
        {rules.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No rules configured</div>
        )}
      </div>
    </div>
  );
}

// ─── CronPanel ───────────────────────────────────────────

function CronPanel({ groupId }: { groupId: string }) {
  const [jobs, setJobs] = useState<GroupCronJob[]>([]);
  const [loading, setLoading] = useState(false);
  const [name, setName] = useState("");
  const [cronExpr, setCronExpr] = useState("");
  const [template, setTemplate] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listCronJobs(groupId);
      setJobs(res.jobs);
    } catch (err) {
      console.error("Failed to load cron jobs:", err);
    } finally {
      setLoading(false);
    }
  }, [groupId]);

  useEffect(() => { load(); }, [load]);

  const handleCreate = async () => {
    if (!name.trim() || !cronExpr.trim()) return;
    try {
      await createCronJob(groupId, {
        name: name.trim(),
        cron_expr: cronExpr.trim(),
        message_template: template.trim() || `${name.trim()} triggered`,
      });
      setName(""); setCronExpr(""); setTemplate("");
      await load();
    } catch (err) {
      console.error("Failed to create cron job:", err);
    }
  };

  const handleToggle = async (job: GroupCronJob) => {
    try {
      await updateCronJob(groupId, job.id, { enabled: !job.enabled });
      await load();
    } catch (err) {
      console.error("Failed to toggle cron job:", err);
    }
  };

  const handleDelete = async (jobId: string) => {
    try {
      await deleteCronJob(groupId, jobId);
      await load();
    } catch (err) {
      console.error("Failed to delete cron job:", err);
    }
  };

  return (
    <div className="w-72 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Cron Jobs</h2>
        <div className="space-y-1">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Job name"
            className="h-7 text-xs"
          />
          <Input
            value={cronExpr}
            onChange={(e) => setCronExpr(e.target.value)}
            placeholder="Cron expr (0 9 * * 1-5)"
            className="h-7 text-xs font-mono"
          />
          <Input
            value={template}
            onChange={(e) => setTemplate(e.target.value)}
            placeholder="Message template"
            className="h-7 text-xs"
          />
          <Button size="sm" className="h-7 px-2 text-xs w-full" onClick={handleCreate}>Add Cron Job</Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {loading && <div className="text-center text-xs text-muted-foreground py-4">Loading...</div>}
        {jobs.map((job) => (
          <Card key={job.id} className={`p-2 ${!job.enabled ? "opacity-50" : ""}`}>
            <div className="flex items-center gap-1 mb-1">
              <span className="text-[11px] font-medium truncate flex-1">{job.name}</span>
              <div className="flex gap-1">
                <button
                  onClick={() => handleToggle(job)}
                  className="text-[9px] px-1 rounded hover:bg-accent"
                  title={job.enabled ? "Disable" : "Enable"}
                >
                  {job.enabled ? "⏸" : "▶"}
                </button>
                <button
                  onClick={() => handleDelete(job.id)}
                  className="text-[9px] px-1 rounded hover:bg-accent text-red-400"
                  title="Delete"
                >
                  ✕
                </button>
              </div>
            </div>
            <div className="text-[9px] font-mono text-muted-foreground">{job.cron_expr}</div>
            <div className="text-[9px] text-muted-foreground mt-1">{job.message_template}</div>
            <div className="flex items-center gap-2 mt-1 text-[8px] text-muted-foreground">
              <span>Runs: {job.run_count}</span>
              {job.last_run_at && <span>Last: {formatTime(job.last_run_at)}</span>}
            </div>
          </Card>
        ))}
        {jobs.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No cron jobs</div>
        )}
      </div>
    </div>
  );
}

// ─── HooksPanel ──────────────────────────────────────────

function HooksPanel({ groupId }: { groupId: string }) {
  const [hooks, setHooks] = useState<AgentHook[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedHook, setSelectedHook] = useState<string | null>(null);
  const [logs, setLogs] = useState<HookLog[]>([]);
  const [agentId, setAgentId] = useState("");
  const [eventTypes, setEventTypes] = useState("message.created");
  const [actionType, setActionType] = useState("notify");
  const [actionConfig, setActionConfig] = useState("{}");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listHooks(groupId);
      setHooks(res.hooks);
    } catch (err) {
      console.error("Failed to load hooks:", err);
    } finally {
      setLoading(false);
    }
  }, [groupId]);

  useEffect(() => { load(); }, [load]);

  const loadLogs = async (hookId: string) => {
    setSelectedHook(hookId);
    try {
      const res = await listHookLogs(groupId, hookId, 20);
      setLogs(res.logs);
    } catch (err) {
      console.error("Failed to load hook logs:", err);
    }
  };

  const handleCreate = async () => {
    if (!agentId.trim()) return;
    try {
      const events = eventTypes.split(",").map((e) => e.trim()).filter(Boolean);
      let config = {};
      try { config = JSON.parse(actionConfig); } catch { /* ignore */ }
      await createHook(groupId, {
        agent_id: agentId.trim(),
        event_types: events,
        action_type: actionType,
        action_config: config,
      });
      setAgentId(""); setEventTypes("message.created"); setActionType("notify"); setActionConfig("{}");
      await load();
    } catch (err) {
      console.error("Failed to create hook:", err);
    }
  };

  const handleToggle = async (hook: AgentHook) => {
    try {
      await updateHook(groupId, hook.id, { enabled: !hook.enabled });
      await load();
    } catch (err) {
      console.error("Failed to toggle hook:", err);
    }
  };

  const handleDelete = async (hookId: string) => {
    try {
      await deleteHook(groupId, hookId);
      if (selectedHook === hookId) { setSelectedHook(null); setLogs([]); }
      await load();
    } catch (err) {
      console.error("Failed to delete hook:", err);
    }
  };

  const statusColors: Record<string, string> = {
    success: "text-green-600",
    failed: "text-red-500",
    skipped: "text-yellow-500",
  };

  return (
    <div className="w-72 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Agent Hooks</h2>
        <div className="space-y-1">
          <Input
            value={agentId}
            onChange={(e) => setAgentId(e.target.value)}
            placeholder="Agent ID"
            className="h-7 text-xs"
          />
          <Input
            value={eventTypes}
            onChange={(e) => setEventTypes(e.target.value)}
            placeholder="Events (comma-separated)"
            className="h-7 text-xs"
          />
          <div className="flex gap-1">
            <select
              value={actionType}
              onChange={(e) => setActionType(e.target.value)}
              className="h-7 text-[10px] rounded border bg-background px-1 flex-1"
            >
              <option value="notify">Notify</option>
              <option value="block">Block</option>
              <option value="approve">Approve</option>
              <option value="webhook">Webhook</option>
            </select>
            <Button size="sm" className="h-7 px-2 text-xs" onClick={handleCreate}>Add</Button>
          </div>
          <textarea
            value={actionConfig}
            onChange={(e) => setActionConfig(e.target.value)}
            placeholder='{"key":"value"}'
            className="w-full h-10 text-[9px] rounded border bg-background px-2 py-1 font-mono resize-none"
          />
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {loading && <div className="text-center text-xs text-muted-foreground py-4">Loading...</div>}
        {hooks.map((hook) => (
          <Card
            key={hook.id}
            className={`p-2 cursor-pointer transition-colors ${!hook.enabled ? "opacity-50" : ""} ${selectedHook === hook.id ? "ring-1 ring-primary" : ""}`}
            onClick={() => loadLogs(hook.id)}
          >
            <div className="flex items-center gap-1 mb-1">
              <span className="text-[10px] font-medium truncate flex-1">{hook.agent_id}</span>
              <Badge className="text-[8px] px-1 py-0 bg-blue-100 text-blue-700">{hook.action_type}</Badge>
              <div className="flex gap-1">
                <button
                  onClick={(e) => { e.stopPropagation(); handleToggle(hook); }}
                  className="text-[9px] px-1 rounded hover:bg-accent"
                  title={hook.enabled ? "Disable" : "Enable"}
                >
                  {hook.enabled ? "⏸" : "▶"}
                </button>
                <button
                  onClick={(e) => { e.stopPropagation(); handleDelete(hook.id); }}
                  className="text-[9px] px-1 rounded hover:bg-accent text-red-400"
                  title="Delete"
                >
                  ✕
                </button>
              </div>
            </div>
            <div className="text-[9px] text-muted-foreground">
              Events: {(() => { try { return JSON.parse(hook.event_types).join(", "); } catch { return hook.event_types; } })()}
            </div>
            <div className="text-[9px] text-muted-foreground">P{hook.priority}</div>
          </Card>
        ))}
        {hooks.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No hooks configured</div>
        )}
      </div>
      {selectedHook && logs.length > 0 && (
        <div className="border-t p-2 max-h-48 overflow-y-auto">
          <div className="text-[10px] font-semibold mb-1">Execution Logs</div>
          {logs.map((log) => (
            <div key={log.id} className="flex items-center gap-1 text-[9px] py-0.5">
              <span className={statusColors[log.status] ?? "text-muted-foreground"}>
                {log.status === "success" ? "✓" : log.status === "failed" ? "✗" : "○"}
              </span>
              <span className="font-mono truncate flex-1">{log.event_type}</span>
              <span className="text-muted-foreground">{formatTime(log.executed_at)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── WorkflowPanel ──────────────────────────────────────

function WorkflowPanel() {
  const [workflows, setWorkflows] = useState<WorkflowDef[]>([]);
  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [checkpoints, setCheckpoints] = useState<RunCheckpoint[]>([]);
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [view, setView] = useState<"defs" | "runs">("defs");
  const [wfName, setWfName] = useState("");
  const [wfId, setWfId] = useState("");
  const [wfYaml, setWfYaml] = useState("");

  const loadWorkflows = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listWorkflows();
      setWorkflows(res.workflows);
    } catch (err) {
      console.error("Failed to load workflows:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadRuns = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listWorkflowRuns({ limit: 50 });
      setRuns(res.runs);
    } catch (err) {
      console.error("Failed to load runs:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (view === "defs") loadWorkflows();
    else loadRuns();
  }, [view, loadWorkflows, loadRuns]);

  const loadCheckpoints = async (runId: string) => {
    setSelectedRun(runId);
    try {
      const res = await listCheckpoints(runId);
      setCheckpoints(res.checkpoints);
    } catch (err) {
      console.error("Failed to load checkpoints:", err);
    }
  };

  const handleCreate = async () => {
    if (!wfId.trim() || !wfName.trim()) return;
    try {
      await createWorkflow({ id: wfId.trim(), name: wfName.trim(), yaml_content: wfYaml || "nodes: []" });
      setWfId(""); setWfName(""); setWfYaml("");
      await loadWorkflows();
    } catch (err) {
      console.error("Failed to create workflow:", err);
    }
  };

  const handleDelete = async (wid: string) => {
    try {
      await deleteWorkflow(wid);
      await loadWorkflows();
    } catch (err) {
      console.error("Failed to delete workflow:", err);
    }
  };

  const handleTriggerRun = async (wf: WorkflowDef) => {
    try {
      await createWorkflowRun({
        workflow_id: wf.id,
        workflow_version: wf.version,
        input: "{}",
      });
      setView("runs");
    } catch (err) {
      console.error("Failed to trigger run:", err);
    }
  };

  const handleUpdateRunStatus = async (runId: string, status: string) => {
    try {
      await updateWorkflowRunStatus(runId, { status });
      await loadRuns();
      if (selectedRun === runId) await loadCheckpoints(runId);
    } catch (err) {
      console.error("Failed to update run:", err);
    }
  };

  const statusColor: Record<string, string> = {
    running: "bg-blue-100 text-blue-700",
    completed: "bg-green-100 text-green-700",
    failed: "bg-red-100 text-red-700",
    cancelled: "bg-gray-100 text-gray-600",
    draft: "bg-yellow-100 text-yellow-700",
    active: "bg-green-100 text-green-700",
  };

  return (
    <div className="w-72 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">Workflows</h2>
        <div className="flex gap-1 mb-2">
          <button
            onClick={() => setView("defs")}
            className={`flex-1 text-xs py-1 rounded ${view === "defs" ? "bg-primary text-primary-foreground" : "hover:bg-accent"}`}
          >
            Definitions
          </button>
          <button
            onClick={() => setView("runs")}
            className={`flex-1 text-xs py-1 rounded ${view === "runs" ? "bg-primary text-primary-foreground" : "hover:bg-accent"}`}
          >
            Runs
          </button>
        </div>
        {view === "defs" && (
          <div className="space-y-1">
            <Input value={wfId} onChange={(e) => setWfId(e.target.value)} placeholder="Workflow ID" className="h-7 text-xs" />
            <Input value={wfName} onChange={(e) => setWfName(e.target.value)} placeholder="Name" className="h-7 text-xs" />
            <textarea
              value={wfYaml}
              onChange={(e) => setWfYaml(e.target.value)}
              placeholder="YAML definition (optional)"
              className="w-full h-12 text-[9px] rounded border bg-background px-2 py-1 font-mono resize-none"
            />
            <Button size="sm" className="h-7 w-full text-xs" onClick={handleCreate}>Create Workflow</Button>
          </div>
        )}
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {loading && <div className="text-center text-xs text-muted-foreground py-4">Loading...</div>}

        {view === "defs" && workflows.map((wf) => (
          <Card key={wf.id} className="p-2">
            <div className="flex items-center gap-1 mb-1">
              <span className="text-[10px] font-medium truncate flex-1">{wf.name}</span>
              <Badge className={`text-[8px] px-1 py-0 ${statusColor[wf.status] ?? ""}`}>{wf.status}</Badge>
              <div className="flex gap-1">
                <button
                  onClick={() => handleTriggerRun(wf)}
                  className="text-[9px] px-1 rounded hover:bg-accent text-green-600"
                  title="Trigger Run"
                >
                  ▶
                </button>
                <button
                  onClick={() => handleDelete(wf.id)}
                  className="text-[9px] px-1 rounded hover:bg-accent text-red-400"
                  title="Delete"
                >
                  ✕
                </button>
              </div>
            </div>
            <div className="text-[9px] text-muted-foreground font-mono">{wf.id} v{wf.version}</div>
          </Card>
        ))}

        {view === "defs" && workflows.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No workflows defined</div>
        )}

        {view === "runs" && runs.map((run) => (
          <Card
            key={run.id}
            className={`p-2 cursor-pointer transition-colors ${selectedRun === run.id ? "ring-1 ring-primary" : ""}`}
            onClick={() => loadCheckpoints(run.id)}
          >
            <div className="flex items-center gap-1 mb-1">
              <span className="text-[10px] font-medium truncate flex-1">{run.workflow_id}</span>
              <Badge className={`text-[8px] px-1 py-0 ${statusColor[run.status] ?? ""}`}>{run.status}</Badge>
              {run.status === "running" && (
                <button
                  onClick={(e) => { e.stopPropagation(); handleUpdateRunStatus(run.id, "cancelled"); }}
                  className="text-[9px] px-1 rounded hover:bg-accent text-red-400"
                  title="Cancel"
                >
                  ■
                </button>
              )}
            </div>
            <div className="text-[9px] text-muted-foreground font-mono">{run.id.slice(0, 8)}...</div>
            {run.error && <div className="text-[9px] text-red-500 mt-1 truncate">{run.error}</div>}
          </Card>
        ))}

        {view === "runs" && runs.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No runs yet</div>
        )}
      </div>

      {selectedRun && checkpoints.length > 0 && (
        <div className="border-t p-2 max-h-48 overflow-y-auto">
          <div className="text-[10px] font-semibold mb-1">Checkpoints ({checkpoints.length})</div>
          {checkpoints.map((cp) => (
            <div key={cp.id} className="flex items-center gap-1 text-[9px] py-0.5">
              <span className="text-green-600">✓</span>
              <span className="font-mono truncate flex-1">{cp.node_id}</span>
              <span className="text-muted-foreground">{formatTime(cp.created_at)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── DelegationsPanel ──────────────────────────────────

function DelegationsPanel({ groupId }: { groupId: string }) {
  const [delegations, setDelegations] = useState<A2ADelegation[]>([]);
  const [loading, setLoading] = useState(false);
  const [executorId, setExecutorId] = useState("");
  const [prompt, setPrompt] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listDelegations(groupId);
      setDelegations(res.delegations);
    } catch (err) {
      console.error("Failed to load delegations:", err);
    } finally {
      setLoading(false);
    }
  }, [groupId]);

  useEffect(() => { load(); }, [load]);

  const handleCreate = async () => {
    if (!executorId.trim() || !prompt.trim()) return;
    try {
      await createDelegation(groupId, { executor_id: executorId.trim(), prompt: prompt.trim() });
      setExecutorId(""); setPrompt("");
      await load();
    } catch (err) {
      console.error("Failed to create delegation:", err);
    }
  };

  const handleIntervene = async (delegationId: string, action: "revoke" | "reroute") => {
    try {
      await interveneDelegation(groupId, delegationId, { action });
      await load();
    } catch (err) {
      console.error("Failed to intervene:", err);
    }
  };

  const statusColor: Record<string, string> = {
    pending: "bg-yellow-100 text-yellow-700",
    running: "bg-blue-100 text-blue-700",
    completed: "bg-green-100 text-green-700",
    failed: "bg-red-100 text-red-700",
    revoked: "bg-gray-100 text-gray-600",
  };

  return (
    <div className="w-72 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b">
        <h2 className="text-sm font-semibold mb-2">A2A Delegations</h2>
        <div className="space-y-1">
          <Input value={executorId} onChange={(e) => setExecutorId(e.target.value)} placeholder="Executor Agent ID" className="h-7 text-xs" />
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="Delegation prompt..."
            className="w-full h-14 text-[10px] rounded border bg-background px-2 py-1 resize-none"
          />
          <Button size="sm" className="h-7 w-full text-xs" onClick={handleCreate}>Delegate</Button>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {loading && <div className="text-center text-xs text-muted-foreground py-4">Loading...</div>}
        {delegations.map((d) => (
          <Card key={d.id} className="p-2">
            <div className="flex items-center gap-1 mb-1">
              <span className="text-[10px] font-medium truncate flex-1">{d.executor_id}</span>
              <Badge className={`text-[8px] px-1 py-0 ${statusColor[d.status] ?? ""}`}>{d.status}</Badge>
              {d.status === "running" && (
                <button
                  onClick={() => handleIntervene(d.id, "revoke")}
                  className="text-[9px] px-1 rounded hover:bg-accent text-red-400"
                  title="Revoke"
                >
                  ■
                </button>
              )}
            </div>
            <div className="text-[9px] text-muted-foreground line-clamp-2">{d.prompt}</div>
            {d.result && <div className="text-[9px] text-green-600 mt-1 line-clamp-2">{d.result}</div>}
          </Card>
        ))}
        {delegations.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No delegations</div>
        )}
      </div>
    </div>
  );
}

// ─── ThreadPanel ─────────────────────────────────────────

function ThreadPanel({ groupId, threadRootId, onClose, userId }: {
  groupId: string;
  threadRootId: string;
  onClose: () => void;
  userId: string;
}) {
  const [replies, setReplies] = useState<GroupMessage[]>([]);
  const [rootMsg, setRootMsg] = useState<GroupMessage | null>(null);
  const [loading, setLoading] = useState(false);
  const [replyText, setReplyText] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await getThread(groupId, threadRootId);
      setRootMsg(res.root ?? null);
      setReplies(res.replies ?? []);
    } catch (err) {
      console.error("Failed to load thread:", err);
    } finally {
      setLoading(false);
    }
  }, [groupId, threadRootId]);

  useEffect(() => { load(); }, [load]);

  const handleReply = async () => {
    if (!replyText.trim()) return;
    try {
      await sendMessage(groupId, {
        sender_id: userId,
        sender_type: "human",
        message_type: "text",
        content: replyText.trim(),
        thread_root_id: threadRootId,
      });
      setReplyText("");
      await load();
    } catch (err) {
      console.error("Failed to reply:", err);
    }
  };

  return (
    <div className="w-80 border-l bg-muted/30 flex flex-col h-full">
      <div className="p-3 border-b flex items-center justify-between">
        <h2 className="text-sm font-semibold">Thread</h2>
        <button onClick={onClose} className="text-xs text-muted-foreground hover:text-foreground">✕</button>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {loading && <div className="text-center text-xs text-muted-foreground py-4">Loading...</div>}
        {rootMsg && (
          <Card className="p-2 border-l-2 border-primary">
            <div className="text-[10px] text-muted-foreground mb-1">{rootMsg.sender_id}</div>
            <div className="text-[12px]">{rootMsg.content}</div>
          </Card>
        )}
        {replies.map((msg) => (
          <Card key={msg.id} className="p-2">
            <div className="flex items-center gap-1 mb-1">
              <span className="text-[10px] font-medium">{msg.sender_id}</span>
              <span className="text-[9px] text-muted-foreground">{formatTime(msg.created_at)}</span>
            </div>
            <div className="text-[12px]">{msg.content}</div>
          </Card>
        ))}
        {replies.length === 0 && !loading && (
          <div className="text-center text-xs text-muted-foreground py-4">No replies yet</div>
        )}
      </div>
      <div className="border-t p-2">
        <div className="flex gap-1">
          <Input
            value={replyText}
            onChange={(e) => setReplyText(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleReply()}
            placeholder="Reply in thread..."
            className="h-8 text-xs flex-1"
          />
          <Button size="sm" className="h-8 px-3 text-xs" onClick={handleReply} disabled={!replyText.trim()}>Send</Button>
        </div>
      </div>
    </div>
  );
}

// ─── Main Page ──────────────────────────────────────────

export default function V3GroupChatPage() {
  const [groups, setGroups] = useState<Group[]>([]);
  const [dms, setDms] = useState<DmListItem[]>([]);
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [messages, setMessages] = useState<GroupMessage[]>([]);
  const [tasks, setTasks] = useState<TaskV3[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRequest[]>([]);
  const [members, setMembers] = useState<GroupMember[]>([]);
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<GroupMessage[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [leftTab, setLeftTab] = useState<"groups" | "dms">("groups");
  const [rightTab, setRightTab] = useState<"tasks" | "board" | "memory" | "members" | "rules" | "cron" | "hooks" | "workflows" | "delegations">("tasks");
  const [threadRootId, setThreadRootId] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const user = getAuthState().user;
  const userId = user?.user_id ?? "anonymous";

  // WebSocket for real-time updates
  const { connected, subscribe, unsubscribe } = useGroupWebSocket({
    onMessage: useCallback((evt: GroupEvent) => {
      if (evt.type === "group.message.sent" && evt.message_id) {
        // Refetch to get full message object
        if (evt.group_id === selectedGroupId) {
          listMessages(selectedGroupId!, { limit: 100 })
            .then((res) => setMessages(res.messages))
            .catch(console.error);
        }
      }
    }, [selectedGroupId]),
    onApproval: useCallback((evt: GroupEvent) => {
      if (evt.group_id === selectedGroupId) {
        listPendingApprovals(userId, selectedGroupId!).then((r) => setApprovals(r.approvals)).catch(console.error);
      }
    }, [selectedGroupId, userId]),
    onTask: useCallback((evt: GroupEvent) => {
      if (selectedGroupId) {
        listTasks({ group_id: selectedGroupId, limit: 50 }).then((r) => setTasks(r.tasks)).catch(console.error);
      }
    }, [selectedGroupId]),
  });

  // Subscribe to selected group
  useEffect(() => {
    if (selectedGroupId) {
      subscribe(selectedGroupId);
      return () => unsubscribe(selectedGroupId);
    }
  }, [selectedGroupId, subscribe, unsubscribe]);

  // Load groups
  const loadGroups = useCallback(async () => {
    try {
      const res = await listGroups();
      setGroups(res.groups);
      if (res.groups.length > 0 && !selectedGroupId) {
        setSelectedGroupId(res.groups[0].id);
      }
    } catch (err) {
      console.error("Failed to load groups:", err);
    }
  }, [selectedGroupId]);

  // Load messages for selected group
  const loadMessages = useCallback(async () => {
    if (!selectedGroupId) return;
    try {
      const res = await listMessages(selectedGroupId, { limit: 100 });
      setMessages(res.messages);
    } catch (err) {
      console.error("Failed to load messages:", err);
    }
  }, [selectedGroupId]);

  // Load tasks
  const loadTasks = useCallback(async () => {
    if (!selectedGroupId) return;
    try {
      const res = await listTasks({ group_id: selectedGroupId, limit: 50 });
      setTasks(res.tasks);
    } catch (err) {
      console.error("Failed to load tasks:", err);
    }
  }, [selectedGroupId]);

  // Load pending approvals
  const loadApprovals = useCallback(async () => {
    if (!selectedGroupId) return;
    try {
      const res = await listPendingApprovals(userId, selectedGroupId);
      setApprovals(res.approvals);
    } catch (err) {
      console.error("Failed to load approvals:", err);
    }
  }, [selectedGroupId, userId]);

  // Load members
  const loadMembers = useCallback(async () => {
    if (!selectedGroupId) return;
    try {
      const res = await listMembers(selectedGroupId);
      setMembers(res.members);
    } catch (err) {
      console.error("Failed to load members:", err);
    }
  }, [selectedGroupId]);

  // Load DMs
  const loadDms = useCallback(async () => {
    try {
      const res = await listDms();
      setDms(res.dms);
    } catch (err) {
      console.error("Failed to load DMs:", err);
    }
  }, []);

  // Start a new DM
  const handleStartDm = async (targetUserId: string) => {
    try {
      const res = await createDm({ target_user_id: targetUserId });
      await loadDms();
      setSelectedGroupId(res.group.id);
      setLeftTab("dms");
    } catch (err) {
      console.error("Failed to create DM:", err);
    }
  };

  // Initial load
  useEffect(() => { loadGroups(); loadDms(); }, []);

  // Load data when group changes
  useEffect(() => {
    if (!selectedGroupId) return;
    setLoading(true);
    Promise.all([loadMessages(), loadTasks(), loadApprovals(), loadMembers()]).finally(() => setLoading(false));
  }, [selectedGroupId]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Create group
  const handleCreateGroup = async (name: string) => {
    try {
      const res = await createGroup({ name });
      setGroups((prev) => [...prev, res.group]);
      setSelectedGroupId(res.group.id);
    } catch (err) {
      console.error("Failed to create group:", err);
    }
  };

  // Send message
  const handleSend = async (content: string, replyToId?: string) => {
    if (!selectedGroupId) return;
    try {
      const res = await sendMessage(selectedGroupId, {
        sender_id: userId,
        sender_type: "human",
        message_type: "text",
        content,
        reply_to_id: replyToId,
      });
      setMessages((prev) => [...prev, res.message]);
    } catch (err) {
      console.error("Failed to send message:", err);
    }
  };

  // Upload attachment
  const handleAttach = async (file: File) => {
    if (!selectedGroupId) return;
    try {
      const res = await uploadMessageAttachment(selectedGroupId, file);
      if (res.attachments?.length > 0) {
        const att = res.attachments[0];
        const isImage = att.content_type?.startsWith("image/");
        const prefix = isImage ? "🖼️" : "📎";
        const sizeStr = att.size > 1024 * 1024
          ? `${(att.size / (1024 * 1024)).toFixed(1)} MB`
          : `${(att.size / 1024).toFixed(1)} KB`;
        const msgRes = await sendMessage(selectedGroupId, {
          sender_id: userId,
          sender_type: "human",
          message_type: isImage ? "image" : "file",
          content: `${att.filename} (${sizeStr})`,
        });
        // Link attachment to the message
        await linkAttachment(att.id, msgRes.message.id);
        setMessages((prev) => [...prev, msgRes.message]);
      }
    } catch (err) {
      console.error("Failed to upload attachment:", err);
    }
  };

  // Pin message
  const handlePin = async (messageId: string) => {
    if (!selectedGroupId) return;
    try {
      await pinMessage(selectedGroupId, messageId, userId);
      setMessages((prev) =>
        prev.map((m) => m.id === messageId ? { ...m, pinned: true } : m)
      );
    } catch (err) {
      console.error("Failed to pin message:", err);
    }
  };

  // Edit message
  const handleEdit = async (messageId: string, _currentContent: string) => {
    if (!selectedGroupId) return;
    const newContent = window.prompt("Edit message:", _currentContent);
    if (!newContent || newContent === _currentContent) return;
    try {
      await editMessage(selectedGroupId, messageId, { editor_id: userId, content: newContent });
      setMessages((prev) =>
        prev.map((m) => m.id === messageId ? { ...m, content: newContent, edited_at: Math.floor(Date.now() / 1000) } : m)
      );
    } catch (err) {
      console.error("Failed to edit message:", err);
    }
  };

  // Delete message
  const handleDelete = async (messageId: string) => {
    if (!selectedGroupId) return;
    if (!window.confirm("Delete this message?")) return;
    try {
      await deleteMessage(selectedGroupId, messageId);
      setMessages((prev) => prev.filter((m) => m.id !== messageId));
    } catch (err) {
      console.error("Failed to delete message:", err);
    }
  };

  // Search messages
  const handleSearch = async () => {
    if (!selectedGroupId || !searchQuery.trim()) {
      setSearchResults(null);
      return;
    }
    try {
      const res = await searchMessages(selectedGroupId, searchQuery.trim(), 50);
      setSearchResults(res.messages);
    } catch (err) {
      console.error("Failed to search:", err);
    }
  };

  // Add reaction
  const handleReact = async (messageId: string, emoji: string) => {
    if (!selectedGroupId) return;
    try {
      await addReaction(selectedGroupId, messageId, { user_id: userId, emoji });
    } catch (err) {
      console.error("Failed to add reaction:", err);
    }
  };

  // Vote on approval
  const handleVote = async (approvalId: string, decision: string) => {
    try {
      await voteApproval(approvalId, { voter_id: userId, decision });
      await loadApprovals();
    } catch (err) {
      console.error("Failed to vote:", err);
    }
  };

  // Transition task
  const handleTransition = async (taskId: string, newStatus: string) => {
    try {
      await transitionTask(taskId, { status: newStatus, changed_by: userId });
      await loadTasks();
    } catch (err) {
      console.error("Failed to transition task:", err);
    }
  };

  return (
    <div className="flex h-screen bg-background">
      {/* Left: Group/DM Sidebar */}
      <div className="w-64 border-r bg-muted/30 flex flex-col h-full">
        <div className="flex border-b">
          <button
            onClick={() => setLeftTab("groups")}
            className={`flex-1 text-xs py-2 text-center ${leftTab === "groups" ? "border-b-2 border-primary font-medium" : "text-muted-foreground"}`}
          >
            Groups
          </button>
          <button
            onClick={() => setLeftTab("dms")}
            className={`flex-1 text-xs py-2 text-center ${leftTab === "dms" ? "border-b-2 border-primary font-medium" : "text-muted-foreground"}`}
          >
            DMs
          </button>
        </div>
        {leftTab === "groups" ? (
          <GroupList
            groups={groups}
            selectedId={selectedGroupId}
            onSelect={setSelectedGroupId}
            onCreate={handleCreateGroup}
          />
        ) : (
          <DmList
            dms={dms}
            selectedId={selectedGroupId}
            onSelect={setSelectedGroupId}
            onStartDm={handleStartDm}
          />
        )}
      </div>

      {/* Center: Chat Area */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Header */}
        <div className="h-12 border-b flex items-center px-4">
          <span className="font-medium text-sm">
            {(() => {
              const dm = dms.find((d) => d.group_id === selectedGroupId);
              if (dm) return `@ ${dm.other_member_id}`;
              return groups.find((g) => g.id === selectedGroupId)?.name ?? "Select a group";
            })()}
          </span>
          {selectedGroupId && (() => {
            const dm = dms.find((d) => d.group_id === selectedGroupId);
            if (dm) {
              return (
                <Badge className="ml-2 text-[9px] px-1 py-0 bg-purple-100 text-purple-700">
                  {dm.dm_type.replace("_", "→")}
                </Badge>
              );
            }
            return (
              <Badge variant="outline" className="ml-2 text-[10px]">
                {groups.find((g) => g.id === selectedGroupId)?.group_type}
              </Badge>
            );
          })()}
          <div className="ml-auto flex items-center gap-2">
            {selectedGroupId && (
              <div className="flex items-center gap-1">
                <Input
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleSearch();
                    if (e.key === "Escape") { setSearchQuery(""); setSearchResults(null); }
                  }}
                  placeholder="Search..."
                  className="h-7 w-40 text-xs"
                />
                {searchResults !== null && (
                  <button
                    onClick={() => { setSearchQuery(""); setSearchResults(null); }}
                    className="text-[10px] text-muted-foreground hover:text-foreground px-1"
                  >
                    ✕
                  </button>
                )}
              </div>
            )}
            <div className={`w-2 h-2 rounded-full ${connected ? "bg-green-500" : "bg-red-400"}`} />
            <span className="text-[10px] text-muted-foreground">{connected ? "Live" : "Offline"}</span>
          </div>
        </div>

        {/* Messages */}
        <div className="flex-1 overflow-y-auto px-4 py-3">
          {loading ? (
            <div className="flex items-center justify-center h-full">
              <Spinner />
            </div>
          ) : searchResults !== null ? (
            <>
              <div className="text-center text-xs text-muted-foreground py-2 mb-2 border-b">
                {searchResults.length} result{searchResults.length !== 1 ? "s" : ""} for &quot;{searchQuery}&quot;
              </div>
              {searchResults.map((msg) => (
                <MessageBubble
                  key={msg.id}
                  msg={msg}
                  onReply={setReplyTo}
                  onPin={handlePin}
                  onReact={handleReact}
                  onEdit={handleEdit}
                  onDelete={handleDelete}
                  onThread={setThreadRootId}
                  isOwn={msg.sender_id === userId}
                />
              ))}
              {searchResults.length === 0 && (
                <div className="text-center text-sm text-muted-foreground py-8">No messages found</div>
              )}
            </>
          ) : (
            <>
              {/* Approvals inline */}
              {approvals.map((a) => (
                <ApprovalCard key={a.id} approval={a} onVote={handleVote} />
              ))}

              {/* Messages */}
              {messages.map((msg) => (
                <MessageBubble
                  key={msg.id}
                  msg={msg}
                  onReply={setReplyTo}
                  onPin={handlePin}
                  onReact={handleReact}
                  onEdit={handleEdit}
                  onDelete={handleDelete}
                  onThread={setThreadRootId}
                  isOwn={msg.sender_id === userId}
                />
              ))}
              <div ref={messagesEndRef} />
            </>
          )}
        </div>

        {/* Input */}
        <MessageInput
          onSend={handleSend}
          replyTo={replyTo}
          onCancelReply={() => setReplyTo(null)}
          onAttach={handleAttach}
        />
      </div>

      {/* Right: Sidebar Tabs */}
      {threadRootId && selectedGroupId ? (
        <ThreadPanel
          groupId={selectedGroupId}
          threadRootId={threadRootId}
          onClose={() => setThreadRootId(null)}
          userId={userId}
        />
      ) : (
        <div className="flex flex-col h-full border-l">
          <div className="flex border-b">
            {(["tasks", "board", "memory", "members", "rules", "cron", "hooks", "workflows", "delegations"] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => setRightTab(tab)}
                className={`flex-1 text-xs py-2 text-center capitalize ${rightTab === tab ? "border-b-2 border-primary font-medium" : "text-muted-foreground"}`}
              >
                {tab}
              </button>
            ))}
          </div>
          {rightTab === "tasks" && <TaskSidebar tasks={tasks} />}
          {rightTab === "board" && <TaskBoard tasks={tasks} onTransition={handleTransition} />}
          {rightTab === "memory" && <MemoryPanel agentId="default" />}
          {rightTab === "members" && <MembersPanel members={members} />}
          {rightTab === "rules" && selectedGroupId && <RulesPanel groupId={selectedGroupId} />}
          {rightTab === "cron" && selectedGroupId && <CronPanel groupId={selectedGroupId} />}
          {rightTab === "hooks" && selectedGroupId && <HooksPanel groupId={selectedGroupId} />}
          {rightTab === "workflows" && <WorkflowPanel />}
          {rightTab === "delegations" && selectedGroupId && <DelegationsPanel groupId={selectedGroupId} />}
        </div>
      )}
    </div>
  );
}
