import Link from "next/link";
import { ArrowLeft, ExternalLink } from "lucide-react";

const constants = [
  ["Sepolia bridge", "0x1348947e282138d8f377b467f7d9c2eb0f335d1f"],
  ["Miden destination network", "76"],
  ["L2 chain ID", "1022211914"],
  ["Miden bridge ID", "mcst1arychvrurzxdy5qwz0mg5p5umsvsepyx"],
  ["Miden ETH faucet ID", "mcst1arnrhfau9svl7cpu2tr8lfzzd5j87wwe"],
  ["Miden RPC", "https://rpc.testnet.miden.io:443"],
];

export default function AggLayerGuidePage() {
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
        <p className="eyebrow">AggLayer testnet helper</p>
        <h1>Generate dry-run plans for Sepolia and Miden testnet</h1>
        <p>
          The AggLayer mode is a command-planning surface for the current public Bali testnet tooling. The backend never
          broadcasts transactions or custodies keys. For Sepolia to Miden, the lab can hand the reviewed transaction to
          an injected wallet for explicit testnet confirmation; Miden to Sepolia remains an operator handoff through the
          bridge-out tooling.
        </p>
      </section>

      <section className="guide-grid" aria-label="AggLayer bridge guide">
        <article className="guide-card">
          <span>1</span>
          <h2>Check live constants</h2>
          <p>
            These values follow the post-relaunch review notes on the Miden bridging tutorial PR. Recheck them before a
            funded run because the public tutorial is still open.
          </p>
          <ul>
            {constants.map(([label, value]) => (
              <li key={label}>
                {label}: {value}
              </li>
            ))}
          </ul>
          <Link href="/api/bridge/agglayer/info">
            <ExternalLink size={15} strokeWidth={1.8} aria-hidden="true" />
            <span>Inspect JSON</span>
          </Link>
        </article>

        <article className="guide-card">
          <span>2</span>
          <h2>Plan Sepolia to Miden</h2>
          <p>
            The backend accepts a Miden account ID in 30-hex form or bech32 form and returns the padded destination
            address, <code>bridgeAsset</code> calldata, status URL, and a dry-run <code>cast send</code> command.
            The lab form can also send this Sepolia transaction through an injected browser wallet after the user
            reviews and confirms the testnet transaction.
          </p>
          <p>
            In a wallet-native version, the Miden account should come from the connected MidenFi wallet adapter instead
            of a pasted field.
          </p>
          <pre>{`curl -s /api/bridge/agglayer/l1/deposit/plan \\
  -H 'content-type: application/json' \\
  -d '{
    "midenAccountId": "0xc98bb07c188cd2500e13f68a069cdc",
    "amountEth": "0.001"
  }'`}</pre>
        </article>

        <article className="guide-card">
          <span>3</span>
          <h2>Plan Miden to Sepolia</h2>
          <p>
            The reverse direction follows the <code>bali-l2-withdraw.sh</code> helper from{" "}
            <code>0xMiden/miden-client#2173</code>. That helper reads a small config file and shells out to{" "}
            <code>bridge-out-tool</code>. This app returns the same command shape and claim-status URL; it does not submit
            the B2AGG note or call <code>claimAsset</code>.
          </p>
          <p>
            Do not copy network IDs, account IDs, or RPC URLs from{" "}
            <code>gateway-fm/miden-agglayer/scripts/e2e-l2-to-l1.sh</code>; that script still contains local test values.
            Use it only as a reference for the later <code>claimAsset</code> calldata shape.
          </p>
          <p>
            For readiness, poll the returned <code>/bridges/&lt;sepolia-address&gt;</code> URL and look for a row with{" "}
            <code>ready_for_claim=true</code>, <code>dest_net=0</code>, and an empty <code>claim_tx_hash</code>. The{" "}
            <code>/claims/&lt;sepolia-address&gt;</code> URL is post-claim history; it can be empty while funds are
            already ready to claim.
          </p>
          <pre>{`curl -s /api/bridge/agglayer/l2/withdraw/plan \\
  -H 'content-type: application/json' \\
  -d '{
    "midenAccountId": "0xc98bb07c188cd2500e13f68a069cdc",
    "ethAccountId": "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
    "midenWithdrawAmount": "10000"
  }'`}</pre>
        </article>

        <article className="guide-card">
          <span>4</span>
          <h2>Keep broadcasts explicit</h2>
          <p>
            Run generated commands only with testnet keys and Sepolia ETH. The helper intentionally defaults to dry-run
            planning so the frontend remains safe to open on shared machines and in previews.
          </p>
          <ul>
            <li>Sepolia to Miden: poll the returned deposit status URL, then consume notes with miden-client.</li>
            <li>
              Miden to Sepolia: wait for certificate settlement, fetch the merkle proof, then run the generated{" "}
              <code>claimAsset</code> command with a funded Sepolia test key.
            </li>
          </ul>
        </article>
      </section>
    </main>
  );
}
