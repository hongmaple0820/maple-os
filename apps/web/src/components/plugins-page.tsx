"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, Badge, Button, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

interface SkillInfo { id: string; description: string }
interface ScaleTool { name: string; description: string }
interface PluginItem {
  id: string;
  name: string;
  desc: string;
  category: string;
  installed: boolean;
  installing?: boolean;
  skillId?: string;
}

const MCP_TOOLS: PluginItem[] = [
  { id: "scale_create", name: "SCALE 创建", desc: "创建 Spec/Plan/Task/Defect 等 Artifact", category: "SCALE 引擎", installed: true },
  { id: "scale_transition", name: "SCALE 状态迁移", desc: "通过 FSM action 迁移 Artifact 状态", category: "SCALE 引擎", installed: true },
  { id: "scale_list", name: "SCALE 列表", desc: "按类型/状态筛选列出 Artifact", category: "SCALE 引擎", installed: true },
  { id: "scale_show", name: "SCALE 详情", desc: "查看 Artifact 详情和 FSM 上下文", category: "SCALE 引擎", installed: true },
  { id: "scale_context", name: "SCALE 上下文", desc: "为当前会话构建 FSM + 知识上下文", category: "SCALE 引擎", installed: true },
  { id: "scale_stats", name: "SCALE 统计", desc: "获取引擎统计数据", category: "SCALE 引擎", installed: true },
  { id: "scale_available_actions", name: "SCALE 可用动作", desc: "获取 Artifact 可执行的 FSM 动作", category: "SCALE 引擎", installed: true },
  { id: "web_search", name: "Web 搜索", desc: "搜索引擎搜索能力", category: "内置技能", installed: true, skillId: "web_search" },
  { id: "code_execute", name: "代码执行", desc: "沙箱内执行代码片段", category: "内置技能", installed: true, skillId: "code_execute" },
  { id: "file_ops", name: "文件操作", desc: "读写文件系统", category: "内置技能", installed: true, skillId: "file_ops" },
  { id: "echo", name: "Echo 测试", desc: "返回输入的测试技能", category: "内置技能", installed: true, skillId: "echo" },
  { id: "http_request", name: "HTTP 请求", desc: "发起 HTTP 请求", category: "内置技能", installed: true, skillId: "http_request" },
  { id: "playwright", name: "Playwright 浏览器", desc: "浏览器自动化控制", category: "待安装", installed: false, skillId: "playwright" },
  { id: "pdf", name: "PDF 处理", desc: "读取和解析 PDF 文档", category: "待安装", installed: false, skillId: "pdf" },
  { id: "vercel_deploy", name: "Vercel 部署", desc: "一键部署到 Vercel", category: "待安装", installed: false, skillId: "vercel_deploy" },
];

const categoryIcons: Record<string, string> = {
  "SCALE 引擎": "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
  "内置技能": "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z",
  "待安装": "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3",
};

