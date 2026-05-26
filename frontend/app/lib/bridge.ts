export type DemoInfo = {
  demoEnabled: boolean;
  uiEnabled: boolean;
  runtimeProfile: string;
  nearIntentsMock: boolean;
  primaryApiFlow: string[];
};

export type Wallet = {
  accountId: string;
  address: string;
};

export type LifecycleEvent = {
  id: number;
  fromStatus?: string | null;
  toStatus: string;
  eventKind: string;
  reason?: string | null;
  metadata?: unknown;
  createdAt: string;
};

export type QuoteRequest = {
  originAsset: string;
  destinationAsset: string;
  amount: string;
  recipient: string;
  refundTo: string;
};

export type Quote = {
  depositAddress?: string | null;
  depositMemo?: string | null;
  amountIn: string;
  amountInFormatted: string;
  amountInUsd: string;
  minAmountIn: string;
  maxAmountIn?: string | null;
  amountOut: string;
  amountOutFormatted: string;
  amountOutUsd: string;
  minAmountOut: string;
  deadline?: string | null;
  timeWhenInactive?: string | null;
  timeEstimate: number;
  virtualChainRecipient?: string | null;
  virtualChainRefundRecipient?: string | null;
  customRecipientMsg?: string | null;
  refundFee?: string | null;
};

export type QuoteResponse = {
  correlationId: string;
  timestamp: string;
  signature: string;
  quoteRequest: QuoteRequest;
  quote: Quote;
};

export type QuoteRequestPayload = {
  dry: boolean;
  depositMode: "SIMPLE";
  swapType: "EXACT_INPUT";
  slippageTolerance: number;
  originAsset: string;
  depositType: "ORIGIN_CHAIN";
  destinationAsset: string;
  amount: string;
  refundTo: string;
  refundType: "ORIGIN_CHAIN";
  recipient: string;
  connectedWallets?: string[];
  recipientType: "DESTINATION_CHAIN";
  deadline: string;
  referral?: string;
};

export type SubmitDepositTxResponse = {
  correlationId: string;
  quoteResponse: QuoteResponse;
  status: string;
  updatedAt: string;
};

export type AgglayerL1DepositPlan = {
  dryRun: true;
  direction: "sepolia-to-miden";
  bridgeAddress: string;
  destinationNetwork: number;
  destinationMidenAccountId: string;
  destinationMidenAccountHex: string;
  destinationBridgeAddress: string;
  amountEth: string;
  amountWei: string;
  gasTokenAddress: string;
  forceUpdateGlobalExitRoot: boolean;
  gasLimit: number;
  calldata: string;
  statusUrl: string;
  command: string[];
  shellCommand: string;
  warnings: string[];
};

export type AgglayerL2WithdrawPlan = {
  dryRun: true;
  direction: "miden-to-sepolia";
  midenAccountId: string;
  midenAccountIdHex: string;
  ethAccountId: string;
  bridgeId: string;
  bridgeIdHex: string;
  faucetId: string;
  faucetIdHex: string;
  destinationNetwork: number;
  amount: string;
  command: string[];
  shellCommand: string;
  statusUrl: string;
  warnings: string[];
};

export type AgglayerPlan = AgglayerL1DepositPlan | AgglayerL2WithdrawPlan;

export type DemoArtifacts = {
  evmDepositTxHashes: string[];
  evmReleaseTxHashes: string[];
  midenMintTxIds: string[];
  midenConsumeTxIds: string[];
  evmRefundTxHashes: string[];
  midenRefundTxIds: string[];
  intentHashes: string[];
  nearTxHashes: string[];
  idempotencyKeys: string[];
};

export type DemoFlowSummary = {
  correlationId: string;
  direction: string;
  status: string;
  originAsset: string;
  destinationAsset: string;
  amount: string;
  updatedAt: string;
};

export type DemoFlow = {
  correlationId: string;
  direction: string;
  status: string;
  updatedAt: string;
  quoteResponse: QuoteResponse;
  lifecycle: LifecycleEvent[];
  artifacts: DemoArtifacts;
};

export const activeStatuses = new Set(["KNOWN_DEPOSIT_TX", "PENDING_DEPOSIT", "PROCESSING"]);
export const terminalStatuses = new Set(["INCOMPLETE_DEPOSIT", "SUCCESS", "REFUNDED", "FAILED"]);

export async function bridgeJson<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (init?.body && !headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }

  const response = await fetch(`/api/bridge${path}`, {
    ...init,
    headers,
    cache: "no-store",
  });
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;

  if (!response.ok) {
    throw new Error(body?.message ?? `${path} returned ${response.status}`);
  }

  return body as T;
}

export function shortId(value: string, head = 8, tail = 6) {
  if (!value || value.length <= head + tail + 3) return value;
  return `${value.slice(0, head)}...${value.slice(-tail)}`;
}

export function flowTitle(flow: Pick<DemoFlow, "direction">) {
  return flow.direction === "evm-to-miden" ? "Sepolia to Miden" : "Miden to Sepolia";
}

export function statusTone(status: string) {
  if (status === "SUCCESS") return "success";
  if (status === "FAILED" || status === "REFUNDED") return "danger";
  if (status === "INCOMPLETE_DEPOSIT") return "warning";
  if (activeStatuses.has(status)) return "active";
  return "neutral";
}

export function humanStatus(status: string) {
  return status.replaceAll("_", " ").toLowerCase();
}

export function amountLabel(flow: DemoFlow) {
  const quote = flow.quoteResponse.quote;
  if (quote.amountInFormatted) return quote.amountInFormatted;
  return quote.amountIn;
}

export function assetLabel(asset: string) {
  const [, symbol] = asset.split(":");
  return (symbol ?? asset).toUpperCase();
}
