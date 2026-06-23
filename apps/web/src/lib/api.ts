export const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "";
export const MAPLE_API_PREFIX = "/api/maple";
export const SCALE_API_PREFIX = "/api/scale";

const TOKEN_KEY = "mapleos_token";
const REFRESH_TOKEN_KEY = "mapleos_refresh_token";
const USER_KEY = "mapleos_user";

export function getAuthState() {
  if (typeof window === "undefined") return { token: null, user: null };
  const token = localStorage.getItem(TOKEN_KEY);
  const user = localStorage.getItem(USER_KEY);
  return { token, user: user ? JSON.parse(user) : null };
}

export function setAuthState(token: string, refreshToken: string, user: { user_id: string; username: string; role: string }) {
  localStorage.setItem(TOKEN_KEY, token);
  localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
  localStorage.setItem(USER_KEY, JSON.stringify(user));
}

export function clearAuthState() {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
}

export function isAuthenticated(): boolean {
  return !!getAuthState().token;
}

export async function fetchApi<T>(
  path: string,
  options?: {
    method?: string;
    body?: unknown;
    headers?: Record<string, string>;
  }
): Promise<T> {
  const url = `${API_BASE_URL}${path}`;
  const { token } = getAuthState();
  const res = await fetch(url, {
    method: options?.method ?? "GET",
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...options?.headers,
    },
    body: options?.body ? JSON.stringify(options.body) : undefined,
  });

  if (res.status === 401) {
    const refreshed = await tryRefreshToken();
    if (refreshed) {
      return fetchApi(path, options);
    }
    clearAuthState();
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("auth:logout"));
    }
    throw new Error("Unauthorized");
  }

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
  const { token } = getAuthState();
  const res = await fetch(`${API_BASE_URL}/rpc`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? null }),
  });

  if (res.status === 401) {
    const refreshed = await tryRefreshToken();
    if (refreshed) {
      return rpcCall(method, params);
    }
    clearAuthState();
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("auth:logout"));
    }
    throw new Error("Unauthorized");
  }

  if (!res.ok) {
    throw new Error(`RPC error: ${res.status} ${res.statusText}`);
  }

  const data = await res.json() as { result?: T; error?: { code: number; message: string } };

  if (data.error) {
    throw new Error(`RPC ${method} error [${data.error.code}]: ${data.error.message}`);
  }

  return data.result as T;
}

async function tryRefreshToken(): Promise<boolean> {
  const refreshToken = typeof window !== "undefined" ? localStorage.getItem(REFRESH_TOKEN_KEY) : null;
  if (!refreshToken) return false;

  try {
    const res = await fetch(`${API_BASE_URL}${MAPLE_API_PREFIX}/api/auth/refresh`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    if (!res.ok) return false;
    const data = await res.json();
    if (data.token) {
      setAuthState(data.token, data.refresh_token, { user_id: data.user_id, username: data.username, role: data.role });
      return true;
    }
    return false;
  } catch {
    return false;
  }
}