# MapleOS 文档

> MapleOS 项目文档索引

---

## 核心文档

| 文档 | 描述 |
|------|------|
| [竞品分析](./competitive-analysis.md) | 深度竞品对比与最佳实践 |
| [统一实施计划](./unified-implementation-plan.md) | 架构升级路线图 |
| [v2.0.0 路线图](./v2.0-roadmap.md) | v2.0.0 功能规划与实现 |
| [v2.0.0 新特性详解](./v2.0-features.md) | v2.0.0 功能详细文档 |

## 架构文档

| 文档 | 描述 |
|------|------|
| [v0.7 架构设计](./v0.7-architecture.md) | v0.7 版本架构设计 |

## 竞品研究

| 文档 | 描述 |
|------|------|
| [hermes-agent 深度分析](./hermes-agent-deep-dive.md) | hermes-agent 架构与功能分析 |
| [cc-haha 深度分析](./cc-haha-deep-dive.md) | cc-haha 功能与实现分析 |
| [golutra 产品设计分析](./golutra-product-design-analysis.md) | golutra 产品设计理念 |
| [rig/claw-code 工具分析](./rig-clawcode-tool-analysis.md) | rig 和 claw-code 工具系统分析 |
| [MapleOS 集成点](./maple-os-integration-points.md) | MapleOS 与其他系统集成方案 |

## 开发文档

| 文档 | 描述 |
|------|------|
| [`#[tool]` 派生宏](./tool-macro.md) | 声明式工具定义，自动生成 JSON Schema 和执行器 |

---

## v2.0.0 功能概览

### P0 — 核心竞争力

- ✅ **RAG-Retrievable Tools** — 向量化工具描述 + 语义搜索 + 分类标签 + 使用频率排序
- ✅ **LLM Provider 生态扩展** — 14+ 提供商支持

### P1 — 生产就绪

- ✅ **Cron 调度器 + 自然语言任务** — 自然语言解析 + 定时执行
- ✅ **终端后端扩展** — Local / Docker / SSH 多执行环境

### P2 — 工程质量

- ✅ **Mock Parity Harness** — 确定性 Mock LLM + E2E 对等测试
- ✅ **ToolSearch 运行时发现** — 关键词工具搜索

---

## 测试统计

- **maple-agent**: 285 单元测试 + 23 集成测试
- **maple-llm**: 69 测试
- **基准测试**: 9 个套件
- **总计**: 377+ 测试通过

## 性能基准

| 组件 | 操作 | 时间 |
|------|------|------|
| Trident Compaction | 20 消息 | 22.5 µs |
| Skill Discovery | 100 技能 | 44.2 µs |
| Workflow DAG | 验证 | 13.5 µs |
| Parallel Tools | 10 并发 | 24.8 µs |
| Trajectory Scoring | 评分 | 23.8 ns |
| Platform Registry | 路由消息 | 994.6 ns |

---

## 快速链接

- [README](../README.md) — 项目主页
- [CHANGELOG](../CHANGELOG.md) — 版本更新日志
- [GitHub](https://github.com/hongmaple0820/maple-os) — 源代码
