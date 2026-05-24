"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

interface ModelInfo { id: string; name: string; provider: string }

export function SettingsPage() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");

  useEffect(() => {
    const load = async () => {
      try { const r = await rpcCall<{ models: ModelInfo[] }>("llm.models"); setModels(r.models ?? []); } catch { setModels([]); }
      setLoading(false);
    };
    load();
  }, []);

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center px-4">
        <h2 className="text-[15px] font-semibold">设置</h2>
      </div>
      <div className="flex-1 overflow-y-auto p-4 space-y-4 max-w-xl">

        {/* 模型配置 */}
        <Card className="shadow-card">
          <CardContent className="p-4 space-y-3">
            <div className="text-[15px] font-medium">模型配置</div>
            <div className="space-y-2">
              <label className="text-[11px] text-muted-foreground">Ollama 地址</label>
              <Input value={ollamaUrl} onChange={(e) => setOllamaUrl(e.target.value)} placeholder="http://localhost:11434" className="h-7 text-xs" />
            </div>
            <div className="text-[11px] text-muted-foreground">已注册模型:</div>
            <div className="flex flex-wrap gap-1.5">
              {models.map((m) => <Badge key={m.id} variant="secondary" className="text-[11px]">{m.name ?? m.id} ({m.provider})</Badge>)}
              {models.length === 0 && <span className="text-xs text-muted-foreground">暂无模型</span>}
            </div>
          </CardContent>
        </Card>

        {/* 同步配置 */}
        <Card className="shadow-card">
          <CardContent className="p-4 space-y-3">
            <div className="text-[15px] font-medium">同步配置</div>
            <div className="space-y-2">
              <label className="text-[11px] text-muted-foreground">WebDAV 地址</label>
              <Input defaultValue="" placeholder="https://your-webdav-server/dav" className="h-7 text-xs" />
            </div>
            <div className="text-[11px] text-muted-foreground">同步策略: Local-first (CRDT)</div>
            <Badge variant="outline" className="text-[11px]">Automerge</Badge>
          </CardContent>
        </Card>

        {/* 安全配置 */}
        <Card className="shadow-card">
          <CardContent className="p-4 space-y-3">
            <div className="text-[15px] font-medium">安全</div>
            <div className="text-[11px] text-muted-foreground">
              <p>SCALE Gateway: 工具调用前拦截、敏感操作检测</p>
              <p>本地优先: 数据默认存储在本地 SQLite</p>
              <p>向量数据库: Qdrant 可选，无 QDRANT_URL 时使用内存模式</p>
            </div>
          </CardContent>
        </Card>

        {/* 团队配置 */}
        <Card className="shadow-card">
          <CardContent className="p-4 space-y-3">
            <div className="text-[15px] font-medium">团队</div>
            <div className="text-[11px] text-muted-foreground">多用户协作（Phase 3 开发中）</div>
            <Badge variant="outline" className="text-[11px]">企业版功能</Badge>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}