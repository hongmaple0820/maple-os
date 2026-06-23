"use client";

import { useState, useEffect, useCallback } from "react";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { rpcCall, mapleApi, getAuthState } from "@/lib/api";
import { useTranslation } from "react-i18next";

interface ModelInfo { id: string; name: string; provider: string; is_local?: boolean; registered?: boolean; context_length?: number }

// Mask an API key for display, keeping the first 4 and last 4 chars visible.
// Returns "" for empty input. Returns the original if too short to mask.
function maskApiKey(key: string): string {
  if (!key) return "";
  if (key.length <= 8) return "•".repeat(key.length);
  return `${key.slice(0, 4)}${"•".repeat(Math.min(key.length - 8, 20))}${key.slice(-4)}`;
}
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

interface GroupRule {
  id: string;
  name: string;
  rule_type: { type: string;[key: string]: unknown };
  enabled: boolean;
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
  const { t } = useTranslation();
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  // T3-3: LLM provider test connection state
  const [testingConn, setTestingConn] = useState<null | { provider: string; status: "loading" | "ok" | "fail"; latency_ms?: number; error?: string }>(null);
  const [showApiKey, setShowApiKey] = useState(false);

  const testConnection = useCallback(async (provider: "ollama" | "openai") => {
    setTestingConn({ provider, status: "loading" });
    try {
      const { token } = getAuthState();
      const res = await fetch("/api/maple/api/llm/test-connection", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({
          provider,
          base_url: config.ollama_url,
          api_key: config.openai_api_key,
        }),
      });
      const data = await res.json();
      if (data.ok) {
        setTestingConn({ provider, status: "ok", latency_ms: data.latency_ms });
      } else {
        setTestingConn({ provider, status: "fail", error: data.error ?? "Unknown error" });
      }
    } catch (e) {
      setTestingConn({ provider, status: "fail", error: e instanceof Error ? e.message : String(e) });
    }
  }, [config.ollama_url, config.openai_api_key]);
  const [activeSection, setActiveSection] = useState("models");

  // Group rules state
  const [rules, setRules] = useState<GroupRule[]>([]);
  const [showNewRule, setShowNewRule] = useState(false);
  const [newRuleName, setNewRuleName] = useState("");
  const [newRuleType, setNewRuleType] = useState("auto_assign");
  const [newRuleKeyword, setNewRuleKeyword] = useState("");
  const [newRuleAgentId, setNewRuleAgentId] = useState("");
  const [newRuleThreshold, setNewRuleThreshold] = useState("0.8");
  const [newRuleRateLimit, setNewRuleRateLimit] = useState("10");
  const [newRuleHours, setNewRuleHours] = useState("9,10,11,12,13,14,15,16,17");
  const [newRuleTimezone, setNewRuleTimezone] = useState("8");
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [editRuleName, setEditRuleName] = useState("");
  const [editRuleKeyword, setEditRuleKeyword] = useState("");
  const [editRuleAgentId, setEditRuleAgentId] = useState("");
  const [editRuleExtra, setEditRuleExtra] = useState("");

  const loadRules = useCallback(async () => {
    try {
      const res = await mapleApi<{ rules: GroupRule[] }>("/api/group-rules");
      setRules(res.rules ?? []);
    } catch { setRules([]); }
  }, []);

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

  useEffect(() => {
    if (activeSection === "rules") loadRules();
  }, [activeSection, loadRules]);

  // Agents state
  const [agents, setAgents] = useState<Array<{ id: string; name: string; status: string; is_online: boolean; description?: string; tags?: string }>>([]);
  const [showRegisterAgent, setShowRegisterAgent] = useState(false);
  const [regAgentName, setRegAgentName] = useState("");
  const [regAgentDesc, setRegAgentDesc] = useState("");
  const [regAgentModel, setRegAgentModel] = useState("");

  const loadAgents = useCallback(async () => {
    try {
      const res = await mapleApi<{ agents: Array<{ id: string; name: string; status: string; is_online: boolean; description?: string; tags?: string }> }>("/api/agents/status");
      setAgents(res.agents ?? []);
    } catch { setAgents([]); }
  }, []);

  useEffect(() => {
    if (activeSection === "agents") loadAgents();
  }, [activeSection, loadAgents]);

  const handleSave = async () => {
    setSaving(true);
    setSaved(false);
    try {
      await rpcCall("config.update", { ...config });
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch (err) {
      alert(t("settings.errors.saveFailed", { error: (err as Error).message }));
    } finally { setSaving(false); }
  };

  const handleReset = async () => {
    setConfig(defaultConfig);
    setSaved(false);
  };

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  const sections = [
    { id: "models", label: t("settings.sections.models") },
    { id: "sync", label: t("settings.sections.sync") },
    { id: "security", label: t("settings.sections.security") },
    { id: "rules", label: t("settings.automation.title") },
    { id: "agents", label: t("settings.agents.title") },
    { id: "teams", label: t("settings.sections.teams") },
  ];

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <h2 className="text-[15px] font-semibold">{t("settings.title")}</h2>
        <div className="flex gap-2">
          {saved && <Badge variant="default" className="text-[11px]">{t("common.saved")}</Badge>}
          <Button size="sm" onClick={handleSave} disabled={saving}>{saving ? t("settings.saving") : t("settings.saveConfig")}</Button>
          <Button size="sm" variant="ghost" onClick={handleReset}>{t("common.reset")}</Button>
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
                  <div className="text-[15px] font-medium">{t("settings.models.llmTitle")}</div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">{t("settings.models.ollamaUrl")}</label>
                    <div className="flex gap-2">
                      <Input value={config.ollama_url} onChange={(e) => setConfig({ ...config, ollama_url: e.target.value })} placeholder="http://localhost:11434" className="h-7 text-xs" />
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 text-xs whitespace-nowrap"
                        disabled={testingConn?.status === "loading" && testingConn?.provider === "ollama"}
                        onClick={() => testConnection("ollama")}
                      >
                        {testingConn?.status === "loading" && testingConn?.provider === "ollama"
                          ? t("settings.models.testing", "Testing...")
                          : t("settings.models.testConn", "Test")}
                      </Button>
                    </div>
                    {testingConn?.provider === "ollama" && testingConn.status !== "loading" && (
                      <div className={`text-[11px] ${testingConn.status === "ok" ? "text-green-600" : "text-red-600"}`}>
                        {testingConn.status === "ok"
                          ? `✓ ${t("settings.models.connOk", "Connected")} (${testingConn.latency_ms}ms)`
                          : `✗ ${testingConn.error}`}
                      </div>
                    )}
                  </div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">{t("settings.models.openaiKey")}</label>
                    <div className="flex gap-2">
                      <Input
                        value={showApiKey ? config.openai_api_key : maskApiKey(config.openai_api_key)}
                        onChange={(e) => setConfig({ ...config, openai_api_key: e.target.value })}
                        placeholder="sk-..."
                        type={showApiKey ? "text" : "password"}
                        className="h-7 text-xs"
                      />
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 text-xs"
                        onClick={() => setShowApiKey((s) => !s)}
                      >
                        {showApiKey ? t("settings.models.hide", "Hide") : t("settings.models.show", "Show")}
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 text-xs whitespace-nowrap"
                        disabled={!config.openai_api_key || (testingConn?.status === "loading" && testingConn?.provider === "openai")}
                        onClick={() => testConnection("openai")}
                      >
                        {testingConn?.status === "loading" && testingConn?.provider === "openai"
                          ? t("settings.models.testing", "Testing...")
                          : t("settings.models.testConn", "Test")}
                      </Button>
                    </div>
                    {testingConn?.provider === "openai" && testingConn.status !== "loading" && (
                      <div className={`text-[11px] ${testingConn.status === "ok" ? "text-green-600" : "text-red-600"}`}>
                        {testingConn.status === "ok"
                          ? `✓ ${t("settings.models.connOk", "Connected")} (${testingConn.latency_ms}ms)`
                          : `✗ ${testingConn.error}`}
                      </div>
                    )}
                  </div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">{t("settings.models.defaultModel")}</label>
                    <select value={config.default_model} onChange={(e) => setConfig({ ...config, default_model: e.target.value })} className="h-7 rounded border bg-background text-xs px-2 w-full">
                      <option value="auto">{t("settings.models.autoSelect")}</option>
                      {models.filter((m) => m.registered !== false).map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.name ?? m.id} ({m.provider}){m.is_local ? " · local" : ""}
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className="text-[11px] text-muted-foreground">{t("settings.models.registeredModels")}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {models.map((m) => (
                      <Badge
                        key={m.id}
                        variant={m.registered === false ? "outline" : "secondary"}
                        className={`text-[11px] ${m.registered === false ? "opacity-60" : ""}`}
                        title={m.registered === false ? t("settings.models.notRegisteredHint", "Discovered but not registered — restart server or save config to register") : undefined}
                      >
                        {m.name ?? m.id} ({m.provider}){m.is_local ? " · local" : ""}{m.registered === false ? " · unreg" : ""}
                      </Badge>
                    ))}
                    {models.length === 0 && <span className="text-xs text-muted-foreground">{t("settings.models.noModels")}</span>}
                  </div>
                </CardContent>
              </Card>

              <Card className="shadow-card">
                <CardContent className="p-4 space-y-3">
                  <div className="text-[15px] font-medium">{t("settings.models.vectorDb")}</div>
                  <div className="space-y-2">
                    <label className="text-[11px] text-muted-foreground">{t("settings.models.qdrantUrl")}</label>
                    <Input value={config.qdrant_url} onChange={(e) => setConfig({ ...config, qdrant_url: e.target.value })} placeholder="http://localhost:6333" className="h-7 text-xs" />
                  </div>
                  <div className="text-[11px] text-muted-foreground">{t("settings.models.qdrantHint")}</div>
                </CardContent>
              </Card>
            </>
          )}

          {activeSection === "sync" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="text-[15px] font-medium">{t("settings.sync.title")}</div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">{t("settings.sync.webdavUrl")}</label>
                  <Input value={config.webdav_url} onChange={(e) => setConfig({ ...config, webdav_url: e.target.value })} placeholder="https://your-webdav-server/dav" className="h-7 text-xs" />
                </div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">{t("settings.sync.webdavUser")}</label>
                  <Input value={config.webdav_username} onChange={(e) => setConfig({ ...config, webdav_username: e.target.value })} placeholder="username" className="h-7 text-xs" />
                </div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">{t("settings.sync.webdavPass")}</label>
                  <Input value={config.webdav_password} onChange={(e) => setConfig({ ...config, webdav_password: e.target.value })} placeholder="password" type="password" className="h-7 text-xs" />
                </div>
                <div className="text-[11px] text-muted-foreground">
                  {t("settings.sync.strategy")}
                  <br />{t("settings.sync.strategyDetail")}
                </div>
                <Badge variant="outline" className="text-[11px]">Automerge CRDT</Badge>
              </CardContent>
            </Card>
          )}

          {activeSection === "security" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="text-[15px] font-medium">{t("settings.security.title")}</div>
                <div className="space-y-2">
                  <label className="text-[11px] text-muted-foreground">{t("settings.security.gatewayMode")}</label>
                  <select value={config.gateway_mode} onChange={(e) => setConfig({ ...config, gateway_mode: e.target.value })} className="h-7 rounded border bg-background text-xs px-2 w-full">
                    <option value="strict">{t("settings.security.strict")}</option>
                    <option value="permissive">{t("settings.security.permissive")}</option>
                    <option value="off">{t("settings.security.off")}</option>
                  </select>
                </div>
                <div className="space-y-2 flex items-center gap-2">
                  <input type="checkbox" checked={config.data_local_only} onChange={(e) => setConfig({ ...config, data_local_only: e.target.checked })} className="rounded" />
                  <label className="text-[12px]">{t("settings.security.localOnly")}</label>
                </div>
                <div className="text-[11px] text-muted-foreground">
                  <div>{t("settings.security.gatewayDesc")}</div>
                  <div>{t("settings.security.localFirst")}</div>
                  <div>{t("settings.security.vectorNote")}</div>
                </div>
              </CardContent>
            </Card>
          )}

          {activeSection === "rules" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div className="text-[15px] font-medium">自动化规则</div>
                  <Button size="sm" onClick={() => setShowNewRule(true)}>+ 新建规则</Button>
                </div>
                <div className="text-[11px] text-muted-foreground">
                  配置自动分配、审批、限流和时间窗口规则。规则在聊天消息到达时自动评估。
                </div>

                {showNewRule && (
                  <div className="border border-border rounded-lg p-3 space-y-2 bg-muted/30">
                    <Input value={newRuleName} onChange={(e) => setNewRuleName(e.target.value)} placeholder={t("settings.automation.ruleName")} className="h-7 text-xs" />
                    <select value={newRuleType} onChange={(e) => setNewRuleType(e.target.value)} className="h-7 rounded border bg-background text-xs px-2 w-full">
                      <option value="auto_assign">自动分配 (AutoAssign)</option>
                      <option value="auto_approve">自动审批 (AutoApprove)</option>
                      <option value="rate_limit">限流 (RateLimit)</option>
                      <option value="time_window">时间窗口 (TimeWindow)</option>
                    </select>
                    {(newRuleType === "auto_assign" || newRuleType === "auto_approve" || newRuleType === "rate_limit" || newRuleType === "time_window") && (
                      <Input value={newRuleAgentId} onChange={(e) => setNewRuleAgentId(e.target.value)} placeholder="目标 Agent ID" className="h-7 text-xs" />
                    )}
                    {newRuleType === "auto_assign" && (
                      <Input value={newRuleKeyword} onChange={(e) => setNewRuleKeyword(e.target.value)} placeholder="触发关键词" className="h-7 text-xs" />
                    )}
                    {newRuleType === "auto_approve" && (
                      <Input value={newRuleThreshold} onChange={(e) => setNewRuleThreshold(e.target.value)} placeholder="置信度阈值 (0-1)" className="h-7 text-xs" />
                    )}
                    {newRuleType === "rate_limit" && (
                      <Input value={newRuleRateLimit} onChange={(e) => setNewRuleRateLimit(e.target.value)} placeholder="每分钟最大消息数" className="h-7 text-xs" />
                    )}
                    {newRuleType === "time_window" && (
                      <>
                        <Input value={newRuleHours} onChange={(e) => setNewRuleHours(e.target.value)} placeholder="允许的小时 (逗号分隔, 如 9,10,11)" className="h-7 text-xs" />
                        <Input value={newRuleTimezone} onChange={(e) => setNewRuleTimezone(e.target.value)} placeholder="时区偏移 (如 8 表示 UTC+8)" className="h-7 text-xs" />
                      </>
                    )}
                    <div className="flex gap-2">
                      <Button size="sm" onClick={async () => {
                        if (!newRuleName.trim()) return;
                        const rule_type = newRuleType === "auto_assign"
                          ? { type: "auto_assign", keyword: newRuleKeyword, agent_id: newRuleAgentId }
                          : newRuleType === "auto_approve"
                          ? { type: "auto_approve", agent_id: newRuleAgentId, confidence_threshold: parseFloat(newRuleThreshold) || 0.8, auto_approve_roles: [] }
                          : newRuleType === "rate_limit"
                          ? { type: "rate_limit", agent_id: newRuleAgentId, max_messages_per_minute: parseInt(newRuleRateLimit) || 10 }
                          : { type: "time_window", agent_id: newRuleAgentId, allowed_hours: newRuleHours, timezone: newRuleTimezone };
                        await mapleApi("/api/group-rules", { method: "POST", body: { id: `rule-${Date.now()}`, name: newRuleName.trim(), rule_type, enabled: true } });
                        setShowNewRule(false); setNewRuleName(""); setNewRuleKeyword(""); setNewRuleAgentId("");
                        loadRules();
                      }}>保存</Button>
                      <Button size="sm" variant="ghost" onClick={() => setShowNewRule(false)}>取消</Button>
                    </div>
                  </div>
                )}

                <div className="space-y-2">
                  {rules.length === 0 && <div className="text-xs text-muted-foreground py-4 text-center">暂无规则</div>}
                  {rules.map((rule) => (
                    editingRuleId === rule.id ? (
                      <div key={rule.id} className="border border-primary/30 rounded-lg p-3 space-y-2 bg-muted/30">
                        <Input value={editRuleName} onChange={(e) => setEditRuleName(e.target.value)} placeholder={t("settings.automation.ruleName")} className="h-7 text-xs" />
                        <Input value={editRuleAgentId} onChange={(e) => setEditRuleAgentId(e.target.value)} placeholder="目标 Agent ID" className="h-7 text-xs" />
                        {rule.rule_type.type === "auto_assign" && (
                          <Input value={editRuleKeyword} onChange={(e) => setEditRuleKeyword(e.target.value)} placeholder="触发关键词" className="h-7 text-xs" />
                        )}
                        {rule.rule_type.type === "auto_approve" && (
                          <Input value={editRuleExtra} onChange={(e) => setEditRuleExtra(e.target.value)} placeholder="置信度阈值 (0-1)" className="h-7 text-xs" />
                        )}
                        {rule.rule_type.type === "rate_limit" && (
                          <Input value={editRuleExtra} onChange={(e) => setEditRuleExtra(e.target.value)} placeholder="每分钟最大消息数" className="h-7 text-xs" />
                        )}
                        {rule.rule_type.type === "time_window" && (
                          <Input value={editRuleExtra} onChange={(e) => setEditRuleExtra(e.target.value)} placeholder="允许小时 (逗号分隔) + 时区，如 9,10,11|8" className="h-7 text-xs" />
                        )}
                        <div className="flex gap-2">
                          <Button size="sm" onClick={async () => {
                            let updated_type: Record<string, unknown>;
                            const rt = rule.rule_type as { type: string;[key: string]: unknown };
                            if (rt.type === "auto_assign") {
                              updated_type = { type: "auto_assign", keyword: editRuleKeyword, agent_id: editRuleAgentId };
                            } else if (rt.type === "auto_approve") {
                              updated_type = { type: "auto_approve", agent_id: editRuleAgentId, confidence_threshold: parseFloat(editRuleExtra) || 0.8, auto_approve_roles: rt.auto_approve_roles || [] };
                            } else if (rt.type === "rate_limit") {
                              updated_type = { type: "rate_limit", agent_id: editRuleAgentId, max_messages_per_minute: parseInt(editRuleExtra) || 10 };
                            } else if (rt.type === "time_window") {
                              const parts = editRuleExtra.split("|");
                              updated_type = { type: "time_window", agent_id: editRuleAgentId, allowed_hours: parts[0] || "9,10,11", timezone: parts[1] || "8" };
                            } else {
                              updated_type = rt;
                            }
                            await mapleApi(`/api/group-rules/${rule.id}`, { method: "PUT", body: { id: rule.id, name: editRuleName.trim(), rule_type: updated_type, enabled: rule.enabled } });
                            setEditingRuleId(null); loadRules();
                          }}>保存</Button>
                          <Button size="sm" variant="ghost" onClick={() => setEditingRuleId(null)}>取消</Button>
                        </div>
                      </div>
                    ) : (
                      <div key={rule.id} className="flex items-center justify-between p-2.5 border border-border rounded-lg">
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <span className="text-xs font-medium truncate">{rule.name}</span>
                            <Badge variant="outline" className="text-[10px] shrink-0">{rule.rule_type.type}</Badge>
                            <span className={`w-2 h-2 rounded-full shrink-0 ${rule.enabled ? "bg-emerald-500" : "bg-slate-300"}`} />
                          </div>
                          <div className="text-[11px] text-muted-foreground mt-0.5">
                            {JSON.stringify(Object.fromEntries(Object.entries(rule.rule_type).filter(([k]) => k !== "type")))}
                          </div>
                        </div>
                        <div className="flex items-center gap-1 ml-2">
                          <button
                            onClick={() => {
                              setEditingRuleId(rule.id); setEditRuleName(rule.name);
                              const rt = rule.rule_type as { type: string; keyword?: string; agent_id?: string; confidence_threshold?: number; max_messages_per_minute?: number; allowed_hours?: string; timezone?: string };
                              setEditRuleKeyword(rt.keyword || ""); setEditRuleAgentId(rt.agent_id || "");
                              if (rt.type === "auto_approve") setEditRuleExtra(String(rt.confidence_threshold ?? 0.8));
                              else if (rt.type === "rate_limit") setEditRuleExtra(String(rt.max_messages_per_minute ?? 10));
                              else if (rt.type === "time_window") setEditRuleExtra(`${rt.allowed_hours || ""}|${rt.timezone || ""}`);
                              else setEditRuleExtra("");
                            }}
                            className="px-2 py-1 text-[11px] text-muted-foreground hover:text-foreground border border-border rounded hover:bg-muted transition-colors"
                          >编辑</button>
                          <button
                            onClick={async () => {
                              await mapleApi(`/api/group-rules/${rule.id}`, { method: "PUT", body: { ...rule, enabled: !rule.enabled } });
                              loadRules();
                            }}
                            className="px-2 py-1 text-[11px] text-muted-foreground hover:text-foreground border border-border rounded hover:bg-muted transition-colors"
                          >{rule.enabled ? t("settings.automation.disabled") : t("settings.automation.enabled")}</button>
                          <button
                            onClick={async () => {
                              await mapleApi(`/api/group-rules/${rule.id}`, { method: "DELETE" });
                              loadRules();
                            }}
                            className="px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 border border-border rounded transition-colors"
                          >删除</button>
                        </div>
                      </div>
                    )
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {activeSection === "agents" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div className="text-[15px] font-medium">已注册 Agent</div>
                  <Button size="sm" onClick={() => setShowRegisterAgent(true)}>+ 注册 Agent</Button>
                </div>
                <div className="text-[11px] text-muted-foreground">
                  查看所有已注册的 Agent 及其状态。Agent 通过 WebSocket 或 HTTP 连接注册。
                </div>

                {showRegisterAgent && (
                  <div className="border border-border rounded-lg p-3 space-y-2 bg-muted/30">
                    <Input value={regAgentName} onChange={(e) => setRegAgentName(e.target.value)} placeholder="Agent 名称" className="h-7 text-xs" />
                    <Input value={regAgentDesc} onChange={(e) => setRegAgentDesc(e.target.value)} placeholder="描述（可选）" className="h-7 text-xs" />
                    <Input value={regAgentModel} onChange={(e) => setRegAgentModel(e.target.value)} placeholder="模型（可选，如 gpt-4）" className="h-7 text-xs" />
                    <div className="flex gap-2">
                      <Button size="sm" onClick={async () => {
                        if (!regAgentName.trim()) return;
                        const agentId = `agent-${Date.now()}`;
                        await mapleApi("/api/agents", {
                          method: "POST",
                          body: {
                            id: agentId, name: regAgentName.trim(),
                            description: regAgentDesc.trim() || undefined,
                            model: regAgentModel.trim() || undefined,
                            transport_type: "http", transport_config: "{}", capabilities: "[]",
                          },
                        });
                        setShowRegisterAgent(false); setRegAgentName(""); setRegAgentDesc(""); setRegAgentModel("");
                        loadAgents();
                      }}>注册</Button>
                      <Button size="sm" variant="ghost" onClick={() => setShowRegisterAgent(false)}>取消</Button>
                    </div>
                  </div>
                )}

                <div className="space-y-2">
                  {agents.length === 0 && <div className="text-xs text-muted-foreground py-4 text-center">暂无已注册 Agent</div>}
                  {agents.map((agent) => (
                    <div key={agent.id} className="flex items-center justify-between p-2.5 border border-border rounded-lg">
                      <div className="flex items-center gap-3">
                        <span className={`w-2.5 h-2.5 rounded-full ${agent.is_online ? "bg-emerald-500 animate-pulse" : "bg-slate-300"}`} />
                        <div>
                          <div className="text-xs font-medium">{agent.name || agent.id}</div>
                          <div className="text-[11px] text-muted-foreground flex items-center gap-2">
                            <span>{agent.is_online ? t("settings.agents.online") : t("settings.agents.offline")}</span>
                            {agent.description && <span>| {agent.description}</span>}
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {agent.tags && (
                          <div className="flex gap-1">
                            {(() => {
                              try { return JSON.parse(agent.tags); } catch { return []; }
                            })().slice(0, 3).map((tag: string, i: number) => (
                              <span key={i} className="px-1.5 py-0.5 bg-muted rounded text-[10px] text-muted-foreground">{tag}</span>
                            ))}
                          </div>
                        )}
                        <Badge variant={agent.is_online ? "default" : "secondary"} className="text-[10px]">
                          {agent.status}
                        </Badge>
                        <button
                          onClick={async () => {
                            await mapleApi(`/api/agents/${agent.id}`, { method: "DELETE" });
                            loadAgents();
                          }}
                          className="px-2 py-1 text-[11px] text-destructive hover:bg-destructive/10 border border-border rounded transition-colors"
                        >删除</button>
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {activeSection === "teams" && (
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-3">
                <div className="text-[15px] font-medium">{t("settings.teams.title")}</div>
                <div className="text-[11px] text-muted-foreground">
                  {t("settings.teams.desc")}
                  <br />{t("settings.teams.multiTenant")}
                  <br />{t("settings.teams.rbac")}
                  <br />{t("settings.teams.shared")}
                  <br />{t("settings.teams.audit")}
                </div>
                <Badge variant="outline" className="text-[11px]">{t("settings.teams.enterprise")}</Badge>
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}