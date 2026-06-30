## Closes / Refs

Closes #<issue>
Refs #<issue>

## User entry

<!-- Where does the user start to trigger this change? e.g. "Chat panel → type message → Send" -->

## Runtime path

<!-- Key files / functions touched, e.g. `server/src/main.rs::chat_stream_handler` -->

## Persistence path

<!-- What tables/files get written? e.g. `execution_events`, `learning_candidates` -->

## Error path

<!-- What does the user see on failure? How do they recover? -->

## Validation evidence

<!-- At least one of the following must be checked -->
- [ ] Playwright `product-gate` E2E passes locally (`pnpm test:e2e:product`)
- [ ] Rust unit/integration tests pass (`cargo test -p <crate> --lib` or `--test <name>`)
- [ ] Manual screenshot or trace id: <paste>
- [ ] Other: <describe>

## CI gate

- [ ] `rust-check` job passes (cargo check + test + clippy)
- [ ] `frontend-check` job passes (typecheck + build)
- [ ] `e2e-product-gate` job passes (Playwright)

PRs that break any CI gate will NOT be merged. If a gate is flaky,
mark it with `#flake` and ping a maintainer.

## Mock / disabled features

<!-- If this PR ships a mock or disabled feature, mark it explicitly so
the UI can label it per docs/MapleOS_Open_Source_Cobuild_Backlog.md §5.3 -->

- [ ] This PR ships a real, end-to-end working feature
- [ ] This PR ships a mock/disabled feature (UI must label it)
