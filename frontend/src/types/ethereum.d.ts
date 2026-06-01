type EthereumProvider = {
  request<T = unknown>(args: { method: string; params?: unknown[] | Record<string, unknown> }): Promise<T>;
  on?(event: "accountsChanged" | "chainChanged" | "disconnect" | "connect", listener: (...args: unknown[]) => void): void;
  removeListener?(event: "accountsChanged" | "chainChanged" | "disconnect" | "connect", listener: (...args: unknown[]) => void): void;
  disconnect?(): Promise<void>;
};

interface Window {
  ethereum?: EthereumProvider;
}
