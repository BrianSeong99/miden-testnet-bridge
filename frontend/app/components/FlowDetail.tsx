"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Clipboard, RefreshCw } from "lucide-react";
import {
  amountLabel,
  assetLabel,
  bridgeJson,
  flowTitle,
  humanStatus,
  shortId,
  statusTone,
  type DemoArtifacts,
  type DemoFlow,
  type LifecycleEvent,
} from "../lib/bridge";

type ArtifactBucket = {
  key: keyof DemoArtifacts;
  label: string;
};

const artifactBuckets: ArtifactBucket[] = [
  { key: "evmDepositTxHashes", label: "Sepolia deposits" },
  { key: "midenMintTxIds", label: "Miden mint txs" },
  { key: "midenConsumeTxIds", label: "Miden consume txs" },
  { key: "evmReleaseTxHashes", label: "Sepolia releases" },
  { key: "evmRefundTxHashes", label: "Sepolia refunds" },
  { key: "midenRefundTxIds", label: "Miden refunds" },
  { key: "intentHashes", label: "Intent hashes" },
  { key: "nearTxHashes", label: "NEAR txs" },
  { key: "idempotencyKeys", label: "Idempotency keys" },
];

export function FlowDetail({ id }: { id: string }) {
  const [flow, setFlow] = useState<DemoFlow | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const loadFlow = useCallback(
    async (silent = false) => {
      if (!silent) setLoading(true);
      setError(null);

      try {
        setFlow(await bridgeJson<DemoFlow>(`/demo/flows/${id}`));
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      } finally {
        setLoading(false);
      }
    },
    [id],
  );

  useEffect(() => {
    void loadFlow();
    const timer = window.setInterval(() => {
      void loadFlow(true);
    }, 3500);

    return () => window.clearInterval(timer);
  }, [loadFlow]);

  const rawJson = useMemo(() => (flow ? JSON.stringify(flow, null, 2) : ""), [flow]);

  async function copyRawJson() {
    if (!rawJson) return;
    await navigator.clipboard.writeText(rawJson);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }

  return (
    <main className="detail-shell">
      <header className="detail-header">
        <Link className="back-link" href="/">
          <ArrowLeft size={17} strokeWidth={1.75} aria-hidden="true" />
          Bridge
        </Link>
        <img src="/miden-logo-horizontal.svg" alt="Miden" />
      </header>

      {loading && !flow ? <DetailSkeleton /> : null}
      {error ? <div className="notice error">{error}</div> : null}

      {flow ? (
        <>
          <section className="detail-hero">
            <div>
              <p className="eyebrow">Transaction</p>
              <h1>{flowTitle(flow)}</h1>
              <p className="detail-subtitle">{flow.correlationId}</p>
            </div>
            <span className={`status-badge tone-${statusTone(flow.status)}`}>{humanStatus(flow.status)}</span>
          </section>

          <section className="detail-grid">
            <article className="detail-card">
              <div className="card-title-row">
                <h2>Route</h2>
                <button type="button" className="icon-button" onClick={() => loadFlow()} disabled={loading} aria-label="Refresh transaction">
                  <RefreshCw size={17} strokeWidth={1.75} aria-hidden="true" />
                </button>
              </div>
              <ValueRow label="From" value={assetLabel(flow.quoteResponse.quoteRequest.originAsset)} />
              <ValueRow label="To" value={assetLabel(flow.quoteResponse.quoteRequest.destinationAsset)} />
              <ValueRow label="Amount" value={amountLabel(flow)} />
              <ValueRow label="Updated" value={flow.updatedAt} />
            </article>

            <article className="detail-card">
              <h2>Quote</h2>
              <ValueRow label="Deposit address" value={flow.quoteResponse.quote.depositAddress ?? "not required"} mono />
              <ValueRow label="Deposit memo" value={flow.quoteResponse.quote.depositMemo ?? "not required"} mono />
              <ValueRow label="Recipient" value={flow.quoteResponse.quoteRequest.recipient} mono />
              <ValueRow label="Refund to" value={flow.quoteResponse.quoteRequest.refundTo} mono />
            </article>
          </section>

          <section className="detail-card wide">
            <div className="card-title-row">
              <div>
                <p className="eyebrow">Evidence</p>
                <h2>Cross-chain artifacts</h2>
              </div>
              <button type="button" className="text-button" onClick={copyRawJson}>
                <Clipboard size={15} strokeWidth={1.75} aria-hidden="true" />
                {copied ? "Copied" : "Copy JSON"}
              </button>
            </div>
            <div className="artifact-grid">
              {artifactBuckets.map((bucket) => (
                <ArtifactList key={bucket.key} label={bucket.label} values={flow.artifacts[bucket.key]} />
              ))}
            </div>
          </section>

          <section className="detail-card wide">
            <h2>Lifecycle</h2>
            <div className="timeline">
              {flow.lifecycle.length === 0 ? (
                <div className="empty-card">
                  <p>No lifecycle events yet.</p>
                  <span>The service will append events as the bridge progresses.</span>
                </div>
              ) : (
                flow.lifecycle.map((event) => <TimelineRow key={event.id} event={event} />)
              )}
            </div>
          </section>

          <details className="raw-details">
            <summary>Raw response</summary>
            <pre>{rawJson}</pre>
          </details>
        </>
      ) : null}
    </main>
  );
}

function ValueRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="value-row">
      <span>{label}</span>
      <strong className={mono ? "mono" : undefined}>{value}</strong>
    </div>
  );
}

function ArtifactList({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="artifact-bucket">
      <span>{label}</span>
      {values.length === 0 ? (
        <p>None yet</p>
      ) : (
        <ul>
          {values.map((value) => (
            <li key={value} title={value}>
              {shortId(value, 12, 8)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function TimelineRow({ event }: { event: LifecycleEvent }) {
  return (
    <article className="timeline-row">
      <span className="timeline-dot" aria-hidden="true" />
      <div>
        <strong>{event.eventKind}</strong>
        <p>
          {event.fromStatus ? `${humanStatus(event.fromStatus)} to ` : ""}
          {humanStatus(event.toStatus)}
        </p>
        {event.reason ? <small>{event.reason}</small> : null}
      </div>
      <time>{event.createdAt}</time>
    </article>
  );
}

function DetailSkeleton() {
  return (
    <div className="detail-skeleton" aria-label="Loading transaction">
      <span />
      <span />
      <span />
    </div>
  );
}
