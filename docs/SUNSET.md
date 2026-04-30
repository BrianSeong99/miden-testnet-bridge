# SUNSET

This bridge is temporary infrastructure with a known end date. It exists to unblock integration work until the real NEAR 1Click service is live, then consumers should move off of it during the W7 cutover window targeting 2026-06-12.

## Cutover

Consumers switch by changing a single base-URL environment variable:

- Old value: this bridge base URL, for example `http://localhost:8080`
- New value: `https://1click.chaindefuser.com`
- Do not append `/v0` to the env var value

Clients should continue requesting the same path-suffixed routes after the swap:

- `/v0/tokens`
- `/v0/quote`
- `/v0/status`

Verification:

```bash
export ONECLICK_BASE_URL="https://1click.chaindefuser.com"
curl -fsS "${ONECLICK_BASE_URL}/v0/tokens" | jq 'length'
```

## Consumer Migration Checklist

1. Find the config entry that currently points to this bridge base URL.
2. Replace only the base URL with `https://1click.chaindefuser.com`.
3. Confirm the client still appends `/v0/*` paths itself.
4. Redeploy or restart the consumer.
5. Verify `GET /v0/tokens` succeeds against the new base URL.
6. Run one full quote and status round-trip in the target environment.
7. Remove any bridge-specific localhost or homelab overrides from deployment config.
8. Treat this bridge as retired after the cutover window closes.

## Plan

- Cutover plan: <https://app.notion.com/p/34f99411cf9081eeb0c5c1c2d63127b8>
- Repo runbook: [docs/RUNBOOK.md](RUNBOOK.md)
- Top-level cutover instructions: [README.md](../README.md)
