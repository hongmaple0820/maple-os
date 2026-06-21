import { fetchApi, getAuthState, API_BASE_URL } from './api';
import type {
  Group, GroupMember, GroupMessage, MessagePage,
  TaskV3, ApprovalRequest, ApprovalVote, ApprovalOutcome,
  AgentMemory, MemoryStats,
  DmListItem, DmToolGrant, A2ADelegation, GroupRule,
  MessageAttachment, GroupCronJob, AgentHook, HookLog,
  WorkflowDef, WorkflowRun, RunCheckpoint,
} from './v3-types';

const V3 = '/api/v3';

// ============================================================
// Groups
// ============================================================

export async function createGroup(data: {
  name: string;
  description?: string;
  group_type?: string;
}): Promise<{ group: Group }> {
  return fetchApi(`${V3}/groups`, { method: 'POST', body: data });
}

export async function listGroups(): Promise<{ groups: Group[] }> {
  return fetchApi(`${V3}/groups`);
}

export async function getGroup(groupId: string): Promise<{ group: Group }> {
  return fetchApi(`${V3}/groups/${groupId}`);
}

export async function listMembers(groupId: string): Promise<{ members: GroupMember[] }> {
  return fetchApi(`${V3}/groups/${groupId}/members`);
}

export async function addMember(groupId: string, data: {
  member_id: string;
  member_type?: string;
  role?: string;
}): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/members`, { method: 'POST', body: data });
}

// ============================================================
// Messages
// ============================================================

export async function listMessages(groupId: string, params?: {
  limit?: number;
  before?: number;
}): Promise<MessagePage> {
  const qs = new URLSearchParams();
  if (params?.limit) qs.set('limit', String(params.limit));
  if (params?.before) qs.set('before', String(params.before));
  const query = qs.toString();
  return fetchApi(`${V3}/groups/${groupId}/messages${query ? `?${query}` : ''}`);
}

export async function sendMessage(groupId: string, data: {
  sender_id: string;
  sender_type?: string;
  message_type?: string;
  content: string;
  reply_to_id?: string;
  thread_root_id?: string;
  source_channel?: string;
}): Promise<{ message: GroupMessage }> {
  return fetchApi(`${V3}/groups/${groupId}/messages`, { method: 'POST', body: data });
}

export async function editMessage(groupId: string, messageId: string, data: {
  editor_id: string;
  content: string;
}): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}`, { method: 'PUT', body: data });
}

export async function deleteMessage(groupId: string, messageId: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}`, { method: 'DELETE' });
}

export async function addReaction(groupId: string, messageId: string, data: {
  user_id: string;
  emoji: string;
}): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}/reactions`, { method: 'POST', body: data });
}

export async function removeReaction(groupId: string, messageId: string, emoji: string, userId: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}/reactions/${emoji}?user_id=${userId}`, { method: 'DELETE' });
}

export async function pinMessage(groupId: string, messageId: string, pinnedBy: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}/pin`, { method: 'POST', body: { pinned_by: pinnedBy } });
}

export async function unpinMessage(groupId: string, messageId: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}/pin`, { method: 'DELETE' });
}

export async function getThread(groupId: string, messageId: string): Promise<{ messages: GroupMessage[] }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}/thread`);
}

