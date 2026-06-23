// MapleOS v3 Types

export interface Group {
  id: string;
  name: string;
  description?: string;
  avatar_url?: string;
  group_type: 'collaboration' | 'project' | 'channel' | 'dm';
  owner_id: string;
  settings: GroupSettings;
  dm_pair_key?: string;
  dm_type?: 'human_human' | 'human_agent' | 'agent_agent';
  member_count: number;
  message_count: number;
  created_at: number;
  updated_at: number;
}

export interface GroupSettings {
  max_agents: number;
  auto_approve: boolean;
  knowledge_base_enabled: boolean;
  default_agent_id?: string;
  allow_member_invite: boolean;
  message_retention_days?: number;
}

export interface GroupMember {
  group_id: string;
  member_id: string;
  member_type: 'human' | 'agent';
  role: 'owner' | 'admin' | 'member' | 'viewer';
  nickname?: string;
  can_approve: boolean;
  joined_at: number;
  last_active_at?: number;
}

export type MessageType =
  | 'text' | 'markdown' | 'image' | 'file' | 'voice'
  | 'tool_call' | 'tool_result' | 'thinking'
  | 'approval_request' | 'approval_response'
  | 'workflow_run' | 'workflow_step' | 'workflow_complete' | 'workflow_failed'
  | 'skill_call' | 'skill_result'
  | 'task_created' | 'task_updated' | 'task_completed'
  | 'system' | 'member_join' | 'member_leave'
  | 'cron_trigger' | 'external_message';

export interface GroupMessage {
  id: string;
  group_id: string;
  sender_id: string;
  sender_type: 'human' | 'agent' | 'system';
  message_type: MessageType;
  content: string;
  reply_to_id?: string;
  thread_root_id?: string;
  thread_reply_count: number;
  source_channel: string;
  pinned: boolean;
  edited_at?: number;
  created_at: number;
  attachment_id?: string;
}

export interface MessagePage {
  messages: GroupMessage[];
  has_more: boolean;
  next_cursor?: string;
}

export type TaskStatus = 'backlog' | 'todo' | 'in_progress' | 'review' | 'done' | 'cancelled';
export type TaskPriority = 'low' | 'medium' | 'high' | 'urgent' | 'critical';

export interface TaskV3 {
  id: string;
  project_id?: string;
  group_id?: string;
  title: string;
  description?: string;
  status: TaskStatus;
  priority: TaskPriority;
  assignee_id?: string;
  creator_id: string;
  parent_task_id?: string;
  source_message_id?: string;
  due_date?: number;
  tags?: string;
  created_at: number;
  updated_at: number;
  completed_at?: number;
}

export type QuorumType = 'any' | 'all' | 'majority' | string;
export type ApprovalUrgency = 'low' | 'normal' | 'high' | 'critical';
export type VoteDecision = 'approve' | 'reject' | 'abstain';

export interface ApprovalRequest {
  id: string;
  group_id: string;
  title: string;
  description?: string;
  request_type: string;
  requester_id: string;
  urgency: ApprovalUrgency;
  quorum_type: QuorumType;
  required_count: number;
  approver_spec: string;
  execution_status: string;
  timeout_at?: number;
  created_at: number;
  resolved_at?: number;
}

export interface ApprovalVote {
  id: string;
  approval_id: string;
  voter_id: string;
  decision: VoteDecision;
  comment?: string;
  voted_at: number;
}

export interface ApprovalOutcome {
  approved: boolean;
  approve_count: number;
  reject_count: number;
  abstain_count: number;
  total_approvers: number;
  quorum_met: boolean;
}

export type MemoryLayer = 'working' | 'episodic' | 'semantic';

export interface AgentMemory {
  id: string;
  agent_id: string;
  memory_type: MemoryLayer;
  content: string;
  summary?: string;
  source_type?: string;
  group_id?: string;
  relevance_score: number;
  access_count: number;
  created_at: number;
}

export interface MemoryStats {
  agent_id: string;
  working_count: number;
  episodic_count: number;
  semantic_count: number;
  total_count: number;
}

// ============================================================
// DM (Private Chat)
// ============================================================

export interface DmListItem {
  group_id: string;
  group_name: string;
  dm_type: 'human_human' | 'human_agent' | 'agent_agent';
  other_member_id: string;
  updated_at: number;
  last_message_at?: number;
}

export interface DmToolGrant {
  id: string;
  dm_group_id: string;
  tool_name: string;
  granted_by: string;
  granted_at: number;
  expires_at?: number;
  scope?: string;
}

// ============================================================
// A2A Delegations
// ============================================================

export type DelegationStatus = 'pending' | 'running' | 'completed' | 'failed' | 'revoked';

export interface A2ADelegation {
  id: string;
  dm_group_id: string;
  delegator_id: string;
  executor_id: string;
  task_id?: string;
  prompt: string;
  status: DelegationStatus;
  result?: string;
  visible_to: string;
  created_at: number;
  completed_at?: number;
}

// ============================================================
// Group Rules Engine
// ============================================================

export type RuleType = 'AutoAssign' | 'AutoApprove' | 'RateLimit' | 'TimeWindow';

export interface GroupRule {
  id: string;
  group_id: string;
  rule_type: RuleType;
  config: Record<string, unknown>;
  priority: number;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

// ============================================================
// Message Attachments
// ============================================================

export interface MessageAttachment {
  id: string;
  filename: string;
  content_type: string;
  message_id?: string;
  uploader_id: string;
  size: number;
  created_at: number;
}

// ============================================================
// Group Cron Jobs
// ============================================================

export interface GroupCronJob {
  id: string;
  group_id: string;
  name: string;
  cron_expr: string;
  message_template: string;
  job_type: string;
  target_agent_id?: string;
  enabled: boolean;
  created_by: string;
  created_at: number;
  last_run_at?: number;
  next_run_at: number;
  run_count: number;
}

// Agent Hooks

export interface AgentHook {
  id: string;
  group_id: string;
  agent_id: string;
  event_types: string;
  condition_expr?: string;
  action_type: string;
  action_config: string;
  enabled: boolean;
  priority: number;
  created_at: number;
  updated_at: number;
}

export interface HookLog {
  id: string;
  hook_id: string;
  event_type: string;
  event_data?: string;
  status: string;
  result?: string;
  error?: string;
  executed_at: number;
}

// ============================================================
// Workflow Definitions & Runs
// ============================================================

export interface WorkflowDef {
  id: string;
  name: string;
  version: number;
  yaml_content: string;
  status: string;
  created_at: number;
  updated_at: number;
}

export interface WorkflowRun {
  id: string;
  workflow_id: string;
  workflow_version: number;
  status: 'running' | 'completed' | 'failed' | 'cancelled';
  input?: string;
  output?: string;
  error?: string;
  group_id?: string;
  agent_id?: string;
  started_at: number;
  completed_at?: number;
}

export interface RunCheckpoint {
  id: number;
  exec_id: string;
  node_id: string;
  output: string;
  context_snapshot: string;
  created_at: number;
}
