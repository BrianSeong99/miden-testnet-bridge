import React from "react";
import {
  AbsoluteFill,
  Easing,
  Img,
  OffthreadVideo,
  Sequence,
  interpolate,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";

const colors = {
  bg: "#f4f2ec",
  ink: "#161815",
  muted: "#6f7168",
  line: "#d7d2c4",
  green: "#234b37",
  red: "#9f2f2f",
  blue: "#244b6f",
  gold: "#b8872b",
  panel: "#fffdf7",
  dark: "#10130f",
};

const shell: React.CSSProperties = {
  fontFamily:
    'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  background: colors.bg,
  color: colors.ink,
};

const mono =
  'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace';

const useFade = (start: number, end: number) => {
  const frame = useCurrentFrame();
  return interpolate(frame, [start, start + 18, end - 18, end], [0, 1, 1, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
};

const Badge: React.FC<{ children: React.ReactNode; tone?: "red" | "green" | "blue" | "gold" }> = ({
  children,
  tone = "green",
}) => {
  const color =
    tone === "red" ? colors.red : tone === "blue" ? colors.blue : tone === "gold" ? colors.gold : colors.green;
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "8px 13px",
        border: `1px solid ${color}`,
        color,
        borderRadius: 999,
        fontSize: 24,
        fontWeight: 700,
        letterSpacing: 0,
        width: "max-content",
      }}
    >
      {children}
    </div>
  );
};

const TitleCard: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const entry = spring({ frame, fps, config: { damping: 30, stiffness: 90 } });
  const y = interpolate(entry, [0, 1], [32, 0]);

  return (
    <AbsoluteFill style={{ ...shell, padding: 88 }}>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1.05fr 0.95fr",
          gap: 72,
          height: "100%",
          alignItems: "center",
        }}
      >
        <div style={{ transform: `translateY(${y}px)`, opacity: entry }}>
          <Badge tone="red">Testnet-only tutorial</Badge>
          <h1
            style={{
              margin: "32px 0 24px",
              fontSize: 104,
              lineHeight: 0.94,
              letterSpacing: 0,
              fontWeight: 820,
              maxWidth: 930,
            }}
          >
            Mock NEAR Intents 1Click bridge for Miden
          </h1>
          <p style={{ fontSize: 34, lineHeight: 1.32, color: colors.muted, maxWidth: 850 }}>
            Sepolia native ETH to public Miden testnet, using the same
            quote, deposit submit, and status shape builders integrate against.
          </p>
        </div>
        <div
          style={{
            border: `1px solid ${colors.line}`,
            background: colors.panel,
            padding: 42,
            borderRadius: 8,
            boxShadow: "0 24px 80px rgba(37, 35, 28, 0.12)",
          }}
        >
          <div style={{ fontSize: 26, color: colors.muted, marginBottom: 24 }}>Default path</div>
          <StepPill label="1" text="Run Sepolia profile locally" />
          <StepPill label="2" text="Use /v0/tokens, /v0/quote, /v0/deposit/submit, /v0/status" />
          <StepPill label="3" text="Verify with Etherscan and Miden tx ids" />
          <div
            style={{
              marginTop: 36,
              paddingTop: 28,
              borderTop: `1px solid ${colors.line}`,
              fontSize: 24,
              lineHeight: 1.4,
              color: colors.red,
              fontWeight: 700,
            }}
          >
            Not a production bridge. Not a mainnet path. Do not use mainnet funds.
          </div>
        </div>
      </div>
    </AbsoluteFill>
  );
};

const StepPill: React.FC<{ label: string; text: string }> = ({ label, text }) => (
  <div style={{ display: "flex", gap: 18, alignItems: "flex-start", marginBottom: 22 }}>
    <div
      style={{
        width: 42,
        height: 42,
        borderRadius: 21,
        background: colors.dark,
        color: colors.bg,
        display: "grid",
        placeItems: "center",
        fontSize: 20,
        fontWeight: 800,
        flex: "0 0 auto",
      }}
    >
      {label}
    </div>
    <div style={{ fontSize: 28, lineHeight: 1.28, fontWeight: 700 }}>{text}</div>
  </div>
);

