param(
    [string]$Repo = "hongmaple0820/maple-os"
)

$ErrorActionPreference = "Stop"

function Invoke-Gh {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
    & gh @Args
    if ($LASTEXITCODE -ne 0) {
        throw "gh $($Args -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Get-OpenIssues {
    $json = gh issue list --repo $Repo --state open --limit 300 --json number,title
    if ($LASTEXITCODE -ne 0) {
        throw "failed to list open issues"
    }
    return $json | ConvertFrom-Json
}

function Ensure-Label {
    param(
        [string]$Name,
        [string]$Color,
        [string]$Description
    )

    $labelsJson = gh label list --repo $Repo --limit 300 --json name
    if ($LASTEXITCODE -ne 0) {
        throw "failed to list labels"
    }

    $exists = ($labelsJson | ConvertFrom-Json | Where-Object { $_.name -eq $Name } | Select-Object -First 1)
    if ($exists) {
        Invoke-Gh label edit $Name --repo $Repo --color $Color --description $Description
        Write-Output "label updated: $Name"
    } else {
        Invoke-Gh label create $Name --repo $Repo --color $Color --description $Description
        Write-Output "label created: $Name"
    }
}

function Add-IssueLabels {
    param(
        [int]$Number,
        [string[]]$Labels
    )

    foreach ($label in $Labels) {
        Invoke-Gh issue edit $Number --repo $Repo --add-label $label
    }
}

function Ensure-Issue {
    param(
        [string]$Title,
        [string]$Body,
        [string[]]$Labels
    )

    $existing = Get-OpenIssues | Where-Object { $_.title -eq $Title } | Select-Object -First 1
    if ($existing) {
        Invoke-Gh issue edit ([string]$existing.number) --repo $Repo --body $Body
        Add-IssueLabels -Number ([int]$existing.number) -Labels $Labels
        Write-Output "issue updated: #$($existing.number) $Title"
        return
    }

    $labelArg = $Labels -join ","
    $url = gh issue create --repo $Repo --title $Title --body $Body --label $labelArg
    if ($LASTEXITCODE -ne 0) {
        throw "failed to create issue: $Title"
    }
    Write-Output "issue created: $url $Title"
}

$labels = @(
    @{ Name = "product-closure"; Color = "5319E7"; Description = "Product and business-flow closure work" },
    @{ Name = "e2e"; Color = "0E8A16"; Description = "End-to-end regression coverage" },
    @{ Name = "ux"; Color = "FBCA04"; Description = "UX and interaction quality" },
    @{ Name = "needs-verification"; Color = "D876E3"; Description = "Implemented or partially implemented, needs verification before closure" },
    @{ Name = "issue-hygiene"; Color = "C5DEF5"; Description = "Issue grooming and stale issue cleanup" }
)

foreach ($label in $labels) {
    Ensure-Label -Name $label.Name -Color $label.Color -Description $label.Description
}

$knownIssueLabels = @(
    @{ Number = 85; Labels = @("bug", "P0", "infra", "needs-verification") },
    @{ Number = 86; Labels = @("bug", "P1", "frontend", "backend", "needs-verification") },
    @{ Number = 87; Labels = @("bug", "P0", "infra", "phase-3", "needs-verification") }
)

foreach ($item in $knownIssueLabels) {
    Add-IssueLabels -Number $item.Number -Labels $item.Labels
    Write-Output "issue labelled: #$($item.Number)"
}

$docPath = "docs/MapleOS_Product_Closure_Roadmap.md"

Ensure-Issue `
    -Title "[P0-closure] E2E product gate: chat/workflow/tool approval/learning/LLM settings" `
    -Labels @("P0", "phase-4", "infra", "frontend", "backend", "product-closure", "e2e") `
    -Body @"
## User value
Prevent MapleOS core product loops from breaking silently during future delivery. Chat, workflow, tool approval, learning writeback, and unified LLM settings must be covered by regression tests.

## Current gap
- Local/manual checks exist for several loops, but there is no product-level Playwright + CI gate.
- Existing #66 and #67 cover the generic E2E/CI foundation; this issue defines the business-flow acceptance paths.

## Acceptance criteria
- Playwright fixture starts backend, frontend, and an isolated SQLite database.
- Covers tool approval: agent triggers high-risk tool_call -> approval task is created -> user approves -> agent resumes -> chat receives final reply.
- Covers chat streaming, context sources, and learning candidate visibility.
- Covers workflow run -> human approval -> artifact writeback to KB.
- Covers LLM provider save, masked key display, connection test, and agent inheritance.
- CI runs the core E2E gate on PRs and blocks failures.

## References
- #66
- #67
- $docPath
"@

Ensure-Issue `
    -Title "[P0-closure] Workflow Canvas authoring: node CRUD/edges/validation/version/run" `
    -Labels @("P0", "phase-1", "frontend", "backend", "product-closure", "ux") `
    -Body @"
## User value
Turn the workflow page into a real authoring surface that can create, edit, validate, run, and trace workflows end to end.

## Current gap
- The product can show nodes and runtime state, but authoring, saving, versioning, validation, and failure recovery are not yet one closed user path.
- Existing #17, #53, and #61 cover parts of this area; this issue defines the full product loop.

## Acceptance criteria
- Canvas supports node create, delete, move, edge linking, and parameter editing.
- Save validates node schemas and graph topology.
- Each save produces a workflow version with diff and rollback.
- Users can run directly from Canvas and enter trace view.
- Human approval nodes can pause, approve, reject, and resume runs.
- Failed nodes show reason, retry action, and dead-letter/recovery path.

## References
- #17
- #53
- #61
- $docPath
"@

Ensure-Issue `
    -Title "[P1-closure] Learning governance hardening: quality gates/rollback/pollution guard/runtime recall" `
    -Labels @("P1", "phase-2", "backend", "frontend", "product-closure") `
    -Body @"
## User value
Make self-learning improve future agent behavior without polluting long-term knowledge, memory, or prompt policy with low-quality or unauthorized content.

## Current gap
- Learning events, KB/Memory/Prompt writeback, and partial UI exist, but quality gates, rollback, and next-run recall verification are still incomplete.

## Acceptance criteria
- Agent run, session stream, workflow finish, and human approval continuation can create learning candidates.
- Candidate includes score, evidence, source execution/artifact/task, and suggested target.
- Candidate is editable before approval; rejection does not enter future context.
- Approved KB/Memory/Prompt writeback appears in the next context preview with explainable source.
- Users can revoke or disable a persisted learning item.
- Tests prove low-confidence or evidence-free candidates cannot auto-enter long-term context.

## References
- #55
- #56
- $docPath
"@

Ensure-Issue `
    -Title "[P1-closure] Unified execution fact chain: execution_events/tool_invocations/tasks/audit/activity" `
    -Labels @("P1", "phase-2", "backend", "frontend", "product-closure") `
    -Body @"
## User value
Users should see the same execution truth from Chat, Workflow, Task, and Agent surfaces instead of fragmented module-specific states.

## Current gap
- Several surfaces already write execution events, tool invocations, tasks, audit, and activity records, but UI/API paths can still interpret state separately.

## Acceptance criteria
- Every execution entry point creates a unified execution id.
- GET /api/executions/:id/events returns delta, tool_call, tool_result, node_started, node_finished, artifact, usage, done, and error events.
- Task details, Chat trace, Workflow trace, and Agent run panels render the same event chain.
- Approval approve/reject, failure retry, cancel, and resume all append to the same fact chain.
- Activity and audit become projections, not conflicting fact sources.

## References
- #18
- #52
- #53
- $docPath
"@

Ensure-Issue `
    -Title "[P1-ux] Modularize web workspace IA and complete interaction states" `
    -Labels @("P1", "phase-2", "frontend", "ux", "product-closure") `
    -Body @"
## User value
Turn the web workspace from a large mixed surface into a clear, maintainable product shell with predictable states and recovery paths.

## Current gap
- The main workspace component is too large, and product states are scattered across modules.

## Acceptance criteria
- Split Dashboard, Messages, Agents, Workflows, Tasks, Knowledge, Plugins, and Settings modules.
- Each module has loading, empty, error, success, and disabled states.
- Shared components are used for SSE parsing, context source viewer, trace panel, and approval card.
- Keyboard access works for key interactions, and disabled buttons explain why.
- Mock or unavailable capabilities are marked clearly and link to roadmap/issues instead of acting as working flows.

## References
- #52
- #53
- #61
- #62
- $docPath
"@

Ensure-Issue `
    -Title "[P1-governance] GitHub issue hygiene and stale issue verification" `
    -Labels @("P1", "infra", "issue-hygiene", "needs-verification") `
    -Body @"
## User value
Keep the roadmap, implementation state, and GitHub issues aligned so completed work is not left open and real blockers are not buried.

## Current gap
- Several old issues appear partially implemented, but they need user-path verification before closure.

## Acceptance criteria
- Every open issue has priority, phase, and area labels where applicable.
- Issues with implementation evidence but no verification carry needs-verification.
- Every closed issue includes test command, screenshot, trace id, or reproducible manual evidence.
- Duplicate issues are merged, and oversized issues are split.
- This sync script is run before each phase and $docPath is updated.

## References
- #85
- #86
- #87
- $docPath
"@

Write-Output "product closure issue sync complete for $Repo"
