import { describe, expect, test } from "vitest";
import type { Activity } from "./bridge-state";
import { deriveMonitoredActivity } from "./bridge-monitor";

const baseReceive: Activity = {
  id: "act-receive-live",
  mode: "receive",
  provider: "agglayer",
  summary: "Receive 1 ETH on Miden",
  status: "source_finality",
  eta: "About 15 min",
  amount: "1",
  asset: "ETH",
  destination: "c98bb07c188cd2500e13f68a069cdc",
  bridgeDestinationAddress: "0x00000000c98bb07c188cd2500e13f68a069cdc00",
  txHash: "0xpreview...pending",
  sourceTxHash: "0x1111111111111111111111111111111111111111111111111111111111111111",
  updatedAt: "2 min ago",
};

const baseSend: Activity = {
  id: "act-send-live",
  mode: "send",
  provider: "agglayer",
  summary: "Send 1 ETH to Sepolia",
  status: "message_observed",
  eta: "30-90 min",
  amount: "1",
  asset: "ETH",
  destination: "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
  txHash: "0xpreview...pending",
  sourceTxHash: "0x0490ad69e87c19c0c2c4b7951b87f0013c98bf5d90b7e14acbe821471ad5b91e",
  updatedAt: "2 min ago",
};

describe("deriveMonitoredActivity", () => {
  test("keeps AggLayer receive waiting for bridge observation after Sepolia source confirmation", () => {
    const next = deriveMonitoredActivity(baseReceive, {
      checkedAt: "Just now",
      sourceTx: {
        hash: baseReceive.sourceTxHash!,
        status: "confirmed",
        confirmations: 3,
        success: true,
      },
      agglayerDeposit: null,
    });

    expect(next.status).toBe("source_finality");
    expect(next.eta).toBe("Sepolia confirmed, waiting for AggLayer");
    expect(next.updatedAt).toBe("Just now");
  });

  test("moves AggLayer receive to message observed when Gateway FM has a non-ready bridge row", () => {
    const next = deriveMonitoredActivity(baseReceive, {
      checkedAt: "Just now",
      sourceTx: {
        hash: baseReceive.sourceTxHash!,
        status: "confirmed",
        confirmations: 5,
        success: true,
      },
      agglayerDeposit: {
        ready_for_claim: false,
        tx_hash: baseReceive.sourceTxHash,
        deposit_cnt: 42,
      },
    });

    expect(next.status).toBe("message_observed");
    expect(next.depositCount).toBe("42");
    expect(next.readyForClaim).toBe(false);
  });

  test("does not mark AggLayer receive complete when the Miden note is only claimable", () => {
    const next = deriveMonitoredActivity(baseReceive, {
      checkedAt: "Just now",
      sourceTx: {
        hash: baseReceive.sourceTxHash!,
        status: "confirmed",
        confirmations: 10,
        success: true,
      },
      agglayerDeposit: {
        ready_for_claim: true,
        tx_hash: baseReceive.sourceTxHash,
        deposit_cnt: 43,
      },
    });

    expect(next.status).toBe("claim_available");
    expect(next.readyForClaim).toBe(true);
    expect(next.status).not.toBe("complete");
  });

  test("moves AggLayer send to claim available when a filtered claim plan is ready", () => {
    const next = deriveMonitoredActivity(baseSend, {
      checkedAt: "Just now",
      claimPlan: {
        readyForClaim: true,
        depositCount: 7,
      },
    });

    expect(next.status).toBe("claim_available");
    expect(next.depositCount).toBe("7");
    expect(next.readyForClaim).toBe(true);
  });

  test("marks submitted Sepolia claim complete only after the claim transaction confirms successfully", () => {
    const next = deriveMonitoredActivity(
      {
        ...baseSend,
        status: "claim_submitted",
        claimTxHash: "0x2222222222222222222222222222222222222222222222222222222222222222",
      },
      {
        checkedAt: "Just now",
        destinationTx: {
          hash: "0x2222222222222222222222222222222222222222222222222222222222222222",
          status: "confirmed",
          confirmations: 2,
          success: true,
        },
      },
    );

    expect(next.status).toBe("complete");
    expect(next.eta).toBe("Settled on Sepolia");
  });

  test("marks failed chain transactions as failed instead of advancing locally", () => {
    const next = deriveMonitoredActivity(baseReceive, {
      checkedAt: "Just now",
      sourceTx: {
        hash: baseReceive.sourceTxHash!,
        status: "confirmed",
        confirmations: 1,
        success: false,
      },
    });

    expect(next.status).toBe("failed");
    expect(next.eta).toBe("Source transaction failed");
  });
});
