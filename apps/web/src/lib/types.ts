export interface KnowledgeRef {
  id: string;
  title: string;
  source_type: string;
  score: number;
  snippet: string;
  /** T3-10: true when this ref comes from an approved learning candidate */
  is_learning?: boolean;
  /** T3-10: the learning candidate id that produced this memory entry */
  candidate_id?: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  timestamp: number;
  toolCalls?: ToolCall[];
  knowledgeRefs?: KnowledgeRef[];
  /**
   * Execution fact chain id (Track 1 / T1-3). When present, the chat panel
   * shows a "View trace" toggle that renders <ExecutionTimeline /> for the
   * full unified execution including tool_calls, tool_results, approval
   * events, etc.
   */
  executionId?: string;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  result?: unknown;
  status: 'pending' | 'running' | 'completed' | 'failed';
}

export interface ChatSession {
  id: string;
  title: string;
  messages: ChatMessage[];
  createdAt: number;
  updatedAt: number;
}