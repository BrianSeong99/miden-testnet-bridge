# Wallet Bridge Clarity

> Testnet only: this document explains the wallet-facing bridge model for
> Sepolia, public Miden testnet, and current testnet provider integrations. It
> is written for wallet UI, wallet chat, and support agents. It is not a mainnet
> runbook.

## Core Mental Model

The wallet should describe bridge actions in wallet-native terms first and
provider terms second.

```text
Wallet action -> technical direction -> provider route
```

Use this vocabulary:

| Wallet term | Technical direction | User intent | Primary entry point |
| --- | --- | --- | --- |
| Receive from another chain | Deposit / inbound | Bring assets from Sepolia or another source chain into the Miden wallet | Receive |
| Send to another chain | Withdraw / outbound | Move assets out of the Miden wallet to Sepolia or another destination chain | Send |
| Swap | Swap or bridge-and-swap | Change asset, chain, or both | Swap, then provider route |
| Earn | App/provider-specific | Use wallet funds in an app such as Epoch | Earn |
| Claim | Completion/recovery | Finish a bridge result that is ready but not yet wallet-settled | Activity or bridge details |

Do not lead with "bridge in" or "bridge out" in end-user copy. Those terms are
useful for builders, but users understand "receive" and "send" faster.

## Provider Terms

Provider names are route labels, not wallet actions:

- `AggLayer`: testnet protocol bridge path for Sepolia <-> Miden.
- `NEAR Intents`: mock testnet 1Click-style service in this repo, not an
  official NEAR Intents production service.
- `Epoch`: testnet integration route for Epoch flows. Use
  `0xMiden/tutorials#199` and its `examples/bridging-app` as the current
  implementation reference.

For user-facing route selection, prefer mode labels in both receive and send
flows:

| Mode | Provider behavior | Product meaning |
| --- | --- | --- |
| Fast | Bridge UI automatically chooses between NEAR Intents mock and Epoch based on cheapest quote, supported asset, source/destination chain coverage, provider availability, and expected completion time | Best default for users who want the simplest route |
| Slow | AggLayer | No provider bridge fee in the current testnet path, but slower settlement and explicit claim/gas handling |

The first screen can show `Fast` and `Slow`. Bridge UI owns the quote comparison
and provider selection behind those modes. The details page should still expose
the actual provider name, quote, fees, ETA, claim steps, and support diagnostics
because performance and fees differ by route and direction.

Current implementation boundary:

- The wallet launches or embeds the Bridge UI and can pre-fill the active Miden
  account.
- The Bridge UI frontend owns quote review, route/provider selection,
  source-chain signing prompts, progress tracking, claim/recovery actions, and
  bridge detail pages.
- The monorepo Next UI uses the MidenFi wallet adapter for account connection
  and keeps pasted Miden account IDs as an explicit testnet override path. See
  `frontend/docs/miden-frontend-integration.md`.
- Future wallet-native versions may mirror these states into wallet Activity,
  but the current source of truth for bridge progress is the Bridge UI.

## Receive From Another Chain

Use this when the user starts outside Miden and wants funds inside the Miden
wallet.

Expected wallet flow:

1. User opens `Receive`.
2. User chooses `Receive from another chain`.
3. Wallet opens the bridge UI with the Miden account pre-filled.
4. User connects the source-chain wallet, such as a Sepolia wallet through
   WalletConnect, MetaMask, or another injected wallet.
5. User chooses `Fast` or `Slow` and enters amount.
6. For `Fast`, Bridge UI auto-selects NEAR Intents mock or Epoch based on
   cheapest quote, asset availability, chain coverage, provider availability,
   and expected completion time.
7. For `Slow`, Bridge UI uses AggLayer. It is the no-provider-fee route in the
   current testnet path, but the user still needs to understand settlement time,
   destination claim behavior, and gas requirements.
8. User reviews the quote, route, ETA, fees, and claim steps, then signs or
   sends on the source chain.
9. Bridge UI tracks source confirmation, provider processing, Miden note
   creation, and Miden note claim/consume.

Important state distinction:

- Bridge service `SUCCESS` can mean the Miden payout note is committed and
  consumable.
- Wallet balance updates only after the recipient wallet syncs and consumes the
  Miden note.