export async function markAsRead(groupId: string, messageId: string, userId: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/messages/${messageId}/read`, { method: 'POST', body: { user_id: userId } });
}

export async function searchMessages(groupId: string, query: string, limit?: number): Promise<{ messages: GroupMessage[] }> {
  const qs = new URLSearchParams({ q: query });
  if (limit) qs.set('limit', String(limit));
  return fetchApi(`${V3}/groups/${groupId}/messages/search?${qs.toString()}`);
}

// ============================================================
// Tasks
// ============================================================

export async function createTask(data: {
  title: string;
  description?: string;
  creator_id: string;
  project_id?: string;
  group_id?: string;
  priority?: string;
  assignee_id?: string;
  source_message_id?: string;
}): Promise<{ task: TaskV3 }> {
  return fetchApi(`${V3}/tasks`, { method: 'POST', body: data });
}

export async function listTasks(params?: {
  group_id?: string;
  status?: string;
  limit?: number;
}): Promise<{ tasks: TaskV3[] }> {
  const qs = new URLSearchParams();
  if (params?.group_id) qs.set('group_id', params.group_id);
  if (params?.status) qs.set('status', params.status);
  if (params?.limit) qs.set('limit', String(params.limit));
  const query = qs.toString();
  return fetchApi(`${V3}/tasks${query ? `?${query}` : ''}`);
}

export async function getTask(taskId: string): Promise<{ task: TaskV3 }> {
  return fetchApi(`${V3}/tasks/${taskId}`);
}

export async function transitionTask(taskId: string, data: {
  status: string;
  changed_by: string;
  reason?: string;
}): Promise<{ task: TaskV3 }> {
  return fetchApi(`${V3}/tasks/${taskId}/transition`, { method: 'POST', body: data });
}

export async function addTaskComment(taskId: string, data: {
  user_id: string;
  content: string;
  source_message_id?: string;
}): Promise<{ id: string }> {
  return fetchApi(`${V3}/tasks/${taskId}/comments`, { method: 'POST', body: data });
}

export async function getTaskHistory(taskId: string): Promise<{ history: Array<{ id: string; old_status: string; new_status: string; changed_by: string; changed_at: number; reason?: string }> }> {
  return fetchApi(`${V3}/tasks/${taskId}/history`);
}

// ============================================================
// Approvals
// ============================================================

export async function createApproval(data: {
  group_id: string;
  title: string;
  description?: string;
  request_type?: string;
  requester_id: string;
  urgency?: string;
  quorum_type?: string;
  approver_spec: string;
  context?: string;
}): Promise<{ approval: ApprovalRequest }> {
  return fetchApi(`${V3}/approvals`, { method: 'POST', body: data });
}

export async function getApproval(approvalId: string): Promise<{ approval: ApprovalRequest }> {
  return fetchApi(`${V3}/approvals/${approvalId}`);
}

export async function voteApproval(approvalId: string, data: {
  voter_id: string;
  decision: string;
  comment?: string;
}): Promise<{ outcome: ApprovalOutcome }> {
  return fetchApi(`${V3}/approvals/${approvalId}/vote`, { method: 'POST', body: data });
}

export async function listVotes(approvalId: string): Promise<{ votes: ApprovalVote[] }> {
  return fetchApi(`${V3}/approvals/${approvalId}/votes`);
}

export async function listPendingApprovals(userId: string, groupId?: string): Promise<{ approvals: ApprovalRequest[] }> {
  const qs = new URLSearchParams({ user_id: userId });
  if (groupId) qs.set('group_id', groupId);
  return fetchApi(`${V3}/approvals/pending?${qs.toString()}`);
}

// ============================================================
// Memory
// ============================================================

export async function storeMemory(data: {
  agent_id: string;
  memory_type?: string;
  content: string;
  summary?: string;
  source_type?: string;
  source_id?: string;
  group_id?: string;
  ttl_hours?: number;
}): Promise<{ memory: AgentMemory }> {
  return fetchApi(`${V3}/memories`, { method: 'POST', body: data });
}

export async function searchMemory(data: {
  agent_id: string;
  query_text?: string;
  memory_type?: string;
  group_id?: string;
  limit?: number;
}): Promise<{ results: Array<{ memory: AgentMemory; score: number }> }> {
  return fetchApi(`${V3}/memories/search`, { method: 'POST', body: data });
}

export async function getMemoryStats(agentId: string): Promise<{ stats: MemoryStats }> {
  return fetchApi(`${V3}/memories/stats?agent_id=${agentId}`);
}

// ============================================================
// DM (Private Chat)
// ============================================================

export async function createDm(data: {
  target_user_id: string;
}): Promise<{ group: Group }> {
  return fetchApi(`${V3}/dms`, { method: 'POST', body: data });
}

export async function listDms(): Promise<{ dms: DmListItem[] }> {
  return fetchApi(`${V3}/dms`);
}

// ============================================================
// DM Tool Grants
// ============================================================

export async function grantTool(dmGroupId: string, data: {
  tool_name: string;
  expires_at?: number;
  scope?: string;
}): Promise<{ id: string }> {
  return fetchApi(`${V3}/dms/${dmGroupId}/grants`, { method: 'POST', body: data });
}

export async function revokeTool(dmGroupId: string, toolName: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/dms/${dmGroupId}/grants/${toolName}`, { method: 'DELETE' });
}

export async function listGrants(dmGroupId: string): Promise<{ grants: DmToolGrant[] }> {
  return fetchApi(`${V3}/dms/${dmGroupId}/grants`);
}