const Architecture: React.FC = () => {
  const frame = useCurrentFrame();
  const draw = (index: number) =>
    interpolate(frame, [index * 10, index * 10 + 24], [0, 1], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
      easing: Easing.out(Easing.cubic),
    });

  return (
    <Scene>
      <SceneHeader
        eyebrow="Component model"
        title="The bridge is a local mock around public testnets"
        subtitle="The solver role is inside the service in this sandbox, but the boundary is explicit."
      />
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 30, marginTop: 48 }}>
        <ComponentCard progress={draw(0)} tone="blue" title="Builder app" lines={["UI or agent", "Sepolia wallet", "Miden wallet"]} />
        <ComponentCard progress={draw(1)} tone="green" title="Bridge API" lines={["/v0/tokens", "/v0/quote", "/v0/deposit/submit", "/v0/status"]} />
        <ComponentCard progress={draw(2)} tone="gold" title="Public testnets" lines={["Sepolia native ETH", "Miden public notes", "Miden tx ids"]} />
      </div>
      <div style={{ marginTop: 44, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 30 }}>
        <FlowCard progress={draw(3)} title="Inbound" from="Sepolia ETH" to="Miden P2ID note" />
        <FlowCard progress={draw(4)} title="Outbound" from="Miden BridgeOutV1 note" to="Sepolia ETH release" />
      </div>
    </Scene>
  );
};

const ComponentCard: React.FC<{
  progress: number;
  title: string;
  lines: string[];
  tone: "blue" | "green" | "gold";
}> = ({ progress, title, lines, tone }) => {
  const color = tone === "blue" ? colors.blue : tone === "gold" ? colors.gold : colors.green;
  return (
    <div
      style={{
        opacity: progress,
        transform: `translateY(${(1 - progress) * 20}px)`,
        background: colors.panel,
        border: `1px solid ${colors.line}`,
        borderTop: `8px solid ${color}`,
        borderRadius: 8,
        padding: 32,
        minHeight: 260,
      }}
    >
      <div style={{ fontSize: 32, fontWeight: 820, marginBottom: 24 }}>{title}</div>
      {lines.map((line) => (
        <div key={line} style={{ fontSize: 25, color: colors.muted, lineHeight: 1.48 }}>
          {line}
        </div>
      ))}
    </div>
  );
};

const FlowCard: React.FC<{ progress: number; title: string; from: string; to: string }> = ({
  progress,
  title,
  from,
  to,
}) => (
  <div
    style={{
      opacity: progress,
      transform: `translateY(${(1 - progress) * 20}px)`,
      background: colors.dark,
      color: colors.bg,
      borderRadius: 8,
      padding: 34,
      minHeight: 190,
    }}
  >
    <div style={{ color: "#b9c8ad", fontSize: 23, fontWeight: 800, marginBottom: 20 }}>{title}</div>
    <div style={{ display: "flex", alignItems: "center", gap: 24, fontSize: 34, fontWeight: 820 }}>
      <span>{from}</span>
      <span style={{ color: colors.gold }}>→</span>
      <span>{to}</span>
    </div>
  </div>
);

const SetupTerminal: React.FC = () => (
  <Scene>
    <SceneHeader
      eyebrow="Setup"
      title="Builders clone the repo and start the Sepolia profile"
      subtitle="The keys are testnet-only. The demo refuses to start while placeholders remain."
    />
    <Terminal
      title="Terminal"
      lines={[
        "git clone https://github.com/BrianSeong99/miden-testnet-bridge.git",
        "cd miden-testnet-bridge",
        "cp .env.sepolia.example .env",
        "perl -0pi -e \"s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$(openssl rand -hex 32)/\" .env",
        "# fill Sepolia RPC, test mnemonic, funded solver key, funded test-user key",
        "make sepolia",
        "./bin/bridgectl status",
        "./bin/bridgectl tokens",
      ]}
    />
  </Scene>
);

const ApiSurface: React.FC = () => (
  <Scene>
    <SceneHeader
      eyebrow="Integration contract"
      title="Apps depend on the 1Click-shaped /v0 API, not the demo helpers"
      subtitle="/demo/* and /lab are Anvil-only local helpers. Sepolia builders use /v0/*."
    />
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 32, marginTop: 42 }}>
      <Endpoint method="GET" path="/v0/tokens" note="Discover supported asset ids" />
      <Endpoint method="POST" path="/v0/quote" note="Create inbound or outbound quote" />
      <Endpoint method="POST" path="/v0/deposit/submit" note="Submit landed Sepolia deposit tx hash" />
      <Endpoint method="GET" path="/v0/status" note="Poll lifecycle and tx metadata" />
    </div>
    <div
      style={{
        marginTop: 42,
        padding: 28,
        borderRadius: 8,
        border: `1px solid ${colors.red}`,
        color: colors.red,
        fontSize: 28,
        fontWeight: 760,
        background: "#fff8f4",
      }}
    >
      Keep app integrations off `/demo/*`. The mock should feel like NEAR
      Intents 1Click from the client side.
    </div>
  </Scene>
);

