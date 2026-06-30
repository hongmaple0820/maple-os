# CI Gate Reference

This document explains what each CI gate checks and how to run it locally
before pushing. The corresponding workflow file is
`.github/workflows/ci.yml`.

## Gates (all PRs must pass)

### 1. `rust-check`

Runs on every PR and on push to `master` / `codex/**`.

```bash
cargo check --all-targets
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

To run locally:
```bash
. "$HOME/.cargo/env"
export CARGO_TARGET_DIR=/tmp/my-project/cargo-target  # avoid filling root partition
cargo check --all-targets
cargo test --lib
cargo clippy --all-targets -- -D warnings
```

**Common failures:**
- `cargo check` errors → fix compilation errors before pushing
- `cargo test --lib` failures → run the specific failing test with
  `cargo test -p <crate> --lib <test_name>` to debug
- `clippy -D warnings` → run `cargo clippy --fix --all-targets` to
  auto-fix most warnings

### 2. `frontend-check`

```bash
pnpm install --no-frozen-lockfile
pnpm --filter=@mapleos/ui --filter=@mapleos/sdk build
pnpm --filter=mapleos-web typecheck
pnpm --filter=mapleos-web build
```

**Note:** `next.config.js` has `typescript.ignoreBuildErrors: true` so
the `build` step will NOT catch TS errors. The `typecheck` step
(`tsc --noEmit`) does. Always run `pnpm --filter=mapleos-web typecheck`
locally before pushing — it catches the type mismatches that `build`
silently ignores (see #86 for an example of a type mismatch that
crashed the UI at runtime but passed `build`).

### 3. `e2e-product-gate`

```bash
pnpm exec playwright install --with-deps chromium
pnpm exec playwright test tests/e2e/product-gate.spec.ts
```

This is the CI-blocking gate (Track 4 / T4-7). It runs all describe
blocks in `tests/e2e/product-gate.spec.ts`:

- **Dashboard smoke** — Local Mode boots, click through each module
- **LLM settings** — provider save, masked key, test connection button
- **Workflow** — create + run + execution_id returned for unified trace
- **Learning governance** — list candidates, blocked content, 404 paths
- **Execution fact chain** — unknown id returns 404
- **Chat streaming** — `test.skip` until mock LLM wired (Track 5)
- **Tool approval** — `test.skip` until mock LLM + mock tool wired (Track 5)

To run locally:
```bash
export CARGO_TARGET_DIR=/tmp/my-project/cargo-target
export CARGO_HOME=/tmp/my-project/cargo-home
export PATH="/home/z/.cargo/bin:$PATH"
pnpm test:e2e:product
```

The test backend (`scripts/qa/start-e2e-backend.mjs`) uses an isolated
SQLite DB at `.tmp/mapleos-e2e.db` and `REQUIRE_AUTH=false` so direct
HTTP API calls work without login.

### 4. `docker-build`

Builds the Docker image to verify the Dockerfile is valid. Does NOT
run tests inside the container.

## Skipping gates

You cannot skip a gate on a PR. If a gate is genuinely flaky:
1. Add `#flake` to the test name
2. File an issue with the failure log
3. Ping a maintainer to re-run

If a gate is broken by a legitimate change (e.g. you intentionally
removed a feature), update the test in the same PR.

## Branch protection (maintainer setup)

To enforce the CI gate on `master`:
1. GitHub repo → Settings → Branches → Branch protection rules
2. Add rule for `master`
3. Require status checks: `rust-check`, `frontend-check`,
   `e2e-product-gate`
4. Require branches to be up to date before merging
5. Require pull request reviews before merging (at least 1)

Once enabled, no PR can merge without all three gates passing.
