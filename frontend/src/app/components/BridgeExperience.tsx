"use client";

import {
  ArrowDown,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  Copy,
  ExternalLink,
  LogOut,
  RefreshCcw,
  ShieldCheck,
  Wallet,
} from "lucide-react";
import dynamic from "next/dynamic";
import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatEther } from "viem";
import { AGGLAYER_BALI, buildSepoliaDepositTransaction, isMidenAccountHex, normalizeMidenAccountHex } from "../lib/agglayer";
import {
  type BridgeProvider,
  type FlowMode,
  type Activity,
  createActivity,
  loadStoredActivities,
  modes,
  providers,
  quoteFor,
  saveActivities,
  shortAddress,
  statusLabel,
  statusTone,
  seedActivities,
} from "../lib/bridge-state";
import {
  type EvmConnectionKind,
  type EvmProvider,
  connectEvmProvider,
  ensureSepolia,
  evmConnectionLabel,
  getExistingEvmConnection,
} from "../lib/evm-wallet";

type MidenWalletSnapshot = {
  address: string;
  connected: boolean;
  error: string;
  balanceText: string;
  noteSyncStatus: string;
  consumableNoteCount: number | null;
};

const emptyMidenWallet: MidenWalletSnapshot = {
  address: "",
  connected: false,
  error: "",
  balanceText: "Not connected",
  noteSyncStatus: "Not connected",
  consumableNoteCount: null,
};

function providerFromParam(value: string | null): BridgeProvider | null {
  if (value === "near-intents" || value === "agglayer" || value === "epoch") return value;
  return null;
}

function modeFromIntent(value: string | null): FlowMode | null {
  if (value === "receive" || value === "deposit") return "receive";
  if (value === "send" || value === "withdraw") return "send";
  return null;
}

const MidenWalletButton = dynamic(() => import("./MidenWalletButton").then((mod) => mod.MidenWalletButton), {
  ssr: false,
  loading: () => (
    <button className="wallet-button wallet-pill" type="button" disabled>
      <span className="wallet-icon">
        <ShieldCheck size={16} aria-hidden="true" />
      </span>
      <span className="wallet-copy">
        <small>
          <span className="wallet-status-dot pending" />
          Miden
        </small>
        <span>Loading</span>
      </span>
    </button>
  ),
});

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "message" in error) return String(error.message);
  return "Something went wrong. Try again.";
}

function compactTokenAmount(value: string) {
  const [whole, fraction = ""] = value.split(".");
  const compactFraction = fraction.slice(0, 4).replace(/0+$/, "");
  return compactFraction ? `${whole}.${compactFraction}` : whole;
}

