# MapleOS · 产品设计系统 & 技术方案蓝图

> AI Native 多 Agent 协作工作站操作系统
> Version 1.0 · Product + UI + Engineering Blueprint

---

## 1. 产品定位

MapleOS 是 AI Native 工作站操作系统、多 Agent 协作中枢、工作流自动化编排平台、Local-first 本地优先智能工作站、可自我进化的 AI 协同平台。

核心理念：Human + Agent + Workflow + Knowledge + Tools

## 2. 设计关键词

| 维度 | 描述 |
|---|---|
| 产品气质 | 专业 / 克制 / 高级 / AI Native |
| 视觉风格 | Linear + Raycast + Cursor + Vercel |
| 交互感受 | 实时、轻量、低干扰 |
| 信息架构 | 工作站式、多面板、高密度 |
| 动效语言 | 微交互 + 状态驱动 |
| 空间感 | 卡片层级 + 半透明 |

## 3. 色彩系统

### Light

- primary: #2563EB
- primary-hover: #1D4ED8
- secondary: #0F172A
- surface: #FFFFFF
- background: #F5F7FB
- border: #E5E7EB
- muted: #6B7280
- success: #10B981
- warning: #F59E0B
- error: #EF4444

### Dark

- primary: #3B82F6
- surface: #111827
- background: #030712
- border: #1F2937
- text: #F9FAFB

## 4. 字体规范

| 场景 | 字体 | 大小 |
|---|---|---|
| H1 | Inter Bold | 32 |
| H2 | Inter SemiBold | 24 |
| H3 | Inter Medium | 18 |
| 正文 | Inter Regular | 14 |
| 代码 | JetBrains Mono | 13 |
| 数据指标 | Inter Bold | 40 |

## 5. 设计 Tokens

- radius-sm: 8px
- radius-md: 12px
- radius-lg: 16px
- shadow-card: 0 4px 12px rgba(0,0,0,0.06)

## 6. 信息架构

```
MapleOS
 ├── Workspace (Dashboard / Tasks / Collaboration / Timeline)
 ├── Workflow (Editor / Templates / Execution / History)
 ├── Agent (Center / Registry / Runtime / Capabilities)
 ├── Knowledge (Documents / Memory / Retrieval / Evolutions)
 ├── Plugins (Skills / MCP / CLI Tools / Marketplace)
 └── Settings (Models / Sync / Security / Teams)
```

## 7. 核心页面布局

### Dashboard: Top Nav + Sidebar + Main Workspace + Bottom Command Dock
### Workflow Editor: Toolbar + Node Lib + Canvas + Console/Logs + Config Panel
### Agent Center: Shared Workspace + Human/Agent Members + Conversation + Task Timeline + Shared Knowledge
### Chat Workspace: Sidebar Sessions + Main Chat + Context Panel

## 8. 技术栈

| Layer | Tech |
|---|---|
| Desktop | Tauri 2 |
| Web | Next.js 15 |
| Mobile | React Native |
| Backend | Rust Axum |
| Runtime | Tokio |
| Workflow | Petgraph |
| Database | SQLite + PostgreSQL |
| Vector DB | Qdrant |
| Sync | Automerge CRDT |

## 9. 开发阶段

- Phase 1: LLM Router + Workflow Engine + Chat Workspace + Tauri Desktop
- Phase 2: Agent Collaboration + Knowledge Evolution + Workflow Visual Editor + WebDAV Sync
- Phase 3: Plugin Marketplace + Enterprise Features + Team Collaboration + Multi Tenant SaaS

## 10. 产品护城河

1. 自我进化 — 系统越用越聪明
2. 多 Agent 协作 — AI Team
3. Local-first — 隐私与离线能力
4. Rust Runtime — 性能级 Agent OS
5. Workflow + Knowledge — 长期资产沉淀