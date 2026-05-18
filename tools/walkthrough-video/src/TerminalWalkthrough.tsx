import React from "react";
import {
  AbsoluteFill,
  Sequence,
  interpolate,
  useCurrentFrame,
} from "remotion";

type TerminalLine = {
  text: string;
  tone?: "plain" | "muted" | "success" | "warn" | "link";
};

type WalkthroughStep = {
  duration: number;
  number: string;
  eyebrow: string;
  title: string;
  explainer: string;
  command: string[];
  output: TerminalLine[];
  sideTitle: string;
  sideLines: string[];
  lifecycle?: string[];
  activeLifecycle?: number;
};

const inbound = {
  correlationId: "3e5ec16b-2fa2-4c0a-8ce7-0ae8725bbac1",
  recipient:
    "mtst1ar5xuhm7c3rctqq79qv29tvf25mzhkdn_qr7qqq9wr6w",
  recipientAccount: "0xe86e5f7ec44785801e2818a2ad8955",
  depositAddress: "0xd3BCEBe1C0de7D3Eb994eb2F0e30D5c41cD980C2",
  evmTx:
    "0xaca72ebac72cfbda3cd5957605b8e01f107c37acc9d2bfe118552b0c7cab311a",
  mintTx:
    "0x7a4c0ed23a0b13eb4f559ed2f9f82282b38b99dda2138a9b1e94759b3aefa0b6",
  claimTx:
    "0x6bf05ee2a2d9f772823abe66cf2417995ecaa71fdbe731b247694ec0f66eccfb",
};

const outbound = {
  senderAccount: "0xe52d545e2a442a8040f329f7fb05e7",
  sender:
    "mtst1arjj64z79fzz4qzq7v5l07c9uunvgtvt_qr7qqq9wr6w",
  correlationId: "ab2b3f5d-0d5e-4b86-a013-0f4ddcef05aa",
  bridgeAccount:
    "mtst1arppx7qwcm64rqzm7p6vg6vwq5gqzyq5_qr7qqq9wr6w",
  quoteHash:
    "0x437f319aab56abb302d6c0430ca4974f1b8836bd1b67a9dfeb017244730a19d9",
  noteTx:
    "0xe9db64f8a00db3527ebb1f5d443c09ce2ec80c639ccabd7e5b4b6195ea045f2d",
  consumeTx:
    "0xbaa1789bb950b97bb8300aaebc53e817760f4791ce04b5d971b85f69e4577f81",
  releaseTx:
    "0x23640d4ad68277a065fa6ec70cc26b6bc7d2acf181bf0b6669da1b03fa668885",
};

const amountWei = "1000000000000";
const lifecycle = [
  "PENDING_DEPOSIT",
  "KNOWN_DEPOSIT_TX",
  "PROCESSING",
  "SUCCESS",
];

const short = (value: string, head = 10, tail = 8) =>
  value.length > head + tail + 3
    ? `${value.slice(0, head)}...${value.slice(-tail)}`
    : value;

