export interface WorkflowItem {
  id: string;
  name: string;
  version: number;
  status: string;
  created_at: number;
  updated_at: number;
}

export interface WorkflowExecution {
  id: string;
  workflow_id: string;
  workflow_version: number;
  status: 'running' | 'completed' | 'failed';
  started_at: number;
  completed_at?: number;
  error?: string;
}

export interface KnowledgeBase {
  id: string;
  name: string;
  description: string;
  doc_count: number;
  status: string;
  created_at: number;
}

export interface AgentInfo {
  id: string;
  name: string;
  type: string;
  status: 'idle' | 'busy' | 'offline';
  capabilities: string[];
  current_task?: string;
  last_active: number;
}