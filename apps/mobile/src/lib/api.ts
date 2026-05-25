import { RpcClient } from "@mapleos/sdk";

const BASE_URL = process.env.EXPO_PUBLIC_MAPLE_API_URL || "http://localhost:7788";

let _client: RpcClient | null = null;

function getClient(): RpcClient {
  if (!_client) {
    _client = new RpcClient(BASE_URL);
  }
  return _client;
}

export async function mobileRpcCall<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  return getClient().request<T>(method, params);
}

export async function mobileRestCall(path: string, body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${BASE_URL}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return res.json();
}

export { BASE_URL };