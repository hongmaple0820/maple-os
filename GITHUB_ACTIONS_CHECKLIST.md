# MapleOS Issues 操作清单

## 快速操作指南

### 第一步：创建 6 个里程碑

在 GitHub 网页上操作：https://github.com/hongmaple0820/maple-os/milestones/new

复制以下内容创建每个里程碑：

#### Milestone 1: Phase 1 - 闭环修复
```
标题: Phase 1: 闭环修复
描述: 修复核心功能闭环，包括 Chat SSE、Workflow SSE、接口对齐等关键问题
截止日期: 2026-05-31
```

#### Milestone 2: Phase 2 - 核心体验
```
标题: Phase 2: 核心体验
描述: 提升用户体验，包括技能补实、Session 管理、Workflow 历史等
截止日期: 2026-06-30
```

#### Milestone 3: Phase 3 - 桌面端
```
标题: Phase 3: 桌面端
描述: Tauri 桌面端功能完善，包括菜单、通知、文件系统、自动更新
截止日期: 2026-07-31
```

#### Milestone 4: Phase 4 - 测试质量
```
标题: Phase 4: 测试质量
描述: 测试覆盖率提升，包括 E2E 框架和单元测试补充
截止日期: 2026-08-31
```

#### Milestone 5: Phase 5 - 移动端
```
标题: Phase 5: 移动端
描述: React Native 移动端项目初始化
截止日期: 2026-09-30
```

#### Milestone 6: Phase 6 - 生态完善
```
标题: Phase 6: 生态完善
描述: 生态系统完善，包括插件机制、技能补实、配置共享包等
截止日期: 2026-10-31
```

---

### 第二步：批量分配 Issues

创建里程碑后，使用以下 GitHub CLI 命令批量分配：

```bash
# 登录 GitHub CLI
gh auth login

# 切换到项目目录
cd /workspace

# 获取里程碑编号
gh api repos/hongmaple0820/maple-os/milestones --jq '.[] | "\(.number): \(.title)"'

# 然后分配 issues（将 X 替换为实际的 milestone 编号）

# Phase 1: issues #52, #53, #54, #55, #56
gh issue edit 52 --milestone "Phase 1: 闭环修复"
gh issue edit 53 --milestone "Phase 1: 闭环修复"
gh issue edit 54 --milestone "Phase 1: 闭环修复"
gh issue edit 55 --milestone "Phase 1: 闭环修复"
gh issue edit 56 --milestone "Phase 1: 闭环修复"

# Phase 2: issues #57, #58, #59, #60, #61, #62
gh issue edit 57 --milestone "Phase 2: 核心体验"
gh issue edit 58 --milestone "Phase 2: 核心体验"
gh issue edit 59 --milestone "Phase 2: 核心体验"
gh issue edit 60 --milestone "Phase 2: 核心体验"
gh issue edit 61 --milestone "Phase 2: 核心体验"
gh issue edit 62 --milestone "Phase 2: 核心体验"

# Phase 3: issues #63, #64, #65
gh issue edit 63 --milestone "Phase 3: 桌面端"
gh issue edit 64 --milestone "Phase 3: 桌面端"
gh issue edit 65 --milestone "Phase 3: 桌面端"

# Phase 4: issues #66, #67
gh issue edit 66 --milestone "Phase 4: 测试质量"
gh issue edit 67 --milestone "Phase 4: 测试质量"

# Phase 5: issue #68
gh issue edit 68 --milestone "Phase 5: 移动端"

# Phase 6: issues #69, #70, #71, #72
gh issue edit 69 --milestone "Phase 6: 生态完善"
gh issue edit 70 --milestone "Phase 6: 生态完善"
gh issue edit 71 --milestone "Phase 6: 生态完善"
gh issue edit 72 --milestone "Phase 6: 生态完善"
```

---

### 第三步：验证分配结果

访问里程碑页面查看分配结果：
https://github.com/hongmaple0820/maple-os/milestones

每个里程碑应该显示：
- Phase 1: 5 issues
- Phase 2: 6 issues
- Phase 3: 3 issues
- Phase 4: 2 issues
- Phase 5: 1 issue
- Phase 6: 4 issues

---

### 备用方案：使用脚本自动化

如果 prefer 使用脚本，项目已提供：

```bash
# 1. 创建里程碑（需要 GITHUB_TOKEN）
export GITHUB_TOKEN="ghp_xxxxxxxxxxxx"
bash /workspace/scripts/create_milestones.sh

# 2. 分配 issues（需要 GITHUB_TOKEN）
bash /workspace/scripts/assign_issues.sh
```

---

## Issues 分布总览

| Phase | Issues 数量 | 优先级分布 |
|-------|-------------|------------|
| Phase 1 | 5 | P0: 4, P1: 1 |
| Phase 2 | 6 | P1: 6 |
| Phase 3 | 3 | P2: 2, P3: 1 |
| Phase 4 | 2 | P2: 1, P3: 1 |
| Phase 5 | 1 | P3: 1 |
| Phase 6 | 4 | P3: 4 |

---

## 下一步行动建议

1. **本周优先**: 完成 Phase 1 的 4 个 P0 issues
2. **本月目标**: 完成 Phase 1 全部 issues
3. **下月规划**: 开始 Phase 2 的 6 个 P1 issues
4. **长期计划**: 按里程碑时间表推进各阶段

---

## 相关文档

- 详细规划文档：`/workspace/ISSUES_ORGANIZATION.md`
- 创建里程碑脚本：`/workspace/scripts/create_milestones.sh`
- 分配 issues 脚本：`/workspace/scripts/assign_issues.sh`
