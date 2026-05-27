# Miden Bridge UI

Next.js app for the wallet-native Miden bridge transfer flow inside the
`miden-testnet-bridge` monorepo.

The current app is a frontend prototype with local mock quote and activity
state plus a first AggLayer Sepolia-to-Miden testnet submit path. It models
deposit, withdraw, route selection, claim, swap, earn, and stuck-funds recovery
before every route is wired to backend state.

## Product model

See [docs/product-requirements.md](docs/product-requirements.md).
Miden wallet integration notes live in
[docs/miden-frontend-integration.md](docs/miden-frontend-integration.md).

## Local development

```bash
npm install
npm run dev
```

For direct host preview on the Mac Studio, use a free port:

```bash
npm run dev -- --hostname 0.0.0.0 --port 3002
```

The `/api/bridge/*` proxy defaults to `http://127.0.0.1:8080` for host
development. In Compose, `BRIDGE_API_BASE` points it at the `bridge` service.

## Validation

```bash
npm run typecheck
npm run lint
npm run build
```

## Docker

```bash
# from the monorepo root
docker compose build lab-ui
docker compose up -d lab-ui
docker compose exec lab-ui node -e "fetch('http://127.0.0.1:3000/health').then(r => r.text()).then(console.log)"
```

The monorepo Compose service is `lab-ui`; when `compose.local.yml` is layered in,
it joins the external `homelab` Docker network and does not need a host port.

## AggLayer testnet path

See [docs/agglayer-bali.md](docs/agglayer-bali.md) for the current Sepolia to Miden testnet integration boundary.
