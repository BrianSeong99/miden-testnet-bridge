import { NextResponse } from "next/server";
import { formatEther } from "viem";

const sepoliaRpcUrl = "https://ethereum-sepolia-rpc.publicnode.com";

export const dynamic = "force-dynamic";

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const address = searchParams.get("address") ?? "";

  if (!/^0x[0-9a-fA-F]{40}$/.test(address)) {
    return NextResponse.json({ error: "address must be a 20-byte hex address." }, { status: 400 });
  }

  const response = await fetch(sepoliaRpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_getBalance",
      params: [address, "latest"],
    }),
    next: { revalidate: 0 },
  });

  if (!response.ok) {
    return NextResponse.json({ error: `Sepolia RPC returned ${response.status}.` }, { status: 502 });
  }

  const payload = (await response.json()) as { result?: `0x${string}`; error?: { message?: string } };
  if (!payload.result) {
    return NextResponse.json({ error: payload.error?.message ?? "Sepolia RPC did not return a balance." }, { status: 502 });
  }

  const balanceWei = BigInt(payload.result);
  return NextResponse.json({
    address,
    balanceWei: balanceWei.toString(),
    balanceEth: formatEther(balanceWei),
  });
}
