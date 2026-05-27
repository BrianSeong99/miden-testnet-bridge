# Miden Frontend Integration

> Testnet only: this note captures what this bridge UI must know before it
> embeds Miden wallet behavior directly. It is grounded in the current
> `0xMiden/agentic-template`, `0xMiden/frontend-template`, and this repo's
> Sepolia plus public Miden testnet bridge shape.

Source references:

- `0xMiden/agentic-template`: https://github.com/0xMiden/agentic-template
- `0xMiden/frontend-template`: https://github.com/0xMiden/frontend-template
- Provider setup:
  https://github.com/0xMiden/frontend-template/blob/main/src/providers.tsx
- Wallet button and readiness handling:
  https://github.com/0xMiden/frontend-template/blob/main/src/components/AppContent.tsx
- Wallet-submitted transaction pattern:
  https://github.com/0xMiden/frontend-template/blob/main/src/hooks/useIncrementCounter.ts

## Review Findings

1. The old repo wording that implied there is no browser Miden wallet provider
   is stale. Current Miden frontend examples use the MidenFi wallet adapter.
2. The lab UI still accepts a pasted Miden testnet account ID. That is an
   implementation boundary in this repo, not a Miden platform limitation.
3. A wallet-native bridge UI should connect two wallets:
   - an EVM wallet for Sepolia signing and balances,
   - a MidenFi wallet for Miden account selection, Miden signing, and
     wallet-side note consumption.
4. Do not add Miden SDK calls to the Next app casually. Once the UI imports the
   Miden Web or React SDK, it also needs the Miden WASM loading constraints,
   serialized client access, and browser headers called out below.
5. Bridge status and Miden wallet balance are different states. A quote can be
   `SUCCESS` while the recipient still needs to sync and consume a public note.

## Current Repo Boundary

The current Next lab UI has these dependencies:

```text
next
react
react-dom
lucide-react
```

It does not currently install:

```text
@miden-sdk/react
@miden-sdk/miden-sdk
@miden-sdk/miden-wallet-adapter-react
@miden-sdk/miden-wallet-adapter-base
```

Because of that, the Miden account field in the current UI is a manual testnet
account-id input. That is acceptable for the mock lab, but production-style
wallet flows should source the Miden account and Miden transaction submission
from the MidenFi wallet adapter instead of a pasted field.

## Target Wallet-Native Setup

Use the MidenFi signer provider outside the Miden React provider:

```tsx
import { MidenProvider } from "@miden-sdk/react";
import { MidenFiSignerProvider } from "@miden-sdk/miden-wallet-adapter-react";
import { WalletAdapterNetwork } from "@miden-sdk/miden-wallet-adapter-base";

<MidenFiSignerProvider
  appName={APP_NAME}
  network={WalletAdapterNetwork.Testnet}
  autoConnect
>
  <MidenProvider
    config={{ rpcUrl: MIDEN_RPC_URL, prover: MIDEN_PROVER }}
    loadingComponent={<div>Loading Miden...</div>}
  >
    <App />
  </MidenProvider>
</MidenFiSignerProvider>;
```

The provider order matters because `MidenProvider` reads signer context while it
initializes its external-keystore client.

Use `useMidenFiWallet()` for wallet UI state instead of a generic signer hook.
The wallet-specific hook exposes readiness, connection state, address,
`connect`, `disconnect`, and `requestTransaction`. Gate connect behavior on the
adapter readiness state so the app does not send users into install fallback
flows when the extension is still being detected.

## Bridge Flow Mapping

### Receive From Sepolia To Miden

1. Connect Sepolia wallet.
2. Connect MidenFi wallet and read the active Miden testnet account.
3. Request a quote with the Miden account as `recipient`.
4. Let the Sepolia wallet submit the source transaction.
5. Submit the Sepolia tx hash through `/v0/deposit/submit`.
6. Track provider processing in the Bridge UI.
7. After bridge `SUCCESS`, show that a Miden payout note exists.
8. Prompt the Miden wallet to sync and consume the public P2ID note when needed.

The key UI copy: bridge completion is not always the same as wallet balance
settlement. The recipient balance updates after sync and note consumption.

### Send From Miden To Sepolia

1. Connect MidenFi wallet.
2. Connect Sepolia wallet or enter a Sepolia recipient.
3. Request a quote with the Sepolia recipient.
4. Build the Miden output-note transaction from the quote's bridge account,
   asset, and memo.
5. Submit the Miden transaction through `wallet.requestTransaction(...)`.
6. Track note publication, provider observation, and destination release or
   claim availability in the Bridge UI.

For the mock NEAR Intents route, the Sepolia release comes from
`SOLVER_PRIVATE_KEY`, so that solver key must be funded before outbound test
runs. For AggLayer Miden -> Sepolia, completion is not automatic until a
Sepolia `claimAsset(...)` transaction lands.

## Miden Frontend Rules To Carry Over

- Prefer `@miden-sdk/react` hooks for account, note, sync, and mutation flows.
- Use `useMidenClient()` only when a needed operation is not covered by a hook.
- Serialize raw client calls with `useMiden().runExclusive(...)`.
- Wait for Miden WASM readiness before rendering components that touch the SDK.
- Represent token amounts as `bigint`, not `number`.
- Sync before reading accounts, notes, balances, or storage state.
- Public bridge notes are intentional when provider or recipient discovery is
  required. Do not switch to private notes unless a route explicitly supports
  private discovery.
- Use account-targeted tags, route-specific tags, or explicit note-tag
  subscriptions for notes that must be delivered during sync. Do not rely on
  `NoteTag(0)` for default wallet discovery.
- Treat wallet-submitted or externally consumed transactions differently from
  locally submitted transactions. Until the React SDK has a subscription
  primitive for externally driven state changes, use bounded polling for
  account or note state.
- Keep the Miden network consistent. A bech32 testnet account and a devnet
  provider config should be rejected before signing.

## Next.js / WASM Requirements

The current UI is a plain Next app with no Miden SDK imports. If the Miden SDK is
added to this Next project, the frontend must be validated for:

- WASM asset loading under Next and Turbopack,
- stable handling for `.wasm` asset URLs such as
  `/_next/static/media/miden_client_web...wasm`,
- `Cross-Origin-Opener-Policy: same-origin`,
- `Cross-Origin-Embedder-Policy: require-corp`,
- worker and SharedArrayBuffer behavior in local and HomeLab preview,
- test mocks for Miden React hooks and wallet adapter hooks,
- browser verification with the MidenFi extension installed.

Do not treat a successful static UI build as wallet integration evidence. Real
evidence requires connect, quote, sign, submit, sync, and claim/consume behavior
against public Miden testnet and Sepolia.

## Implementation Backlog

1. Add the Miden SDK and MidenFi wallet adapter to the bridge UI only when the
   Next WASM loading path is confirmed.
2. Replace the pasted Miden account field with a Miden wallet selector, while
   keeping manual entry behind a test-only advanced path.
3. Show Sepolia and Miden balances next to the `From` and `To` selectors after
   both wallets connect.
4. Use `requestTransaction(...)` for Miden-side bridge notes instead of asking
   the user to create notes out of band.
5. Add activity states for Miden note created, note consumable, note consumed,
   AggLayer claim available, claim submitted, and complete.
6. Add tests that mock both the EVM wallet provider and the MidenFi wallet
   adapter.
7. Run browser verification with real extensions before calling the wallet
   flow usable.
