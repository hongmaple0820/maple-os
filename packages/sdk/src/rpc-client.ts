export class RpcClient {
  private baseUrl: string;
  private requestId = 0;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  async request(method: string, params?: any): Promise<any> {
    const id = ++this.requestId;
    const response = await fetch(`${this.baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id,
        method,
        params: params ?? null,
      }),
    });

    const data = await response.json();

    if (data.error) {
      throw new RpcError(data.error.code, data.error.message, data.error.data);
    }

    return data.result;
  }

  async systemInfo(): Promise<any> {
    return this.request('system.info');
  }

  async systemHealth(): Promise<any> {
    return this.request('system.health');
  }
}

export class RpcError extends Error {
  code: number;
  data?: any;

  constructor(code: number, message: string, data?: any) {
    super(message);
    this.name = 'RpcError';
    this.code = code;
    this.data = data;
  }
}
