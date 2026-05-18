# Walkthrough Video Tooling

Optional tooling for producing the terminal-first Sepolia walkthrough video in
the root README.

This folder is intentionally isolated from the bridge runtime. Normal bridge
setup, Docker Compose usage, Rust builds, and E2E tests do not install or use
these Node dependencies.

## Use

```bash
cd tools/walkthrough-video
npm ci
npm run render
```

The default render writes:

```text
out/miden-testnet-bridge-terminal-demo.mp4
```

To refresh the slide-style walkthrough assets first:

```bash
npm run record
npm run render:slides
```

Generated media is ignored by git:

```text
out/
recordings/
public/assets/
```

## Scope

The walkthrough is for the testnet-only mock NEAR Intents 1Click builder
sandbox. It shows the Sepolia `/v0/quote`, native ETH deposit,
`/v0/deposit/submit`, `/v0/status`, Miden public P2ID claim, and outbound
`BridgeOutV1` flow using the live evidence values already recorded in this
repo. It is not a production bridge walkthrough and must not be used with
mainnet funds.
