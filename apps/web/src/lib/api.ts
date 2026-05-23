export const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "";
export const MAPLE_API_PREFIX = "/api/maple";
export const SCALE_API_PREFIX = "/api/scale";

export async function fetchApi<T>(
  path: string,
  options?: {
    method?: string;
    body?: unknown;
    headers?: Record<string, string>;
  }
): Promise<T> {
  const url = `${API_BASE_URL}${path}`;
  const res = await fetch(url, {
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

export async function mapleApi<T>(
  path: string,
  options?: {
    method?: string;
    body?: unknown;
  }
): Promise<T> {
  return fetchApi<T>(`${MAPLE_API_PREFIX}${path}`, options);
}

export async function scaleApi<T>(
  path: string,
  options?: {
    method?: string;
    body?: unknown;
  }
): Promise<T> {
  return fetchApi<T>(`${SCALE_API_PREFIX}${path}`, options);
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