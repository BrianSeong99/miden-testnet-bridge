# Collaboration Guide

## Milestone

- Milestone: <https://github.com/BrianSeong99/miden-testnet-bridge/milestone/1>
- Docs for this branch: [README.md](README.md), [docs/RUNBOOK.md](docs/RUNBOOK.md), [docs/SUNSET.md](docs/SUNSET.md)

## Working Rules

- Branch from `main` for every task branch. Do not stack PRs on top of other in-flight PRs.
- Keep the local Docker gate green with `bash scripts/ci.sh` before asking for review and before push.
- Do not add attribution trailers, origin notes, model branding, or generator notes in commits, docs, comments, or pull requests.
- Default to minimal implementations that satisfy the active issue scope.

## Review Flow

- Draft the implementation in one pass.
- Run an independent cross-review pass before push.
- Both implementation and review passes must clear `bash scripts/ci.sh` before the branch is pushed.
- Cross-check the issue acceptance list, local CI result, and any manual smoke-test notes before merge.

## Execution Mode

- Unsupervised mode is acceptable only for scoped repo tasks with a clear acceptance checklist.
- Stop and hand back if the task would require destructive git operations, external secrets, or scope changes not captured in the issue.

## Operations

- Runbook: [docs/RUNBOOK.md](docs/RUNBOOK.md)
- Sunset and cutover: [docs/SUNSET.md](docs/SUNSET.md)
