"use client";

import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Circle,
  Download,
  ExternalLink,
  ReceiptText,
  RefreshCcw,
} from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import { AGGLAYER_BALI, type AgglayerClaimPlan, type AgglayerDepositStatus } from "../lib/agglayer";
import {
  agglayerPollMs,
  type BridgeMonitorObservation,
  type ChainTxObservation,
  deriveMonitoredActivity,
  sourceTxPollMs,
} from "../lib/bridge-monitor";
import {
  type Activity,
  loadStoredActivities,
  modes,
  providers,
  quoteFor,
  saveActivities,
  seedActivities,
  sourceExplorer,
  statusLabel,
  statusTone,
  destinationExplorer,
  timeline,
} from "../lib/bridge-state";
import { connectEvmProvider, ensureSepolia } from "../lib/evm-wallet";

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "message" in error) return String(error.message);
  return "Something went wrong. Try again.";
}

function optionalDepositCount(value: string | undefined) {
  if (!value) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

function isSepoliaTxHash(value: string | undefined) {
  return Boolean(value && /^0x[0-9a-fA-F]{64}$/.test(value));
}

function isSepoliaAddress(value: string | undefined) {
  return Boolean(value && /^0x[0-9a-fA-F]{40}$/.test(value));
}

function matchingDeposit(status: AgglayerDepositStatus, sourceTxHash?: string) {
  if (!sourceTxHash) return status.latestDeposit;
  const normalized = sourceTxHash.toLowerCase();
  return status.deposits.find((deposit) => deposit.tx_hash?.toLowerCase() === normalized) ?? null;
}

function claimPlanDepositCount(plan: AgglayerClaimPlan) {
  const deposit = plan.deposit as
    | {
        depositCnt?: string | number;
        deposit_cnt?: string | number;
      }
    | null;
  return deposit?.depositCnt ?? deposit?.deposit_cnt;
}

async function fetchSepoliaTx(hash: string): Promise<ChainTxObservation> {
  const response = await fetch(`/api/sepolia/transaction?hash=${hash}`, { cache: "no-store" });
  const payload = (await response.json()) as ChainTxObservation | { error?: string };
  if (!response.ok) {
    throw new Error("error" in payload ? payload.error ?? "Unable to read Sepolia transaction." : "Unable to read Sepolia transaction.");
  }
  return payload as ChainTxObservation;
}

export function ActivityDetail({ id }: { id: string }) {
  const [activities, setActivities] = useState<Activity[]>(seedActivities);
  const [claimBusy, setClaimBusy] = useState(false);
  const [claimError, setClaimError] = useState("");
  const [monitorError, setMonitorError] = useState("");
  const [lastChecked, setLastChecked] = useState("");
  const activity = activities.find((item) => item.id === id) ?? seedActivities.find((item) => item.id === id);
  const quote = useMemo(
    () => (activity ? quoteFor(activity.mode, activity.provider, activity.amount) : null),
    [activity],
  );
  const modeCopy = activity ? modes[activity.mode] : null;
  const sourceLink = activity ? sourceExplorer(activity) : null;
  const destinationLink = activity ? destinationExplorer(activity) : null;
  const agglayerMonitorLink = activity?.provider === "agglayer" ? AGGLAYER_BALI.monitorUrl : null;
  const currentIndex = activity ? timeline.findIndex((step) => step.status === activity.status) : -1;
  const needsRecovery = activity?.status === "claim_available" || activity?.status === "failed";
  const canClaimOnSepolia =
    activity?.provider === "agglayer" && activity.mode === "send" && activity.status === "claim_available" && Boolean(activity.depositCount);
  const settlement = activity ? settlementRows(activity) : [];
  const receiptTitle = activity?.mode === "send" ? "Cross-chain send receipt" : "Cross-chain receive receipt";
  const receiptAmountLabel =
    activity?.status === "complete"
      ? activity.mode === "receive"
        ? "Received on Miden"
        : "Released on Sepolia"
      : activity?.mode === "receive"
        ? "Expected on Miden"
        : "Expected on Sepolia";
  const nextAction =
    activity?.status === "signature"
      ? "Confirm the source transaction in your wallet."
      : activity?.status === "source_finality"
        ? "Wait for source-chain confirmation and bridge finality."
        : activity?.status === "message_observed"
          ? "Wait for the destination claim to become available."
          : activity?.status === "claim_available"
            ? "Claim funds on the destination side."
            : activity?.status === "claim_submitted"
              ? "Wait for the Sepolia claim transaction to confirm."
            : activity?.status === "failed"
              ? "Retry claim or export diagnostics for support."
              : "Funds are available in the destination account.";

  const observeActivity = useCallback(
    (activityId: string, observation: BridgeMonitorObservation) => {
      setActivities((current) => {
        const currentActivity = current.find((item) => item.id === activityId) ?? activity;
        if (!currentActivity) return current;
        const nextActivity = deriveMonitoredActivity(currentActivity, observation);
        const updated = current.some((item) => item.id === activityId)
          ? current.map((item) => (item.id === activityId ? nextActivity : item))
          : [nextActivity, ...current];
        saveActivities(updated);
        setLastChecked(observation.checkedAt);
        setMonitorError("");
        return updated;
      });
    },
    [activity],
  );

  useEffect(() => {
    try {
      const stored = loadStoredActivities();
      queueMicrotask(() => setActivities(stored));
    } catch {
      queueMicrotask(() => setActivities(seedActivities));
    }
  }, []);

  useEffect(() => {
    if (!activity?.bridgeDestinationAddress || activity.provider !== "agglayer" || activity.mode !== "receive") return;
    if (activity.status === "complete" || activity.status === "failed") return;

    let cancelled = false;
    const activityId = activity.id;
    const bridgeDestinationAddress = activity.bridgeDestinationAddress;
    const sourceTxHash = activity.sourceTxHash;
    async function pollAgglayerStatus() {
      const response = await fetch(`/api/agglayer/deposits?destinationAddress=${bridgeDestinationAddress}`);
      if (!response.ok || cancelled) return;
      const status = (await response.json()) as AgglayerDepositStatus;
      const latestDeposit = matchingDeposit(status, sourceTxHash);

      setActivities((current) => {
        const currentActivity = current.find((item) => item.id === activityId) ?? activity;
        if (!currentActivity) return current;
        const updatedActivity = deriveMonitoredActivity(currentActivity, {
          checkedAt: "Just now",
          agglayerDeposit: latestDeposit,
        });
        const updated = current.map((item) => {
          if (item.id !== activityId) return item;
          return updatedActivity;
        });
        saveActivities(updated);
        setLastChecked("Just now");
        setMonitorError("");
        return updated;
      });
    }

    pollAgglayerStatus().catch(() => undefined);
    const interval = window.setInterval(() => {
      pollAgglayerStatus().catch(() => undefined);
    }, agglayerPollMs);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activity, activity?.bridgeDestinationAddress, activity?.id, activity?.mode, activity?.provider, activity?.sourceTxHash, activity?.status]);

  useEffect(() => {
    if (!activity || activity.provider !== "agglayer" || activity.mode !== "receive") return;
    if (activity.status === "complete" || activity.status === "failed") return;
    if (!isSepoliaTxHash(activity.sourceTxHash)) return;

    let cancelled = false;
    const activityId = activity.id;
    const sourceHash = activity.sourceTxHash;

    async function pollSourceTransaction() {
      try {
        const sourceTx = await fetchSepoliaTx(sourceHash!);
        if (cancelled) return;
        observeActivity(activityId, { checkedAt: "Just now", sourceTx });
      } catch (error) {
        if (!cancelled) setMonitorError(errorMessage(error));
      }
    }

    pollSourceTransaction();
    const interval = window.setInterval(pollSourceTransaction, sourceTxPollMs);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activity?.id, activity?.mode, activity?.provider, activity?.sourceTxHash, activity?.status]);

  useEffect(() => {
    if (!activity || activity.provider !== "agglayer" || activity.mode !== "send") return;
    if (activity.status === "complete" || activity.status === "failed") return;
    if (!isSepoliaAddress(activity.destination)) return;
    if (activity.status === "claim_submitted") return;
    if (!activity.depositCount) return;

    let cancelled = false;
    const activityId = activity.id;
    const destination = activity.destination;
    const expectedDepositCount = activity.depositCount;

    async function pollClaimReadiness() {
      try {
        const response = await fetch("/api/bridge/agglayer/l2/withdraw/claim/plan", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            ethAccountId: destination,
            depositCount: optionalDepositCount(expectedDepositCount),
          }),
          cache: "no-store",
        });
        const plan = (await response.json()) as AgglayerClaimPlan | { message?: string };
        if (!response.ok) throw new Error("message" in plan ? plan.message ?? "Unable to read claim readiness." : "Unable to read claim readiness.");
        if (!("readyForClaim" in plan)) return;
        if (cancelled) return;
        observeActivity(activityId, {
          checkedAt: "Just now",
          claimPlan: {
            readyForClaim: plan.readyForClaim,
            depositCount: claimPlanDepositCount(plan),
          },
        });
      } catch (error) {
        if (!cancelled) setMonitorError(errorMessage(error));
      }
    }

    pollClaimReadiness();
    const interval = window.setInterval(pollClaimReadiness, agglayerPollMs);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activity?.depositCount, activity?.destination, activity?.id, activity?.mode, activity?.provider, activity?.status]);

  useEffect(() => {
    if (!activity || activity.provider !== "agglayer" || activity.mode !== "send") return;
    if (activity.status !== "claim_submitted") return;
    const claimHash = activity.claimTxHash ?? activity.destinationTxHash;
    if (!isSepoliaTxHash(claimHash)) return;

    let cancelled = false;
    const activityId = activity.id;

    async function pollClaimTransaction() {
      try {
        const destinationTx = await fetchSepoliaTx(claimHash!);
        if (cancelled) return;
        observeActivity(activityId, { checkedAt: "Just now", destinationTx });
      } catch (error) {
        if (!cancelled) setMonitorError(errorMessage(error));
      }
    }

    pollClaimTransaction();
    const interval = window.setInterval(pollClaimTransaction, sourceTxPollMs);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activity?.claimTxHash, activity?.destinationTxHash, activity?.id, activity?.mode, activity?.provider, activity?.status]);

  function updateActivity(nextActivity: Activity) {
    const updated = activities.some((item) => item.id === nextActivity.id)
      ? activities.map((item) => (item.id === nextActivity.id ? nextActivity : item))
      : [nextActivity, ...activities];
    setActivities(updated);
    saveActivities(updated);
  }

  function markFailed() {
    if (!activity) return;
    updateActivity({ ...activity, status: "failed", eta: "Needs retry", updatedAt: "Just now" });
  }

  function retryClaim() {
    if (!activity) return;
    updateActivity({ ...activity, status: "message_observed", eta: "2 min", updatedAt: "Just now" });
  }

  async function claimOnSepolia() {
    if (!activity) return;

    setClaimBusy(true);
    setClaimError("");
    try {
      const destinationAddress = activity.destination;
      if (!destinationAddress || !/^0x[0-9a-fA-F]{40}$/.test(destinationAddress)) {
        throw new Error("This activity is missing the Sepolia destination address needed to look up the claim proof.");
      }

      const { provider } = await connectEvmProvider();
      const accounts = await provider.request<string[]>({ method: "eth_accounts" });
      const account = accounts[0];
      if (!account) throw new Error("Connect a Sepolia wallet to pay gas for claimAsset.");
      await ensureSepolia(provider);

      const response = await fetch("/api/bridge/agglayer/l2/withdraw/claim/plan", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          ethAccountId: destinationAddress,
          depositCount: optionalDepositCount(activity.depositCount),
        }),
      });
      const plan = (await response.json()) as AgglayerClaimPlan | { message?: string; detail?: string };
      if (!response.ok) {
        throw new Error("message" in plan ? plan.message ?? "Unable to build claim plan." : "Unable to build claim plan.");
      }
      if (!("readyForClaim" in plan) || !plan.readyForClaim || !plan.transaction) {
        throw new Error("AggLayer proof is not ready yet. Keep this transfer in activity and try again later.");
      }

      const txHash = await provider.request<string>({
        method: "eth_sendTransaction",
        params: [
          {
            from: account,
            to: plan.transaction.to,
            data: plan.transaction.data,
            value: plan.transaction.value,
            gas: plan.transaction.gas,
          },
        ],
      });

      updateActivity({
        ...activity,
        status: "claim_submitted",
        eta: "Waiting for Sepolia",
        claimTxHash: txHash,
        destinationTxHash: txHash,
        readyForClaim: true,
        updatedAt: "Just now",
      });
    } catch (error) {
      setClaimError(errorMessage(error));
    } finally {
      setClaimBusy(false);
    }
  }

  function exportDiagnostic() {
    if (!activity || !quote) return;
    const payload = JSON.stringify(
      {
        exportedAt: new Date().toISOString(),
        activity,
        provider: providers[activity.provider],
        quote,
        timeline,
      },
      null,
      2,
    );
    const blob = new Blob([payload], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${activity.id}-diagnostic.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  return (
    <main className="detail-shell">
      <header className="detail-topbar">
        <Link className="brand" href="/" aria-label="Back to bridge">
          <Image src="/miden-logo-horizontal.svg" alt="Miden" width={112} height={34} priority />
          <span>Bridge</span>
        </Link>
        <Link className="back-link" href="/">
          <ArrowLeft size={17} aria-hidden="true" />
          New transfer
        </Link>
      </header>

      {!activity || !quote ? (
        <section className="detail-empty">
          <h1>Activity not found</h1>
          <p>This transfer is not in local activity history on this browser.</p>
          <Link className="primary-button" href="/">
            Back to bridge
          </Link>
        </section>
      ) : (
        <section className="detail-grid">
          <div className="detail-main">
            <section className="receipt-card">
              <div className="receipt-header">
                <span className="receipt-icon">
                  <ReceiptText size={20} aria-hidden="true" />
                </span>
                <div>
                  <p className="kicker">Receipt</p>
                  <h1>{receiptTitle}</h1>
                  <span>{activity.id}</span>
                </div>
                <span className={`status-badge ${statusTone(activity.status)}`}>{statusLabel(activity.status)}</span>
              </div>

              <div className="receipt-amount">
                <span>{receiptAmountLabel}</span>
                <strong>{quote.expectedReceived}</strong>
              </div>

              <div className="receipt-next-action">
                <span>Next action</span>
                <strong>{nextAction}</strong>
              </div>

              <div className="receipt-route">
                <div>
                  <span>From</span>
                  <strong>{modeCopy?.from}</strong>
                  <small>{activity.mode === "receive" ? activity.sourceTxHash ?? activity.txHash : activity.midenTxId ?? activity.txHash}</small>
                </div>
                <ArrowRight size={18} aria-hidden="true" />
                <div>
                  <span>To</span>
                  <strong>{modeCopy?.to}</strong>
                  <small>{activity.mode === "receive" ? activity.midenTxId ?? activity.txHash : activity.destinationTxHash ?? activity.txHash}</small>
                </div>
              </div>

              <div className="receipt-explorer-grid">
                {sourceLink ? (
                  <a href={sourceLink.href} target="_blank" rel="noreferrer" className="explorer-link">
                    <span>Source proof</span>
                    <strong>{sourceLink.label}</strong>
                    <ExternalLink size={15} aria-hidden="true" />
                  </a>
                ) : null}
                {destinationLink ? (
                  <a href={destinationLink.href} target="_blank" rel="noreferrer" className="explorer-link">
                    <span>Destination proof</span>
                    <strong>{destinationLink.label}</strong>
                    <ExternalLink size={15} aria-hidden="true" />
                  </a>
                ) : null}
                {agglayerMonitorLink ? (
                  <a href={agglayerMonitorLink} target="_blank" rel="noreferrer" className="explorer-link">
                    <span>Public monitor</span>
                    <strong>Gateway FM Bali</strong>
                    <ExternalLink size={15} aria-hidden="true" />
                  </a>
                ) : null}
              </div>

              <div className="receipt-lines">
                <ReceiptLine label="Route" value={providers[activity.provider].label} />
                <ReceiptLine label="ETA" value={activity.eta} />
                <ReceiptLine label="Minimum received" value={quote.minReceived} />
                <ReceiptLine label="Network fee" value={quote.networkFee} />
                <ReceiptLine label="Bridge fee" value={quote.bridgeFee} />
                <ReceiptLine label="Relayer fee" value={quote.relayerFee} />
                {activity.depositCount ? <ReceiptLine label="AggLayer bridge event" value={`#${activity.depositCount}`} /> : null}
                {activity.bridgeDestinationAddress ? (
                  <ReceiptLine label="Bridge destination" value={activity.bridgeDestinationAddress} />
                ) : null}
              </div>

              <div className="disclosure receipt-disclosure">
                <AlertTriangle size={17} aria-hidden="true" />
                <span>{providers[activity.provider].disclosure}</span>
              </div>
            </section>

            <section className="detail-card">
              <div className="detail-title">
                <div>
                  <p className="kicker">Progress</p>
                  <h2>State machine</h2>
                </div>
              </div>

              <div className="timeline">
                {timeline.map((step, index) => {
                  const isDone = currentIndex > index || activity.status === "complete";
                  const isCurrent = step.status === activity.status;
                  return (
                    <div className={`timeline-step ${isDone ? "done" : ""} ${isCurrent ? "current" : ""}`} key={step.status}>
                      <div className="step-icon">
                        {isDone ? <CheckCircle2 size={18} /> : isCurrent ? <RefreshCcw size={17} /> : <Circle size={16} />}
                      </div>
                      <div>
                        <strong>{step.label}</strong>
                        <span>{step.detail}</span>
                      </div>
                    </div>
                  );
                })}
              </div>

              <div className="live-monitor-panel" aria-live="polite">
                <div>
                  <span className="status-dot active" />
                  <strong>Live monitor</strong>
                </div>
                <span>
                  {activity.provider !== "agglayer"
                    ? "This route is not wired to a live provider monitor yet."
                    : activity.mode === "send" && !activity.depositCount
                      ? "Waiting for a Miden bridge event id before polling Sepolia claim proofs."
                      : "Sepolia receipts poll every 12s. AggLayer rows and proofs poll every 30s."}
                </span>
                <small>{lastChecked ? `Last checked ${lastChecked}` : "Starting automatic checks"}</small>
                {monitorError ? <p className="form-error compact">{monitorError}</p> : null}
              </div>

              <div className="detail-actions">
                <button className="secondary-button" type="button" onClick={markFailed}>
                  Mark as stuck
                </button>
              </div>
            </section>
          </div>

          <aside className="detail-side">
            <section className="detail-card">
              <div className="detail-title">
                <div>
                  <p className="kicker">Settlement</p>
                  <h2>{activity.mode === "receive" ? "Miden wallet state" : "Sepolia claim state"}</h2>
                </div>
              </div>

              <div className="metric-grid one-column settlement-grid">
                {settlement.map((row) => (
                  <Metric label={row.label} value={row.value} key={row.label} />
                ))}
              </div>
            </section>

            <section className={`detail-card recovery-card ${needsRecovery ? "needs-action" : ""}`}>
              <div className="detail-title">
                <div>
                  <p className="kicker">Claim and recovery</p>
                  <h2>{needsRecovery ? "Action available" : "No action needed"}</h2>
                </div>
              </div>

              <div className="metric-grid one-column">
                <Metric label="Source tx" value={activity.txHash} />
                <Metric
                  label="Proof status"
                  value={
                    activity.readyForClaim ||
                    activity.status === "claim_available" ||
                    activity.status === "claim_submitted" ||
                    activity.status === "complete"
                      ? "Ready for claim"
                      : activity.status === "message_observed"
                        ? "Observed"
                        : "Pending"
                  }
                />
                <Metric
                  label="Destination claim"
                  value={
                    activity.status === "complete"
                      ? "Settled"
                      : activity.claimTxHash
                        ? "Submitted"
                        : activity.status === "claim_available" && (activity.mode === "receive" || activity.depositCount)
                          ? "Ready"
                          : activity.status === "claim_available"
                            ? "Needs bridge event id"
                          : "Not ready"
                  }
                />
              </div>

              <p className="recovery-copy">
                {activity.status === "failed"
                  ? "This transfer is stuck. Retry claim, use manual claim, or export diagnostics for support."
                  : activity.status === "claim_available" && activity.mode === "send" && !activity.depositCount
                    ? "This send needs a specific AggLayer bridge event id before claimAsset can be submitted safely."
                  : activity.status === "claim_available"
                    ? "Funds are ready to claim on the destination side."
                    : "Recovery tools appear here when the transfer needs a user action."}
              </p>

              <div className="recovery-actions">
                {canClaimOnSepolia ? (
                  <button className="primary-button compact-action" type="button" onClick={claimOnSepolia} disabled={claimBusy}>
                    {claimBusy ? "Waiting for wallet" : "Claim on Sepolia"}
                  </button>
                ) : null}
                <button className="secondary-button" type="button" onClick={retryClaim}>
                  Retry claim
                </button>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={canClaimOnSepolia ? claimOnSepolia : undefined}
                  disabled={!canClaimOnSepolia || claimBusy}
                >
                  Manual claim
                </button>
                <button className="secondary-button" type="button" onClick={exportDiagnostic}>
                  <Download size={16} aria-hidden="true" />
                  Export
                </button>
              </div>
              {claimError ? <p className="form-error compact">{claimError}</p> : null}
            </section>
          </aside>
        </section>
      )}
    </main>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function settlementRows(activity: Activity) {
  if (activity.mode === "receive") {
    return [
      {
        label: "Miden claim",
        value:
          activity.status === "message_observed" || activity.status === "claim_available" || activity.status === "complete"
            ? "Bridge message available"
            : "Waiting for bridge message",
      },
      {
        label: "Note sync",
        value: activity.status === "complete" ? "Synced" : activity.status === "claim_available" ? "Ready for wallet sync" : "Pending",
      },
      {
        label: "Note consume",
        value: activity.status === "complete" ? "Consumed" : "Awaiting wallet consume",
      },
      {
        label: "Claim settlement",
        value: activity.status === "complete" ? "Balance settled" : "Not wallet-settled",
      },
    ];
  }

  return [
    {
      label: "Miden bridge note",
      value:
        activity.status === "source_finality" ||
        activity.status === "message_observed" ||
        activity.status === "claim_available" ||
        activity.status === "claim_submitted" ||
        activity.status === "complete"
          ? "Submitted"
          : "Needs wallet signature",
    },
    {
      label: "Proof status",
      value:
        activity.readyForClaim ||
        activity.status === "claim_available" ||
        activity.status === "claim_submitted" ||
        activity.status === "complete"
          ? "Ready"
          : "Pending",
    },
    {
      label: "Sepolia claim",
      value: activity.claimTxHash ? "Submitted" : activity.status === "claim_available" ? "Ready to submit" : "Not ready",
    },
    {
      label: "Claim settlement",
      value: activity.status === "complete" ? "Settled on Sepolia" : activity.claimTxHash ? "Waiting confirmation" : "Not started",
    },
  ];
}

function ReceiptLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="receipt-line">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