export function BridgeExperience() {
  const router = useRouter();
  const [provider, setProvider] = useState<BridgeProvider>("agglayer");
  const [mode, setMode] = useState<FlowMode>("receive");
  const [amount, setAmount] = useState("100");
  const [destination, setDestination] = useState("");
  const [evmProvider, setEvmProvider] = useState<EvmProvider | null>(null);
  const [evmConnectionKind, setEvmConnectionKind] = useState<EvmConnectionKind | "">("");
  const [walletConnected, setWalletConnected] = useState(false);
  const [walletAccount, setWalletAccount] = useState("");
  const [evmBalance, setEvmBalance] = useState("");
  const [midenWallet, setMidenWallet] = useState<MidenWalletSnapshot>(emptyMidenWallet);
  const [launchMidenAccount, setLaunchMidenAccount] = useState("");
  const [walletError, setWalletError] = useState("");
  const [bridgeError, setBridgeError] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [activities, setActivities] = useState<Activity[]>(seedActivities);
  const [hydrated, setHydrated] = useState(false);
  const [evmMenuOpen, setEvmMenuOpen] = useState(false);
  const [evmCopied, setEvmCopied] = useState(false);
  const evmMenuRef = useRef<HTMLDivElement>(null);
  const [routeMenuOpen, setRouteMenuOpen] = useState(false);
  const routeMenuRef = useRef<HTMLDivElement>(null);

  const copy = modes[mode];
  const providerCopy = providers[provider];
  const quote = useMemo(() => quoteFor(mode, provider, amount), [amount, mode, provider]);
  const isLiveAgglayerReceive = provider === "agglayer" && mode === "receive";
  const midenAddress = midenWallet.address || launchMidenAccount;
  const evmWalletLabel = evmConnectionLabel(evmConnectionKind);
  const evmBalanceText = walletConnected ? evmBalance || "Balance unavailable" : "Not connected";
  const midenBalanceText = midenWallet.connected ? midenWallet.balanceText : launchMidenAccount ? "Launch account" : "Not connected";
  const sourceBalance = mode === "receive" ? evmBalanceText : midenBalanceText;
  const destinationBalance = mode === "receive" ? midenBalanceText : evmBalanceText;
  const hasDestination = Boolean(destination.trim() || (mode === "receive" ? midenAddress : walletAccount));
  const routeTone = providers[provider].disabled ? "disabled" : provider === "near-intents" ? "mock" : "testnet";
  const routeNote =
    provider === "near-intents"
      ? "NEAR Intents is paused in this build while AggLayer and Epoch are the active testnet routes."
      : provider === "agglayer"
        ? mode === "receive"
          ? "Slow testnet route. Your Sepolia wallet sends to Miden through AggLayer with no provider bridge fee."
          : "Slow testnet route. Miden-side bridge note creation is tracked separately; claimAsset is submitted on Sepolia when proof is ready."
        : "Testnet route. Epoch integration status is tracked from activity details.";
  const primaryActionLabel = isSubmitting
    ? "Waiting for wallet"
    : isLiveAgglayerReceive && !walletConnected
      ? "Connect Sepolia wallet"
      : isLiveAgglayerReceive && !hasDestination
        ? "Add Miden account"
        : isLiveAgglayerReceive
          ? "Receive on Miden"
          : mode === "receive"
            ? "Start receive"
            : "Start send";
  const destinationHelp = isLiveAgglayerReceive
    ? midenWallet.connected
      ? `Leave empty to use connected Miden wallet ${shortAddress(midenAddress)}, or paste a 30-hex account ID to override.`
      : launchMidenAccount
        ? `Preloaded from wallet launch: ${shortAddress(launchMidenAccount)}. Connect MidenFi before signing Miden-side actions.`
      : "Connect Miden wallet, or paste a 30-hex Miden account ID from the Miden CLI."
    : mode === "send" && walletConnected
      ? `Leave empty to use connected Sepolia wallet ${shortAddress(walletAccount)}, or paste another address.`
      : "Paste the destination account for this transfer.";
  const showDestinationHelp = isLiveAgglayerReceive || (mode === "send" && walletConnected);
  const destinationPlaceholder = isLiveAgglayerReceive ? "Miden account ID or address" : copy.destinationPlaceholder;
  const handleMidenWalletState = useCallback((next: MidenWalletSnapshot) => setMidenWallet(next), []);

  useEffect(() => {
    try {
      const stored = loadStoredActivities();
      queueMicrotask(() => setActivities(stored));
    } catch {
      queueMicrotask(() => setActivities(seedActivities));
    } finally {
      setHydrated(true);
    }
  }, []);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const nextMode = modeFromIntent(params.get("intent") ?? params.get("mode"));
    const nextProvider = providerFromParam(params.get("provider") ?? params.get("route"));
    const nextMidenAccount = params.get("midenAccount") ?? params.get("miden_account") ?? params.get("account");
    const nextEvmAddress = params.get("evmAddress") ?? params.get("evm_address") ?? params.get("recipient");
    const resolvedMode = nextMode ?? "receive";

    queueMicrotask(() => {
      if (nextProvider && !providers[nextProvider].disabled) setProvider(nextProvider);
      if (nextMode) setMode(nextMode);

      if (nextMidenAccount) {
        setLaunchMidenAccount(nextMidenAccount);
        if (resolvedMode === "receive") {
          setDestination(nextMidenAccount);
        }
      }

      if (nextEvmAddress && resolvedMode === "send") {
        setDestination(nextEvmAddress);
      }
    });
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    saveActivities(activities);
  }, [activities, hydrated]);

  useEffect(() => {
    if (!evmMenuOpen) return;

    function closeMenu(event: MouseEvent | PointerEvent) {
      if (!evmMenuRef.current?.contains(event.target as Node)) setEvmMenuOpen(false);
    }

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setEvmMenuOpen(false);
    }

    document.addEventListener("pointerdown", closeMenu);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeMenu);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [evmMenuOpen]);

  useEffect(() => {
    if (!routeMenuOpen) return;

    function closeMenu(event: MouseEvent | PointerEvent) {
      if (!routeMenuRef.current?.contains(event.target as Node)) setRouteMenuOpen(false);
    }

    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setRouteMenuOpen(false);
    }

    document.addEventListener("pointerdown", closeMenu);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeMenu);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [routeMenuOpen]);

  useEffect(() => {
    getExistingEvmConnection()
      .then(async (connection) => {
        if (!connection) return;
        const accounts = await connection.provider.request<string[]>({ method: "eth_accounts" });
        const account = accounts[0] ?? "";
        setEvmProvider(connection.provider);
        setEvmConnectionKind(connection.kind);
        setEvmBalance("");
        setWalletAccount(account);
        setWalletConnected(Boolean(account));
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!evmProvider) return;
    const handleAccounts = (...args: unknown[]) => {
      const accounts = Array.isArray(args[0]) ? (args[0] as string[]) : [];
      const account = accounts[0] ?? "";
      setEvmBalance("");
      setWalletAccount(account);
      setWalletConnected(Boolean(account));
    };
    const handleDisconnect = () => {
      setEvmProvider(null);
      setEvmConnectionKind("");
      setEvmBalance("");
      setWalletAccount("");
      setWalletConnected(false);
      setEvmMenuOpen(false);
    };

    evmProvider.on?.("accountsChanged", handleAccounts);
    evmProvider.on?.("disconnect", handleDisconnect);
    return () => {
      evmProvider.removeListener?.("accountsChanged", handleAccounts);
      evmProvider.removeListener?.("disconnect", handleDisconnect);
    };
  }, [evmProvider]);

  useEffect(() => {
    if (!walletAccount) return;

    let cancelled = false;
    fetch(`/api/sepolia/balance?address=${walletAccount}`)
      .then((response) => (response.ok ? response.json() : Promise.reject(new Error("Unable to fetch balance"))))
      .then((payload: { balanceWei: string }) => {
        if (cancelled) return;
        setEvmBalance(`${compactTokenAmount(formatEther(BigInt(payload.balanceWei)))} ETH`);
      })
      .catch(() => {
        if (!cancelled) setEvmBalance("");
      });

    return () => {
      cancelled = true;
    };
  }, [walletAccount]);

  function selectMode(nextMode: FlowMode) {
    setMode(nextMode);
    setAmount(nextMode === "receive" ? "100" : "0.25");
    setDestination("");
    setBridgeError("");
  }

  function selectProvider(nextProvider: BridgeProvider) {
    if (providers[nextProvider].disabled) return;
    setProvider(nextProvider);
    setBridgeError("");
    if (nextProvider === "agglayer" && mode === "receive" && !isMidenAccountHex(destination)) {
      setDestination("");
    }
  }

  function selectRouteOption(nextProvider: BridgeProvider) {
    selectProvider(nextProvider);
    setRouteMenuOpen(false);
  }

  async function connectWalletSession() {
    setWalletError("");
    const connection = await connectEvmProvider();
    const accounts = await connection.provider.request<string[]>({ method: "eth_accounts" });
    const account = accounts[0] ?? "";
    setEvmProvider(connection.provider);
    setEvmConnectionKind(connection.kind);
    setEvmBalance("");
    setWalletAccount(account);
    setWalletConnected(Boolean(account));
    return { account, provider: connection.provider };
  }

  async function connectWallet() {
    const session = await connectWalletSession();
    return session.account;
  }

  async function openWalletPermissions() {
    setWalletError("");
    const provider = evmProvider;
    if (!provider) return;

    try {
      await provider.request({
        method: "wallet_requestPermissions",
        params: [{ eth_accounts: {} }],
      });
      const accounts = await provider.request<string[]>({ method: "eth_accounts" });
      const account = accounts[0] ?? "";
      setEvmBalance("");
      setWalletAccount(account);
      setWalletConnected(Boolean(account));
      setEvmMenuOpen(false);
    } catch (permissionError) {
      const code =
        typeof permissionError === "object" && permissionError && "code" in permissionError
          ? Number(permissionError.code)
          : 0;
      if (code === 4001) {
        setWalletError("Wallet account permission was rejected.");
        return;
      }

      try {
        await connectWallet();
        setEvmMenuOpen(false);
      } catch (connectError) {
        setWalletError(errorMessage(connectError));
      }
    }
  }

  async function copyEvmAddress() {
    if (!walletAccount) return;

    try {
      await navigator.clipboard.writeText(walletAccount);
      setEvmCopied(true);
      window.setTimeout(() => setEvmCopied(false), 1400);
    } catch {
      setWalletError("Could not copy the Sepolia address from this browser.");
    }
  }

  function forgetEvmWallet() {
    void evmProvider?.disconnect?.().catch(() => undefined);
    setEvmProvider(null);
    setEvmConnectionKind("");
    setEvmBalance("");
    setWalletAccount("");
    setWalletConnected(false);
    setEvmMenuOpen(false);
  }

  async function switchSepoliaFromMenu() {
    try {
      const provider = evmProvider;
      if (!provider) throw new Error("Connect your EVM wallet first.");
      await ensureSepolia(provider);
      setEvmMenuOpen(false);
    } catch (error) {
      setWalletError(errorMessage(error));
    }
  }

  async function handleEvmWalletClick() {
    if (walletConnected) {
      setEvmMenuOpen((open) => !open);
      return;
    }

    try {
      await connectWallet();
    } catch (error) {
      setWalletError(errorMessage(error));
    }
  }

  async function submitTransfer() {
    setBridgeError("");
    setWalletError("");

    if (isLiveAgglayerReceive) {
      setIsSubmitting(true);
      try {
        let activeProvider = evmProvider;
        let account = walletAccount;
        if (!activeProvider || !account) {
          const session = await connectWalletSession();
          activeProvider = session.provider;
          account = session.account;
        }
        if (!account) return;

        await ensureSepolia(activeProvider);
        const destinationAccount = destination.trim() || midenAddress;
        if (!destinationAccount) {
          throw new Error("Connect Miden wallet or paste a Miden account ID before receiving.");
        }
        const midenAccountId = normalizeMidenAccountHex(destinationAccount);
        const transaction = buildSepoliaDepositTransaction({ amountEth: amount, midenAccountId });
        const txHash = await activeProvider.request<string>({
          method: "eth_sendTransaction",
          params: [
            {
              from: account,
              to: transaction.to,
              data: transaction.data,
              value: transaction.value,
              gas: transaction.gas,
            },
          ],
        });

        const next = createActivity(mode, provider, amount, {
          status: "source_finality",
          eta: "About 15 min",
          destination: destinationAccount,
          bridgeDestinationAddress: transaction.destinationAddress,
          txHash: shortAddress(txHash),
          sourceTxHash: txHash,
          midenTxId: transaction.destinationAddress,
          sourceNetworkId: AGGLAYER_BALI.sourceNetworkId,
          destinationNetworkId: AGGLAYER_BALI.destinationNetworkId,
        });
        const updated = [next, ...activities];
        setActivities(updated);
        saveActivities(updated);
        router.push(`/activity/${next.id}`);
      } catch (error) {
        setBridgeError(errorMessage(error));
      } finally {
        setIsSubmitting(false);
      }
      return;
    }

    const resolvedDestination = destination.trim() || (mode === "receive" ? midenAddress : walletAccount);
    const next = createActivity(mode, provider, amount, { destination: resolvedDestination });
    const updated = [next, ...activities];
    setActivities(updated);
    saveActivities(updated);
    router.push(`/activity/${next.id}`);
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <Link className="brand" href="/" aria-label="Miden bridge home">
          <Image src="/miden-logo-horizontal.svg" alt="Miden" width={112} height={34} priority />
          <span>Bridge</span>
        </Link>

        <div className="wallet-cluster" aria-label="Connected wallets">
          <div className="wallet-menu-root" ref={evmMenuRef}>
            <button
              className={`wallet-button wallet-pill ${walletConnected ? "connected" : ""}`}
              type="button"
              onClick={handleEvmWalletClick}
              aria-expanded={walletConnected ? evmMenuOpen : undefined}
              aria-haspopup={walletConnected ? "menu" : undefined}
            >
              <span className={`wallet-icon ${walletConnected ? "connected" : ""}`}>
                <Wallet size={16} aria-hidden="true" />
              </span>
              <span className="wallet-copy">
                <small>
                  <span className={`wallet-status-dot ${walletConnected ? "connected" : ""}`} />
                  {evmWalletLabel}
                </small>
                <span>{walletConnected ? shortAddress(walletAccount) : "Connect wallet"}</span>
              </span>
              {walletConnected ? <span className="wallet-balance">{evmBalanceText}</span> : null}
              {walletConnected ? <ChevronDown className="wallet-menu-chevron" size={15} aria-hidden="true" /> : null}
            </button>

            {walletConnected ? (
              <div className={`wallet-actions-menu ${evmMenuOpen ? "open" : ""}`} role="menu">
                <div className="wallet-menu-summary">
                  <span className="wallet-menu-avatar connected">
                    <Wallet size={16} aria-hidden="true" />
                  </span>
                  <span>
                    <strong>{evmWalletLabel}</strong>
                    <small>
                      {shortAddress(walletAccount)} · {evmBalanceText}
                    </small>
                  </span>
                </div>
                <button type="button" role="menuitem" className="wallet-menu-item" onClick={openWalletPermissions}>
                  <RefreshCcw size={15} aria-hidden="true" />
                  <span>Account permissions</span>
                </button>
                <button type="button" role="menuitem" className="wallet-menu-item" onClick={switchSepoliaFromMenu}>
                  <ShieldCheck size={15} aria-hidden="true" />
                  <span>Switch to Sepolia</span>
                </button>
                <button type="button" role="menuitem" className="wallet-menu-item" onClick={copyEvmAddress}>
                  <Copy size={15} aria-hidden="true" />
                  <span>{evmCopied ? "Copied" : "Copy address"}</span>
                </button>
                <a
                  role="menuitem"
                  className="wallet-menu-item"
                  href={`${AGGLAYER_BALI.sepoliaExplorer}/address/${walletAccount}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  <ExternalLink size={15} aria-hidden="true" />
                  <span>View on Etherscan</span>
                </a>
                <span className="wallet-menu-separator" />
                <button type="button" role="menuitem" className="wallet-menu-item danger" onClick={forgetEvmWallet}>
                  <LogOut size={15} aria-hidden="true" />
                  <span>Forget in app</span>
                </button>
              </div>
            ) : null}
          </div>

          <MidenWalletButton onStateChange={handleMidenWalletState} />
        </div>
      </header>
      {midenWallet.error ? <p className="form-error topbar-error">{midenWallet.error}</p> : null}

      <section className="swap-stage">
        <section className="swap-card" aria-label="Miden bridge">
          <div className="swap-card-top">
            <h1>Bridge</h1>
            <div className="route-menu-root" ref={routeMenuRef}>
              <button
                className="route-trigger"
                type="button"
                aria-expanded={routeMenuOpen}
                aria-haspopup="listbox"
                onClick={() => setRouteMenuOpen((open) => !open)}
              >
                <span>Route</span>
                <strong>{providerCopy.label}</strong>
                <ChevronDown size={15} aria-hidden="true" />
              </button>

              {routeMenuOpen ? (
                <div className="route-options-menu open" role="listbox" aria-label="Bridge route">
                  {(Object.keys(providers) as BridgeProvider[]).map((key) => {
                    const option = providers[key];
                    const selected = key === provider;
                    const disabled = option.disabled === true;
                    return (
                      <button
                        className={`route-option ${selected ? "selected" : ""} ${disabled ? "disabled" : ""}`}
                        type="button"
                        role="option"
                        aria-selected={selected}
                        aria-disabled={disabled}
                        disabled={disabled}
                        key={key}
                        onClick={() => selectRouteOption(key)}
                      >
                        <span>
                          <strong>{option.label}</strong>
                          <small>{option.badge}</small>
                        </span>
                        <em>{option.route}</em>
                      </button>
                    );
                  })}
                </div>
              ) : null}
            </div>
          </div>
          <div className="route-status-line">
            <span className={`route-pill ${routeTone}`}>{providerCopy.badge}</span>
            <span>{providerCopy.route}</span>
          </div>
          {walletError ? <p className="form-error compact">{walletError}</p> : null}

          <div className="mode-switch" role="group" aria-label="Cross-chain direction" data-active={mode}>
            {(Object.keys(modes) as FlowMode[]).map((item) => (
              <button key={item} type="button" aria-pressed={item === mode} onClick={() => selectMode(item)}>
                {modes[item].label}
              </button>
            ))}
          </div>

          <div className="swap-box">
            <div>
              <span>From</span>
              <strong>{copy.from}</strong>
              <small className="balance-line">Available {sourceBalance}</small>
            </div>
            <label>
              <input
                aria-label="Amount"
                inputMode="decimal"
                value={amount}
                onChange={(event) => setAmount(event.target.value)}
              />
              <span>{copy.assetIn}</span>
            </label>
          </div>

          <div className="swap-divider" aria-hidden="true">
            <ArrowDown size={18} />
          </div>

          <div className="swap-box">
            <div>
              <span>To</span>
              <strong>{copy.to}</strong>
              <small className="balance-line">Wallet balance {destinationBalance}</small>
            </div>
            <label className="readonly-amount">
              <strong>{quote.expectedReceived}</strong>
              <span>Expected</span>
            </label>
          </div>

          <label className="destination-input">
            <span>{copy.destinationLabel}</span>
            <input
              value={destination}
              onChange={(event) => setDestination(event.target.value)}
              placeholder={destinationPlaceholder}
              aria-label={copy.destinationLabel}
            />
            {showDestinationHelp ? <small>{destinationHelp}</small> : null}
          </label>
          {midenWallet.connected ? (
            <div className="wallet-state-strip" aria-label="Miden wallet state">
              <span>
                <strong>Miden balance</strong>
                {midenWallet.balanceText}
              </span>
              <span>
                <strong>Note sync</strong>
                {midenWallet.noteSyncStatus}
              </span>
              <span>
                <strong>Consumable</strong>
                {midenWallet.consumableNoteCount ?? "Unknown"}
              </span>
            </div>
          ) : null}
          {bridgeError ? <p className="form-error">{bridgeError}</p> : null}

          <div className="quote-summary" aria-label="Route quote">
            <div>
              <span>ETA</span>
              <strong>{quote.eta}</strong>
            </div>
            <div>
              <span>Min received</span>
              <strong>{quote.minReceived}</strong>
            </div>
            <div>
              <span>Network fee</span>
              <strong>{quote.networkFee}</strong>
            </div>
          </div>

          <button className="primary-button" type="button" onClick={submitTransfer} disabled={isSubmitting}>
            {primaryActionLabel}
            <ArrowRight size={18} aria-hidden="true" />
          </button>

          <div className={`route-disclaimer ${routeTone}`}>
            <ShieldCheck size={16} aria-hidden="true" />
            <span>{routeNote}</span>
          </div>
        </section>

        <section className="home-activity" aria-label="Recent activity">
          <div className="home-activity-title">
            <h2>Activity</h2>
            <span>{activities.length}</span>
          </div>
          {activities.length > 0 ? (
            <div className="home-activity-list">
              {activities.slice(0, 3).map((activity) => (
                <Link className="home-activity-item" href={`/activity/${activity.id}`} key={activity.id}>
                  <span className={`status-dot ${statusTone(activity.status)}`} />
                  <span className="activity-copy">
                    <strong>{activity.summary}</strong>
                    <small>
                      {providers[activity.provider].label} - {statusLabel(activity.status)}
                    </small>
                  </span>
                  <span className="activity-meta">
                    <strong>
                      {activity.amount} {activity.asset}
                    </strong>
                    <small>{activity.updatedAt}</small>
                  </span>
                  <ChevronRight size={16} aria-hidden="true" />
                </Link>
              ))}
            </div>
          ) : (
            <div className="home-activity-empty">
              <strong>No transfers yet</strong>
              <span>Cross-chain activity will appear here after your first receive or send.</span>
            </div>
          )}
        </section>
      </section>
    </main>
  );
}
