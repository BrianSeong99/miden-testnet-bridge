export type BridgeProvider = "near-intents" | "agglayer" | "epoch";
export type FlowMode = "receive" | "send";
export type ActivityStatus =
  | "signature"
  | "source_finality"
  | "message_observed"
  | "claim_available"
  | "failed"
  | "complete";

export type Quote = {
  eta: string;
  networkFee: string;
  bridgeFee: string;
  relayerFee: string;
  expectedReceived: string;
  minReceived: string;
  sourceGas: string;
  destinationGas: string;
  warning: string;
};

export type Activity = {
  id: string;
  mode: FlowMode;
  provider: BridgeProvider;
  summary: string;
  status: ActivityStatus;
  eta: string;
  amount: string;
  asset: string;
  destination?: string;
  bridgeDestinationAddress?: string;
  txHash: string;
  sourceTxHash?: string;
  destinationTxHash?: string;
  midenTxId?: string;
  depositCount?: string;
  readyForClaim?: boolean;
  sourceNetworkId?: number;
  destinationNetworkId?: number;
  updatedAt: string;
};

export const activityStorageKey = "miden.bridge.ui.activities";

export const providers: Record<
  BridgeProvider,
  {
    label: string;
    badge: string;
    route: string;
    disclosure: string;
  }
> = {
  "near-intents": {
    label: "NEAR Intents",
    badge: "Testnet mock",
    route: "Mock intent solver",
    disclosure:
      "This is a testnet mock service for this bridge prototype. It is not an official NEAR Intents service.",
  },
  agglayer: {
    label: "AggLayer",
    badge: "Testnet",
    route: "AggLayer testnet route",
    disclosure:
      "AggLayer testnet routing depends on source confirmation and the bridge message becoming available to Miden.",
  },
  epoch: {
    label: "Epoch",
    badge: "Testnet",
    route: "Epoch testnet route",
    disclosure:
      "Epoch is represented as a testnet service path. Production assumptions should be revisited when the integration contract is fixed.",
  },
};

export const modes: Record<
  FlowMode,
  {
    label: string;
    from: string;
    to: string;
    assetIn: string;
    assetOut: string;
    destinationLabel: string;
    destinationPlaceholder: string;
  }
> = {
  receive: {
    label: "Receive",
    from: "Sepolia",
    to: "Miden",
    assetIn: "ETH",
    assetOut: "Miden ETH",
    destinationLabel: "Miden account",
    destinationPlaceholder: "mcst1... or 0x account id",
  },
  send: {
    label: "Send",
    from: "Miden",
    to: "Sepolia",
    assetIn: "Miden ETH",
    assetOut: "ETH",
    destinationLabel: "Sepolia address",
    destinationPlaceholder: "0x...",
  },
};

export const timeline: Array<{ status: ActivityStatus; label: string; detail: string }> = [
  {
    status: "signature",
    label: "Sign source transaction",
    detail: "Confirm the transfer in the source wallet.",
  },
  {
    status: "source_finality",
    label: "Wait for finality",
    detail: "The source transaction needs confirmation before the route can continue.",
  },
  {
    status: "message_observed",
    label: "Bridge message observed",
    detail: "The provider has observed the message or proof.",
  },
  {
    status: "claim_available",
    label: "Claim available",
    detail: "Destination funds can be claimed or released.",
  },
  {
    status: "complete",
    label: "Complete",
    detail: "Funds are available in the destination account.",
  },
];

export const defaultEvmAddress = "0x71C7656EC7ab88b098defB751B7401B5f6d8976F";
export const defaultMidenAccount = "mcst1qpr8midenwalletpreviewaccount0000000000000";
export const explorerUrls = {
  sepolia: "https://sepolia.etherscan.io",
  miden: "https://testnet.midenscan.com",
};

