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
import { useEffect, useMemo, useState } from "react";
import type { AgglayerDepositStatus } from "../lib/agglayer";
import {
  type Activity,
  loadStoredActivities,
  modes,
  nextStatus,
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

export function ActivityDetail({ id }: { id: string }) {
  const [activities, setActivities] = useState<Activity[]>(seedActivities);
  const activity = activities.find((item) => item.id === id) ?? seedActivities.find((item) => item.id === id);
  const quote = useMemo(
    () => (activity ? quoteFor(activity.mode, activity.provider, activity.amount) : null),
    [activity],
  );
  const modeCopy = activity ? modes[activity.mode] : null;
  const sourceLink = activity ? sourceExplorer(activity) : null;
  const destinationLink = activity ? destinationExplorer(activity) : null;
  const currentIndex = activity ? timeline.findIndex((step) => step.status === activity.status) : -1;
  const needsRecovery = activity?.status === "claim_available" || activity?.status === "failed";
  const receiptTitle = activity?.mode === "withdraw" ? "Withdrawal receipt" : "Deposit receipt";
  const receiptAmountLabel =
    activity?.status === "complete"
      ? activity.mode === "deposit"
        ? "Received on Miden"
        : "Released on Sepolia"
      : activity?.mode === "deposit"
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
            : activity?.status === "failed"
              ? "Retry claim or export diagnostics for support."
              : "Funds are available in the destination account.";

  useEffect(() => {
    try {
      const stored = loadStoredActivities();
      queueMicrotask(() => setActivities(stored));
    } catch {
      queueMicrotask(() => setActivities(seedActivities));
    }
  }, []);

  useEffect(() => {
    if (!activity?.bridgeDestinationAddress || activity.provider !== "agglayer" || activity.mode !== "deposit") return;

    let cancelled = false;
    const activityId = activity.id;
    const bridgeDestinationAddress = activity.bridgeDestinationAddress;
    async function pollAgglayerStatus() {
      const response = await fetch(`/api/agglayer/deposits?destinationAddress=${bridgeDestinationAddress}`);
      if (!response.ok || cancelled) return;
      const status = (await response.json()) as AgglayerDepositStatus;
      const latestDeposit = status.latestDeposit;
      if (!latestDeposit) return;

      setActivities((current) => {
        const updated = current.map((item) => {
          if (item.id !== activityId) return item;
          const readyForClaim = latestDeposit.ready_for_claim === true;
          const status: Activity["status"] = readyForClaim ? "claim_available" : "message_observed";
          return {
            ...item,
            status,
            eta: readyForClaim ? "Ready now" : "Waiting for claim note",
            readyForClaim,
            depositCount: latestDeposit.deposit_cnt ? String(latestDeposit.deposit_cnt) : item.depositCount,
            sourceTxHash: latestDeposit.tx_hash ?? item.sourceTxHash,
            txHash: latestDeposit.tx_hash ? latestDeposit.tx_hash : item.txHash,
            updatedAt: "Just now",
          };
        });
        saveActivities(updated);
        return updated;
      });
    }

    pollAgglayerStatus().catch(() => undefined);
    const interval = window.setInterval(() => {
      pollAgglayerStatus().catch(() => undefined);
    }, 30_000);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activity?.bridgeDestinationAddress, activity?.id, activity?.mode, activity?.provider]);

  function updateActivity(nextActivity: Activity) {
    const updated = activities.some((item) => item.id === nextActivity.id)
      ? activities.map((item) => (item.id === nextActivity.id ? nextActivity : item))
      : [nextActivity, ...activities];
    setActivities(updated);
    saveActivities(updated);
  }

  function advanceActivity() {
    if (!activity) return;
    updateActivity({ ...activity, status: nextStatus(activity), updatedAt: "Just now" });
  }

  function markFailed() {
    if (!activity) return;
    updateActivity({ ...activity, status: "failed", eta: "Needs retry", updatedAt: "Just now" });
  }

  function retryClaim() {
    if (!activity) return;
    updateActivity({ ...activity, status: "message_observed", eta: "2 min", updatedAt: "Just now" });
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
                  <small>{activity.mode === "deposit" ? activity.sourceTxHash ?? activity.txHash : activity.midenTxId ?? activity.txHash}</small>
                </div>
                <ArrowRight size={18} aria-hidden="true" />
                <div>
                  <span>To</span>
                  <strong>{modeCopy?.to}</strong>
                  <small>{activity.mode === "deposit" ? activity.midenTxId ?? activity.txHash : activity.destinationTxHash ?? activity.txHash}</small>
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
              </div>

              <div className="receipt-lines">
                <ReceiptLine label="Route" value={providers[activity.provider].label} />
                <ReceiptLine label="ETA" value={activity.eta} />
                <ReceiptLine label="Minimum received" value={quote.minReceived} />
                <ReceiptLine label="Network fee" value={quote.networkFee} />
                <ReceiptLine label="Bridge fee" value={quote.bridgeFee} />
                <ReceiptLine label="Relayer fee" value={quote.relayerFee} />
                {activity.depositCount ? <ReceiptLine label="AggLayer deposit" value={`#${activity.depositCount}`} /> : null}
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

              <div className="detail-actions">
                <button className="secondary-button" type="button" onClick={markFailed}>
                  Mark as stuck
                </button>
                <button className="primary-button" type="button" onClick={advanceActivity}>
                  Simulate next step
                  <ArrowRight size={17} aria-hidden="true" />
                </button>
              </div>
            </section>
          </div>

          <aside className="detail-side">
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
                    activity.readyForClaim || activity.status === "claim_available"
                      ? "Ready for claim"
                      : activity.status === "message_observed"
                        ? "Observed"
                        : "Pending"
                  }
                />
                <Metric label="Destination claim" value={activity.status === "claim_available" ? "Ready" : "Not ready"} />
              </div>

              <p className="recovery-copy">
                {activity.status === "failed"
                  ? "This transfer is stuck. Retry claim, use manual claim, or export diagnostics for support."
                  : activity.status === "claim_available"
                    ? "Funds are ready to claim on the destination side."
                    : "Recovery tools appear here when the transfer needs a user action."}
              </p>

              <div className="recovery-actions">
                <button className="secondary-button" type="button" onClick={retryClaim}>
                  Retry claim
                </button>
                <button className="secondary-button" type="button">
                  Manual claim
                </button>
                <button className="secondary-button" type="button" onClick={exportDiagnostic}>
                  <Download size={16} aria-hidden="true" />
                  Export
                </button>
              </div>
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

function ReceiptLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="receipt-line">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
