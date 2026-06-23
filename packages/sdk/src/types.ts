export interface SystemInfo {
  version: string;
  uptime_secs: number;
  agents_count: number;
  workflows_count: number;
  tasks_count: number;
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  services: Record<string, ServiceHealth>;
  timestamp: string;
}

export interface ServiceHealth {
  status: 'up' | 'down';
  latency_ms?: number;
  message?: string;
}

export interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: number;
  method: string;
  params: Record<string, unknown> | null;
}

export interface JsonRpcResponse<T> {
  jsonrpc: '2.0';
  id: number;
  result?: T;
  error?: JsonRpcError;
}

export interface JsonRpcError {
  code: number;
  message: string;
  data?: unknown;
}

export interface RequestOptions {
  retries?: number;
  retryDelayMs?: number;
  headers?: Record<string, string>;
}