// ============================================================
// A2A Delegations
// ============================================================

export async function createDelegation(dmGroupId: string, data: {
  executor_id: string;
  prompt: string;
  task_id?: string;
}): Promise<{ delegation: A2ADelegation }> {
  return fetchApi(`${V3}/dms/${dmGroupId}/delegations`, { method: 'POST', body: data });
}

export async function listDelegations(dmGroupId: string): Promise<{ delegations: A2ADelegation[] }> {
  return fetchApi(`${V3}/dms/${dmGroupId}/delegations`);
}

export async function interveneDelegation(dmGroupId: string, delegationId: string, data: {
  action: 'revoke' | 'reroute';
  reason?: string;
  new_executor_id?: string;
}): Promise<{ status: string }> {
  return fetchApi(`${V3}/dms/${dmGroupId}/delegations/${delegationId}/intervene`, { method: 'POST', body: data });
}

// ============================================================
// Group Rules Engine
// ============================================================

export async function listRules(groupId: string): Promise<{ rules: GroupRule[] }> {
  return fetchApi(`${V3}/groups/${groupId}/rules`);
}

export async function createRule(groupId: string, data: {
  rule_type: string;
  config: Record<string, unknown>;
  priority?: number;
}): Promise<{ rule: GroupRule }> {
  return fetchApi(`${V3}/groups/${groupId}/rules`, { method: 'POST', body: data });
}

export async function updateRule(groupId: string, ruleId: string, data: {
  config?: Record<string, unknown>;
  priority?: number;
  enabled?: boolean;
}): Promise<{ rule: GroupRule }> {
  return fetchApi(`${V3}/groups/${groupId}/rules/${ruleId}`, { method: 'PUT', body: data });
}

export async function deleteRule(groupId: string, ruleId: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/rules/${ruleId}`, { method: 'DELETE' });
}

// ============================================================
// Message Attachments
// ============================================================

export async function uploadMessageAttachment(groupId: string, file: File): Promise<{ attachments: MessageAttachment[] }> {
  const formData = new FormData();
  formData.append('file', file);
  const { token } = getAuthState();
  const res = await fetch(`${API_BASE_URL}${V3}/groups/${groupId}/attachments`, {
    method: 'POST',
    body: formData,
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  return res.json();
}

export async function listMessageAttachments(groupId: string): Promise<{ attachments: MessageAttachment[] }> {
  return fetchApi(`${V3}/groups/${groupId}/attachments`);
}

export function getAttachmentDownloadUrl(attachmentId: string): string {
  return `${API_BASE_URL}${V3}/attachments/${attachmentId}`;
}

export async function linkAttachment(attachmentId: string, messageId: string): Promise<{ linked: boolean }> {
  return fetchApi(`${V3}/attachments/${attachmentId}`, { method: 'PUT', body: { message_id: messageId } });
}

export async function deleteAttachment(attachmentId: string): Promise<{ deleted: boolean }> {
  return fetchApi(`${V3}/attachments/${attachmentId}`, { method: 'DELETE' });
}

// ============================================================
// Group Cron Jobs
// ============================================================

export async function listCronJobs(groupId: string): Promise<{ jobs: GroupCronJob[] }> {
  return fetchApi(`${V3}/groups/${groupId}/cron`);
}

export async function createCronJob(groupId: string, data: {
  name: string;
  cron_expr: string;
  message_template: string;
  job_type?: string;
  target_agent_id?: string;
  enabled?: boolean;
}): Promise<{ job: GroupCronJob }> {
  return fetchApi(`${V3}/groups/${groupId}/cron`, { method: 'POST', body: data });
}

export async function updateCronJob(groupId: string, cronId: string, data: {
  name?: string;
  cron_expr?: string;
  message_template?: string;
  enabled?: boolean;
}): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/cron/${cronId}`, { method: 'PUT', body: data });
}

export async function deleteCronJob(groupId: string, cronId: string): Promise<{ status: string }> {
  return fetchApi(`${V3}/groups/${groupId}/cron/${cronId}`, { method: 'DELETE' });
}

// ============================================================
// Agent Hooks
// ============================================================

export async function listHooks(groupId: string): Promise<{ hooks: AgentHook[] }> {
  return fetchApi(`${V3}/groups/${groupId}/hooks`);
}