export const seedActivities: Activity[] = [
  {
    id: "act-receive-001",
    mode: "receive",
    provider: "near-intents",
    summary: "Receive 100 ETH on Miden",
    status: "claim_available",
    eta: "Ready now",
    amount: "100",
    asset: "ETH",
    destination: "c0ffee000000000000000000c0ffee",
    bridgeDestinationAddress: "0x00000000c0ffee000000000000000000c0ffee00",
    txHash: "0x9fb3...72ac",
    sourceTxHash: "0x9fb3f3d6b7c0e2a947a0d9b0327d6de063a08f46d8dc6f2ced343b9a7e1772ac",
    midenTxId: "0x0490ad6902c47f34e8a1dc57a85b6c019a14cf27b0130d4ba6b9134fd08f72ac",
    updatedAt: "2 min ago",
  },
  {
    id: "act-send-002",
    mode: "send",
    provider: "agglayer",
    summary: "Send 0.25 ETH to Sepolia",
    status: "source_finality",
    eta: "8 min",
    amount: "0.25",
    asset: "ETH",
    destination: defaultEvmAddress,
    txHash: "0x52a1...b91e",
    sourceTxHash: "0x0490ad69e87c19c0c2c4b7951b87f0013c98bf5d90b7e14acbe821471ad5b91e",
    destinationTxHash: "0x52a18acb48115396081a3d4f1e7b58e45f0ff687a3af3ff6d83b947d85cdb91e",
    updatedAt: "12 min ago",
  },
  {
    id: "act-receive-003",
    mode: "receive",
    provider: "epoch",
    summary: "Receive 0.4 ETH on Miden",
    status: "failed",
    eta: "Needs retry",
    amount: "0.4",
    asset: "ETH",
    destination: "a11ce0000000000000000000a11ce0",
    bridgeDestinationAddress: "0x00000000a11ce0000000000000000000a11ce000",
    txHash: "0x31cc...e010",
    sourceTxHash: "0x31cc2df2871077df360a531ee8da8aa27a90683e37e0ff94d9ef3e3d762fe010",
    midenTxId: "0x0490ad6960b9ad9386602830e8bd7d467641dd32eb7d06263f3b0e622f64e010",
    updatedAt: "1 hr ago",
  },
];

export function shortAddress(value: string) {
  if (value.length <= 16) return value;
  return `${value.slice(0, 6)}...${value.slice(-6)}`;
}

export function quoteFor(mode: FlowMode, provider: BridgeProvider, amount: string): Quote {
  const parsedAmount = Number(amount) || 0;
  const expected = Math.max(parsedAmount * 0.999, 0);
  const routeName = providers[provider].label;
  const networkFee = provider === "agglayer" ? "Sepolia gas" : provider === "near-intents" ? "0.14 USD" : "0.18 USD";
  const bridgeFee = provider === "agglayer" ? "No provider fee" : "0.05%";
  const relayerFee = provider === "agglayer" ? "None" : "0.03 USD";

  return {
    eta: provider === "agglayer" ? (mode === "receive" ? "About 15 min" : "30-90 min") : "3-6 min",
    networkFee,
    bridgeFee,
    relayerFee,
    expectedReceived: `${expected.toLocaleString(undefined, { maximumFractionDigits: 6 })} ${modes[mode].assetOut.replace("Miden ", "")}`,
    minReceived: `${(expected * 0.995).toLocaleString(undefined, { maximumFractionDigits: 6 })} ${modes[mode].assetOut.replace("Miden ", "")}`,
    sourceGas: mode === "receive" ? "Sepolia ETH" : "Miden fee credit",
    destinationGas: mode === "receive" ? "Miden fee credit" : "Sepolia ETH",
    warning:
      provider === "near-intents"
        ? "Using a project-owned testnet mock, not the official NEAR Intents service."
        : `${routeName} is configured as a testnet route.`,
  };
}

export function statusLabel(status: ActivityStatus) {
  const labels: Record<ActivityStatus, string> = {
    signature: "Needs signature",
    source_finality: "Confirming",
    message_observed: "Message observed",
    claim_available: "Claim funds",
    failed: "Needs recovery",
    complete: "Complete",
  };
  return labels[status];
}

