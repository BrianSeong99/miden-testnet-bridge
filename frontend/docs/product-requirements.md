# Miden Bridge UI Product Requirements

## Product shape

The Bridge UI is a focused cross-chain handoff surface. It is not the wallet's
Swap surface and it is not the wallet's Earn surface.

The first screen has one job: help the user move value between Sepolia and
Miden through either Cross-chain Receive or Cross-chain Send. Everything else
lives in activity details.

## Naming rules

- Use `Cross-chain Receive` when value starts outside Miden and lands in the
  Miden wallet.
- Use `Cross-chain Send` when value starts in the Miden wallet and lands on
  another chain.
- In compact buttons, `Receive` and `Send` are acceptable because the Bridge UI
  context already implies cross-chain movement.
- Do not use technical accounting labels as product labels. They are allowed
  only when quoting protocol field names, API paths, or upstream contract docs.
- Swap and Earn are wallet features. They may use bridge providers underneath,
  but they should not become Bridge UI modes.

## Home screen

- Light theme.
- Centered bridge card.
- Two actions only: Receive and Send.
- From network/account.
- To network/account.
- Asset and amount.
- Destination address, resolving to the user's connected EVM address or Miden
  account when left blank. Do not pre-fill the input box with a demo address.
- Route selector at the top of the card.
- Small quote summary only: route type, ETA, and minimum received.
- Recent cross-chain activity list below the card.

## Routes

- NEAR Intents: testnet mock service built for this project, not the official
  NEAR Intents service.
- AggLayer: testnet service route.
- Epoch: testnet service route. Use `0xMiden/tutorials#199` and its
  `examples/bridging-app` as the current implementation reference for
  Sepolia <-> Miden Epoch flows.

## Wallet-launched flows

When the Bridge UI is opened from the Miden wallet, the wallet should pass the
active Miden account to the Bridge UI or prompt the user to connect MidenFi.

- Cross-chain Receive: Miden is the destination chain. The connected Miden
  account should preload as the destination account.
- Cross-chain Send: Miden is the source chain. The connected Miden account
  should appear as the source wallet, while the destination input resolves to
  the connected Sepolia wallet or a pasted EVM address.

The app should not pre-fill arbitrary defaults. Preloading is only valid when it
comes from a connected wallet session or explicit launch parameters.

## Wallet-level Swap and Earn

Swap and Earn are not Bridge UI modes.

Swap belongs inside the wallet's Swap surface:

- Same-chain swap can use native Miden liquidity when available.
- Cross-chain swap can use a bridge provider, currently Epoch in the v0 model.
- If the cross-chain swap result lands on Miden, wallet Activity must
  distinguish provider success from Miden note sync/consume.

Earn belongs inside the wallet's Earn surface:

- Epoch is the first known testnet route to model this.
- The current reference is `0xMiden/tutorials#199`, which adds an Epoch protocol
  intent SDK example for Sepolia <-> Miden movement plus Miden Wallet note
  consumption.
- Earn must show whether the user is entering a position, exiting one, or
  claiming proceeds.
- Earn activity details must include app proof/status, bridge proof/status, and
  recovery actions for stuck position actions.

The Bridge UI can provide infrastructure, route receipts, and activity detail
components that wallet Swap/Earn reuse, but it must not expose Swap/Earn as
first-screen actions.

## Activity details

Clicking an activity item opens the details page. The details page owns the
information that would make the home screen too heavy:

- Provider and route used.
- Estimated arrival time.
- Network fee, bridge fee, relayer fee.
- Expected received and minimum received.
- Required source and destination gas.
- Provider/testnet warning.
- Progress timeline.
- Claim and recovery actions.
- Diagnostic export.

## State machine

Every transfer remains state-machine driven:

1. Sign source transaction.
2. Wait for confirmation and finality.
3. Bridge message observed.
4. Claim available.
5. Complete.

Failed or stuck transfers must show retry, manual claim, and diagnostic export
in the activity detail page.

## Acceptance checklist

- Receive and Send are the only Bridge UI modes.
- Swap and Earn are represented as wallet-level features, not Bridge UI modes.
- Fees, ETA, route/provider, expected received, and minimum received are visible
  before or immediately after starting a transfer.
- Every async state is resumable from activity history.
- Progress, claim, and recovery are moved off the home screen and into activity
  details.
- Privacy and provider boundaries are explained in details, especially for
  testnet mock routes.
- Stuck-funds paths are explicit enough for support and user recovery.

## Integration boundary

This UI lives inside the `miden-testnet-bridge` monorepo. It uses local mock
quote and activity state until each backend/provider contract is wired. Backend
integration should keep the same state-machine model and replace mock
quote/activity functions with API calls.

Miden-specific frontend rules are captured in
[miden-frontend-integration.md](miden-frontend-integration.md). Use that note
before changing wallet adapter behavior, Miden SDK dependencies, or Miden
claim/consume UX.
