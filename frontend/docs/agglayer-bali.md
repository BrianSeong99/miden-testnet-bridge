# AggLayer Bali Integration

This UI has a first live AggLayer testnet slice for Cross-chain Receive from
Ethereum Sepolia into Miden testnet.

## Supported now

- Route: Sepolia to Miden.
- Action: `bridgeAsset(uint32,address,uint256,address,bool,bytes)` on the Sepolia bridge contract.
- Contract: `0x1348947e282138d8f377b467f7d9c2eb0f335d1f`.
- Destination network ID: `76`.
- Token: native Sepolia ETH.
- Wallet: WalletConnect when `NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID` is set,
  with `window.ethereum` as a local extension fallback.
- Status: proxied through `/api/agglayer/deposits`, which polls Gateway FM's Miden bridge status API.
- Public evidence: Gateway FM's Bali bridge monitor at
  `https://gateway-fm.github.io/miden-agglayer/bridge-monitor/bali/`.

The UI accepts either a Miden testnet bech32 account address, such as the
address returned by the Miden wallet extension, or the 30-hex Miden account ID
printed by `miden client new-wallet`. It maps the account ID into the 20-byte
bridge destination slot as:

```text
0x00000000<MIDEN_ACCOUNT_ID>00
```

## Cross-chain Send support

Cross-chain Send from Miden to Sepolia still needs the Miden-side bridge note
created by a wallet-native Miden client flow or an operator runner. Once Gateway
FM reports the bridge row as ready, the UI can call the backend claim plan API
and submit `claimAsset(...)` on Sepolia through the connected EVM wallet.

The Activity detail page keeps these states separate:

- Miden bridge note submitted.
- Gateway FM proof ready.
- Sepolia `claimAsset` submitted.
- Sepolia claim settled.

## Product behavior

- NEAR Intents remains visible but disabled in this UI while AggLayer and Epoch
  are the active testnet routes.
- AggLayer Cross-chain Receive is the first route that can submit a real
  Sepolia transaction.
- AggLayer activity receipts link to Etherscan, Midenscan, and the Gateway FM
  Bali monitor so the frontend can show both wallet-local state and public
  bridge evidence.
- AggLayer Cross-chain Send can submit the Sepolia `claimAsset` transaction once
  proof is ready, but should not claim full one-click support until the Miden
  side is wallet-native.
- Activity details poll bridge status and update the receipt once the bridge
  service reports a bridge event for the destination.

## Tailnet preview

Use the Homelab route when reviewing the UI on Brian's Mac Studio:

```text
https://homelab.tail477b3c.ts.net:9001/
```

The health check is:

```text
https://homelab.tail477b3c.ts.net:9001/health
```

## Constant hygiene

The upstream Bali bridge docs are still in review, and some examples have used
older network IDs. This UI follows the current bridge backend defaults for the
testnet helper. Recheck `AGGLAYER_BALI` in `src/app/lib/agglayer.ts` before a
funded testnet run.
