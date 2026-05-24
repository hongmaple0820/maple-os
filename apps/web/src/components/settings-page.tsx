"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

interface ModelInfo { id: string; name: string; provider: string }
interface AppConfig {
  ollama_url: string;
  openai_api_key: string;
  default_model: string;
  webdav_url: string;
  webdav_username: string;
  webdav_password: string;
  qdrant_url: string;
  gateway_mode: string;
  data_local_only: boolean;
}

const defaultConfig: AppConfig = {
  ollama_url: "http://localhost:11434",
  openai_api_key: "",
  default_model: "auto",
  webdav_url: "",
  webdav_username: "",
  webdav_password: "",
  qdrant_url: "",
  gateway_mode: "strict",
  data_local_only: true,
};

export function SettingsPage() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [activeSection, setActiveSection] = useState("models");

  useEffect(() => {
    const load = async () => {
      try { const r = await rpcCall<{ models: ModelInfo[] }>("llm.models"); setModels(r.models ?? []); } catch { setModels([]); }
      try {
        const c = await rpcCall<AppConfig>("config.get");
        setConfig({ ...defaultConfig, ...c });
      } catch { setConfig(defaultConfig); }
      setLoading(false);
    };
    load();
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setSaved(false);
    try {
      await rpcCall("config.update", { ...config });
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch (err) {
      alert(`保存失败: ${(err as Error).message}`);
    } finally { setSaving(false); }
  };

  const handleReset = async () => {
    setConfig(defaultConfig);
    setSaved(false);
  };

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  const sections = [
    { id: "models", label: "模型配置" },
    { id: "sync", label: "同步配置" },
    { id: "security", label: "安全配置" },
    { id: "teams", label: "团队配置" },
  ];

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <h2 className="text-[15px] font-semibold">设置</h2>
        <div className="flex gap-2">
          {saved && <Badge variant="default" className="text-[11px]">已保存</Badge>}
          <Button size="sm" onClick={handleSave} disabled={saving}>{saving ? "保存中..." : "保存配置"}</Button>
          <Button size="sm" variant="ghost" onClick={handleReset}>重置</Button>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        <div className="w-40 border-r bg-card p-2 space-y-0.5">
          {sections.map((s) => (
            <button
              key={s.id}
              onClick={() => setActiveSection(s.id)}
              className={`w-full text-left px-3 py-1.5 rounded-md text-[13px] transition-colors ${
                activeSection === s.id ? "bg-primary/10 text-primary font-medium" : "text-muted-foreground hover:bg-accent"
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto p-4 max-w-xl space-y-4">
          {activeSection === "models" && (
            <>
              <Card className="shadow-card">
                <CardContent className="p-4 space-y-3">
                  <div className="text-[15px] font-medium">LLM 模型</div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">Ollama 地址</label>
                    <Input value={config.ollama_url} onChange={(e) => setConfig({ ...config, ollama_url: e.target.value })} placeholder="http://localhost:11434" className="h-7 text-xs" />
                  </div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">OpenAI API Key</label>
                    <Input value={config.openai_api_key} onChange={(e) => setConfig({ ...config, openai_api_key: e.target.value })} placeholder="sk-..." type="password" className="h-7 text-xs" />
                  </div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">默认模型路由</label>
                    <select value={config.default_model} onChange={(e) => setConfig({ ...config, default_model: e.target.value })} className="h-7 rounded border bg-background text-xs px-2 w-full">
                      <option value="auto">auto (自动选择)</option>
                      {models.map((m) => <option key={m.id} value={m.id}>{m.name ?? m.id} ({m.provider})</option>)}
                    </select>
                  </div>
                  <div className="text-[11px] text-muted-foreground">已注册模型:</div>
                  <div className="flex flex-wrap gap-1.5">
                    {models.map((m) => <Badge key={m.id} variant="secondary" className="text-[11px]">{m.name ?? m.id} ({m.provider})</Badge>)}
                    {models.length === 0 && <span className="text-xs text-muted-foreground">暂无模型 — 请检查 Ollama 地址</span>}
                  </div>
                </CardContent>
              </Card>

              <Card className="shadow-card">
                <CardContent className="p-4 space-y-3">
                  <div className="text-[15px] font-medium">向量数据库</div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">Qdrant 地址 (可选)</label>
                    <Input value={config.qdrant_url} onChange={(e) => setConfig({ ...config, qdrant_url: e.target.value })} placeholder="http://localhost:6333 (留空使用内存模式)" className="h-7 text-xs" />
                  </div>
                  <div className="text-[11px] text-muted-foreground">留空 Qdrant 地址时，系统自动使用内存向量检索模式</div>
                </CardContent>
              </Card>
            </>
          )}

          {activeSection === "sync" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="text-[15px] font-medium">同步配置</div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">WebDAV 地址</label>
                  <Input value={config.webdav_url} onChange={(e) => setConfig({ ...config, webdav_url: e.target.value })} placeholder="https://your-webdav-server/dav" className="h-7 text-xs" />
                </div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">WebDAV 用户名</label>
                  <Input value={config.webdav_username} onChange={(e) => setConfig({ ...config, webdav_username: e.target.value })} placeholder="username" className="h-7 text-xs" />
                </div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">WebDAV 密码</label>
                  <Input value={config.webdav_password} onChange={(e) => setConfig({ ...config, webdav_password: e.target.value })} placeholder="password" type="password" className="h-7 text-xs" />
                </div>
                <div className="text-[11px] text-muted-foreground">
                  同步策略: Local-first (CRDT + Automerge)
                  <br />数据优先存储在本地 SQLite，WebDAV 用于跨设备同步
                </div>
                <Badge variant="outline" className="text-[11px]">Automerge CRDT</Badge>
              </CardContent>
            </Card>
          )}

          {activeSection === "security" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="text-[15px] font-medium">安全配置</div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">SCALE Gateway 模式</label>
                  <select value={config.gateway_mode} onChange={(e) => setConfig({ ...config, gateway_mode: e.target.value })} className="h-7 rounded border bg-background text-xs px-2 w-full">
                    <option value="strict">strict (严格: 所有工具调用需审批)</option>
                    <option value="permissive">permissive (宽松: 仅敏感操作需审批)</option>
                    <option value="off">off (关闭: 无拦截)</option>
                  </select>
                </div>
                <div className="space-y-2 flex items-center gap-2">
                  <input type="checkbox" checked={config.data_local_only} onChange={(e) => setConfig({ ...config, data_local_only: e.target.checked })} className="rounded" />
                  <label className="text-[12px]">数据仅存储本地 (不发送到云端)</label>
                </div>
                <div className="text-[11px] text-muted-foreground">
                  <div>SCALE Gateway: 工具调用拦截、敏感操作检测、暴力破解检测</div>
                  <div>本地优先: 数据默认存储在本地 SQLite</div>
                  <div>向量数据库: Qdrant 可选，无地址时自动使用内存模式</div>
                </div>
              </CardContent>
            </Card>
          )}

          {activeSection === "teams" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="text-[15px] font-medium">团队配置</div>
                <div className="text-[11px] text-muted-foreground">
                  多用户协作功能 (Phase 3 开发中):
                  <br />- 多租户隔离
                  <br />- 角色 & 权限管理
                  <br />- 共享工作流 & 知识库
                  <br />- 审计日志
                </div>
                <Badge variant="outline" className="text-[11px]">企业版功能 — Phase 3</Badge>
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}