const Endpoint: React.FC<{ method: string; path: string; note: string }> = ({ method, path, note }) => (
  <div
    style={{
      background: colors.panel,
      border: `1px solid ${colors.line}`,
      borderRadius: 8,
      padding: 30,
      minHeight: 150,
    }}
  >
    <div style={{ display: "flex", alignItems: "center", gap: 16, marginBottom: 16 }}>
      <span
        style={{
          fontFamily: mono,
          fontSize: 22,
          color: colors.bg,
          background: colors.green,
          padding: "7px 12px",
          borderRadius: 6,
          fontWeight: 800,
        }}
      >
        {method}
      </span>
      <span style={{ fontFamily: mono, fontSize: 30, fontWeight: 800 }}>{path}</span>
    </div>
    <div style={{ fontSize: 24, color: colors.muted }}>{note}</div>
  </div>
);

const BrowserClip: React.FC<{
  title: string;
  subtitle: string;
  clip: "repo" | "guide" | "evidence";
  fallback: string;
}> = ({ title, subtitle, clip, fallback }) => (
  <Scene>
    <SceneHeader eyebrow="Recorded with Playwright" title={title} subtitle={subtitle} compact />
    <div
      style={{
        marginTop: 28,
        borderRadius: 8,
        overflow: "hidden",
        border: `1px solid ${colors.line}`,
        height: 760,
        background: colors.dark,
        boxShadow: "0 24px 80px rgba(37, 35, 28, 0.15)",
      }}
    >
      <MediaOrFallback clip={clip} fallback={fallback} />
    </div>
  </Scene>
);

const MediaOrFallback: React.FC<{ clip: "repo" | "guide" | "evidence"; fallback: string }> = ({
  clip,
  fallback,
}) => {
  return (
    <>
      <OffthreadVideo
        src={staticFile(`assets/${clip}.webm`)}
        muted
        startFrom={0}
        style={{ width: "100%", height: "100%", objectFit: "cover" }}
      />
      <Img
        src={staticFile(`assets/${fallback}.png`)}
        style={{
          position: "absolute",
          opacity: 0.001,
          width: 1,
          height: 1,
        }}
      />
    </>
  );
};

const InboundFlow: React.FC = () => (
  <Scene>
    <SceneHeader
      eyebrow="Inbound"
      title="Sepolia ETH becomes a public Miden P2ID payout note"
      subtitle="Bridge SUCCESS means the note is committed and consumable; the wallet still claims it."
    />
    <div style={{ display: "grid", gridTemplateColumns: "0.95fr 1.05fr", gap: 32, marginTop: 38 }}>
      <FlowSteps
        steps={[
          "POST /v0/quote eth-sepolia:eth to miden-testnet:eth",
          "User sends native ETH to depositAddress",
          "App calls /v0/deposit/submit with tx hash",
          "Bridge verifies Sepolia tx and submits P2ID mint",
          "Recipient consumes public Miden note",
        ]}
      />
      <Terminal
        compact
        title="Inbound commands"
        lines={[
          'curl -s http://localhost:8080/v0/quote \\',
          '  -H "content-type: application/json" \\',
          '  -d \'{"originAsset":"eth-sepolia:eth","destinationAsset":"miden-testnet:eth",...}\'',
          "",
          'curl -s http://localhost:8080/v0/deposit/submit \\',
          '  -d \'{"txHash":"0x...","depositAddress":"0x..."}\'',
          "",
          'curl -s "http://localhost:8080/v0/status?depositAddress=0x..."',
        ]}
      />
    </div>
  </Scene>
);

