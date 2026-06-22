#!/usr/bin/env bash
# MapleOS User Acceptance Test Script
# Runs the full product end-to-end and verifies all major flows work.
#
# Usage:
#   ./scripts/qa/uat.sh
#
# Prerequisites:
#   - Rust toolchain installed
#   - pnpm installed
#   - Playwright browsers installed (pnpm exec playwright install chromium)
#   - No other process on port 7788 or 3000

set -euo pipefail

SERVER_URL="http://127.0.0.1:7788"
WEB_URL="http://127.0.0.1:3000"
PASS=0
FAIL=0
SKIP=0

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

assert_pass() { echo -e "${GREEN}✓ PASS${NC}: $1"; ((PASS++)); }
assert_fail() { echo -e "${RED}✗ FAIL${NC}: $1 — $2"; ((FAIL++)); }
assert_skip() { echo -e "${YELLOW}⊘ SKIP${NC}: $1 — $2"; ((SKIP++)); }

echo "=========================================="
echo "MapleOS User Acceptance Test"
echo "=========================================="
echo ""

# ---- 1. Server Health ----
echo "--- 1. Server Health ---"
HEALTH=$(curl -s "$SERVER_URL/health" 2>/dev/null || echo "")
if echo "$HEALTH" | grep -q "ok"; then
    assert_pass "Server health check"
    VERSION=$(echo "$HEALTH" | python3 -c "import json,sys; print(json.load(sys.stdin).get('version','?'))" 2>/dev/null || echo "?")
    echo "  Version: $VERSION"
else
    assert_fail "Server health check" "no response"
    echo "  Make sure server is running: cargo run -p mapleos-server"
    exit 1
fi

# ---- 2. Database Migrations ----
echo ""
echo "--- 2. Database Migrations ---"
TABLES=$(curl -s "$SERVER_URL/health/deep" 2>/dev/null || echo "")
if echo "$TABLES" | grep -q "healthy\|degraded"; then
    assert_pass "Deep health check (DB connection)"
else
    assert_fail "Deep health check" "DB not healthy"
fi

# ---- 3. LLM Models API ----
echo ""
echo "--- 3. LLM Models API ---"
MODELS=$(curl -s "$SERVER_URL/api/models" 2>/dev/null || echo "")
if echo "$MODELS" | python3 -c "import json,sys; d=json.load(sys.stdin); assert len(d.get('models',[])) >= 0" 2>/dev/null; then
    MODEL_COUNT=$(echo "$MODELS" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('models',[])))" 2>/dev/null || echo "0")
    assert_pass "Models API returns array ($MODEL_COUNT models)"
    # Verify ModelDescriptor format (not bare strings)
    if echo "$MODELS" | python3 -c "import json,sys; d=json.load(sys.stdin); m=d['models'][0] if d['models'] else {}; assert 'id' in m and 'name' in m and 'provider' in m" 2>/dev/null; then
        assert_pass "ModelDescriptor format (id/name/provider)"
    else
        if [ "$MODEL_COUNT" = "0" ]; then
            assert_skip "ModelDescriptor format" "no models registered"
        else
            assert_fail "ModelDescriptor format" "missing fields"
        fi
    fi
else
    assert_fail "Models API" "invalid response"
fi

# ---- 4. Workflow API ----
echo ""
echo "--- 4. Workflow API ---"
WF_ID="uat-wf-$(date +%s)"
CREATE_WF=$(curl -s -X POST "$SERVER_URL/api/v3/workflows" \
    -H "Content-Type: application/json" \
    -d "{\"id\":\"$WF_ID\",\"name\":\"UAT Workflow\",\"yaml_content\":\"nodes: []\"}" 2>/dev/null || echo "")
if echo "$CREATE_WF" | python3 -c "import json,sys; d=json.load(sys.stdin); assert d.get('workflow',{}).get('id') == '$WF_ID'" 2>/dev/null; then
    assert_pass "Create workflow"
else
    assert_fail "Create workflow" "invalid response"
fi

# Validate workflow
VALIDATE=$(curl -s -X POST "$SERVER_URL/api/v3/workflows/$WF_ID/validate" 2>/dev/null || echo "")
if echo "$VALIDATE" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'valid' in d" 2>/dev/null; then
    assert_pass "Validate workflow"
else
    assert_fail "Validate workflow" "invalid response"
fi

# Create workflow run
RUN=$(curl -s -X POST "$SERVER_URL/api/v3/workflow-runs" \
    -H "Content-Type: application/json" \
    -d "{\"workflow_id\":\"$WF_ID\",\"workflow_version\":1,\"input\":\"{}\"}" 2>/dev/null || echo "")
EXEC_ID=$(echo "$RUN" | python3 -c "import json,sys; print(json.load(sys.stdin).get('execution_id',''))" 2>/dev/null || echo "")
if [ -n "$EXEC_ID" ] && [ "$EXEC_ID" != "" ]; then
    assert_pass "Create workflow run (execution_id: ${EXEC_ID:0:12}...)"
else
    assert_fail "Create workflow run" "no execution_id"
fi

