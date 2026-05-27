import Link from "next/link";
import { ArrowLeft, ExternalLink } from "lucide-react";

const primaryRoutes = ["GET /v0/tokens", "POST /v0/quote", "POST /v0/deposit/submit", "GET /v0/status"];

export default function NearIntentsGuidePage() {
  return (
    <main className="guide-shell">
      <header className="detail-header">
        <Link className="brand" href="/" aria-label="Miden bridge lab home">
          <img src="/miden-logo-horizontal.svg" alt="Miden" />
          <span>Bridge</span>
        </Link>
        <Link className="back-link" href="/">
          <ArrowLeft size={17} strokeWidth={1.8} aria-hidden="true" />
          <span>Back to lab</span>
        </Link>
      </header>

      <section className="guide-hero">
        <p className="eyebrow">NEAR Intents mock service</p>
        <h1>Connect this frontend to your own 1Click backend</h1>
        <p>
          This repository ships a local mock for builder testing. It is useful for UI and flow validation, but production
          integrations need a backend that owns solver policy, quote signing, liquidity, deposit verification, settlement,
          refunds, and status indexing.
        </p>
      </section>

      <section className="guide-grid" aria-label="Near Intents integration guide">
        <article className="guide-card">
          <span>1</span>
          <h2>Run a compatible backend</h2>
          <p>
            Your backend should expose the 1Click-shaped routes used by the lab. The frontend calls them through its
            server-side proxy, so wallets and apps do not need to know the private service topology.
          </p>
          <ul>
            {primaryRoutes.map((route) => (
              <li key={route}>{route}</li>
            ))}
          </ul>
        </article>

        <article className="guide-card">
          <span>2</span>
          <h2>Connect wallets</h2>
          <p>
            The lab uses an injected Sepolia wallet for deposits, refunds, and EVM recipients. Miden currently uses a
            manual testnet account ID field because this Next app does not embed the Miden SDK or MidenFi wallet adapter
            yet. A wallet-native version should connect through <code>@miden-sdk/miden-wallet-adapter-react</code> and
            submit Miden-side actions through the connected Miden wallet.
          </p>
        </article>

        <article className="guide-card">
          <span>3</span>
          <h2>Point the frontend at it</h2>
          <p>
            In Docker Compose, set <code>LAB_UI_BRIDGE_API_BASE</code> to the bridge service URL reachable from the{" "}
            <code>lab-ui</code> container. For a standalone Next.js run, set <code>BRIDGE_API_BASE</code> before
            starting the app.
          </p>
          <pre>{`LAB_UI_BRIDGE_API_BASE=http://bridge:8080
BRIDGE_API_BASE=https://your-bridge.example`}</pre>
        </article>

        <article className="guide-card">
          <span>4</span>
          <h2>Keep demo routes separate</h2>
          <p>
            <code>/demo/*</code> exists only for this lab. Builder integrations should depend on <code>/v0/*</code>;
            demo endpoints may create test wallets, submit helper transactions, or shortcut actions that a real NEAR
            Intents service would not expose.
          </p>
        </article>

        <article className="guide-card">
          <span>5</span>
          <h2>Validate the contract</h2>
          <p>
            Start with dry quotes, confirm token IDs, submit a known Sepolia tx hash, and poll status through terminal
            states. The UI transaction detail page expects lifecycle and chain evidence to be available for each
            correlation ID.
          </p>
          <Link href="/api/bridge/v0/tokens">
            <ExternalLink size={15} strokeWidth={1.8} aria-hidden="true" />
            <span>Check tokens</span>
          </Link>
        </article>
      </section>
    </main>
  );
}
