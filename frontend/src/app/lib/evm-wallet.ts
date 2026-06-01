import { AGGLAYER_BALI } from "./agglayer";

export type EvmProvider = {
  request<T = unknown>(args: { method: string; params?: unknown[] | Record<string, unknown> }): Promise<T>;
  on?(event: "accountsChanged" | "chainChanged" | "disconnect" | "connect", listener: (...args: unknown[]) => void): void;
  removeListener?(event: "accountsChanged" | "chainChanged" | "disconnect" | "connect", listener: (...args: unknown[]) => void): void;
  disconnect?(): Promise<void>;
};

export type EvmConnectionKind = "walletconnect" | "injected";

export type EvmConnection = {
  provider: EvmProvider;
  kind: EvmConnectionKind;
};

const walletConnectProjectId = process.env.NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID ?? "";

let walletConnectProviderPromise: Promise<EvmProvider> | null = null;

export function isWalletConnectConfigured() {
  return walletConnectProjectId.trim().length > 0;
}

export function evmConnectionLabel(kind: EvmConnectionKind | "") {
  if (kind === "walletconnect") return "WalletConnect";
  if (kind === "injected") return "Injected wallet";
  return isWalletConnectConfigured() ? "WalletConnect" : "Sepolia";
}

export function getInjectedProvider(): EvmProvider | null {
  return typeof window === "undefined" ? null : window.ethereum ?? null;
}

async function getWalletConnectProvider() {
  if (!walletConnectProviderPromise) {
    walletConnectProviderPromise = import("@walletconnect/ethereum-provider").then(async ({ EthereumProvider }) => {
      const origin = window.location.origin;
      const provider = await EthereumProvider.init({
        projectId: walletConnectProjectId,
        chains: [AGGLAYER_BALI.sepoliaChainId],
        optionalChains: [AGGLAYER_BALI.sepoliaChainId],
        showQrModal: true,
        rpcMap: {
          [AGGLAYER_BALI.sepoliaChainId]: AGGLAYER_BALI.sepoliaRpcUrl,
        },
        metadata: {
          name: "Miden Bridge",
          description: "Miden testnet bridge UI",
          url: origin,
          icons: [`${origin}/favicon.ico`],
        },
        methods: [
          "eth_accounts",
          "eth_requestAccounts",
          "eth_sendTransaction",
          "wallet_switchEthereumChain",
          "wallet_addEthereumChain",
        ],
        events: ["accountsChanged", "chainChanged", "disconnect", "connect"],
      });
      return provider as EvmProvider;
    });
  }

  return walletConnectProviderPromise;
}

export async function connectEvmProvider(): Promise<EvmConnection> {
  if (isWalletConnectConfigured()) {
    const provider = await getWalletConnectProvider();
    await provider.request<string[]>({ method: "eth_requestAccounts" });
    return { provider, kind: "walletconnect" };
  }

  const injected = getInjectedProvider();
  if (!injected) {
    throw new Error("Set NEXT_PUBLIC_WALLETCONNECT_PROJECT_ID or open this app in a browser with an injected EVM wallet.");
  }
  await injected.request<string[]>({ method: "eth_requestAccounts" });
  return { provider: injected, kind: "injected" };
}

export async function getExistingEvmConnection(): Promise<EvmConnection | null> {
  if (isWalletConnectConfigured()) {
    const provider = await getWalletConnectProvider();
    const accounts = await provider.request<string[]>({ method: "eth_accounts" }).catch(() => []);
    if (accounts.length > 0) return { provider, kind: "walletconnect" };
  }

  const injected = getInjectedProvider();
  if (!injected) return null;
  const accounts = await injected.request<string[]>({ method: "eth_accounts" });
  return accounts.length > 0 ? { provider: injected, kind: "injected" } : null;
}

export async function ensureSepolia(provider: EvmProvider) {
  const chainId = await provider.request<string>({ method: "eth_chainId" });
  if (chainId === AGGLAYER_BALI.sepoliaChainHex) return;

  try {
    await provider.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: AGGLAYER_BALI.sepoliaChainHex }],
    });
  } catch (error) {
    const code = typeof error === "object" && error && "code" in error ? Number(error.code) : 0;
    if (code !== 4902) throw error;
    await provider.request({
      method: "wallet_addEthereumChain",
      params: [
        {
          chainId: AGGLAYER_BALI.sepoliaChainHex,
          chainName: "Ethereum Sepolia",
          nativeCurrency: { name: "Sepolia ETH", symbol: "ETH", decimals: 18 },
          rpcUrls: [AGGLAYER_BALI.sepoliaRpcUrl],
          blockExplorerUrls: [AGGLAYER_BALI.sepoliaExplorer],
        },
      ],
    });
  }
}