For the mock NEAR Intents path in this repo, the backend submits a public P2ID
payout note on Miden. The recipient still needs to claim/consume the note.

For AggLayer Sepolia -> Miden, the user must eventually sync and consume the
Miden-side claim note.

## Swap

Use this when the user wants asset conversion and chain movement in one flow.
Swap is a route intent layered behind `Receive` or `Send`, not a separate
top-level wallet action for the first screen.

Expected wallet flow:

1. User chooses receive or send.
2. User chooses a source asset and a destination asset.
3. Bridge UI selects `Fast` or `Slow` route behavior using available providers.
4. User reviews expected received, minimum received, swap fee, bridge fee,
   network fee, ETA, and provider.
5. User signs the source transaction.
6. Bridge UI tracks source confirmation, bridge finality, swap execution,
   claim availability, and completion.

If the destination is Miden, bridge/provider success only means the Miden note
or claim is available. Wallet balance updates after sync and consume.

## Earn

Use this when the route enters, exits, or claims from an app/provider position
instead of simply moving liquid assets. Epoch is the current known testnet route
for this category, with the active reference in `0xMiden/tutorials#199`.

Expected wallet flow:

1. User opens an app-specific route such as Epoch.
2. Bridge UI shows whether the action enters a position, exits a position, or
   claims proceeds.
3. User reviews app/provider, source account, destination position or receipt,
   bridge route, fees, ETA, and recovery path.
4. User signs the source wallet action.
5. Bridge UI tracks app status, bridge status, claim status, and completion.

Earn copy should not say funds are simply "bridged" when the user is entering
or exiting an app position.

The Epoch reference app covers Sepolia -> Miden, Miden -> Sepolia, and Miden
Wallet consumption of the delivered P2ID note. Preserve its important product
states in wallet copy: reclaim height for Miden -> EVM, in-flight withdraw quote
recovery after tab changes, and explicit Miden note consumption for EVM ->
Miden.

## Send To Another Chain

Use this when the user starts in Miden and wants funds on another chain.

Expected wallet flow:

1. User opens `Send`.
2. User chooses a destination chain/account, such as Sepolia.
3. User chooses `Fast` or `Slow`.
4. For `Fast`, Bridge UI auto-selects NEAR Intents mock or Epoch based on
   cheapest quote, asset availability, chain coverage, and provider availability.
5. For `Slow`, Bridge UI uses AggLayer. It is the no-provider-fee route in the
   current testnet path, but the user still needs to understand settlement time
   and who pays destination claim gas.
6. User reviews the quote, route, ETA, fees, and whether a destination claim is
   required.
7. User signs or submits the Miden-side action.
8. Bridge UI tracks Miden transaction, provider observation, finality, claim
   availability, and completion.

For the mock NEAR Intents path in this repo, outbound means:

1. `/v0/quote` returns the stable Miden bridge account plus `BridgeOutV1` memo.
2. User creates a public Miden note carrying the quoted asset and memo.
3. The bridge service consumes that note.
4. The service releases Sepolia ETH from the configured `SOLVER_PRIVATE_KEY`.

That mock solver key must be pre-funded for the destination amount plus Sepolia
gas. If it is not funded, the Miden note can be consumed while the quote remains
stuck before the Sepolia release transaction.

For AggLayer Miden -> Sepolia, outbound is not an automatic release:

1. User submits a B2AGG note from Miden using `bridge-out-tool`.
2. The bridge service eventually reports a row under `/bridges/{sepoliaAddress}`.
3. The row is claimable when `ready_for_claim=true`, `dest_net=0`, and
   `claim_tx_hash` is empty.
4. A caller fetches `/merkle-proof?deposit_cnt=...&net_id=...`.
5. A caller broadcasts `claimAsset(...)` on Sepolia.

Anyone can submit the AggLayer `claimAsset` call if they have the proof and pay
Sepolia gas. It does not have to be the destination address, but the product must
make the gas payer explicit.

Do not use `/claims/{sepoliaAddress}` as the readiness check. That endpoint is
post-claim history and may be empty while a claim is already available.

## Wallet-Launched Bridge UI

When the Miden wallet opens the Bridge UI, it should either pass the active
Miden account as launch context or prompt the user to connect MidenFi before
quote creation.

