"use client";

import { useState, useEffect } from "react";
import { Card, CardContent, Badge, Button, Spinner } from "@mapleos/ui";
import { rpcCall } from "@/lib/api";

interface SkillInfo { id: string; description: string }
interface ScaleTool { name: string; description: string }

const MCP_TOOLS = [
  { id: "scale_create", name: "SCALE 创建", desc: "创建 Spec/Plan/Task/Defect 等 Artifact", category: "SCALE 引擎", installed: true },
  { id: "scale_transition", name: "SCALE 状态迁移", desc: "通过 FSM action 迁移 Artifact 状态", category: "SCALE 引擎", installed: true },
  { id: "scale_list", name: "SCALE 列表", desc: "按类型/状态筛选列出 Artifact", category: "SCALE 引擎", installed: true },
  { id: "scale_show", name: "SCALE 详情", desc: "查看 Artifact 详情和 FSM 上下文", category: "SCALE 引擎", installed: true },
  { id: "scale_context", name: "SCALE 上下文", desc: "为当前会话构建 FSM + 知识上下文", category: "SCALE 引擎", installed: true },
  { id: "scale_stats", name: "SCALE 统计", desc: "获取引擎统计数据", category: "SCALE 引擎", installed: true },
  { id: "scale_available_actions", name: "SCALE 可用动作", desc: "获取 Artifact 可执行的 FSM 动作", category: "SCALE 引擎", installed: true },
  { id: "web_search", name: "Web 搜索", desc: "搜索引擎搜索能力", category: "内置技能", installed: true },
  { id: "code_execute", name: "代码执行", desc: "沙箱内执行代码片段", category: "内置技能", installed: true },
  { id: "file_ops", name: "文件操作", desc: "读写文件系统", category: "内置技能", installed: true },
  { id: "echo", name: "Echo 测试", desc: "返回输入的测试技能", category: "内置技能", installed: true },
  { id: "http_request", name: "HTTP 请求", desc: "发起 HTTP 请求", category: "内置技能", installed: true },
  { id: "playwright", name: "Playwright 浏览器", desc: "浏览器自动化控制", category: "待安装", installed: false },
  { id: "pdf", name: "PDF 处理", desc: "读取和解析 PDF 文档", category: "待安装", installed: false },
  { id: "vercel_deploy", name: "Vercel 部署", desc: "一键部署到 Vercel", category: "待安装", installed: false },
];

export function PluginsPage() {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try { const r = await rpcCall<{ skills: SkillInfo[] }>("skill.list"); setSkills(r.skills ?? []); } catch { setSkills([]); }
      setLoading(false);
    };
    load();
  }, []);

  if (loading) return <div className="flex items-center justify-center h-full"><Spinner className="w-8 h-8" /></div>;

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center px-4">
        <h2 className="text-[15px] font-semibold">插件市场</h2>
      </div>
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        <div className="text-[11px] text-muted-foreground">已注册技能: {skills.length} 个 | MCP 工具: {MCP_TOOLS.filter(t => t.installed).length} 个已安装</div>

        {MCP_TOOLS.map((tool) => (
          <Card key={tool.id} className={`shadow-card ${!tool.installed ? "opacity-60" : ""}`}>
            <CardContent className="p-3">
              <div className="flex items-center justify-between">
                <div>
                  <span className="text-[13px] font-medium">{tool.name}</span>
                  <Badge variant={tool.installed ? "default" : "outline"} className="text-[10px] ml-2">{tool.installed ? "已安装" : "未安装"}</Badge>
                </div>
                <Badge variant="secondary" className="text-[10px]">{tool.category}</Badge>
              </div>
              <div className="text-[11px] text-muted-foreground mt-1">{tool.desc}</div>
              {!tool.installed && <Button size="sm" variant="outline" className="mt-2 text-xs">安装</Button>}
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}