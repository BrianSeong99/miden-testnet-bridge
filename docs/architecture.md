# Architecture

> Testnet only: this architecture describes the Sepolia and public Miden
> testnet mock bridge path. It is not a production bridge or a mainnet
> integration path.

## Component Model

```mermaid
flowchart TB
    subgraph Builder["Builder app or test agent"]
      App["App UI / script"]
      EvmWallet["Sepolia wallet"]
      MidenWallet["Miden wallet"]
    end

    subgraph Bridge["Mock NEAR Intents 1Click Bridge API"]
      Tokens["GET /v0/tokens"]
      Quote["POST /v0/quote"]
      Submit["POST /v0/deposit/submit"]
      Status["GET /v0/status"]
      Solver["Solver role"]
      Poller["Miden public-note poller"]
    end

    subgraph State["Durable state"]
      Postgres["Postgres lifecycle records"]
      MidenStore["miden-client store"]
    end

    subgraph Networks["Public testnets"]
      Sepolia["Sepolia native ETH"]
      Miden["Miden testnet"]
    end

    App --> Tokens
    App --> Quote
    App --> Submit
    App --> Status
    EvmWallet --> Sepolia
    MidenWallet --> Miden
    Quote --> Postgres
    Submit --> Postgres
    Status --> Postgres
    Solver --> Sepolia
    Solver --> Miden
    Poller --> Miden
    Poller --> Postgres
    Solver --> MidenStore
    Poller --> MidenStore
```

## Inbound: Sepolia To Miden

```mermaid
sequenceDiagram
    participant User
    participant Sepolia
    participant Bridge
    participant Postgres
    participant Miden
    participant Recipient

    User->>Bridge: POST /v0/quote eth-sepolia:eth to miden-testnet:eth
    Bridge->>Postgres: insert quote + chain_artifacts
    Bridge-->>User: quote with Sepolia depositAddress
    User->>Sepolia: send native ETH to depositAddress
    User->>Bridge: POST /v0/deposit/submit with tx hash
    Bridge->>Sepolia: verify tx recipient, value, receipt, confirmations
    Bridge->>Postgres: KNOWN_DEPOSIT_TX -> PENDING_DEPOSIT -> PROCESSING
    Bridge->>Miden: submit solver-signed public P2ID mint
    Bridge->>Postgres: persist Miden tx id and mark SUCCESS
    Recipient->>Miden: consume public P2ID note
    User->>Bridge: GET /v0/status
    Bridge-->>User: SUCCESS + tx metadata
```

## Outbound: Miden To Sepolia

```mermaid
sequenceDiagram
    participant User
    participant Bridge
    participant Postgres
    participant Miden
    participant Poller
    participant Sepolia

    User->>Bridge: POST /v0/quote miden-testnet:eth to eth-sepolia:eth
    Bridge->>Postgres: insert quote + BridgeOutV1 memo
    Bridge-->>User: stable Miden bridge account + depositMemo
    User->>Miden: create public BridgeOutV1 note
    Poller->>Miden: sync and scan public notes
    Poller->>Poller: validate bridge account, quote hash, faucet, amount
    Poller->>Miden: consume matched note with bridge account
    Poller->>Postgres: KNOWN_DEPOSIT_TX -> PENDING_DEPOSIT -> PROCESSING
    Poller->>Sepolia: release native ETH to EVM recipient
    Poller->>Postgres: persist tx ids and mark SUCCESS
    User->>Bridge: GET /v0/status
    Bridge-->>User: SUCCESS + tx metadata
```

## Anvil Fallback

The local Anvil profile follows the same logical shape with `eth-anvil:*`
assets and local EVM transactions. It is documented separately in
[`anvil/local-sandbox.md`](anvil/local-sandbox.md).