- Receive / deposit: Miden is the destination chain, so the active Miden account
  should preload as the destination account.
- Send / withdraw: Miden is the source chain, so the active Miden account should
  appear as the source wallet. The destination resolves to the connected
  Sepolia wallet or a pasted EVM address.
- Swap: preload the Miden account on whichever side of the route is Miden.
- Earn: preload the Miden account as source when entering an app position from
  Miden, or as destination when exiting/claiming into Miden.

Do not preload arbitrary demo addresses. Preloading is valid only when the value
comes from a connected wallet or explicit launch parameter.

## Claim And Recovery

Claims should be represented as activity states, not hidden implementation
details.

Minimum activity states:

| State | Meaning | User action |
| --- | --- | --- |
| Waiting for signature | Wallet or source-chain signature is needed | Sign or cancel |
| Waiting for source confirmation | Source transaction was sent | Wait |
| Waiting for provider/finality | Provider has not made destination action available | Wait, view diagnostics |
| Claim available | Destination claim can be submitted | Claim or let a relayer claim |
| Claim submitted | Claim transaction is on destination chain | Wait for confirmation |
| Complete | Destination funds are settled or Miden note consumed | None |
| Needs recovery | Flow is stuck, underfunded, expired, or rejected | Retry, claim manually, export diagnostics |

For wallet chat, the safest answer to "where are my funds?" is a state-machine
answer:

1. Identify direction: receive into Miden or send out of Miden.
2. Identify provider: AggLayer, NEAR Intents mock, or Epoch.
3. Identify current known chain event: source tx, Miden tx/note, bridge row,
   proof availability, destination tx.
4. Explain the next required action and who pays gas.

## Provider-Specific Notes

### AggLayer

- Testnet route for Sepolia <-> Miden.
- Sepolia -> Miden uses `bridgeAsset` on Sepolia and a Miden-side note claim.
- Miden -> Sepolia uses B2AGG, hourly-ish settlement cadence, then manual
  `claimAsset` on Sepolia.
- Until `0xMiden/miden-client#2173` is merged and constants settle, treat
  `bali-l2-withdraw.sh` as the withdrawal config reference.
- `gateway-fm/miden-agglayer/scripts/e2e-l2-to-l1.sh` contains local hardcoded
  values. Use it only as a reference for `claimAsset` calldata construction.

### NEAR Intents Mock

- This repo implements a mock 1Click-style testnet service.
- It is useful for wallet and builder testing, not an official NEAR Intents
  production backend.
- `/v0/*` is the stable app-facing surface:
  `GET /v0/tokens`, `POST /v0/quote`, `POST /v0/deposit/submit`,
  `GET /v0/status`.
- Outbound Sepolia releases are solver-funded from `SOLVER_PRIVATE_KEY`.

### Epoch

- Treat Epoch as a testnet provider route.
- Track `0xMiden/tutorials#199` and `examples/bridging-app` as the current
  implementation reference for Epoch Sepolia <-> Miden behavior.
- Keep Epoch-specific staking, earn, or app semantics separate from core bridge
  terminology.
- If Epoch requires a claim or second transaction, surface it through the same
  activity model instead of inventing separate wallet language.

## Wallet Chat Rules

Wallet chat should:

- Translate user language into wallet action first: receive, send, swap, earn,
  or claim.
- Ask for provider only when needed to answer accurately.
- Never say an AggLayer Miden -> Sepolia withdraw is complete just because a
  bridge row exists; it is complete only after `claimAsset` lands on Sepolia.
- Never tell users to poll `/claims` for AggLayer readiness; use `/bridges`.
- Explain that Miden balances may require note sync/consume after a bridge
  service reports success.
- Explain who pays gas before asking the user to claim.
- Keep all guidance testnet-only unless a future production document replaces
  this one.

## Suggested Architecture Direction

Build working provider flows first, then abstract.

Longer term, the wallet/client should expose a small bridge interface while
provider implementations live in separate packages. That mirrors the external
signer pattern: the wallet gets consistent activity, quote, claim, and recovery
semantics while AggLayer, NEAR Intents, Epoch, or future providers can evolve
independently.

For frontend implementation details, use
`frontend/docs/miden-frontend-integration.md` as the guardrail before changing
Miden SDK or MidenFi wallet-adapter behavior in the Next UI.
