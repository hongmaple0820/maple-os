export class MapleAgentClient {
  private ws: WebSocket | null = null;
  private serverUrl: string;
  private token: string;
  private agentId: string;
  private onTaskHandler?: (task: any, ctx: TaskContext) => Promise<void>;

  constructor(config: { serverUrl: string; token: string; agentId: string }) {
    this.serverUrl = config.serverUrl;
    this.token = config.token;
    this.agentId = config.agentId;
  }

  register(config: {
    capabilities: {
      tools: string[];
      maxContextLength: number;
      supportsStreaming: boolean;
      supportsImage: boolean;
    };
    systemPrompt: string;
    maxConcurrentTasks: number;
  }): void {
    this.send({
      type: 'register',
      agent_id: this.agentId,
      capabilities: config.capabilities,
      system_prompt: config.systemPrompt,
      max_concurrent_tasks: config.maxConcurrentTasks,
    });
  }

  onTask(handler: (task: any, ctx: TaskContext) => Promise<void>): void {
    this.onTaskHandler = handler;
  }

  connect(): void {
    this.ws = new WebSocket(this.serverUrl);
    this.ws.addEventListener('open', () => {
      this.send({ type: 'ping' });
    });
    this.ws.addEventListener('message', (event) => {
      const data = JSON.parse(event.data);
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
          complete: async (result: any) => {
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
      setTimeout(() => this.connect(), 5000);
    });
  }

  disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  private send(data: any): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }
}

export interface TaskContext {
  progress: (percent: number, message: string) => Promise<void>;
  complete: (result: any) => Promise<void>;
  fail: (error: string) => Promise<void>;
}
