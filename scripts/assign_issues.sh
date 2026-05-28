#!/bin/bash
# 分配 Issues 到里程碑
# 使用说明: GITHUB_TOKEN=xxx bash assign_issues.sh

REPO="hongmaple0820/maple-os"

if [ -z "$GITHUB_TOKEN" ]; then
  echo "请设置 GITHUB_TOKEN 环境变量"
  echo "export GITHUB_TOKEN=your_token_here"
  exit 1
fi

echo "=== 获取里程碑 ID ==="
MILESTONES=$(curl -s -H "Authorization: token $GITHUB_TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/repos/$REPO/milestones)

echo "当前里程碑:"
echo "$MILESTONES" | grep -E '"number"|"title"'

echo ""
echo "=== 分配 Phase 1 Issues ==="
# Phase 1: 52, 53, 54, 55, 56
for issue in 52 53 54 55 56; do
  echo "分配 Issue #$issue 到 Phase 1"
  curl -s -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/$REPO/issues/$issue \
    -d '{"milestone": 1}' > /dev/null
  sleep 0.5
done

echo "=== 分配 Phase 2 Issues ==="
# Phase 2: 57, 58, 59, 60, 61, 62
for issue in 57 58 59 60 61 62; do
  echo "分配 Issue #$issue 到 Phase 2"
  curl -s -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/$REPO/issues/$issue \
    -d '{"milestone": 2}' > /dev/null
  sleep 0.5
done

echo "=== 分配 Phase 3 Issues ==="
# Phase 3: 63, 64, 65
for issue in 63 64 65; do
  echo "分配 Issue #$issue 到 Phase 3"
  curl -s -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/$REPO/issues/$issue \
    -d '{"milestone": 3}' > /dev/null
  sleep 0.5
done

echo "=== 分配 Phase 4 Issues ==="
# Phase 4: 66, 67
for issue in 66 67; do
  echo "分配 Issue #$issue 到 Phase 4"
  curl -s -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/$REPO/issues/$issue \
    -d '{"milestone": 4}' > /dev/null
  sleep 0.5
done

echo "=== 分配 Phase 5 Issues ==="
# Phase 5: 68
for issue in 68; do
  echo "分配 Issue #$issue 到 Phase 5"
  curl -s -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/$REPO/issues/$issue \
    -d '{"milestone": 5}' > /dev/null
  sleep 0.5
done

echo "=== 分配 Phase 6 Issues ==="
# Phase 6: 69, 70, 71, 72
for issue in 69 70 71 72; do
  echo "分配 Issue #$issue 到 Phase 6"
  curl -s -X PATCH \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/repos/$REPO/issues/$issue \
    -d '{"milestone": 6}' > /dev/null
  sleep 0.5
done

echo ""
echo "=== 分配完成 ==="
echo "请访问 https://github.com/hongmaple0820/maple-os/milestones 查看结果"