export async function createHook(groupId: string, data: {
  agent_id: string;
  event_types: string[];
  condition_expr?: string;
  action_type: string;
  action_config: Record<string, unknown>;
  priority?: number;
}): Promise<{ hook: AgentHook }> {
  return fetchApi(`${V3}/groups/${groupId}/hooks`, { method: 'POST', body: data });
}

export async function getHook(groupId: string, hookId: string): Promise<{ hook: AgentHook }> {
  return fetchApi(`${V3}/groups/${groupId}/hooks/${hookId}`);
}

export async function updateHook(groupId: string, hookId: string, data: Record<string, unknown>): Promise<{ updated: boolean }> {
  return fetchApi(`${V3}/groups/${groupId}/hooks/${hookId}`, { method: 'PUT', body: data });
}

export async function deleteHook(groupId: string, hookId: string): Promise<{ deleted: boolean }> {
  return fetchApi(`${V3}/groups/${groupId}/hooks/${hookId}`, { method: 'DELETE' });
}

export async function listHookLogs(groupId: string, hookId: string, limit?: number): Promise<{ logs: HookLog[] }> {
  const q = limit ? `?limit=${limit}` : '';
  return fetchApi(`${V3}/groups/${groupId}/hooks/${hookId}/logs${q}`);
}

// ============================================================
// Workflow Definitions
// ============================================================

export async function listWorkflows(): Promise<{ workflows: WorkflowDef[] }> {
  return fetchApi(`${V3}/workflows`);
}

export async function createWorkflow(data: {
  id: string;
  name: string;
  yaml_content: string;
}): Promise<{ workflow: WorkflowDef }> {
  return fetchApi(`${V3}/workflows`, { method: 'POST', body: data });
}

export async function getWorkflow(wid: string): Promise<{ workflow: WorkflowDef }> {
  return fetchApi(`${V3}/workflows/${wid}`);
}

export async function updateWorkflow(wid: string, data: {
  name?: string;
  yaml_content?: string;
  status?: string;
}): Promise<{ updated: boolean }> {
  return fetchApi(`${V3}/workflows/${wid}`, { method: 'PUT', body: data });
}

export async function deleteWorkflow(wid: string): Promise<{ deleted: boolean }> {
  return fetchApi(`${V3}/workflows/${wid}`, { method: 'DELETE' });
}

// ============================================================
// Workflow Runs
// ============================================================

export async function listWorkflowRuns(params?: {
  workflow_id?: string;
  group_id?: string;
  status?: string;
  limit?: number;
}): Promise<{ runs: WorkflowRun[] }> {
  const qs = new URLSearchParams();
  if (params?.workflow_id) qs.set('workflow_id', params.workflow_id);
  if (params?.group_id) qs.set('group_id', params.group_id);
  if (params?.status) qs.set('status', params.status);
  if (params?.limit) qs.set('limit', String(params.limit));
  const query = qs.toString();
  return fetchApi(`${V3}/workflow-runs${query ? `?${query}` : ''}`);
}

export async function createWorkflowRun(data: {
  workflow_id: string;
  workflow_version: number;
  input: string;
  group_id?: string;
  agent_id?: string;
}): Promise<{ run: WorkflowRun }> {
  return fetchApi(`${V3}/workflow-runs`, { method: 'POST', body: data });
}

export async function getWorkflowRun(rid: string): Promise<{ run: WorkflowRun }> {
  return fetchApi(`${V3}/workflow-runs/${rid}`);
}

export async function updateWorkflowRunStatus(rid: string, data: {
  status: string;
  output?: string;
  error?: string;
}): Promise<{ updated: boolean }> {
  return fetchApi(`${V3}/workflow-runs/${rid}/status`, { method: 'PUT', body: data });
}

export async function listCheckpoints(rid: string): Promise<{ checkpoints: RunCheckpoint[] }> {
  return fetchApi(`${V3}/workflow-runs/${rid}/checkpoints`);
}

export async function recordCheckpoint(rid: string, data: {
  node_id: string;
  output: string;
  context_snapshot: string;
}): Promise<{ id: number }> {
  return fetchApi(`${V3}/workflow-runs/${rid}/checkpoints`, { method: 'POST', body: data });
}

// ============================================================
// Execution fact chain (Track 1 / T1-2)
// See docs/execution-fact-chain-spec.md
// ============================================================

