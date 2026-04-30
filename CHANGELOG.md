# Changelog

## v0.1.0

- Added a local 1Click-compatible bridge surface for quote, status, tokens, deposit-submit, withdrawals, and health checks.
- Added Dockerized local infrastructure for bridge, Postgres, Anvil, bootstrap token deployment, and a bundled Miden node.
- Implemented deterministic deposit-address derivation, Miden outbound deposit accounts, and persistent chain artifacts.
- Implemented lifecycle tracking for deposit detection, confirmation, settlement, incomplete deposits, refunds, deadline expiry, and resume-on-restart behavior.
- Added local CI and end-to-end coverage for inbound, outbound, refund, incomplete-deposit, restart-resume, and hardening flows.
- Added operator docs for quickstart, cutover, incidents, architecture, and service sunset.
