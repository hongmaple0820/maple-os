# MapleOS Issues 组织指南

## 当前 Issues 状态

- **总 Issues**: 50
- **Open**: 21
- **Closed**: 29
- **PRs**: 7

## 建议的里程碑规划

### Phase 1: 闭环修复 (2026年5月)
**目标**: 修复核心功能闭环
**截止日期**: 2026-05-31

**包含 Issues**:
- #52 Chat SSE 流式输出
- #53 Workflow SSE 实时节点状态更新
- #54 Chat→Knowledge 交叉引用
- #55 memory_search 接口对齐
- #56 kb_search 结果补充 source_type

### Phase 2: 核心体验 (2026年6月)
**目标**: 提升用户体验
**截止日期**: 2026-06-30

**包含 Issues**:
- #57 web_search 技能补实
- #58 code_execute 技能补实 (WASM sandbox)
- #59 Scheduler 后台启动
- #60 routing_rules.yaml 默认模型路由
- #61 Workflow 执行历史 UI
- #62 Chat Session 管理

### Phase 3: 桌面端 (2026年7月)
**目标**: Tauri 桌面端功能完善
**截止日期**: 2026-07-31

**包含 Issues**:
- #63 Tauri 2 项目结构完善
- #64 原生菜单 + 通知 + 文件系统
- #65 桌面端自动更新

### Phase 4: 测试质量 (2026年8月)
**目标**: 测试覆盖率提升
**截止日期**: 2026-08-31

**包含 Issues**:
- #66 Playwright E2E 框架搭建
- #67 Rust 单元测试补充 + CI Pipeline

### Phase 5: 移动端 (2026年9月)
**目标**: React Native 移动端项目初始化
**截止日期**: 2026-09-30

**包含 Issues**:
- #68 Expo React Native 项目初始化

### Phase 6: 生态完善 (2026年10月)
**目标**: 生态系统完善
**截止日期**: 2026-10-31

**包含 Issues**:
- #69 Plugins 真实加载机制
- #70 Automerge CRDT 替换自定义 merge
- #71 packages/config 共享配置包
- #72 file_ops + http_request 技能补实

## 执行步骤

### 步骤 1: 创建里程碑

方法一：使用 GitHub Web 界面
1. 访问 https://github.com/hongmaple0820/maple-os/milestones
2. 点击 "New milestone"
3. 按上述规划创建 6 个里程碑

方法二：使用 GitHub CLI
```bash
# 安装 gh (如果未安装)
# 登录
cd /workspace
gh auth login

# 创建里程碑
gh api repos/hongmaple0820/maple-os/milestones \
  --method POST \
  --field title="Phase 1: 闭环修复" \
  --field state=open \
  --field description="修复核心功能闭环" \
  --field due_on="2026-05-31T23:59:59Z"
```

方法三：使用脚本 (需要 GITHUB_TOKEN)
```bash
export GITHUB_TOKEN="your_token_here"
bash /workspace/scripts/create_milestones.sh
```

### 步骤 2: 分配 Issues 到里程碑

创建里程碑后，获取里程碑 ID:
```bash
curl https://api.github.com/repos/hongmaple0820/maple-os/milestones
```

然后分配 issues:
```bash
# 分配 Phase 1 issues (假设 milestone_id = 1)
for issue in 52 53 54 55 56; do
  curl -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/hongmaple0820/maple-os/issues/$issue \
    -d '{"milestone": 1}'
done
```

### 步骤 3: 更新 Labels

建议添加以下 labels:
- `P0` - blocked/critical
- `P1` - high-priority
- `P2` - medium-priority
- `P3` - low-priority
- `phase-1` ~ `phase-6`
- `backend` - Rust 后端
- `frontend` - Web 前端
- `infra` - 基础设施

## 优先级建议

### 立即处理 (P0)
- #52 Chat SSE 流式输出
- #53 Workflow SSE 实时节点状态更新
- #54 Chat→Knowledge 交叉引用
- #55 memory_search 接口对齐

### 本周处理 (P1)
- #56 kb_search 结果补充 source_type
- #57 web_search 技能补实
- #58 code_execute 技能补实

### 后续规划 (P2/P3)
其他 issues 按计划分配到各 phase 里程碑

## 完成标准

每个 Phase 里程碑完成时:
1. 所有关联 issues 已关闭
2. 相关功能已测试通过
3. 文档已更新
4. CHANGELOG 已添加

## 监控 Dashboard

可以通过以下链接查看里程碑进度:
- https://github.com/hongmaple0820/maple-os/milestones

或使用 GitHub API 获取统计信息:
```bash
curl https://api.github.com/repos/hongmaple0820/maple-os/milestones
```
