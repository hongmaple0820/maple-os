export const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:7788";

export async function fetchApi<T>(
  path: string,
  options?: {
    method?: string;
    body?: unknown;
    headers?: Record<string, string>;
  }
): Promise<T> {
  const res = await fetch(`${API_BASE_URL}${path}`, {
    method: options?.method ?? "GET",
    headers: {
      "Content-Type": "application/json",
      ...options?.headers,
    },
    body: options?.body ? JSON.stringify(options.body) : undefined,
  });

  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }

  return res.json() as Promise<T>;
}

let rpcRequestId = 0;

export async function rpcCall<T>(
  method: string,
  params?: Record<string, unknown>
): Promise<T> {
  const id = ++rpcRequestId;
  const res = await fetch(`${API_BASE_URL}/rpc`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? null }),
  });

  if (!res.ok) {
    throw new Error(`RPC error: ${res.status} ${res.statusText}`);
  }

  const data = await res.json() as { result?: T; error?: { code: number; message: string } };

  if (data.error) {
    throw new Error(`RPC ${method} error [${data.error.code}]: ${data.error.message}`);
  }

  return data.result as T;
}