const OutboundFlow: React.FC = () => (
  <Scene>
    <SceneHeader
      eyebrow="Outbound"
      title="A public Miden BridgeOutV1 note releases Sepolia ETH"
      subtitle="The bridge watches public notes targeted to its stable bridge account."
    />
    <div style={{ display: "grid", gridTemplateColumns: "0.95fr 1.05fr", gap: 32, marginTop: 38 }}>
      <FlowSteps
        steps={[
          "POST /v0/quote miden-testnet:eth to eth-sepolia:eth",
          "Quote returns bridge account and depositMemo",
          "User creates public BridgeOutV1 note on Miden",
          "Poller validates quote hash, faucet, target, amount",
          "Bridge consumes note and releases Sepolia ETH",
        ]}
      />
      <Terminal
        compact
        title="Outbound fields"
        lines={[
          "correlationId",
          "quote.depositAddress  # stable Miden bridge account",
          "quote.depositMemo     # BridgeOutV1 payload",
          "",
          "status query:",
          "GET /v0/status?",
          "  depositAddress=<miden-bridge-account-address>",
          "  depositMemo=<deposit-memo>",
        ]}
      />
    </div>
  </Scene>
);

const FlowSteps: React.FC<{ steps: string[] }> = ({ steps }) => {
  const frame = useCurrentFrame();
  return (
    <div>
      {steps.map((step, i) => {
        const progress = interpolate(frame, [i * 12, i * 12 + 18], [0, 1], {
          extrapolateLeft: "clamp",
          extrapolateRight: "clamp",
        });
        return (
          <div
            key={step}
            style={{
              display: "grid",
              gridTemplateColumns: "54px 1fr",
              gap: 18,
              alignItems: "start",
              marginBottom: 22,
              opacity: progress,
              transform: `translateX(${(1 - progress) * -24}px)`,
            }}
          >
            <div
              style={{
                width: 48,
                height: 48,
                borderRadius: 24,
                background: colors.green,
                color: colors.bg,
                display: "grid",
                placeItems: "center",
                fontWeight: 850,
                fontSize: 22,
              }}
            >
              {i + 1}
            </div>
            <div
              style={{
                background: colors.panel,
                border: `1px solid ${colors.line}`,
                borderRadius: 8,
                padding: "20px 22px",
                fontSize: 25,
                lineHeight: 1.28,
                fontWeight: 720,
              }}
            >
              {step}
            </div>
          </div>
        );
      })}
    </div>
  );
};

const Evidence: React.FC = () => (
  <Scene>
    <SceneHeader
      eyebrow="Evidence"
      title="The acceptance proof is public testnet evidence"
      subtitle="The video is a tutorial. The evidence remains tx hashes, Miden ids, and SUCCESS statuses."
      compact
    />
    <div style={{ display: "grid", gridTemplateColumns: "1.1fr 0.9fr", gap: 28, marginTop: 28 }}>
      <div
        style={{
          height: 720,
          borderRadius: 8,
          overflow: "hidden",
          border: `1px solid ${colors.line}`,
          background: colors.dark,
        }}
      >
        <OffthreadVideo
          src={staticFile("assets/evidence.webm")}
          muted
          style={{ width: "100%", height: "100%", objectFit: "cover" }}
        />
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
        <EvidenceCard label="Inbound" value="Sepolia ETH → Miden P2ID → claim" />
        <EvidenceCard label="Outbound" value="BridgeOutV1 note → consume → Sepolia release" />
        <EvidenceCard label="Checks" value="Etherscan links, Miden tx ids, final SUCCESS" />
        <div
          style={{
            marginTop: 8,
            padding: 26,
            borderRadius: 8,
            background: "#fff8f4",
            border: `1px solid ${colors.red}`,
            color: colors.red,
            fontSize: 24,
            lineHeight: 1.34,
            fontWeight: 760,
          }}
        >
          Do not claim validation from the walkthrough alone. Claim validation
          only from live Sepolia and Miden evidence.
        </div>
      </div>
    </div>
  </Scene>
);

const EvidenceCard: React.FC<{ label: string; value: string }> = ({ label, value }) => (
  <div
    style={{
      background: colors.panel,
      border: `1px solid ${colors.line}`,
      borderRadius: 8,
      padding: 28,
    }}
  >
    <div style={{ fontSize: 21, color: colors.muted, fontWeight: 760, marginBottom: 8 }}>{label}</div>
    <div style={{ fontSize: 30, lineHeight: 1.18, fontWeight: 820 }}>{value}</div>
  </div>
);

