"use client";

import { useState, useEffect } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Spinner } from "@mapleos/ui";
import { rpcCall, scaleApi } from "@/lib/api";

interface ScaleTool { name: string; description: string; inputSchema: Record<string, unknown> }
interface ScaleCallResult { ok: boolean; result?: unknown; error?: string }

export function ScaleEngineManager() {
  const [tools, setTools] = useState<ScaleTool[]>([]);
  const [loading, setLoading] = useState(true);
  const [callResult, setCallResult] = useState<string | null>(null);
  const [callLoading, setCallLoading] = useState(false);
  const [selectedTool, setSelectedTool] = useState<string | null>(null);
  const [argsInput, setArgsInput] = useState("{}");
  const [bridgeOnline, setBridgeOnline] = useState(false);

  const loadTools = async () => {
    try {
      const res = await scaleApi<{ tools: ScaleTool[] }>("/tools");
      setTools(res.tools ?? []);
      setBridgeOnline(true);
    } catch {
      try {
        const result = await rpcCall<{ raw: string }>("scale.tools");
        const parsed = JSON.parse(result.raw);
        setTools(parsed.tools ?? []);
        setBridgeOnline(true);
      } catch {
        setBridgeOnline(false);
        setTools([]);
      }
    }
    setLoading(false);
  };

  useEffect(() => { loadTools(); }, []);

  const handleCall = async () => {
    if (!selectedTool) return;
    setCallLoading(true);
    setCallResult(null);
    try {
      const args = JSON.parse(argsInput);
      const res = await scaleApi<ScaleCallResult>("/call", { method: "POST", body: { name: selectedTool, arguments: args } });
      setCallResult(JSON.stringify(res, null, 2));
    } catch {
      try {
        const args = JSON.parse(argsInput);
        const rpcRes = await rpcCall<{ raw: string }>("scale.call", { tool_name: selectedTool, arguments: args });
        setCallResult(rpcRes.raw);
      } catch (rpcErr) {
        setCallResult(`错误: ${(rpcErr as Error).message}`);
      }
    } finally {
      setCallLoading(false);
    }
  };

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">SCALE 引擎</h2>
        <div className="flex gap-2 items-center">
          <Badge variant={bridgeOnline ? "default" : "destructive"}>{bridgeOnline ? "桥接服务在线" : "桥接服务离线"}</Badge>
          <Button onClick={loadTools}>刷新工具</Button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {tools.length === 0 && !bridgeOnline && (
          <Card>
            <CardContent className="text-sm text-muted-foreground p-4">
              <p>SCALE 引擎桥接服务尚未启动。</p>
              <p className="mt-2">启动方式：</p>
              <code className="block bg-muted p-2 rounded text-xs mt-1">node /workspace/core/scale-engine/bridge-http.mjs</code>
              <p className="mt-2">桥接服务监听端口 7790，提供以下接口：</p>
              <ul className="text-xs list-disc pl-4 mt-1 space-y-1">
                <li>GET /health — 健康检查</li>
                <li>GET /tools — 列出可用 MCP 工具</li>
                <li>POST /call — 调用指定工具</li>
                <li>POST /mcp — MCP JSON-RPC 协议</li>
              </ul>
            </CardContent>
          </Card>
        )}

        {tools.map((tool) => (
          <Card
            key={tool.name}
            className={`hover:shadow-md transition-shadow cursor-pointer ${selectedTool === tool.name ? "ring-2 ring-primary" : ""}`}
            onClick={() => { setSelectedTool(tool.name); setArgsInput("{}"); }}
          >
            <CardHeader className="pb-2"><CardTitle className="text-base">{tool.name}</CardTitle></CardHeader>
            <CardContent><p className="text-sm text-muted-foreground">{tool.description}</p></CardContent>
          </Card>
        ))}

        {selectedTool && (
          <Card className="border-primary">
            <CardHeader className="pb-2"><CardTitle className="text-base">调用: {selectedTool}</CardTitle></CardHeader>
            <CardContent className="space-y-2">
              <label className="text-sm text-muted-foreground">参数 (JSON)</label>
              <textarea
                value={argsInput}
                onChange={(e) => setArgsInput(e.target.value)}
                className="w-full h-24 rounded-md border border-input bg-transparent px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                placeholder="输入 JSON 格式参数..."
              />
              <Button onClick={handleCall} disabled={callLoading}>{callLoading ? "执行中..." : "执行"}</Button>
              {callResult && <pre className="bg-muted p-3 rounded text-xs overflow-x-auto max-h-48 overflow-y-auto whitespace-pre-wrap">{callResult}</pre>}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}