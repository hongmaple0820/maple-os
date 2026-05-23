import type { JsonRpcError } from './types';

export interface AgentCapabilities {
  tools: string[];
  maxContextLength: number;
  supportsStreaming: boolean;
  supportsImage: boolean;
}

export interface AgentRegistration {
  capabilities: AgentCapabilities;
  systemPrompt: string;
  maxConcurrentTasks: number;
}

export interface TaskPayload {
  task_id: string;
  type: string;
  input: Record<string, unknown>;
  metadata?: Record<string, unknown>;
}

export interface TaskResult {
  task_id: string;
  result: unknown;
  status: 'completed' | 'failed';
}

export interface TaskProgress {
  task_id: string;
  progress: number;
  output: string;
}

export interface TaskContext {
  progress: (percent: number, message: string) => Promise<void>;
  complete: (result: unknown) => Promise<void>;
  fail: (error: string) => Promise<void>;
}

export interface MapleAgentClientConfig {
  serverUrl: string;
  token: string;
  agentId: string;
  reconnectIntervalMs?: number;
}

type WsMessage =
  | { type: 'ping' }
  | { type: 'register'; agent_id: string; capabilities: AgentCapabilities; system_prompt: string; max_concurrent_tasks: number }
  | { type: 'progress'; task_id: string; progress: number; output: string }
  | { type: 'task_result'; task_id: string; result: unknown; status: 'completed' | 'failed' };

export class MapleAgentClient {
  private ws: WebSocket | null = null;
  private config: MapleAgentClientConfig;
  private onTaskHandler?: (task: TaskPayload, ctx: TaskContext) => Promise<void>;

  constructor(config: MapleAgentClientConfig) {
    this.config = config;
  }

  register(registration: AgentRegistration): void {
    this.send({
      type: 'register',
      agent_id: this.config.agentId,
      capabilities: registration.capabilities,
      system_prompt: registration.systemPrompt,
      max_concurrent_tasks: registration.maxConcurrentTasks,
    });
  }

  onTask(handler: (task: TaskPayload, ctx: TaskContext) => Promise<void>): void {
    this.onTaskHandler = handler;
  }

  connect(): void {
    const url = new URL(this.config.serverUrl);
    url.searchParams.set('token', this.config.token);
    url.searchParams.set('agent_id', this.config.agentId);

    this.ws = new WebSocket(url.toString());

    this.ws.addEventListener('open', () => {
      this.send({ type: 'ping' });
    });

    this.ws.addEventListener('message', (event) => {
      const data = JSON.parse(event.data as string) as TaskPayload & { type: string };

      if (data.type === 'task' && this.onTaskHandler) {
        const ctx: TaskContext = {
          progress: async (percent: number, message: string) => {
            this.send({
              type: 'progress',
              task_id: data.task_id,
              progress: percent,
              output: message,
            });
          },
          complete: async (result: unknown) => {
            this.send({
              type: 'task_result',
              task_id: data.task_id,
              result,
              status: 'completed',
            });
          },
          fail: async (error: string) => {
            this.send({
              type: 'task_result',
              task_id: data.task_id,
              result: { error },
              status: 'failed',
            });
          },
        };
        this.onTaskHandler(data, ctx);
      }
    });

    this.ws.addEventListener('close', () => {
      setTimeout(
        () => this.connect(),
        this.config.reconnectIntervalMs ?? 5000
      );
    });
  }

  disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  private send(data: WsMessage): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }
}