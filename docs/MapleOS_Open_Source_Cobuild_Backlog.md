# MapleOS Open-Source Co-build Backlog

Version: 2026-06-15
Scope: shipped baseline, remaining closure gaps, and community implementation contracts

## 1. Why this document exists

This file is the handoff note for open-source contributors.

It does not replace:
- `docs/MapleOS_Product_Closure_Roadmap.md`
- `docs/unified-implementation-plan.md`
- `docs/2026-06-04-prodoct/mapleos-v3-complete-spec.md`

Instead, it answers one practical question:

What is already real, and what still needs high-quality community implementation to close the product loop?

## 2. Current baseline already landed

The following slices are no longer just static design ideas:

| Area | Current state |
| --- | --- |
| Rust backend + web workstation | Core backend, dashboard, chat, workflow, agent, knowledge, settings, collaboration surfaces already exist |
| Unified local-mode boot path | Local mode device login can boot the web app against the local Rust server |
| Product-closure issue map | `docs/MapleOS_Product_Closure_Roadmap.md` is already aligned to GitHub issues |
| Real CI entry point | CI now runs on `pull_request` and `push` to `master` / `codex/**`, not just release tags |
| Real E2E backend bootstrap | `scripts/qa/start-e2e-backend.mjs` starts the Rust server with isolated SQLite and cargo target dirs |
| Real product-gate spec | `tests/e2e/product-gate.spec.ts` covers local mode, dashboard, agents, knowledge, settings, chat, workflows |

## 3. What is still not fully closed

These are the high-value gaps still blocking MapleOS from being a clean product loop.

### P0 closure gaps

| Workstream | Gap | What "done" means |
| --- | --- | --- |
| Product E2E gate | The new gate is real, but still a smoke path. It does not yet prove approval, learning, or full execution trace. | CI blocks PRs on a full loop: chat -> tool approval -> workflow -> artifact -> learning -> next-run effect |
| Workflow editor | Canvas is present, but editing, validation, execution, recovery, and history are not yet product-grade as one coherent flow. | A user can create, edit, validate, run, inspect, retry, and resume workflows without dropping context |
| Unified execution fact chain | Different screens still partially explain state from different sources. | Chat, workflow, task, approval, audit, and activity all resolve from one execution fact chain |
| Unified LLM settings | Settings UI exists, but provider visibility, model inheritance, test-connection flow, and image/chat separation still need hardening. | Provider config is explicit, testable, masked, and consistently used by agent/chat/image paths |
| First-run experience | Desktop and fresh local setup still require more deterministic verification. | A clean machine can follow docs and start Web or Desktop without hidden repo knowledge |

### P1 closure gaps

| Workstream | Gap | What "done" means |
| --- | --- | --- |
| Private chat runtime | Streaming exists, but tool-call rendering, context-source explanation, artifacts, and recovery states are incomplete | A private chat can explain what it used, what it produced, and how to recover on failure |
| Group collaboration | Group logic and rules exist in pieces, but the four-step group creation and trigger loop are not closed | Group -> rule -> task -> approval -> execution -> notification forms one path |
| KB / Memory / Evolver | Components exist, but next-run effectiveness and anti-pollution governance need stronger proof | Approved learning changes visibly affect later context preview and later runs |
| Real tools / MCP / plugins | Several abilities are still mock-like or only partially wired | Tools are authorized, auditable, testable, and safe to run in a real user path |

## 4. Recommended community workstreams

Contributors should not pick random UI polish first. Work should be pulled in this order.

### Track A — Product closure

1. Expand the Playwright gate from smoke to full closure
2. Finish workflow run + trace + retry + approval
3. Finish unified execution timeline across chat/workflow/task/audit

Primary issues:
- `#89`
- `#90`
- `#92`

### Track B — LLM / runtime governance

1. Harden provider settings and test-connection UX
2. Make runtime context preview trustworthy
3. Verify approved learning affects future runs

Primary issues:
- `#86`
- `#91`

### Track C — Real tools and plugin system

1. Replace partial/mock tool paths with real tool execution
2. Add permission boundaries and review states
3. Make MCP/plugin install and health visible

Primary issues:
- `#57`
- `#58`
- `#72`
- `#22`
- `#69`

### Track D — Desktop / sync / multi-endpoint

1. First-run desktop verification
2. WebDAV/CRDT sync hardening
3. CLI/mobile lightweight operator surfaces

Primary issues:
- `#85`
- `#87`
- `#70`
- `#25`
- `#68`

## 5. Rules for community contributions

To keep the product coherent, community PRs should follow these rules:

1. Do not submit page-only PRs for closure work.
2. Every closure PR must describe:
   - user entry
   - runtime path
   - persistence path
   - error path
   - validation evidence
3. If a feature is still mock, label it clearly in UI or keep it out of the main path.
4. If a PR changes workflow, chat, approval, memory, or provider settings, add or update Playwright coverage.
5. If a PR closes an issue, include one of:
   - Playwright result
   - backend test result
   - screenshot set
   - trace / execution evidence

## 6. Suggested issue labels

Use these labels consistently:

- `product-closure`
- `needs-verification`
- `area:chat`
- `area:workflow`
- `area:agents`
- `area:knowledge`
- `area:llm-settings`
- `area:plugins`
- `area:desktop`
- `area:sync`
- `priority:p0`
- `priority:p1`

## 7. Good first high-impact PRs

These are suitable for strong contributors who want to help immediately:

1. Extend `product-gate.spec.ts` to cover tool approval and workflow trace
2. Add a stable execution timeline component shared by chat/workflow/task views
3. Add LLM provider connection test UI and backend endpoint contract
4. Add visible mock/disabled markers for non-real tools
5. Add clean-machine first-run verification docs for Web and Tauri

## 8. Definition of done for open-source co-build

A product-loop PR is only considered done when all of the following are true:

- the user can start from a real entry point
- the backend writes a traceable execution fact chain
- the UI shows success and failure states
- the recovery path is visible
- at least one automated verification path exists

If any one of those is missing, the work is still partial.