const Closing: React.FC = () => (
  <AbsoluteFill style={{ ...shell, padding: 88, justifyContent: "center" }}>
    <div style={{ maxWidth: 1180 }}>
      <Badge tone="blue">Reproduce it</Badge>
      <h2 style={{ fontSize: 86, lineHeight: 0.98, margin: "30px 0 24px", letterSpacing: 0 }}>
        Start with the Sepolia builder guide
      </h2>
      <Terminal
        title="Links"
        lines={[
          "https://github.com/BrianSeong99/miden-testnet-bridge",
          "docs/builder-testing-guide.md",
          "docs/smoke-test-report.html",
          "",
          "Default: Sepolia + public Miden testnet",
          "Fallback: docs/anvil/local-sandbox.md",
        ]}
      />
    </div>
  </AbsoluteFill>
);

const Terminal: React.FC<{ title: string; lines: string[]; compact?: boolean }> = ({
  title,
  lines,
  compact = false,
}) => (
  <div
    style={{
      background: colors.dark,
      color: "#f3f0e8",
      borderRadius: 8,
      border: "1px solid #2a3028",
      overflow: "hidden",
      boxShadow: "0 24px 80px rgba(37, 35, 28, 0.15)",
    }}
  >
    <div
      style={{
        height: 48,
        display: "flex",
        alignItems: "center",
        padding: "0 18px",
        gap: 9,
        borderBottom: "1px solid #2a3028",
        color: "#9ea895",
        fontSize: 18,
        fontFamily: mono,
      }}
    >
      <Dot color="#e75f4d" />
      <Dot color="#e8bb4d" />
      <Dot color="#58b66d" />
      <span style={{ marginLeft: 12 }}>{title}</span>
    </div>
    <div style={{ padding: compact ? 24 : 34, fontFamily: mono, fontSize: compact ? 20 : 23, lineHeight: 1.6 }}>
      {lines.map((line, i) => (
        <div key={`${line}-${i}`} style={{ color: line.startsWith("#") ? "#9ea895" : "#f3f0e8" }}>
          {line || "\u00a0"}
        </div>
      ))}
    </div>
  </div>
);

const Dot: React.FC<{ color: string }> = ({ color }) => (
  <span style={{ width: 12, height: 12, borderRadius: 6, background: color, display: "inline-block" }} />
);

const SceneHeader: React.FC<{
  eyebrow: string;
  title: string;
  subtitle: string;
  compact?: boolean;
}> = ({ eyebrow, title, subtitle, compact = false }) => (
  <div>
    <Badge tone="green">{eyebrow}</Badge>
    <h2
      style={{
        margin: "24px 0 14px",
        fontSize: compact ? 58 : 72,
        lineHeight: 1,
        letterSpacing: 0,
        fontWeight: 850,
        maxWidth: 1300,
      }}
    >
      {title}
    </h2>
    <p style={{ margin: 0, fontSize: compact ? 26 : 30, lineHeight: 1.32, color: colors.muted, maxWidth: 1280 }}>
      {subtitle}
    </p>
  </div>
);

const Scene: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const opacity = useFade(0, 240);
  return (
    <AbsoluteFill style={{ ...shell, padding: 74, opacity }}>
      {children}
    </AbsoluteFill>
  );
};

export const BridgeWalkthrough: React.FC = () => {
  return (
    <AbsoluteFill style={shell}>
      <Sequence from={0} durationInFrames={210}>
        <TitleCard />
      </Sequence>
      <Sequence from={210} durationInFrames={240}>
        <Architecture />
      </Sequence>
      <Sequence from={450} durationInFrames={270}>
        <SetupTerminal />
      </Sequence>
      <Sequence from={720} durationInFrames={240}>
        <ApiSurface />
      </Sequence>
      <Sequence from={960} durationInFrames={270}>
        <BrowserClip
          title="The guide is Sepolia-first"
          subtitle="A builder or agent starts here, then uses the local mock /v0 API."
          clip="guide"
          fallback="guide"
        />
      </Sequence>
      <Sequence from={1230} durationInFrames={255}>
        <InboundFlow />
      </Sequence>
      <Sequence from={1485} durationInFrames={255}>
        <OutboundFlow />
      </Sequence>
      <Sequence from={1740} durationInFrames={300}>
        <Evidence />
      </Sequence>
      <Sequence from={2040} durationInFrames={120}>
        <Closing />
      </Sequence>
    </AbsoluteFill>
  );
};