export interface ExecutionEvent {
  id: string;
  execution_id: string;
  parent_execution_id: string | null;
  source: 'chat' | 'workflow' | 'task' | 'approval' | 'agent' | 'tool' | 'scheduler' | 'system';
  event_type:
    | 'started' | 'delta' | 'tool_call' | 'tool_result'
    | 'node_started' | 'node_finished' | 'artifact' | 'usage'
    | 'approval_requested' | 'approval_decided'
    | 'retry' | 'cancelled' | 'resumed' | 'paused'
    | 'done' | 'error';
  payload: Record<string, unknown>;
  actor: string | null;
  actor_type: 'human' | 'agent' | 'system' | null;
  created_at: number;
}

export interface Execution {
  id: string;
  parent_execution_id: string | null;
  source: string;
  status: 'pending' | 'running' | 'paused' | 'success' | 'failed' | 'cancelled';
  actor: string | null;
  actor_type: 'human' | 'agent' | 'system' | null;
  trigger_type: string | null;
  trigger_payload: Record<string, unknown> | null;
  started_at: number;
  completed_at: number | null;
  error: string | null;
  event_count: number;
  updated_at: number;
}

export async function getExecution(executionId: string): Promise<Execution> {
  return fetchApi(`${V3}/executions/${executionId}`);
}

export async function listExecutionEvents(executionId: string): Promise<{
  execution_id: string;
  events: ExecutionEvent[];
}> {
  return fetchApi(`${V3}/executions/${executionId}/events`);
}

/**
 * Subscribe to an execution's event stream via Server-Sent Events.
 *
 * Returns an unsubscribe function. The callback is invoked once per event
 * plus once with a 'stream_end' marker when the execution reaches a
 * terminal state.
 *
 * Falls back to polling listExecutionEvents every 1s if EventSource is
 * unavailable (e.g. during SSR or in tests).
 */
export function subscribeExecutionEvents(
  executionId: string,
  onEvent: (event: ExecutionEvent) => void,
  onEnd?: (finalStatus: string) => void,
  onError?: (err: Error) => void
): () => void {
  const { token } = getAuthState();
  const url = `${API_BASE_URL}${V3}/executions/${executionId}/events/stream`;

  // EventSource doesn't support custom headers — pass token as query param.
  const streamUrl = token ? `${url}?token=${encodeURIComponent(token)}` : url;

  if (typeof window === 'undefined' || typeof EventSource === 'undefined') {
    // SSR / test fallback: poll listExecutionEvents every 1s
    let stopped = false;
    let lastSeen = 0;
    const poll = async () => {
      if (stopped) return;
      try {
        const { events } = await listExecutionEvents(executionId);
        for (let i = lastSeen; i < events.length; i++) {
          onEvent(events[i]);
        }
        lastSeen = events.length;
        // Heuristic: if last event is terminal, stop
        const last = events[events.length - 1];
        if (last && (last.event_type === 'done' || last.event_type === 'error' || last.event_type === 'cancelled')) {
          onEnd?.(last.event_type === 'done' ? 'success' : last.event_type);
          return;
        }
      } catch (e) {
        onError?.(e as Error);
        return;
      }
      setTimeout(poll, 1000);
    };
    poll();
    return () => { stopped = true; };
  }

  const es = new EventSource(streamUrl);

  // EventSource allows listening to named events. The server emits events
  // with `event: <event_type>` so we register handlers for each known type.
  const eventTypes = [
    'started', 'delta', 'tool_call', 'tool_result',
    'node_started', 'node_finished', 'artifact', 'usage',
    'approval_requested', 'approval_decided',
    'retry', 'cancelled', 'resumed', 'paused',
    'done', 'error', 'stream_end',
  ];

  const handler = (ev: MessageEvent) => {
    try {
      const payload = JSON.parse(ev.data);
      if (ev.type === 'stream_end') {
        onEnd?.(payload.final_status ?? 'success');
        es.close();
        return;
      }
      onEvent(payload as ExecutionEvent);
    } catch (e) {
      onError?.(e as Error);
    }
  };

  for (const t of eventTypes) {
    es.addEventListener(t, handler as EventListener);
  }
  es.onerror = () => {
    // EventSource auto-reconnects; only surface fatal errors after retries
    // fail. For now, log and let the browser handle reconnection.
    // If the stream is closed (readyState === CLOSED), surface the error.
    if (es.readyState === EventSource.CLOSED) {
      onError?.(new Error('SSE stream closed'));
    }
  };

  return () => {
    es.close();
  };
}
