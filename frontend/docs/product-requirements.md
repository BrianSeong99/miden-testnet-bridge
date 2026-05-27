# Miden Bridge UI Product Requirements

## Product shape

The bridge UI is a wallet-native transfer flow. The home screen should feel closer to a simple wallet swap or Uniswap transfer card than a dashboard.

The first screen has one job: let the user deposit into Miden or withdraw from Miden. Everything else lives in activity details.

## Home screen

- Light theme.
- Centered bridge card.
- Two actions only: Deposit and Withdraw.
- From network/account.
- To network/account.
- Asset and amount.
- Destination address, resolving to the user's connected EVM address or Miden
  account when left blank. Do not pre-fill the input box with a default address.
- Route selector at the top of the card.
- Small quote summary only: route type, ETA, and minimum received.
- Recent activity list below the card.

## Routes

- NEAR Intents: testnet mock service built for this project, not the official NEAR Intents service.
- AggLayer: testnet service route.
- Epoch: testnet service route. Use `0xMiden/tutorials#199` and its
  `examples/bridging-app` as the current implementation reference for
  Sepolia <-> Miden Epoch flows.

## Wallet-launched flows

When the bridge UI is opened from the Miden wallet, the wallet should pass the
active Miden account to the bridge UI or prompt the user to connect MidenFi.

- Receive / Deposit: Miden is the destination chain. The connected Miden account
  should preload as the destination account.
- Send / Withdraw: Miden is the source chain. The connected Miden account should
  appear as the source wallet, while the destination input resolves to the
  connected Sepolia wallet or a pasted EVM address.
- Swap: if the swap ends on Miden, preload the Miden account as destination; if
  the swap starts on Miden, preload it as source.
- Earn: if the earn route deposits into an Epoch or app position from Miden,
  preload the Miden account as source. If it receives yield or exits into Miden,
  preload the Miden account as destination.

The app should not pre-fill random defaults. Preloading is only valid when it
comes from a connected wallet session or explicit launch parameters.

## Swap flow

Swap is not a third top-level tab yet. It is a route intent that can sit behind
Deposit or Withdraw once a provider supports asset conversion.

Minimum swap requirements:

- Source asset and destination asset can differ.
- Quote shows provider, expected received, minimum received, swap fee, bridge
  fee, network fee, and ETA before signing.
- Activity details separate source transaction, bridge finality, swap execution,
  claim, and completion.
- If the swap result lands on Miden, the UI must distinguish provider success
  from Miden note sync/consume.

## Earn flow

Earn is an app/provider-specific route, not a generic bridge action. Epoch is
the first known testnet route to model this. The current reference is
`0xMiden/tutorials#199`, which adds an Epoch protocol intent SDK example for
Sepolia <-> Miden bridging plus Miden Wallet note consumption.

Minimum earn requirements:

- The route must show the app/provider, source account, destination position or
  receipt, fees, expected settlement, and exit/claim path.
- The bridge UI must make clear whether funds are moving into an app position,
  withdrawing from one, or claiming proceeds.
- Earn activity details must include app proof/status, bridge proof/status, and
  recovery actions for stuck deposits or exits.
- Earn should not reuse generic bridge copy when the user is actually entering
  or exiting a position.

## Activity details

Clicking an activity item opens the details page. The details page owns the information that would make the home screen too heavy:

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

Failed or stuck transfers must show retry, manual claim, and diagnostic export in the activity detail page.

## Acceptance checklist

- Deposit and withdraw are the only top-level modes.
- Fees, ETA, route/provider, expected received, and minimum received are visible before or immediately after starting a transfer.
- Every async state is resumable from activity history.
- Progress, claim, and recovery are moved off the home screen and into activity details.
- Privacy and provider boundaries are explained in details, especially for testnet mock routes.
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
