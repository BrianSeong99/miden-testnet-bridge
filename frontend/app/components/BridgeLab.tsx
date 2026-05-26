"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowDownUp,
  ArrowRight,
  BookOpen,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  PlugZap,
  RefreshCw,
  Wallet as WalletIcon,
} from "lucide-react";
import {
  type AgglayerPlan,
  activeStatuses,
  amountLabel,
  assetLabel,
  bridgeJson,
  flowTitle,
  humanStatus,
  shortId,
  statusTone,
  type DemoFlow,
  type DemoFlowSummary,
  type DemoInfo,
  type QuoteRequestPayload,
  type QuoteResponse,
  type SubmitDepositTxResponse,
  type Wallet,
} from "../lib/bridge";
import {
  type ConnectedEvmWallet,
  connectSepoliaWallet,
  formatWalletAddress,
  injectedEthereum,
  isMidenAccountLike,
  readConnectedEvmWallet,
  sendSepoliaTransaction,
} from "../lib/wallet";

type Direction = "evm-to-miden" | "miden-to-evm";
type BridgeMode = "near-intents" | "agglayer" | "epoch";
type BridgeModeAction = "demo" | "planner" | "disabled";

type RuntimeState = {
  apiHealth: string;
  demoState: string;
  runtimeProfile: string;
};

const defaultAmount = "1000000000000";
const storageKeys = {
  inboundWallet: "miden.bridge.lab.inboundWallet",
  outboundWallet: "miden.bridge.lab.outboundWallet",
  midenAccountId: "miden.bridge.lab.midenAccountId",
};

function defaultAmountFor(mode: BridgeMode, nextDirection: Direction) {
  if (mode === "agglayer") {
    return nextDirection === "evm-to-miden" ? "0.001" : "10000";
  }
  return defaultAmount;
}

const directionCopy: Record<
  Direction,
  {
    fromChain: string;
    toChain: string;
    fromToken: string;
    toToken: string;
    title: string;
    submitLabel: string;
  }
> = {
  "evm-to-miden": {
    fromChain: "Sepolia",
    toChain: "Miden",
    fromToken: "ETH",
    toToken: "Miden ETH",
    title: "Bridge ETH to Miden",
    submitLabel: "Bridge to Miden",
  },
  "miden-to-evm": {
    fromChain: "Miden",
    toChain: "Sepolia",
    fromToken: "Miden ETH",
    toToken: "ETH",
    title: "Bridge ETH back to Sepolia",
    submitLabel: "Bridge to Sepolia",
  },
};

const bridgeModeCopy: Record<
  BridgeMode,
  {
    label: string;
    badge: string;
    title: string;
    description: string;
    action: BridgeModeAction;
    guideHref?: string;
  }
> = {
  "near-intents": {
    label: "NEAR Intents",
    badge: "Mock 1Click",
    title: "NEAR Intents mock",
    description: "Runs the current Sepolia to Miden mock 1Click flow through this lab backend.",
    action: "demo",
    guideHref: "/guides/near-intents",
  },
  agglayer: {
    label: "AggLayer",
    badge: "Dry-run helper",
    title: "AggLayer testnet",
    description: "Connects a Sepolia wallet and generates dry-run command plans from the current public Bali testnet tooling.",
    action: "planner",
    guideHref: "/guides/agglayer",
  },
  epoch: {
    label: "Epoch",
    badge: "Pending API",
    title: "Epoch bridge",
    description: "Reserved for an Epoch integration contract once its testnet API and assets are defined.",
    action: "disabled",
  },
};

function loadStorage<T>(key: string, fallback: T): T {
  if (typeof window === "undefined") return fallback;
  const raw = window.localStorage.getItem(key);
  if (!raw) return fallback;

  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function saveStorage<T>(key: string, value: T) {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(key, JSON.stringify(value));
  }
}

