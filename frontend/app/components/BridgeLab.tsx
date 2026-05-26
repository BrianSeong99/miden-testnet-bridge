"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowDownUp, ArrowRight, CheckCircle2, ChevronRight, RefreshCw } from "lucide-react";
import {
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
  type Wallet,
} from "../lib/bridge";

type Direction = "evm-to-miden" | "miden-to-evm";

type RuntimeState = {
  apiHealth: string;
  demoState: string;
  runtimeProfile: string;
};

const defaultAmount = "1000000000000";
const storageKeys = {
  inboundWallet: "miden.bridge.lab.inboundWallet",
  outboundWallet: "miden.bridge.lab.outboundWallet",
};

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

export function BridgeLab() {
  const [runtime, setRuntime] = useState<RuntimeState>({
    apiHealth: "checking",
    demoState: "checking",
    runtimeProfile: "unknown",
  });
  const [direction, setDirection] = useState<Direction>("evm-to-miden");
  const [amount, setAmount] = useState(defaultAmount);
  const [flows, setFlows] = useState<Map<string, DemoFlow>>(new Map());
  const [inboundWallet, setInboundWallet] = useState<Wallet | null>(null);
  const [outboundWallet, setOutboundWallet] = useState<Wallet | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [lastFlowId, setLastFlowId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const config = directionCopy[direction];

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
      await Promise.all([refreshRuntime(), refreshFlows()]);
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

  useEffect(() => {
    setInboundWallet(loadStorage<Wallet | null>(storageKeys.inboundWallet, null));
    setOutboundWallet(loadStorage<Wallet | null>(storageKeys.outboundWallet, null));
    void Promise.allSettled([refreshRuntime(), refreshFlows()]);
  }, [refreshFlows, refreshRuntime]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshActiveFlows();
    }, 3000);
    return () => window.clearInterval(timer);
  }, [refreshActiveFlows]);

  async function startInbound() {
    await run("start-inbound", async () => {
      const response = await bridgeJson<{ wallet: Wallet; flow: DemoFlow }>("/demo/flows/inbound/start", {
        method: "POST",
        body: JSON.stringify({ amount, asset: "eth" }),
      });
      setInboundWallet(response.wallet);
      saveStorage(storageKeys.inboundWallet, response.wallet);
      upsertFlow(response.flow);
      setLastFlowId(response.flow.correlationId);
      setNotice("Deposit sent on Sepolia. Track the Miden note in transaction details.");
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
      const response = await bridgeJson<{ wallet: Wallet; flow: DemoFlow }>("/demo/flows/outbound/fund", {
        method: "POST",
        body: JSON.stringify({ amount, asset: "eth" }),
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
      const response = await bridgeJson<{ flow: DemoFlow }>("/demo/flows/outbound/submit", {
        method: "POST",
        body: JSON.stringify({
          senderAccountId: outboundWallet.accountId,
          amount,
          asset: "eth",
        }),
      });
      upsertFlow(response.flow);
      setLastFlowId(response.flow.correlationId);
      setNotice("Bridge note submitted from Miden. Watch details for the Sepolia release.");
    });
  }

  async function submitPrimaryAction() {
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
    setDirection((current) => (current === "evm-to-miden" ? "miden-to-evm" : "evm-to-miden"));
  }

  const primaryLabel =
    busy === "start-inbound" || busy === "fund-outbound" || busy === "submit-outbound"
      ? "Submitting"
      : direction === "miden-to-evm" && !outboundWallet
        ? "Fund Miden wallet"
        : config.submitLabel;

  const runtimeOk = runtime.apiHealth === "ok" && runtime.demoState === "enabled";

  return (
    <main className="app-shell">
      <header className="app-header">
        <Link className="brand" href="/" aria-label="Miden bridge lab home">
          <img src="/miden-logo-horizontal.svg" alt="Miden" />
          <span>Bridge</span>
        </Link>
        <div className="top-actions">
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
              <p className="eyebrow">Local bridge lab</p>
              <h1>{config.title}</h1>
            </div>
            <p className="active-count">{activeFlowCount} active</p>
          </div>

          <div className="direction-tabs" role="group" aria-label="Bridge direction">
            {(Object.keys(directionCopy) as Direction[]).map((item) => (
              <button
                key={item}
                type="button"
                className={direction === item ? "selected" : ""}
                aria-pressed={direction === item}
                onClick={() => setDirection(item)}
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
                  onChange={(event) => setAmount(event.target.value)}
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

          <button type="button" className="primary-action" onClick={submitPrimaryAction} disabled={busy !== null || amount.trim() === ""}>
            <span>{primaryLabel}</span>
            <ArrowRight size={18} strokeWidth={1.75} aria-hidden="true" />
          </button>

          {direction === "evm-to-miden" && inboundWallet ? (
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
