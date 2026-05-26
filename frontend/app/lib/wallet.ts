export const SEPOLIA_CHAIN_ID = 11155111;
export const SEPOLIA_CHAIN_ID_HEX = "0xaa36a7";

const sepoliaAddChainParams = {
  chainId: SEPOLIA_CHAIN_ID_HEX,
  chainName: "Sepolia",
  nativeCurrency: {
    name: "Sepolia Ether",
    symbol: "ETH",
    decimals: 18,
  },
  rpcUrls: ["https://ethereum-sepolia-rpc.publicnode.com"],
  blockExplorerUrls: ["https://sepolia.etherscan.io"],
};

export type Eip1193Provider = {
  request<T = unknown>(request: { method: string; params?: unknown[] | Record<string, unknown> }): Promise<T>;
  on?(event: "accountsChanged" | "chainChanged", listener: (...args: unknown[]) => void): void;
  removeListener?(event: "accountsChanged" | "chainChanged", listener: (...args: unknown[]) => void): void;
};

export type InjectedEthereum = Eip1193Provider & {
  providers?: Eip1193Provider[];
};

declare global {
  interface Window {
    ethereum?: InjectedEthereum;
  }
}

export type ConnectedEvmWallet = {
  address: string;
  chainId: string;
  isSepolia: boolean;
};

export function injectedEthereum(): Eip1193Provider | null {
  if (typeof window === "undefined") return null;
  return window.ethereum?.providers?.[0] ?? window.ethereum ?? null;
}

export function normalizeEvmAddress(value: string) {
  const trimmed = value.trim();
  if (!/^0x[a-fA-F0-9]{40}$/.test(trimmed)) {
    throw new Error("Expected a 20-byte EVM address");
  }
  return trimmed.toLowerCase();
}

export function isEvmAddress(value: string) {
  try {
    normalizeEvmAddress(value);
    return true;
  } catch {
    return false;
  }
}

export function formatWalletAddress(value: string) {
  const normalized = normalizeEvmAddress(value);
  return `${normalized.slice(0, 6)}...${normalized.slice(-4)}`;
}

export function isSepoliaChain(chainId: string | number) {
  if (typeof chainId === "number") return chainId === SEPOLIA_CHAIN_ID;
  const normalized = chainId.trim().toLowerCase();
  if (normalized.startsWith("0x")) {
    return Number.parseInt(normalized, 16) === SEPOLIA_CHAIN_ID;
  }
  return Number(normalized) === SEPOLIA_CHAIN_ID;
}

export function isMidenAccountLike(value: string) {
  const trimmed = value.trim();
  return /^(0x)?[a-fA-F0-9]{30}$/.test(trimmed) || /^m[c,t]st1[a-z0-9]{20,}$/i.test(trimmed);
}

export function hexQuantity(value: string | number | bigint) {
  const bigintValue = typeof value === "bigint" ? value : BigInt(value);
  if (bigintValue < 0n) {
    throw new Error("Hex quantities must be non-negative");
  }
  return `0x${bigintValue.toString(16)}`;
}

export async function readConnectedEvmWallet(provider = injectedEthereum()): Promise<ConnectedEvmWallet | null> {
  if (!provider) return null;
  const accounts = await provider.request<string[]>({ method: "eth_accounts" });
  const address = accounts[0];
  if (!address) return null;
  const chainId = await provider.request<string>({ method: "eth_chainId" });
  return {
    address: normalizeEvmAddress(address),
    chainId,
    isSepolia: isSepoliaChain(chainId),
  };
}

export async function connectSepoliaWallet(provider = injectedEthereum()): Promise<ConnectedEvmWallet> {
  if (!provider) {
    throw new Error("No browser wallet found. Install MetaMask, Rabby, or another EVM wallet.");
  }

  const accounts = await provider.request<string[]>({ method: "eth_requestAccounts" });
  const address = accounts[0];
  if (!address) {
    throw new Error("No wallet account was selected.");
  }

  let chainId = await provider.request<string>({ method: "eth_chainId" });
  if (!isSepoliaChain(chainId)) {
    try {
      await provider.request({
        method: "wallet_switchEthereumChain",
        params: [{ chainId: SEPOLIA_CHAIN_ID_HEX }],
      });
    } catch (caught) {
      const code = typeof caught === "object" && caught && "code" in caught ? Number(caught.code) : undefined;
      if (code !== 4902) throw caught;
      await provider.request({
        method: "wallet_addEthereumChain",
        params: [sepoliaAddChainParams],
      });
    }
    chainId = await provider.request<string>({ method: "eth_chainId" });
  }

  return {
    address: normalizeEvmAddress(address),
    chainId,
    isSepolia: isSepoliaChain(chainId),
  };
}

export async function sendSepoliaTransaction(
  provider: Eip1193Provider,
  wallet: ConnectedEvmWallet,
  transaction: {
    to: string;
    valueWei: string;
    data?: string;
    gasLimit?: number | string;
  },
) {
  if (!wallet.isSepolia) {
    throw new Error("Switch the connected wallet to Sepolia before sending.");
  }
  const txHash = await provider.request<string>({
    method: "eth_sendTransaction",
    params: [
      {
        from: wallet.address,
        to: normalizeEvmAddress(transaction.to),
        value: hexQuantity(transaction.valueWei),
        data: transaction.data,
        gas: transaction.gasLimit ? hexQuantity(transaction.gasLimit) : undefined,
      },
    ],
  });
  return txHash;
}
