#!/bin/bash
# MapleOS Issues 组织脚本
# 创建里程碑并分配 Issues

# 需要先设置 GITHUB_TOKEN 环境变量
# export GITHUB_TOKEN="your_token_here"

REPO="hongmaple0820/maple-os"

echo "=== 创建里程碑 ==="

# Phase 1: 闭环修复 (5月完成)
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones \
  -d '{
    "title": "Phase 1: 闭环修复",
    "state": "open",
    "description": "修复核心功能闭环，包括 Chat SSE、Workflow SSE、接口对齐等关键问题",
    "due_on": "2026-05-31T23:59:59Z"
  }'

# Phase 2: 核心体验 (6月完成)
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones \
  -d '{
    "title": "Phase 2: 核心体验",
    "state": "open",
    "description": "提升用户体验，包括技能补实、Session 管理、Workflow 历史等",
    "due_on": "2026-06-30T23:59:59Z"
  }'

# Phase 3: 桌面端 (7月完成)
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones \
  -d '{
    "title": "Phase 3: 桌面端",
    "state": "open",
    "description": "Tauri 桌面端功能完善，包括菜单、通知、文件系统、自动更新",
    "due_on": "2026-07-31T23:59:59Z"
  }'

# Phase 4: 测试质量 (8月完成)
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones \
  -d '{
    "title": "Phase 4: 测试质量",
    "state": "open",
    "description": "测试覆盖率提升，包括 E2E 框架和单元测试补充",
    "due_on": "2026-08-31T23:59:59Z"
  }'

# Phase 5: 移动端 (9月完成)
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones \
  -d '{
    "title": "Phase 5: 移动端",
    "state": "open",
    "description": "React Native 移动端项目初始化",
    "due_on": "2026-09-30T23:59:59Z"
  }'

# Phase 6: 生态完善 (10月完成)
curl -X POST \
  -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones \
  -d '{
    "title": "Phase 6: 生态完善",
    "state": "open",
    "description": "生态系统完善，包括插件机制、技能补实、配置共享包等",
    "due_on": "2026-10-31T23:59:59Z"
  }'

echo "=== 里程碑创建完成 ==="

# 获取里程碑 ID (需要手动获取并设置)
# echo "请访问 https://api.github.com/repos/$REPO/milestones 获取里程碑 ID"
