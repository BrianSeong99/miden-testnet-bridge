# AggLayer Bali Integration

This UI has a first live AggLayer testnet slice for deposits from Ethereum Sepolia into Miden testnet.

## Supported now

- Route: Sepolia to Miden.
- Action: `bridgeAsset(uint32,address,uint256,address,bool,bytes)` on the Sepolia bridge contract.
- Contract: `0x1348947e282138d8f377b467f7d9c2eb0f335d1f`.
- Destination network ID: `76`.
- Token: native Sepolia ETH.
- Wallet: browser EVM wallet via `window.ethereum`.
- Status: proxied through `/api/agglayer/deposits`, which polls Gateway FM's Miden bridge status API.

The UI accepts either a Miden testnet bech32 account address, such as the
address returned by the Miden wallet extension, or the 30-hex Miden account ID
printed by `miden client new-wallet`. It maps the account ID into the 20-byte
bridge destination slot as:

```text
0x00000000<MIDEN_ACCOUNT_ID>00
```

## Not supported yet

Withdrawals from Miden to Sepolia are not a browser-only operation yet. The current documented path uses `bridge-out-tool` from `gateway-fm/miden-agglayer`, a local Miden client store, and a later Sepolia `claimAsset` transaction. That needs a backend worker or a wallet-native Miden client integration before it should be exposed as a real one-click UI action.

## Product behavior

- NEAR Intents remains a project-owned testnet mock route.
- AggLayer deposit is the first route that can submit a real Sepolia transaction.
- AggLayer withdraw stays visible as a testnet route, but should not claim full live support until the Miden-side runner exists.
- Activity details poll bridge status and update the receipt once the bridge service reports a deposit for the destination.

## Constant hygiene

The upstream Bali bridge docs are still in review, and some examples have used
older network IDs. This UI follows the current bridge backend defaults for the
testnet helper. Recheck `AGGLAYER_BALI` in `src/app/lib/agglayer.ts` before a
funded testnet run.