const steps: WalkthroughStep[] = [
  {
    duration: 300,
    number: "01",
    eyebrow: "Sepolia profile",
    title: "Start the mock 1Click bridge",
    explainer:
      "The service runs locally, but the assets in this recording are live Sepolia ETH and public Miden testnet notes. Secrets stay in .env.",
    command: [
      "cp .env.sepolia.example .env",
      "perl -0pi -e 's/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/' .env",
      "docker compose --env-file .env -f compose.yaml -f compose.sepolia.yaml up -d --build",
      "curl -s http://localhost:8080/readyz | jq '{status, evmChainId, midenRpcUrl}'",
    ],
    output: [
      { text: "{", tone: "plain" },
      { text: '  "status": "ready",', tone: "success" },
      { text: '  "evmChainId": 11155111,', tone: "plain" },
      { text: '  "midenRpcUrl": "https://rpc.testnet.miden.io"', tone: "plain" },
      { text: "}", tone: "plain" },
    ],
    sideTitle: "Actors",
    sideLines: [
      "Builder app calls the mock NEAR Intents 1Click API.",
      "User signs Sepolia transactions from their own wallet.",
      "Solver signs Miden public P2ID mints and consumes outbound notes.",
    ],
  },
  {
    duration: 390,
    number: "02",
    eyebrow: "Inbound quote",
    title: "Ask for Sepolia ETH -> Miden testnet ETH",
    explainer:
      "The app sends the same public 1Click-style quote request it would send to a NEAR Intents-compatible service.",
    command: [
      "curl -s http://localhost:8080/v0/quote \\",
      "  -H 'content-type: application/json' \\",
      "  -d '{",
      '    "originAsset": "eth-sepolia:eth",',
      '    "destinationAsset": "miden-testnet:eth",',
      `    "amount": "${amountWei}",`,
      `    "recipient": "${inbound.recipient}",`,
      '    "refundTo": "$SEPOLIA_USER_ADDRESS",',
      '    "deadline": "2027-01-01T00:00:00Z"',
      "  }' | jq '{correlationId, depositAddress: .quote.depositAddress, amountIn: .quote.amountIn, amountOut: .quote.amountOut}'",
    ],
    output: [
      { text: "{", tone: "plain" },
      { text: `  "correlationId": "${inbound.correlationId}",`, tone: "plain" },
      { text: `  "depositAddress": "${inbound.depositAddress}",`, tone: "success" },
      { text: `  "amountIn": "${amountWei}",`, tone: "plain" },
      { text: `  "amountOut": "${amountWei}"`, tone: "plain" },
      { text: "}", tone: "plain" },
    ],
    sideTitle: "What changed",
    sideLines: [
      "Bridge API reserved a Sepolia deposit address.",
      "Quote is still pending until the user actually sends ETH.",
      "Recipient is a public Miden testnet account address.",
    ],
    lifecycle,
    activeLifecycle: 0,
  },
  {
    duration: 330,
    number: "03",
    eyebrow: "User action",
    title: "User sends native Sepolia ETH",
    explainer:
      "This is not a bridge-side transfer. The user wallet sends native ETH directly to the quote deposit address.",
    command: [
      `cast send ${inbound.depositAddress} \\`,
      `  --value ${amountWei}wei \\`,
      "  --private-key $SEPOLIA_USER_PRIVATE_KEY \\",
      "  --rpc-url https://gateway.tenderly.co/public/sepolia",
    ],
    output: [
      { text: "status              1", tone: "success" },
      { text: `transactionHash     ${inbound.evmTx}`, tone: "plain" },
      {
        text: `etherscan           https://sepolia.etherscan.io/tx/${inbound.evmTx}`,
        tone: "link",
      },
    ],
    sideTitle: "User wallet",
    sideLines: [
      "Deposit is native Sepolia ETH.",
      "The bridge does not mark the quote complete from this alone.",
      "The app still needs to submit the tx hash.",
    ],
    lifecycle,
    activeLifecycle: 0,
  },
  {
    duration: 360,
    number: "04",
    eyebrow: "Deposit submit",
    title: "App submits the landed tx hash",
    explainer:
      "Sepolia mode is explicit: the app submits the transaction hash and deposit address instead of asking the bridge to scan from genesis.",
    command: [
      "curl -s http://localhost:8080/v0/deposit/submit \\",
      "  -H 'content-type: application/json' \\",
      `  -d '{"txHash":"${inbound.evmTx}","depositAddress":"${inbound.depositAddress}"}' \\`,
      "  | jq '{correlationId, status, txHash: .swapDetails.evmDepositTxHash}'",
    ],
    output: [
      { text: "{", tone: "plain" },
      { text: `  "correlationId": "${inbound.correlationId}",`, tone: "plain" },
      { text: '  "status": "KNOWN_DEPOSIT_TX",', tone: "success" },
      { text: `  "txHash": "${inbound.evmTx}"`, tone: "plain" },
      { text: "}", tone: "plain" },
    ],
    sideTitle: "Bridge verification",
    sideLines: [
      "Bridge loads the Sepolia receipt.",
      "It checks recipient deposit address and amount.",
      "It waits for the configured confirmation policy.",
    ],
    lifecycle,
    activeLifecycle: 1,
  },
  {
    duration: 390,
    number: "05",
    eyebrow: "Bridge processing",
    title: "Bridge verifies Sepolia, then mints on Miden",
    explainer:
      "After verification, the solver submits a public P2ID mint transaction on Miden testnet to the user's recipient account.",
    command: [
      `curl -s "http://localhost:8080/v0/status?depositAddress=${inbound.depositAddress}" \\`,
      "  | jq '{status, midenMintTxIds: .swapDetails.midenMintTxIds}'",
    ],
    output: [
      { text: "{", tone: "plain" },
      { text: '  "status": "PROCESSING",', tone: "warn" },
      { text: '  "midenMintTxIds": []', tone: "muted" },
      { text: "}", tone: "plain" },
      { text: "", tone: "plain" },
      { text: "# a few seconds later", tone: "muted" },
      { text: "{", tone: "plain" },
      { text: '  "status": "SUCCESS",', tone: "success" },
      { text: `  "midenMintTxIds": ["${inbound.mintTx}"]`, tone: "plain" },
      { text: "}", tone: "plain" },
    ],
    sideTitle: "Meaning of SUCCESS",
    sideLines: [
      "The public P2ID note is committed and consumable.",
      "The user still claims it on Miden to update wallet balance.",
      "This is why the claim tx is shown as a separate step.",
    ],
    lifecycle,
    activeLifecycle: 3,
  },
  {
    duration: 330,
    number: "06",
    eyebrow: "Miden claim",
    title: "User syncs and consumes the public P2ID note",
    explainer:
      "The bridge completed its side at mint. The recipient wallet claims the public note using native Miden client behavior.",
    command: [
      "# recipient wallet action, using native miden-client code in the runner",
      `RUSTFLAGS='-C debug-assertions=no' cargo run --bin sepolia_e2e 2>&1 \\`,
      `  | rg 'inbound|claim_tx_id|${short(inbound.claimTx, 14, 10)}'`,
    ],
    output: [
      { text: `recipient_account_id  ${inbound.recipientAccount}`, tone: "plain" },
      { text: `claim_tx_id           ${inbound.claimTx}`, tone: "success" },
      {
        text: `midenscan             https://testnet.midenscan.com/tx/${inbound.claimTx}`,
        tone: "link",
      },
    ],
    sideTitle: "Inbound result",
    sideLines: [
      "Sepolia tx verified.",
      "Solver minted public P2ID payout on Miden.",
      "Recipient claimed the note.",
    ],
    lifecycle,
    activeLifecycle: 3,
  },
  {
    duration: 390,
    number: "07",
    eyebrow: "Outbound quote",
    title: "Ask for Miden testnet ETH -> Sepolia ETH",
    explainer:
      "Outbound uses a stable bridge account and a BridgeOutV1 memo. The user creates a public programmable Miden note with that memo.",
    command: [
      "curl -s http://localhost:8080/v0/quote \\",
      "  -H 'content-type: application/json' \\",
      "  -d '{",
      '    "originAsset": "miden-testnet:eth",',
      '    "destinationAsset": "eth-sepolia:eth",',
      `    "amount": "${amountWei}",`,
      '    "recipient": "$SEPOLIA_USER_ADDRESS",',
      `    "refundTo": "${outbound.sender}",`,
      '    "deadline": "2027-01-01T00:00:00Z"',
      "  }' | jq '{correlationId, bridgeAccount: .quote.depositAddress, memo: .quote.depositMemo}'",
    ],
    output: [
      { text: "{", tone: "plain" },
      { text: `  "correlationId": "${outbound.correlationId}",`, tone: "plain" },
      { text: `  "bridgeAccount": "${outbound.bridgeAccount}",`, tone: "success" },
      { text: '  "memo": "{\\"version\\":\\"bridge-out-v1\\", ... }"', tone: "plain" },
      { text: `  "quoteHash": "${outbound.quoteHash}"`, tone: "plain" },
      { text: "}", tone: "plain" },
    ],
    sideTitle: "Outbound contract",
    sideLines: [
      "Deposit address is a Miden bridge account, not an EVM address.",
      "Memo encodes quote hash, target recipient, amount, refund, deadline.",
      "The note must be public so the bridge poller can consume it.",
    ],
    lifecycle,
    activeLifecycle: 0,
  },
  {
    duration: 360,
    number: "08",
    eyebrow: "Miden note",
    title: "User creates the public BridgeOutV1 note",
    explainer:
      "There is no bridge-only HTTP shortcut here. The builder app uses Miden client primitives to create a public note targeted at the bridge account.",
    command: [
      "# app-side native Miden action",
      "# build_bridge_out_note(sender, bridge_account, asset, depositMemo)",
      `cargo run --bin sepolia_e2e 2>&1 | rg 'bridgeOutNoteTxId|${short(outbound.noteTx, 14, 10)}'`,
    ],
    output: [
      { text: `sender_account_id     ${outbound.senderAccount}`, tone: "plain" },
      { text: `bridge_account        ${outbound.bridgeAccount}`, tone: "plain" },
      { text: `bridge_out_note_tx    ${outbound.noteTx}`, tone: "success" },
      {
        text: `midenscan             https://testnet.midenscan.com/tx/${outbound.noteTx}`,
        tone: "link",
      },
    ],
    sideTitle: "Why public notes",
    sideLines: [
      "Bridge account does not need a fresh per-quote Miden account.",
      "Poller can inspect public note metadata and storage.",
      "Conditionally consumable note carries the outbound intent.",
    ],
    lifecycle,
    activeLifecycle: 1,
  },
  {
    duration: 390,
    number: "09",
    eyebrow: "Outbound release",
    title: "Bridge consumes the note and releases Sepolia ETH",
    explainer:
      "The solver validates the BridgeOutV1 storage, consumes the public note on Miden, then sends native Sepolia ETH to the destination recipient.",
    command: [
      `curl -G http://localhost:8080/v0/status \\`,
      `  --data-urlencode "depositAddress=${outbound.bridgeAccount}" \\`,
      `  --data-urlencode "quoteHash=${outbound.quoteHash}" \\`,
      "  | jq '{status, midenConsumeTxIds: .swapDetails.midenConsumeTxIds, evmReleaseTxHashes: .swapDetails.evmReleaseTxHashes}'",
    ],
    output: [
      { text: "{", tone: "plain" },
      { text: '  "status": "SUCCESS",', tone: "success" },
      { text: `  "midenConsumeTxIds": ["${outbound.consumeTx}"],`, tone: "plain" },
      { text: `  "evmReleaseTxHashes": ["${outbound.releaseTx}"]`, tone: "plain" },
      { text: "}", tone: "plain" },
      { text: `recipient_balance_delta_wei=${amountWei}`, tone: "success" },
    ],
    sideTitle: "Outbound result",
    sideLines: [
      "Miden note consumed by bridge account.",
      "Sepolia release tx pays the EVM recipient.",
      "Observed recipient balance delta equals quoted amount.",
    ],
    lifecycle,
    activeLifecycle: 3,
  },
  {
    duration: 300,
    number: "10",
    eyebrow: "Evidence",
    title: "What to check after the run",
    explainer:
      "The terminal evidence is the source of truth: quote ids, tx hashes, lifecycle, and explorer links.",
    command: [
      "rg 'SEPOLIA_E2E_EVIDENCE|SUCCESS|balance_delta' sepolia-e2e-live.log",
    ],
    output: [
      {
        text: `inbound  correlation_id=${inbound.correlationId}`,
        tone: "plain",
      },
      {
        text: `         evm_deposit_tx_hash=${inbound.evmTx}`,
        tone: "plain",
      },
      { text: `         claim_tx_id=${inbound.claimTx}`, tone: "plain" },
      {
        text: `outbound correlation_id=${outbound.correlationId}`,
        tone: "plain",
      },
      { text: `         bridge_out_note_tx_id=${outbound.noteTx}`, tone: "plain" },
      { text: `         evm_release_tx_hash=${outbound.releaseTx}`, tone: "plain" },
      { text: `         balance_delta_wei=${amountWei}`, tone: "success" },
    ],
    sideTitle: "Explorer links",
    sideLines: [
      `Sepolia deposit: ${short(inbound.evmTx)}`,
      `Miden claim: ${short(inbound.claimTx)}`,
      `Miden outbound note: ${short(outbound.noteTx)}`,
      `Sepolia release: ${short(outbound.releaseTx)}`,
    ],
  },
];

