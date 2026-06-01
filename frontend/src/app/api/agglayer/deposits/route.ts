import { NextResponse } from "next/server";
import {
  type AgglayerDeposit,
  type AgglayerDepositStatus,
  bridgeStatusUrl,
  midenAccountToBridgeDestination,
} from "@/app/lib/agglayer";

export const dynamic = "force-dynamic";

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const rawDestinationAddress = searchParams.get("destinationAddress");
  const rawMidenAccountId = searchParams.get("midenAccountId");

  let destinationAddress: string;
  try {
    destinationAddress = rawDestinationAddress ?? midenAccountToBridgeDestination(rawMidenAccountId ?? "");
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "Invalid Miden account ID." },
      { status: 400 },
    );
  }

  if (!/^0x[0-9a-fA-F]{40}$/.test(destinationAddress)) {
    return NextResponse.json({ error: "destinationAddress must be a 20-byte hex address." }, { status: 400 });
  }

  const response = await fetch(bridgeStatusUrl(destinationAddress), {
    headers: { accept: "application/json" },
    next: { revalidate: 0 },
  });

  if (!response.ok) {
    return NextResponse.json(
      { error: `Bridge service returned ${response.status}.` },
      { status: 502 },
    );
  }

  const payload = (await response.json()) as { deposits?: AgglayerDeposit[] };
  const deposits = Array.isArray(payload.deposits) ? payload.deposits : [];
  const body: AgglayerDepositStatus = {
    destinationAddress,
    deposits,
    latestDeposit: deposits[0] ?? null,
  };

  return NextResponse.json(body);
}