function bridgeQuoteRequest({
  originAsset,
  destinationAsset,
  amount,
  recipient,
  refundTo,
  connectedWallets,
}: {
  originAsset: string;
  destinationAsset: string;
  amount: string;
  recipient: string;
  refundTo: string;
  connectedWallets?: string[];
}): QuoteRequestPayload {
  return {
    dry: false,
    depositMode: "SIMPLE",
    swapType: "EXACT_INPUT",
    slippageTolerance: 100,
    originAsset,
    depositType: "ORIGIN_CHAIN",
    destinationAsset,
    amount,
    refundTo,
    refundType: "ORIGIN_CHAIN",
    recipient,
    connectedWallets,
    recipientType: "DESTINATION_CHAIN",
    deadline: "2027-01-01T00:00:00Z",
    referral: "miden-testnet-bridge-lab",
  };
}

export function BridgeLab() {
  const [runtime, setRuntime] = useState<RuntimeState>({
    apiHealth: "checking",
    demoState: "checking",
    runtimeProfile: "unknown",
  });
  const [bridgeMode, setBridgeMode] = useState<BridgeMode>("near-intents");
  const [direction, setDirection] = useState<Direction>("evm-to-miden");
  const [amount, setAmount] = useState(defaultAmount);
  const [flows, setFlows] = useState<Map<string, DemoFlow>>(new Map());
  const [evmWallet, setEvmWallet] = useState<ConnectedEvmWallet | null>(null);
  const [midenAccountId, setMidenAccountId] = useState("");
  const [inboundWallet, setInboundWallet] = useState<Wallet | null>(null);
  const [outboundWallet, setOutboundWallet] = useState<Wallet | null>(null);
  const [agglayerPlan, setAgglayerPlan] = useState<AgglayerPlan | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [lastFlowId, setLastFlowId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const config = directionCopy[direction];
  const selectedBridgeMode = bridgeModeCopy[bridgeMode];
  const mockApiEnabled = runtime.demoState === "enabled";
  const midenAccountReady = midenAccountId.trim() !== "" && isMidenAccountLike(midenAccountId);
  const walletReady = evmWallet?.isSepolia === true;

  const sortedFlows = useMemo(
    () => [...flows.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [flows],
  );
  const recentFlows = sortedFlows.slice(0, 6);
  const activeFlowCount = useMemo(
    () => sortedFlows.filter((flow) => activeStatuses.has(flow.status)).length,
    [sortedFlows],
  );

  const upsertFlow = useCallback((flow: DemoFlow) => {
    setFlows((current) => {
      const next = new Map(current);
      next.set(flow.correlationId, flow);
      return next;
    });
  }, []);

  const loadFlow = useCallback(
    async (id: string) => {
      const flow = await bridgeJson<DemoFlow>(`/demo/flows/${id}`);
      upsertFlow(flow);
      return flow;
    },
    [upsertFlow],
  );

  const refreshRuntime = useCallback(async () => {
    const next: RuntimeState = {
      apiHealth: "offline",
      demoState: "unavailable",
      runtimeProfile: "unknown",
    };

    try {
      const health = await fetch("/api/bridge/healthz", { cache: "no-store" });
      next.apiHealth = health.ok ? "ok" : `http ${health.status}`;
    } catch {
      next.apiHealth = "offline";
    }

    try {
      const info = await bridgeJson<DemoInfo>("/demo/info");
      next.demoState = info.demoEnabled ? "enabled" : "disabled";
      next.runtimeProfile = info.runtimeProfile;
    } catch {
      next.demoState = "unavailable";
    }

    setRuntime(next);
    return next;
  }, []);

  const refreshFlows = useCallback(async () => {
    const summaries = await bridgeJson<DemoFlowSummary[]>("/demo/flows");
    await Promise.all(summaries.slice(0, 12).map((flow) => loadFlow(flow.correlationId)));
  }, [loadFlow]);

  const refreshActiveFlows = useCallback(async () => {
    const activeFlows = [...flows.values()].filter((flow) => activeStatuses.has(flow.status));
    await Promise.all(activeFlows.map((flow) => loadFlow(flow.correlationId).catch(() => undefined)));
  }, [flows, loadFlow]);

  const refreshAll = useCallback(async () => {
    setBusy("refresh");
    setError(null);
    try {
      const nextRuntime = await refreshRuntime();
      if (nextRuntime.demoState === "enabled") {
        await refreshFlows();
      } else {
        setFlows(new Map());
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(null);
    }
  }, [refreshFlows, refreshRuntime]);

  const run = useCallback(async (key: string, task: () => Promise<void>) => {
    setBusy(key);
    setError(null);
    setNotice(null);
    try {
      await task();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(null);
    }
  }, []);

  const connectWallet = useCallback(async () => {
    const wallet = await connectSepoliaWallet();
    setEvmWallet(wallet);
    setAgglayerPlan((current) => (current?.direction === "miden-to-sepolia" ? null : current));
    setNotice("Sepolia wallet connected.");
    return wallet;
  }, []);

  const ensureWallet = useCallback(async () => {
    if (evmWallet?.isSepolia) return evmWallet;
    return connectWallet();
  }, [connectWallet, evmWallet]);

  useEffect(() => {
    setInboundWallet(loadStorage<Wallet | null>(storageKeys.inboundWallet, null));
    setOutboundWallet(loadStorage<Wallet | null>(storageKeys.outboundWallet, null));
    setMidenAccountId(loadStorage<string>(storageKeys.midenAccountId, ""));
    void readConnectedEvmWallet().then((wallet) => {
      if (wallet) {
        setEvmWallet(wallet);
        setAgglayerPlan((current) => (current?.direction === "miden-to-sepolia" ? null : current));
      }
    });
    void refreshRuntime().then((nextRuntime) => {
      if (nextRuntime.demoState === "enabled") {
        void refreshFlows().catch(() => undefined);
      }
    });
  }, [refreshFlows, refreshRuntime]);

  useEffect(() => {
    const provider = injectedEthereum();
    if (!provider?.on) return undefined;

    const refreshWallet = () => {
      void readConnectedEvmWallet(provider).then((wallet) => {
        setEvmWallet(wallet);
        setAgglayerPlan((current) => (current?.direction === "miden-to-sepolia" ? null : current));
      });
    };
    provider.on("accountsChanged", refreshWallet);
    provider.on("chainChanged", refreshWallet);

    return () => {
      provider.removeListener?.("accountsChanged", refreshWallet);
      provider.removeListener?.("chainChanged", refreshWallet);
    };
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (mockApiEnabled) {
        void refreshActiveFlows();
      }
    }, 3000);
    return () => window.clearInterval(timer);
  }, [mockApiEnabled, refreshActiveFlows]);

  async function startInbound() {
    await run("start-inbound", async () => {
      const wallet = await ensureWallet();
      const provider = injectedEthereum();
      const recipient = midenAccountId.trim();
      if (!provider) throw new Error("No browser wallet found.");
      if (!isMidenAccountLike(recipient)) throw new Error("Add a Miden testnet account before bridging.");

      const quote = await bridgeJson<QuoteResponse>("/v0/quote", {
        method: "POST",
        body: JSON.stringify(
          bridgeQuoteRequest({
            originAsset: "eth-sepolia:eth",
            destinationAsset: "miden-testnet:eth",
            amount,
            recipient,
            refundTo: wallet.address,
            connectedWallets: [wallet.address],
          }),
        ),
      });
      const depositAddress = quote.quote.depositAddress;
      if (!depositAddress) throw new Error("Quote did not include a Sepolia deposit address.");

      const txHash = await sendSepoliaTransaction(provider, wallet, {
        to: depositAddress,
        valueWei: amount,
      });
      const submitted = await bridgeJson<SubmitDepositTxResponse>("/v0/deposit/submit", {
        method: "POST",
        body: JSON.stringify({ txHash, depositAddress }),
      });
      const flow = await loadFlow(submitted.correlationId);
      setInboundWallet(null);
      upsertFlow(flow);
      setLastFlowId(flow.correlationId);
      setNotice("Sepolia wallet submitted the deposit. Track the Miden note in transaction details.");
    });
  }

  async function claimInbound() {
    if (!inboundWallet) return;
    await run("claim-inbound", async () => {
      await bridgeJson<unknown>("/demo/flows/inbound/claim", {
        method: "POST",
        body: JSON.stringify({ accountId: inboundWallet.accountId }),
      });
      setNotice("Claim request submitted to the Miden wallet.");
      await refreshFlows();
    });
  }

  async function fundOutbound() {
    await run("fund-outbound", async () => {
      const wallet = await ensureWallet();
      const response = await bridgeJson<{ wallet: Wallet; flow: DemoFlow }>("/demo/flows/outbound/fund", {
        method: "POST",
        body: JSON.stringify({ amount, asset: "eth", refundTo: wallet.address }),
      });
      setOutboundWallet(response.wallet);
      saveStorage(storageKeys.outboundWallet, response.wallet);
      upsertFlow(response.flow);
      setLastFlowId(response.flow.correlationId);
      setNotice("Miden wallet funded. Bridge back to Sepolia when the funding note is ready.");
    });
  }

  async function submitOutbound() {
    if (!outboundWallet) return;
    await run("submit-outbound", async () => {
      const wallet = await ensureWallet();
      const response = await bridgeJson<{ flow: DemoFlow }>("/demo/flows/outbound/submit", {
        method: "POST",
        body: JSON.stringify({
          senderAccountId: outboundWallet.accountId,
          amount,
          asset: "eth",
          recipient: wallet.address,
        }),
      });
      upsertFlow(response.flow);
      setLastFlowId(response.flow.correlationId);
      setNotice("Bridge note submitted from Miden. Watch details for the Sepolia release.");
    });
  }

  async function planAgglayer() {
    await run("plan-agglayer", async () => {
      const midenAccount = midenAccountId.trim();
      if (!isMidenAccountLike(midenAccount)) throw new Error("Add a Miden testnet account before planning.");

      if (direction === "evm-to-miden") {
        const plan = await bridgeJson<AgglayerPlan>("/agglayer/l1/deposit/plan", {
          method: "POST",
          body: JSON.stringify({ midenAccountId: midenAccount, amountEth: amount }),
        });
        setAgglayerPlan(plan);
        setNotice("AggLayer Sepolia to Miden dry-run plan generated.");
        return;
      }

      const wallet = await ensureWallet();
      const plan = await bridgeJson<AgglayerPlan>("/agglayer/l2/withdraw/plan", {
        method: "POST",
        body: JSON.stringify({
          midenAccountId: midenAccount,
          ethAccountId: wallet.address,
          midenWithdrawAmount: amount,
        }),
      });
      setAgglayerPlan(plan);
      setNotice("AggLayer Miden to Sepolia dry-run plan generated.");
    });
  }

  async function sendAgglayerDeposit() {
    if (!agglayerPlan || agglayerPlan.direction !== "sepolia-to-miden") return;
    await run("send-agglayer", async () => {
      const currentMidenAccount = midenAccountId.trim();
      if (agglayerPlan.destinationMidenAccountId !== currentMidenAccount || agglayerPlan.amountEth !== amount.trim()) {
        throw new Error("Regenerate the AggLayer plan after editing the amount or Miden account.");
      }
      const wallet = await ensureWallet();
      const provider = injectedEthereum();
      if (!provider) throw new Error("No browser wallet found.");
      const txHash = await sendSepoliaTransaction(provider, wallet, {
        to: agglayerPlan.bridgeAddress,
        valueWei: agglayerPlan.amountWei,
        data: agglayerPlan.calldata,
        gasLimit: agglayerPlan.gasLimit,
      });
      setNotice(`AggLayer Sepolia transaction submitted: ${txHash}`);
    });
  }

  async function submitPrimaryAction() {
    if (selectedBridgeMode.action === "planner") {
      if (direction === "miden-to-evm" && !walletReady) {
        await run("connect-wallet", async () => {
          await connectWallet();
        });
        return;
      }
      await planAgglayer();
      return;
    }

    if (selectedBridgeMode.action !== "demo" || !mockApiEnabled) return;

    if (!walletReady) {
      await run("connect-wallet", async () => {
        await connectWallet();
      });
      return;
    }

    if (direction === "evm-to-miden") {
      await startInbound();
      return;
    }

    if (outboundWallet) {
      await submitOutbound();
      return;
    }

    await fundOutbound();
  }

  function flipDirection() {
    setDirection((current) => {
      const next = current === "evm-to-miden" ? "miden-to-evm" : "evm-to-miden";
      setAmount(defaultAmountFor(bridgeMode, next));
      setAgglayerPlan(null);
      return next;
    });
  }

  function selectBridgeMode(next: BridgeMode) {
    setBridgeMode(next);
    setAmount(defaultAmountFor(next, direction));
    setAgglayerPlan(null);
  }

  function selectDirection(next: Direction) {
    setDirection(next);
    setAmount(defaultAmountFor(bridgeMode, next));
    setAgglayerPlan(null);
  }

  function updateMidenAccount(value: string) {
    setMidenAccountId(value);
    setAgglayerPlan(null);
    if (agglayerPlan) setNotice(null);
    saveStorage(storageKeys.midenAccountId, value);
  }

  function updateAmount(value: string) {
    setAmount(value);
    setAgglayerPlan(null);
    if (agglayerPlan) setNotice(null);
  }

  function getPrimaryLabel() {
    if (selectedBridgeMode.action === "planner") {
      if (direction === "miden-to-evm" && !walletReady) return "Connect Sepolia wallet";
      if (!midenAccountReady) return "Add Miden account";
      if (busy === "plan-agglayer") return "Planning";
      return "Generate dry-run plan";
    }
    if (selectedBridgeMode.action === "disabled") return `${selectedBridgeMode.label} unavailable`;
    if (!mockApiEnabled) return runtime.demoState === "checking" ? "Checking mock API" : "Mock API disabled";
    if (!walletReady) return "Connect Sepolia wallet";
    if (direction === "evm-to-miden" && !midenAccountReady) return "Add Miden account";
    if (busy === "start-inbound" || busy === "fund-outbound" || busy === "submit-outbound") return "Submitting";
    if (direction === "miden-to-evm" && !outboundWallet) return "Fund Miden wallet";
    return config.submitLabel;
  }

  const primaryLabel = getPrimaryLabel();

  const runtimeOk = runtime.apiHealth === "ok" && runtime.demoState === "enabled";
  const routeExecution =
    selectedBridgeMode.action === "planner"
      ? direction === "evm-to-miden"
        ? "Wallet send"
        : "Operator handoff"
      : selectedBridgeMode.action === "demo"
        ? "Mock service"
        : "Pending";
  const primaryDisabled =
    selectedBridgeMode.action === "disabled" ||
    (selectedBridgeMode.action === "demo" && !mockApiEnabled) ||
    ((selectedBridgeMode.action === "planner" || (selectedBridgeMode.action === "demo" && direction === "evm-to-miden")) &&
      !midenAccountReady) ||
    amount.trim() === "" ||
    busy !== null;

  return (
    <main className="app-shell">
      <header className="app-header">
        <Link className="brand" href="/" aria-label="Miden bridge lab home">
          <img src="/miden-logo-horizontal.svg" alt="Miden" />
          <span>Bridge</span>
        </Link>
        <div className="top-actions">
          <div className="nav-toolbar" aria-label="Bridge controls">
            <label className="nav-route-select">
              <span>Bridge</span>
              <select
                value={bridgeMode}
                onChange={(event) => selectBridgeMode(event.target.value as BridgeMode)}
                aria-label="Bridge type"
              >
                {(Object.keys(bridgeModeCopy) as BridgeMode[]).map((item) => (
                  <option key={item} value={item}>
                    {bridgeModeCopy[item].label}
                  </option>
                ))}
              </select>
              <small>{selectedBridgeMode.badge}</small>
              <ChevronDown size={15} strokeWidth={1.8} aria-hidden="true" />
            </label>
            <button
              type="button"
              className={`nav-wallet ${walletReady ? "connected" : evmWallet ? "wrong" : ""}`}
              onClick={() =>
                run("connect-wallet", async () => {
                  await connectWallet();
                })
              }
              disabled={busy !== null}
              aria-label={walletReady ? `Connected Sepolia wallet ${evmWallet.address}` : "Connect Sepolia wallet"}
            >
              <WalletIcon size={16} strokeWidth={1.8} aria-hidden="true" />
              <span>{walletReady ? formatWalletAddress(evmWallet.address) : evmWallet ? "Wrong network" : "Connect wallet"}</span>
              <small>{walletReady ? "Sepolia" : "Sepolia"}</small>
            </button>
          </div>
          <span className={`health-pill ${runtimeOk ? "tone-success" : "tone-warning"}`}>
            <span aria-hidden="true" />
            {runtimeOk ? "live" : runtime.apiHealth}
          </span>
          <button type="button" className="icon-button" onClick={refreshAll} disabled={busy === "refresh"} aria-label="Refresh">
            <RefreshCw size={17} strokeWidth={1.8} aria-hidden="true" />
          </button>
        </div>
      </header>

      <section className="bridge-stage" aria-label="Bridge form">
        <div className="bridge-card">
          <div className="bridge-heading">
            <div>
              <p className="eyebrow">{selectedBridgeMode.title}</p>
              <h1>{config.title}</h1>
            </div>
            <p className="active-count">{activeFlowCount} active</p>
          </div>

          <div className={`mode-callout ${selectedBridgeMode.action === "demo" ? "" : "pending"}`}>
            <AlertTriangle size={17} strokeWidth={1.8} aria-hidden="true" />
            <p>
              {selectedBridgeMode.action === "demo" ? (
                <>
                  This is a mock NEAR Intents service. Connect a Sepolia wallet for deposits, and provide a Miden
                  testnet account for payouts.
                </>
              ) : (
                selectedBridgeMode.description
              )}
            </p>
            {selectedBridgeMode.guideHref ? (
              <Link href={selectedBridgeMode.guideHref}>
                <BookOpen size={15} strokeWidth={1.8} aria-hidden="true" />
                <span>Guide</span>
              </Link>
            ) : null}
          </div>

          <section className="wallet-panel account-panel" aria-label="Miden account">
            <label className={`wallet-card miden-wallet ${midenAccountReady ? "connected" : midenAccountId.trim() ? "invalid" : ""}`}>
              <div>
                <span>Miden account</span>
                <strong>{midenAccountReady ? "Testnet account set" : "Paste account ID"}</strong>
                <small>{midenAccountId.trim() && !midenAccountReady ? "Use mcst1... or 30-byte account hex" : "Used for Miden recipient/source planning"}</small>
              </div>
              <input
                value={midenAccountId}
                onChange={(event) => updateMidenAccount(event.target.value)}
                aria-label="Miden account ID"
                placeholder="mcst1... or 0xc98b..."
              />
            </label>
          </section>

          <div className="direction-tabs" role="group" aria-label="Bridge direction">
            {(Object.keys(directionCopy) as Direction[]).map((item) => (
              <button
                key={item}
                type="button"
                className={direction === item ? "selected" : ""}
                aria-pressed={direction === item}
                onClick={() => selectDirection(item)}
              >
                {directionCopy[item].fromChain} to {directionCopy[item].toChain}
              </button>
            ))}
          </div>

          <div className="swap-box">
            <label className="amount-box">
              <span>From</span>
              <div className="amount-row">
                <input
                  value={amount}
                  inputMode="numeric"
                  aria-label={`Amount from ${config.fromChain}`}
                  onChange={(event) => updateAmount(event.target.value)}
                />
                <span className="token-pill">
                  <strong>{config.fromToken}</strong>
                  <small>{config.fromChain}</small>
                </span>
              </div>
            </label>

            <button type="button" className="switch-button" onClick={flipDirection} aria-label="Switch bridge direction">
              <ArrowDownUp size={18} strokeWidth={1.75} aria-hidden="true" />
            </button>

            <div className="amount-box readonly">
              <span>To</span>
              <div className="amount-row">
                <p>{amount || "0"}</p>
                <span className="token-pill">
                  <strong>{config.toToken}</strong>
                  <small>{config.toChain}</small>
                </span>
              </div>
            </div>
          </div>

          <div className="route-strip" aria-label="Selected bridge route">
            <div>
              <span>Route</span>
              <strong>{selectedBridgeMode.label}</strong>
            </div>
            <div>
              <span>Path</span>
              <strong>
                {config.fromChain} to {config.toChain}
              </strong>
            </div>
            <div>
              <span>Execution</span>
              <strong>{routeExecution}</strong>
            </div>
          </div>

          <button
            type="button"
            className="primary-action"
            onClick={submitPrimaryAction}
            disabled={primaryDisabled}
          >
            <span>{primaryLabel}</span>
            <ArrowRight size={18} strokeWidth={1.75} aria-hidden="true" />
          </button>

          {selectedBridgeMode.action === "planner" && agglayerPlan ? (
            <AgglayerPlanCard plan={agglayerPlan} onSend={sendAgglayerDeposit} busy={busy} walletReady={walletReady} />
          ) : null}

          {selectedBridgeMode.action === "demo" && mockApiEnabled && direction === "evm-to-miden" && inboundWallet ? (
            <button type="button" className="secondary-action" onClick={claimInbound} disabled={busy !== null}>
              <CheckCircle2 size={17} strokeWidth={1.75} aria-hidden="true" />
              <span>{busy === "claim-inbound" ? "Claiming" : "Claim latest Miden note"}</span>
            </button>
          ) : null}

          {notice ? (
            <div className="notice" role="status">
              <span>{notice}</span>
              {lastFlowId ? <Link href={`/flows/${lastFlowId}`}>View details</Link> : null}
            </div>
          ) : null}
          {error ? <div className="notice error" role="alert">{error}</div> : null}

          <dl className="runtime-row" aria-label="Runtime status">
            <div>
              <dt>Runtime</dt>
              <dd>{runtime.runtimeProfile}</dd>
            </div>
            <div>
              <dt>Mock API</dt>
              <dd>{runtime.demoState}</dd>
            </div>
          </dl>
        </div>

        <section className="recent-panel" aria-label="Recent transactions">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Recent</p>
              <h2>Transactions</h2>
            </div>
            <button type="button" className="text-button" onClick={refreshAll} disabled={busy === "refresh"}>
              Refresh
            </button>
          </div>

          <div className="transaction-list">
            {recentFlows.length === 0 ? (
              <div className="empty-card">
                <p>No bridge transactions yet.</p>
                <span>Start with the bridge form above.</span>
              </div>
            ) : (
              recentFlows.map((flow) => <TransactionRow key={flow.correlationId} flow={flow} />)
            )}
          </div>
        </section>
      </section>
    </main>
  );
}

function AgglayerPlanCard({
  plan,
  onSend,
  busy,
  walletReady,
}: {
  plan: AgglayerPlan;
  onSend: () => void;
  busy: string | null;
  walletReady: boolean;
}) {
  const canSend = plan.direction === "sepolia-to-miden";

  return (
    <section className="plan-card" aria-label="AggLayer dry-run plan">
      <div className="card-title-row">
        <div>
          <p className="eyebrow">Dry-run plan</p>
          <h2>{plan.direction === "sepolia-to-miden" ? "Sepolia to Miden" : "Miden to Sepolia"}</h2>
        </div>
        <a href={plan.statusUrl} target="_blank" rel="noreferrer">
          Status
        </a>
      </div>
      <pre>{plan.shellCommand}</pre>
      <p>
        {canSend
          ? "The backend returned a dry-run plan. Browser-wallet sending requires your explicit Sepolia confirmation."
          : "After the B2AGG note is submitted, poll the Status link for a ready unclaimed Miden-origin row before running claimAsset."}
      </p>
      {!canSend ? (
        <div className="plan-links">
          <a href={plan.claimsUrl} target="_blank" rel="noreferrer">
            Claims history
          </a>
          <span>{plan.readinessChecks.join(" · ")}</span>
        </div>
      ) : null}
      {canSend ? (
        <button type="button" className="secondary-action" onClick={onSend} disabled={busy !== null || !walletReady}>
          <PlugZap size={16} strokeWidth={1.8} aria-hidden="true" />
          <span>{busy === "send-agglayer" ? "Sending" : walletReady ? "Send with wallet" : "Connect wallet to send"}</span>
        </button>
      ) : null}
    </section>
  );
}

function TransactionRow({ flow }: { flow: DemoFlow }) {
  const request = flow.quoteResponse.quoteRequest;

  return (
    <Link className="transaction-row" href={`/flows/${flow.correlationId}`}>
      <span className={`status-dot tone-${statusTone(flow.status)}`} aria-hidden="true" />
      <span className="transaction-main">
        <strong>{flowTitle(flow)}</strong>
        <small>
          {amountLabel(flow)} {assetLabel(request.originAsset)} to {assetLabel(request.destinationAsset)}
        </small>
      </span>
      <span className="transaction-meta">
        <span>{humanStatus(flow.status)}</span>
        <small>{shortId(flow.correlationId)}</small>
      </span>
      <ChevronRight size={17} strokeWidth={1.75} aria-hidden="true" />
    </Link>
  );
}