export const terminalWalkthroughDurationFrames = steps.reduce(
  (total, step) => total + step.duration,
  0,
);

const toneColor = (tone: TerminalLine["tone"]) => {
  switch (tone) {
    case "muted":
      return "#7e877b";
    case "success":
      return "#72d58b";
    case "warn":
      return "#f4c66a";
    case "link":
      return "#74b7ff";
    default:
      return "#e9f0e5";
  }
};

const TerminalWindow: React.FC<{
  step: WalkthroughStep;
  frame: number;
}> = ({ step, frame }) => {
  const commandText = step.command.map((line, index) => {
    if (line.startsWith("  ") || line.startsWith("#")) {
      return line;
    }
    return `${index === 0 ? "$ " : "$ "}${line}`;
  }).join("\n");
  const typingStart = 34;
  const typingFrames = Math.min(150, Math.max(72, commandText.length / 3.4));
  const typedChars = Math.floor(
    interpolate(
      frame,
      [typingStart, typingStart + typingFrames],
      [0, commandText.length],
      { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
    ),
  );
  const outputStart = typingStart + typingFrames + 16;
  const visibleOutputLines = Math.floor(
    interpolate(
      frame,
      [outputStart, outputStart + step.output.length * 10],
      [0, step.output.length],
      { extrapolateLeft: "clamp", extrapolateRight: "clamp" },
    ),
  );
  const showCursor = frame < outputStart && frame % 28 < 14;

  return (
    <div style={styles.terminal}>
      <div style={styles.terminalBar}>
        <div style={styles.dotGroup}>
          <span style={{ ...styles.dot, background: "#d55f5a" }} />
          <span style={{ ...styles.dot, background: "#d8af4f" }} />
          <span style={{ ...styles.dot, background: "#5abf79" }} />
        </div>
        <span style={styles.terminalTitle}>
          brian@miden-testnet-bridge:~/miden-testnet-bridge
        </span>
      </div>
      <pre style={styles.command}>
        {commandText.slice(0, typedChars)}
        {showCursor ? "|" : ""}
      </pre>
      <div style={styles.output}>
        {step.output.slice(0, visibleOutputLines).map((line, index) => (
          <div
            key={`${line.text}-${index}`}
            style={{ ...styles.outputLine, color: toneColor(line.tone) }}
          >
            {line.text || "\u00a0"}
          </div>
        ))}
      </div>
    </div>
  );
};

const Lifecycle: React.FC<{
  items?: string[];
  active?: number;
}> = ({ items, active }) => {
  if (!items) {
    return null;
  }

  return (
    <div style={styles.lifecycle}>
      {items.map((item, index) => {
        const done = active !== undefined && index <= active;
        const current = active === index;
        return (
          <div key={item} style={styles.lifecycleRow}>
            <div
              style={{
                ...styles.lifecycleDot,
                background: done ? "#215b3b" : "#cbc6b8",
                borderColor: current ? "#ec9f3d" : done ? "#215b3b" : "#cbc6b8",
              }}
            />
            <span
              style={{
                ...styles.lifecycleText,
                color: done ? "#1d291f" : "#827c70",
                fontWeight: current ? 800 : 650,
              }}
            >
              {item}
            </span>
          </div>
        );
      })}
    </div>
  );
};

const StepScene: React.FC<{
  step: WalkthroughStep;
}> = ({ step }) => {
  const frame = useCurrentFrame();
  const sceneOpacity = interpolate(frame, [0, 16, step.duration - 18, step.duration], [0, 1, 1, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const y = interpolate(frame, [0, 26], [20, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  return (
    <AbsoluteFill
      style={{
        ...styles.scene,
        opacity: sceneOpacity,
        transform: `translateY(${y}px)`,
      }}
    >
      <div style={styles.header}>
        <div>
          <div style={styles.badge}>TESTNET ONLY / MOCK NEAR INTENTS 1CLICK</div>
          <h1 style={styles.title}>{step.title}</h1>
          <p style={styles.explainer}>{step.explainer}</p>
        </div>
        <div style={styles.stepBadge}>
          <span style={styles.stepNumber}>{step.number}</span>
          <span style={styles.stepEyebrow}>{step.eyebrow}</span>
        </div>
      </div>
      <div style={styles.body}>
        <TerminalWindow step={step} frame={frame} />
        <aside style={styles.sidePanel}>
          <h2 style={styles.sideTitle}>{step.sideTitle}</h2>
          <div style={styles.sideLines}>
            {step.sideLines.map((line) => (
              <div key={line} style={styles.sideLine}>
                {line}
              </div>
            ))}
          </div>
          <Lifecycle items={step.lifecycle} active={step.activeLifecycle} />
        </aside>
      </div>
      <div style={styles.footer}>
        Sepolia RPC: https://gateway.tenderly.co/public/sepolia
        <span style={styles.footerDot}>/</span>
        Miden RPC: https://rpc.testnet.miden.io
      </div>
    </AbsoluteFill>
  );
};

export const TerminalWalkthrough: React.FC = () => {
  let cursor = 0;

  return (
    <AbsoluteFill style={styles.root}>
      {steps.map((step) => {
        const from = cursor;
        cursor += step.duration;
        return (
          <Sequence
            key={`${step.number}-${step.title}`}
            from={from}
            durationInFrames={step.duration}
          >
            <StepScene step={step} />
          </Sequence>
        );
      })}
    </AbsoluteFill>
  );
};

const styles: Record<string, React.CSSProperties> = {
  root: {
    background: "#ece7da",
    color: "#151713",
    fontFamily:
      'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  },
  scene: {
    padding: "54px 64px 42px",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    gap: 36,
    alignItems: "flex-start",
  },
  badge: {
    display: "inline-flex",
    border: "1px solid #a33c34",
    color: "#a33c34",
    borderRadius: 999,
    padding: "8px 12px",
    fontSize: 16,
    lineHeight: 1,
    fontWeight: 850,
    letterSpacing: 0,
  },
  title: {
    margin: "18px 0 10px",
    fontSize: 58,
    lineHeight: 1.02,
    letterSpacing: 0,
    fontWeight: 880,
    maxWidth: 1120,
  },
  explainer: {
    margin: 0,
    maxWidth: 1110,
    color: "#54564e",
    fontSize: 24,
    lineHeight: 1.35,
    letterSpacing: 0,
  },
  stepBadge: {
    minWidth: 190,
    padding: "18px 18px",
    border: "1px solid #cdc6b5",
    borderRadius: 8,
    background: "#fffdf7",
  },
  stepNumber: {
    display: "block",
    fontSize: 50,
    lineHeight: 0.95,
    fontWeight: 900,
    letterSpacing: 0,
  },
  stepEyebrow: {
    display: "block",
    marginTop: 8,
    color: "#5f6359",
    fontSize: 17,
    fontWeight: 800,
    letterSpacing: 0,
  },
  body: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) 420px",
    gap: 24,
    marginTop: 34,
    minHeight: 690,
  },
  terminal: {
    overflow: "hidden",
    borderRadius: 8,
    background: "#101410",
    border: "1px solid #283228",
    boxShadow: "0 28px 90px rgba(30, 28, 21, 0.25)",
  },
  terminalBar: {
    height: 50,
    borderBottom: "1px solid #283228",
    background: "#171d17",
    display: "flex",
    alignItems: "center",
    gap: 18,
    padding: "0 18px",
  },
  dotGroup: {
    display: "flex",
    gap: 8,
  },
  dot: {
    display: "block",
    width: 13,
    height: 13,
    borderRadius: 999,
  },
  terminalTitle: {
    color: "#9aa69b",
    fontFamily:
      '"SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace',
    fontSize: 18,
    letterSpacing: 0,
  },
  command: {
    margin: 0,
    minHeight: 318,
    maxHeight: 386,
    overflow: "hidden",
    padding: "22px 28px 8px",
    color: "#e9f0e5",
    fontFamily:
      '"SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace',
    fontSize: 22,
    lineHeight: 1.28,
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
    letterSpacing: 0,
  },
  output: {
    borderTop: "1px solid #283228",
    minHeight: 254,
    padding: "18px 28px 24px",
    background: "#0c100c",
  },
  outputLine: {
    fontFamily:
      '"SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace',
    fontSize: 22,
    lineHeight: 1.32,
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
    letterSpacing: 0,
  },
  sidePanel: {
    borderRadius: 8,
    border: "1px solid #cdc6b5",
    background: "#fffdf7",
    padding: "26px 26px 24px",
    display: "flex",
    flexDirection: "column",
  },
  sideTitle: {
    margin: 0,
    fontSize: 34,
    lineHeight: 1.05,
    letterSpacing: 0,
    fontWeight: 880,
  },
  sideLines: {
    display: "grid",
    gap: 14,
    marginTop: 24,
  },
  sideLine: {
    borderLeft: "3px solid #215b3b",
    paddingLeft: 13,
    color: "#3d4038",
    fontSize: 23,
    lineHeight: 1.25,
    fontWeight: 650,
    letterSpacing: 0,
  },
  lifecycle: {
    marginTop: "auto",
    borderTop: "1px solid #ddd7c7",
    paddingTop: 22,
    display: "grid",
    gap: 12,
  },
  lifecycleRow: {
    display: "flex",
    alignItems: "center",
    gap: 12,
  },
  lifecycleDot: {
    width: 17,
    height: 17,
    borderRadius: 999,
    border: "3px solid",
  },
  lifecycleText: {
    fontSize: 18,
    lineHeight: 1,
    letterSpacing: 0,
  },
  footer: {
    marginTop: 18,
    color: "#706d62",
    fontSize: 18,
    fontWeight: 700,
    letterSpacing: 0,
  },
  footerDot: {
    padding: "0 12px",
    color: "#9a9487",
  },
};