export function PluginsPage() {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [plugins, setPlugins] = useState<PluginItem[]>(MCP_TOOLS);
  const [filter, setFilter] = useState<string>("全部");
  const [installLog, setInstallLog] = useState<string[]>([]);

  useEffect(() => {
    const load = async () => {
      try {
        const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list");
        const skillList = r.skills ?? [];
        setSkills(skillList);
        const skillIds = skillList.map((s) => s.id);
        setPlugins((prev) => prev.map((p) => {
          if (p.skillId && skillIds.includes(p.skillId)) return { ...p, installed: true };
          return p;
        }));
      } catch { setSkills([]); }
      setLoading(false);
    };
    load();
  }, []);

  const handleInstall = async (plugin: PluginItem) => {
    setPlugins((prev) => prev.map((p) => p.id === plugin.id ? { ...p, installing: true } : p));
    setInstallLog((prev) => [...prev, `[安装] 开始安装 ${plugin.name}...`]);
    try {
      if (plugin.skillId) {
        await rpcCall("skill.install", { skill_id: plugin.skillId });
      }
      setPlugins((prev) => prev.map((p) => p.id === plugin.id ? { ...p, installed: true, installing: false } : p));
      setInstallLog((prev) => [...prev, `[安装] ${plugin.name} 安装成功`]);
      try {
        const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list");
        setSkills(r.skills ?? []);
      } catch { /* ignore */ }
    } catch (err) {
      setPlugins((prev) => prev.map((p) => p.id === plugin.id ? { ...p, installing: false } : p));
      setInstallLog((prev) => [...prev, `[安装] ${plugin.name} 安装失败: ${(err as Error).message}`]);
    }
  };

  const handleUninstall = async (plugin: PluginItem) => {
    setPlugins((prev) => prev.map((p) => p.id === plugin.id ? { ...p, installing: true } : p));
    setInstallLog((prev) => [...prev, `[卸载] 开始卸载 ${plugin.name}...`]);
    try {
      if (plugin.skillId) {
        await rpcCall("skill.uninstall", { skill_id: plugin.skillId });
      }
      setPlugins((prev) => prev.map((p) => p.id === plugin.id ? { ...p, installed: false, installing: false } : p));
      setInstallLog((prev) => [...prev, `[卸载] ${plugin.name} 已卸载`]);
      try {
        const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list");
        setSkills(r.skills ?? []);
      } catch { /* ignore */ }
    } catch (err) {
      setPlugins((prev) => prev.map((p) => p.id === plugin.id ? { ...p, installing: false } : p));
      setInstallLog((prev) => [...prev, `[卸载] ${plugin.name} 卸载失败: ${(err as Error).message}`]);
    }
  };

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  const categories = ["全部", ...new Set(plugins.map((p) => p.category))];
  const filtered = filter === "全部" ? plugins : plugins.filter((p) => p.category === filter);
  const installedCount = plugins.filter((p) => p.installed).length;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <h2 className="text-[15px] font-semibold">插件市场</h2>
        <div className="flex items-center gap-2">
          <Badge variant="default" className="text-[10px]">{installedCount} 已安装</Badge>
          <Badge variant="outline" className="text-[10px]">{skills.length} 已注册技能</Badge>
        </div>
      </div>

      <div className="h-9 border-b bg-muted/30 flex items-center gap-2 px-4">
        {categories.map((cat) => (
          <button
            key={cat}
            onClick={() => setFilter(cat)}
            className={`px-2 py-1 rounded text-[11px] transition-colors ${
              filter === cat ? "bg-primary text-primary-foreground font-medium" : "text-muted-foreground hover:bg-accent"
            }`}
          >
            {cat}
          </button>
        ))}
      </div>

      <div className="flex flex-1 overflow-hidden">
        <div className="flex-1 overflow-y-auto p-4 space-y-2">
          {filtered.map((plugin) => (
            <Card key={plugin.id} className={`shadow-card transition-all ${plugin.installing ? "animate-pulse" : ""} ${!plugin.installed ? "opacity-70" : ""}`}>
              <CardContent className="p-3">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <svg className="w-4 h-4 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d={categoryIcons[plugin.category] ?? categoryIcons["待安装"]} />
                    </svg>
                    <span className="text-[13px] font-medium">{plugin.name}</span>
                    <Badge variant={plugin.installed ? "default" : "outline"} className="text-[10px]">
                      {plugin.installing ? "安装中..." : plugin.installed ? "已安装" : "未安装"}
                    </Badge>
                  </div>
                  <Badge variant="secondary" className="text-[10px]">{plugin.category}</Badge>
                </div>
                <div className="text-[11px] text-muted-foreground mt-1">{plugin.desc}</div>
                <div className="mt-2 flex gap-2">
                  {!plugin.installed && !plugin.installing && (
                    <Button size="sm" variant="outline" className="text-xs" onClick={() => handleInstall(plugin)}>安装</Button>
                  )}
                  {plugin.installed && !plugin.installing && plugin.skillId && (
                    <Button size="sm" variant="ghost" className="text-xs text-destructive" onClick={() => handleUninstall(plugin)}>卸载</Button>
                  )}
                  {plugin.installing && <Spinner className="w-4 h-4" />}
                </div>
              </CardContent>
            </Card>
          ))}
        </div>

        <div className="w-44 border-l bg-card overflow-y-auto p-3">
          <div className="text-[11px] text-muted-foreground mb-1.5">安装日志</div>
          <div className="space-y-0.5">
            {installLog.map((log, i) => (
              <div key={i} className="text-[11px] font-mono text-muted-foreground leading-tight">{log}</div>
            ))}
            {installLog.length === 0 && <div className="text-[11px] text-muted-foreground">暂无日志</div>}
          </div>

          <div className="mt-4 text-[11px] text-muted-foreground mb-1.5">已注册技能</div>
          <div className="flex flex-wrap gap-1">
            {skills.map((s) => <Badge key={s.id} variant="outline" className="text-[10px]">{s.id}</Badge>)}
          </div>
        </div>
      </div>
    </div>
  );
}