export function statusTone(status: ActivityStatus) {
  if (status === "complete") return "success";
  if (status === "failed") return "danger";
  if (status === "claim_available") return "warning";
  return "active";
}

export function nextStatus(activity: Activity): ActivityStatus {
  if (activity.status === "failed") return "claim_available";
  const index = timeline.findIndex((step) => step.status === activity.status);
  if (index === -1) return "signature";
  return timeline[Math.min(index + 1, timeline.length - 1)].status;
}

export function createActivity(
  mode: FlowMode,
  provider: BridgeProvider,
  amount: string,
  overrides: Partial<Activity> = {},
): Activity {
  const copy = modes[mode];
  const asset = copy.assetIn.replace("Miden ", "");
  const destination = mode === "receive" ? "Miden" : "Sepolia";

  const activity: Activity = {
    id: `act-${Date.now().toString(36)}`,
    mode,
    provider,
    summary: mode === "receive" ? `Receive ${amount || "0"} ${asset} on ${destination}` : `Send ${amount || "0"} ${asset} to ${destination}`,
    status: "signature",
    eta: provider === "agglayer" ? "8 min" : "4 min",
    amount: amount || "0",
    asset,
    txHash: "0xpreview...pending",
    sourceTxHash:
      mode === "receive"
        ? "0x9fb3f3d6b7c0e2a947a0d9b0327d6de063a08f46d8dc6f2ced343b9a7e1772ac"
        : "0x0490ad69e87c19c0c2c4b7951b87f0013c98bf5d90b7e14acbe821471ad5b91e",
    destinationTxHash:
      mode === "send"
        ? "0x52a18acb48115396081a3d4f1e7b58e45f0ff687a3af3ff6d83b947d85cdb91e"
        : undefined,
    midenTxId:
      mode === "receive"
        ? "0x0490ad6902c47f34e8a1dc57a85b6c019a14cf27b0130d4ba6b9134fd08f72ac"
        : "0x0490ad69e87c19c0c2c4b7951b87f0013c98bf5d90b7e14acbe821471ad5b91e",
    updatedAt: "Just now",
  };

  return { ...activity, ...overrides };
}

export function sourceExplorer(activity: Activity) {
  if (activity.mode === "receive") {
    return {
      label: "View on Etherscan",
      href: `${explorerUrls.sepolia}/tx/${activity.sourceTxHash ?? activity.txHash}`,
    };
  }
  return {
    label: "View on Midenscan",
    href: `${explorerUrls.miden}/txs`,
  };
}

export function destinationExplorer(activity: Activity) {
  if (activity.mode === "receive") {
    return {
      label: "View on Midenscan",
      href: `${explorerUrls.miden}/txs`,
    };
  }
  return {
    label: "View on Etherscan",
    href: `${explorerUrls.sepolia}/tx/${activity.destinationTxHash ?? activity.txHash}`,
  };
}

function normalizeActivity(activity: Activity): Activity {
  const legacyMode = activity.mode as FlowMode | "deposit" | "withdraw";
  const mode: FlowMode = legacyMode === "deposit" ? "receive" : legacyMode === "withdraw" ? "send" : legacyMode;
  const summary = activity.summary
    .replace(/^Deposit\b/, "Receive")
    .replace(/^Withdraw\b/, "Send")
    .replace(/^Receive (.+) to Miden$/, "Receive $1 on Miden");
  return { ...activity, mode, summary };
}

export function loadStoredActivities() {
  const raw = window.localStorage.getItem(activityStorageKey);
  if (!raw) return seedActivities;
  const parsed = JSON.parse(raw) as Activity[];
  return Array.isArray(parsed) && parsed.length > 0 ? parsed.map(normalizeActivity) : seedActivities;
}

export function saveActivities(activities: Activity[]) {
  window.localStorage.setItem(activityStorageKey, JSON.stringify(activities));
}
