# Miden Frontend Integration

> Testnet only: this note is the bridge UI's Miden-specific implementation
> checklist. It follows the current `0xMiden/agentic-template` and
> `0xMiden/frontend-template` patterns.

Source references:

- `0xMiden/agentic-template`: https://github.com/0xMiden/agentic-template
- `0xMiden/frontend-template`: https://github.com/0xMiden/frontend-template
- Epoch bridging tutorial PR:
  https://github.com/0xMiden/tutorials/pull/199
- Provider setup:
  https://github.com/0xMiden/frontend-template/blob/main/src/providers.tsx
- Wallet button and readiness handling:
  https://github.com/0xMiden/frontend-template/blob/main/src/components/AppContent.tsx
- Wallet-submitted transaction pattern:
  https://github.com/0xMiden/frontend-template/blob/main/src/hooks/useIncrementCounter.ts

## Current State

This repo already installs the Miden SDK and MidenFi wallet adapter packages:

```text
@miden-sdk/react
@miden-sdk/miden-sdk
@miden-sdk/miden-wallet-adapter-react
@miden-sdk/miden-wallet-adapter-base
```

The app currently uses the MidenFi wallet adapter for account connection. It
does not yet use `MidenProvider` or the React SDK hooks for Miden client state,
balances, notes, or transactions. That means the UI can read a connected Miden
account, but it should not claim wallet-side note sync, note consume, or Miden
transaction submission until those flows are wired and verified.

## Provider Rules

When adding Miden React SDK hooks or raw client access, use the same provider
ordering as the frontend template:

```tsx
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
</MidenFiSignerProvider>
```

The order matters because `MidenProvider` reads signer context while it
initializes its external-keystore client.

Use `useMidenFiWallet()` for the wallet button. Gate connect behavior on
`wallet.readyState` so the app does not trigger extension install fallback while
the adapter is still detecting the wallet.

## Bridge Flow Mapping

### Wallet-Launched Bridge UI

When the Miden wallet opens this bridge UI, it should either provide the active
Miden account as launch context or prompt the user to connect MidenFi first.

- Receive / deposit: Miden is the destination, so the active Miden account is
  preloaded as the destination account.
- Send / withdraw: Miden is the source, so the active Miden account is displayed
  as the source wallet. The destination resolves to the connected Sepolia wallet
  or a pasted EVM address.
- Swap: preload the Miden account on whichever side of the route is Miden.
- Earn: preload the Miden account as the source when entering an Epoch/app
  position from Miden, or as the destination when exiting/claiming into Miden.

Do not preload placeholder accounts. Address preloading must come from a
connected wallet session or explicit launch parameters.

### Deposit Into Miden

1. Connect Sepolia wallet.
2. Connect MidenFi wallet.
3. Leave the destination input blank to use the connected Miden account, or
   paste a testnet account ID/address for an explicit override.
4. Request the route quote.
5. Submit the Sepolia transaction through the EVM wallet.
6. Track source finality, bridge message observation, Miden claim-note
   availability, and wallet-side note consumption.

Bridge completion is not identical to wallet balance settlement. The Miden
balance updates only after the wallet syncs and consumes the claim or payout
note.

### Withdraw From Miden

1. Connect MidenFi wallet.
2. Connect Sepolia wallet or enter a Sepolia recipient.
3. Build the Miden-side bridge note or route-specific transaction request from
   the quote.
4. Submit through `wallet.requestTransaction(...)`.
5. Track provider observation, settlement, claim availability, Sepolia claim tx,
   and completion.

For AggLayer, withdrawal is the slow testnet route and requires a Miden-side
runner plus a later Sepolia `claimAsset(...)` transaction. The UI should not
present it as one-click complete until that path exists.

### Swap

Swap is a provider route intent layered on top of deposit or withdraw. The UI
must show source asset, destination asset, expected received, minimum received,
swap fee, bridge fee, network fee, ETA, and the provider that owns execution.
If the swap ends on Miden, completion still requires wallet sync and note
consume when the provider creates a Miden note.

### Epoch / Earn

Earn is app-specific. Epoch is the current testnet route to track from
`0xMiden/tutorials#199`. That PR adds a runnable `examples/bridging-app` using
the Epoch protocol intent SDK for Sepolia -> Miden, Miden -> Sepolia, and Miden
Wallet consumption of the delivered P2ID note.

Carry these product constraints into the bridge UI:

- Miden -> EVM needs a reclaim height and must preserve the in-flight withdraw
  quote if the user switches tabs or returns later.
- EVM -> Miden completion means the Miden wallet can consume the delivered
  P2ID note; the UI must not collapse that into generic provider success.
- The wallet adapter provider should be shared so the Miden wallet request
  popup is not triggered repeatedly during one page load.
- Balance display must be BigInt-safe and route-specific decimals must come
  from the faucet/token metadata, not hardcoded display math.

The UI should show whether the user is entering a position, exiting one, or
claiming proceeds, and the activity detail should separate app status, bridge
status, claim status, and recovery actions.

## SDK Rules

- Prefer `@miden-sdk/react` hooks for accounts, notes, sync, and mutations.
- Use `useMidenClient()` only for operations not covered by hooks.
- Serialize raw client calls with `useMiden().runExclusive(...)`.
- Wait for Miden WASM readiness before rendering SDK-dependent components.
- Use `bigint` for token amounts.
- Sync before reading balances, notes, or account state.
- Use public notes only when provider or recipient discovery requires them.
- Use account-targeted tags, route-specific tags, or explicit subscriptions for
  notes that must be delivered during sync. Do not rely on `NoteTag(0)` for
  default wallet discovery.
- Treat wallet-submitted or externally consumed transactions differently from
  locally submitted transactions. Until a React SDK subscription primitive
  covers externally driven state changes, use bounded polling.
- Reject network mismatches before signing. A testnet bech32 account must not be
  used with a devnet provider configuration.

## Next.js Requirements

Because this project imports Miden wallet and SDK packages, keep these headers
enabled in `next.config.ts`:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Resource-Policy: same-origin
```

These are required for the Miden WASM path and SharedArrayBuffer behavior. If a
future Next or Turbopack release changes `.wasm` asset handling, verify the
browser can load files like `/_next/static/media/miden_client_web...wasm`
before adding more Miden client logic.

## Remaining Gaps

1. Real Miden balance display from wallet/client state.
2. Miden-side `requestTransaction(...)` for withdraw instead of mock activity
   creation.
3. Miden note sync and consume prompts after deposit claim-note availability.
4. AggLayer Miden -> Sepolia runner and Sepolia `claimAsset(...)` handoff.
5. Tests that mock both EVM wallet behavior and the MidenFi wallet adapter.
6. Browser verification with MidenFi installed, including connect, reconnect,
   reset, and popup-cancel recovery.
