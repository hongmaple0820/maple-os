import { RpcClient } from "@mapleos/sdk";
import AsyncStorage from "@react-native-async-storage/async-storage";

const TOKEN_KEY = "mapleos_token";
const REFRESH_TOKEN_KEY = "mapleos_refresh_token";
const USER_KEY = "mapleos_user";

export async function getMobileAuthState() {
  const token = await AsyncStorage.getItem(TOKEN_KEY);
  const userStr = await AsyncStorage.getItem(USER_KEY);
  return { token, user: userStr ? JSON.parse(userStr) : null };
}

export async function setMobileAuthState(token: string, refreshToken: string, user: { user_id: string; username: string; role: string }) {
  await AsyncStorage.setItem(TOKEN_KEY, token);
  await AsyncStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
  await AsyncStorage.setItem(USER_KEY, JSON.stringify(user));
}

export async function clearMobileAuthState() {
  await AsyncStorage.removeItem(TOKEN_KEY);
  await AsyncStorage.removeItem(REFRESH_TOKEN_KEY);
  await AsyncStorage.removeItem(USER_KEY);
}

export async function isMobileAuthenticated(): Promise<boolean> {
  return !!(await AsyncStorage.getItem(TOKEN_KEY));
}

const BASE_URL = process.env.EXPO_PUBLIC_MAPLE_API_URL || "http://localhost:7788";

let _client: RpcClient | null = null;

function getClient(): RpcClient {
  if (!_client) {
    _client = new RpcClient(BASE_URL);
  }
  return _client;
}

export async function mobileRpcCall<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const token = await AsyncStorage.getItem(TOKEN_KEY);
  const client = getClient();
  const res = await client.request<T>(method, params, token ? { Authorization: `Bearer ${token}` } : undefined);
  return res;
}

export async function mobileRestCall(path: string, body: Record<string, unknown>): Promise<unknown> {
  const token = await AsyncStorage.getItem(TOKEN_KEY);
  let res = await fetch(`${BASE_URL}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body),
  });
  if (res.status === 401) {
    const refreshed = await tryMobileRefreshToken();
    if (refreshed) {
      const newToken = await AsyncStorage.getItem(TOKEN_KEY);
      res = await fetch(`${BASE_URL}${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(newToken ? { Authorization: `Bearer ${newToken}` } : {}),
        },
        body: JSON.stringify(body),
      });
    } else {
      await clearMobileAuthState();
    }
  }
  return res.json();
}

export { BASE_URL };

async function tryMobileRefreshToken(): Promise<boolean> {
  const refreshToken = await AsyncStorage.getItem(REFRESH_TOKEN_KEY);
  if (!refreshToken) return false;

  const apiUrl = process.env.EXPO_PUBLIC_MAPLE_API_URL ?? "http://localhost:7788";
  try {
    const res = await fetch(`${apiUrl}/api/auth/refresh`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    if (!res.ok) return false;
    const data = await res.json();
    if (data.token) {
      await setMobileAuthState(data.token, data.refresh_token, {
        user_id: data.user_id,
        username: data.username,
        role: data.role,
      });
      return true;
    }
    return false;
  } catch {
    return false;
  }
}