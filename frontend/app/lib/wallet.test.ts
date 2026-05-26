import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  formatWalletAddress,
  hexQuantity,
  isMidenAccountLike,
  isSepoliaChain,
  normalizeEvmAddress,
} from "./wallet";

describe("wallet helpers", () => {
  it("normalizes and formats EVM wallet addresses", () => {
    const address = normalizeEvmAddress("  0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc  ");

    assert.equal(address, "0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc");
    assert.equal(formatWalletAddress(address), "0x9965...a4dc");
  });

  it("recognizes Sepolia chain ids from wallet providers", () => {
    assert.equal(isSepoliaChain("0xaa36a7"), true);
    assert.equal(isSepoliaChain("11155111"), true);
    assert.equal(isSepoliaChain("0x1"), false);
  });

  it("accepts Miden testnet account ids in bech32 or 30-byte hex form", () => {
    assert.equal(isMidenAccountLike("0xc98bb07c188cd2500e13f68a069cdc"), true);
    assert.equal(isMidenAccountLike("c98bb07c188cd2500e13f68a069cdc"), true);
    assert.equal(isMidenAccountLike("mcst1arychvrurzxdy5qwz0mg5p5umsvsepyx"), true);
    assert.equal(isMidenAccountLike("0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc"), false);
  });

  it("encodes transaction numeric values as hex quantities", () => {
    assert.equal(hexQuantity("1000000000000000"), "0x38d7ea4c68000");
    assert.throws(() => hexQuantity("-1"), /non-negative/);
  });
});