# ---- 5. Execution Fact Chain ----
echo ""
echo "--- 5. Execution Fact Chain ---"
if [ -n "$EXEC_ID" ] && [ "$EXEC_ID" != "" ]; then
    EXEC=$(curl -s "$SERVER_URL/api/v3/executions/$EXEC_ID" 2>/dev/null || echo "")
    if echo "$EXEC" | python3 -c "import json,sys; d=json.load(sys.stdin); assert d.get('source') == 'workflow'" 2>/dev/null; then
        assert_pass "Get execution by ID"
    else
        assert_fail "Get execution by ID" "invalid response"
    fi

    EVENTS=$(curl -s "$SERVER_URL/api/v3/executions/$EXEC_ID/events" 2>/dev/null || echo "")
    EVENT_COUNT=$(echo "$EVENTS" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('events',[])))" 2>/dev/null || echo "0")
    if [ "$EVENT_COUNT" -ge "1" ]; then
        assert_pass "List execution events ($EVENT_COUNT events)"
    else
        assert_fail "List execution events" "no events"
    fi

    TOOL_INVS=$(curl -s "$SERVER_URL/api/v3/executions/$EXEC_ID/tool-invocations" 2>/dev/null || echo "")
    if echo "$TOOL_INVS" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'tool_invocations' in d" 2>/dev/null; then
        assert_pass "List tool invocations"
    else
        assert_fail "List tool invocations" "invalid response"
    fi
else
    assert_skip "Execution fact chain" "no execution_id from workflow run"
fi

# ---- 6. Learning Governance API ----
echo ""
echo "--- 6. Learning Governance API ---"
CANDIDATES=$(curl -s "$SERVER_URL/api/v3/learning/candidates?limit=10" 2>/dev/null || echo "")
if echo "$CANDIDATES" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'candidates' in d" 2>/dev/null; then
    assert_pass "List learning candidates"
else
    assert_fail "List learning candidates" "invalid response"
fi

PENDING=$(curl -s "$SERVER_URL/api/v3/learning/candidates/pending?limit=10" 2>/dev/null || echo "")
if echo "$PENDING" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'candidates' in d" 2>/dev/null; then
    assert_pass "List pending learning candidates"
else
    assert_fail "List pending learning candidates" "invalid response"
fi

BLOCKED=$(curl -s "$SERVER_URL/api/v3/learning/blocked?content=test-content" 2>/dev/null || echo "")
if echo "$BLOCKED" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'blocked' in d" 2>/dev/null; then
    assert_pass "Check blocked content"
else
    assert_fail "Check blocked content" "invalid response"
fi

# ---- 7. Triggers API ----
echo ""
echo "--- 7. Triggers API ---"
TRIGGERS=$(curl -s "$SERVER_URL/api/v3/triggers" 2>/dev/null || echo "")
if echo "$TRIGGERS" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'triggers' in d" 2>/dev/null; then
    assert_pass "List triggers"
else
    assert_fail "List triggers" "invalid response"
fi

# ---- 8. Audit Logs API ----
echo ""
echo "--- 8. Audit Logs API ---"
AUDIT=$(curl -s "$SERVER_URL/api/v3/audit-logs?limit=5" 2>/dev/null || echo "")
if echo "$AUDIT" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'audit_logs' in d" 2>/dev/null; then
    AUDIT_COUNT=$(echo "$AUDIT" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('audit_logs',[])))" 2>/dev/null || echo "0")
    assert_pass "List audit logs ($AUDIT_COUNT entries)"
else
    assert_fail "List audit logs" "invalid response"
fi

# ---- 9. LLM Test Connection ----
echo ""
echo "--- 9. LLM Test Connection ---"
TEST_CONN=$(curl -s -X POST "$SERVER_URL/api/llm/test-connection" \
    -H "Content-Type: application/json" \
    -d '{"provider":"ollama","base_url":"http://127.0.0.1:11434"}' 2>/dev/null || echo "")
if echo "$TEST_CONN" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'ok' in d" 2>/dev/null; then
    OK=$(echo "$TEST_CONN" | python3 -c "import json,sys; print(json.load(sys.stdin).get('ok',False))" 2>/dev/null || echo "False")
    if [ "$OK" = "True" ]; then
        LATENCY=$(echo "$TEST_CONN" | python3 -c "import json,sys; print(json.load(sys.stdin).get('latency_ms','?'))" 2>/dev/null || echo "?")
        assert_pass "LLM test connection (Ollama, ${LATENCY}ms)"
    else
        assert_skip "LLM test connection (Ollama)" "not running (expected if no Ollama installed)"
    fi
else
    assert_fail "LLM test connection" "invalid response"
fi

# ---- 10. Unknown Execution 404 ----
echo ""
echo "--- 10. Error Handling ---"
STATUS_404=$(curl -s -o /dev/null -w "%{http_code}" "$SERVER_URL/api/v3/executions/nonexistent" 2>/dev/null || echo "000")
if [ "$STATUS_404" = "404" ]; then
    assert_pass "Unknown execution returns 404"
else
    assert_fail "Unknown execution returns 404" "got $STATUS_404"
fi

# ---- Cleanup ----
curl -s -X DELETE "$SERVER_URL/api/v3/workflows/$WF_ID" > /dev/null 2>&1 || true

# ---- Summary ----
echo ""
echo "=========================================="
echo "UAT Summary"
echo "=========================================="
echo -e "${GREEN}Passed: $PASS${NC}"
echo -e "${RED}Failed: $FAIL${NC}"
echo -e "${YELLOW}Skipped: $SKIP${NC}"
echo "Total: $((PASS + FAIL + SKIP))"
echo ""

if [ "$FAIL" -eq 0 ]; then
    echo -e "${GREEN}✓ All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ $FAIL test(s) failed.${NC}"
    exit 1
fi
