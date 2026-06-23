import type {
  SystemInfo,
  SystemHealth,
  JsonRpcRequest,
  JsonRpcResponse,
  RequestOptions,
} from './types';

export class RpcError extends Error {
  code: number;
  data?: unknown;

  constructor(code: number, message: string, data?: unknown) {
    super(message);
    this.name = 'RpcError';
    this.code = code;
    this.data = data;
  }
}

export class RpcClient {
  private baseUrl: string;
  private requestId = 0;
  private defaultHeaders: Record<string, string>;

  constructor(baseUrl: string, defaultHeaders?: Record<string, string>) {
    this.baseUrl = baseUrl;
    this.defaultHeaders = defaultHeaders ?? {};
  }

  async request<T>(
    method: string,
    params?: Record<string, unknown>,
    options?: RequestOptions
  ): Promise<T> {
    const id = ++this.requestId;
    const retries = options?.retries ?? 2;
    const retryDelayMs = options?.retryDelayMs ?? 1000;

    const body: JsonRpcRequest = {
      jsonrpc: '2.0',
      id,
      method,
      params: params ?? null,
    };

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...this.defaultHeaders,
      ...options?.headers,
    };

    for (let attempt = 0; attempt <= retries; attempt++) {
      const response = await fetch(`${this.baseUrl}/rpc`, {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      });

      if (!response.ok && attempt < retries) {
        await this.delay(retryDelayMs * (attempt + 1));
        continue;
      }

      const data: JsonRpcResponse<T> = await response.json();

      if (data.error) {
        throw new RpcError(data.error.code, data.error.message, data.error.data);
      }

      return data.result as T;
    }

    throw new RpcError(-1, `Request failed after ${retries + 1} attempts`);
  }

  async systemInfo(): Promise<SystemInfo> {
    return this.request<SystemInfo>('system.info');
  }

  async systemHealth(): Promise<SystemHealth> {
    return this.request<SystemHealth>('system.health');
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}