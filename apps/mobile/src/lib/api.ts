import { mapleApi, from "@mapleos/sdk";

export async function mobileRpcCall(method: string, params: Record<string, unknown> = {}) {
  const baseUrl = process.env.EXPO_PUBLIC_MAPLE_API_URL || "http://localhost:7788";
  return rpcCall(method, params, baseUrl);
}