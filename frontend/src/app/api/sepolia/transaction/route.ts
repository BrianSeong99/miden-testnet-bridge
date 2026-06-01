import { NextResponse } from "next/server";

const sepoliaRpcUrl =
  process.env.AGGLAYER_SEPOLIA_RPC_URL ?? process.env.EVM_RPC_URL ?? "https://ethereum-sepolia-rpc.publicnode.com";

type RpcResponse<T> = {
  result?: T;
  error?: { message?: string };
};

type TransactionReceipt = {
  blockNumber: `0x${string}`;
  status: `0x${string}`;
  transactionHash: `0x${string}`;
};

export const dynamic = "force-dynamic";

async function rpc<T>(method: string, params: unknown[]) {
  const response = await fetch(sepoliaRpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    next: { revalidate: 0 },
  });

  if (!response.ok) {
    throw new Error(`Sepolia RPC returned ${response.status}.`);
  }

  const payload = (await response.json()) as RpcResponse<T>;
  if (payload.error) throw new Error(payload.error.message ?? "Sepolia RPC returned an error.");
  return payload.result;
}

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const hash = searchParams.get("hash") ?? "";

  if (!/^0x[0-9a-fA-F]{64}$/.test(hash)) {
    return NextResponse.json({ error: "hash must be a 32-byte transaction hash." }, { status: 400 });
  }

  try {
    const receipt = await rpc<TransactionReceipt | null>("eth_getTransactionReceipt", [hash]);
    if (!receipt) {
      return NextResponse.json({
        hash,
        status: "pending",
        confirmations: 0,
      });
    }

    const latestBlockHex = await rpc<`0x${string}`>("eth_blockNumber", []);
    const latestBlock = BigInt(latestBlockHex ?? "0x0");
    const receiptBlock = BigInt(receipt.blockNumber);
    const confirmations = latestBlock >= receiptBlock ? Number(latestBlock - receiptBlock + 1n) : 0;

    return NextResponse.json({
      hash: receipt.transactionHash,
      status: "confirmed",
      blockNumber: receipt.blockNumber,
      confirmations,
      success: receipt.status === "0x1",
    });
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "Unable to read Sepolia transaction status." },
      { status: 502 },
    );
  }
}
