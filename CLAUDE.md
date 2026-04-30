# Collaboration Guide

## Milestone

- Milestone: https://github.com/BrianSeong99/miden-testnet-bridge/milestone/1

## Working Rules

- Branch from `main` for every task branch.
- Keep the local Docker gate green with `bash scripts/ci.sh` before asking for review.
- Do not add attribution trailers, origin notes, or tool branding in commits, docs, comments, or pull requests.
- Default to minimal implementations that satisfy the active issue scope.

## Review Flow

- First pass: implementation on the task branch.
- Second pass: independent review from another model before push.
- Cross-check the issue acceptance list, local CI result, and any manual smoke-test notes before merging.

## Execution Mode

- Unsupervised mode is acceptable only for scoped repo tasks with a clear acceptance checklist.
- Stop and hand back if the task would require destructive git operations, external secrets, or scope changes not captured in the issue.
