import { encodeFunctionData, parseEther, toHex } from "viem";

export const AGGLAYER_BALI = {
  sepoliaChainId: 11155111,
  sepoliaChainHex: "0xaa36a7",
  sepoliaBridgeAddress: "0x1348947e282138d8f377b467f7d9c2eb0f335d1f",
  destinationNetworkId: 76,
  sourceNetworkId: 0,
  nativeTokenAddress: "0x0000000000000000000000000000000000000000",
  gasLimit: BigInt(300000),
  bridgeServiceApi: "https://miden-testnet-bridge.dev.eu-north-3.gateway.fm/api",
  monitorUrl: "https://gateway-fm.github.io/miden-agglayer/bridge-monitor/bali/",
  sepoliaRpcUrl: "https://ethereum-sepolia-rpc.publicnode.com",
  sepoliaExplorer: "https://sepolia.etherscan.io",
  midenExplorer: "https://testnet.midenscan.com",
  midenEthFaucetId: "mcst1arnrhfau9svl7cpu2tr8lfzzd5j87wwe",
  midenEthDecimals: 8,
} as const;

const bridgeAssetAbi = [
  {
    type: "function",
    name: "bridgeAsset",
    stateMutability: "payable",
    inputs: [
      { name: "destinationNetwork", type: "uint32" },
      { name: "destinationAddress", type: "address" },
      { name: "amount", type: "uint256" },
      { name: "token", type: "address" },
      { name: "forceUpdateGlobalExitRoot", type: "bool" },
      { name: "permitData", type: "bytes" },
    ],
    outputs: [],
  },
] as const;

export type AgglayerDeposit = {
  ready_for_claim?: boolean;
  tx_hash?: string;
  amount?: string;
  deposit_cnt?: string | number;
  block_num?: string | number;
  global_index?: string;
};

export type AgglayerDepositStatus = {
  destinationAddress: string;
  deposits: AgglayerDeposit[];
  latestDeposit: AgglayerDeposit | null;
};

export type AgglayerClaimPlan = {
  readyForClaim: boolean;
  direction: "miden-to-sepolia";
  bridgeAddress: string;
  ethAccountId: string;
  statusUrl: string;
  claimsUrl: string;
  merkleProofUrl: string | null;
  deposit: AgglayerDeposit | null;
  calldata: string | null;
  transaction: {
    to: string;
    data: string;
    value: string;
    gas: string;
  } | null;
  warnings: string[];
};

export function isMidenAccountHex(value: string) {
  return /^[0-9a-fA-F]{30}$/.test(value.trim().replace(/^0x/, ""));
}

const BECH32M_CONST = 0x2bc830a3;
const BECH32_CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

function bech32Polymod(values: number[]) {
  const generators = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
  let chk = 1;

  for (const value of values) {
    const top = chk >> 25;
    chk = ((chk & 0x1ffffff) << 5) ^ value;
    for (let i = 0; i < 5; i += 1) {
      if ((top >> i) & 1) chk ^= generators[i];
    }
  }

  return chk;
}

function bech32HrpExpand(hrp: string) {
  const expanded: number[] = [];
  for (let i = 0; i < hrp.length; i += 1) expanded.push(hrp.charCodeAt(i) >> 5);
  expanded.push(0);
  for (let i = 0; i < hrp.length; i += 1) expanded.push(hrp.charCodeAt(i) & 31);
  return expanded;
}

function convertBits(data: number[], fromBits: number, toBits: number) {
  let acc = 0;
  let bits = 0;
  const maxv = (1 << toBits) - 1;
  const result: number[] = [];

  for (const value of data) {
    if (value < 0 || value >> fromBits !== 0) throw new Error("Invalid Miden bech32 address.");
    acc = (acc << fromBits) | value;
    bits += fromBits;
    while (bits >= toBits) {
      bits -= toBits;
      result.push((acc >> bits) & maxv);
    }
  }

  if (bits >= fromBits || ((acc << (toBits - bits)) & maxv) !== 0) {
    throw new Error("Invalid Miden bech32 address padding.");
  }

  return result;
}

function midenBech32AccountToHex(value: string) {
  const identifier = value.trim().split("_")[0];
  const lower = identifier.toLowerCase();
  if (identifier !== lower && identifier !== identifier.toUpperCase()) {
    throw new Error("Miden bech32 addresses cannot mix uppercase and lowercase characters.");
  }

  const separator = lower.lastIndexOf("1");
  if (separator < 1 || separator + 7 > lower.length) {
    throw new Error("Invalid Miden bech32 address.");
  }

  const hrp = lower.slice(0, separator);
  const values = [...lower.slice(separator + 1)].map((char) => {
    const index = BECH32_CHARSET.indexOf(char);
    if (index === -1) throw new Error("Invalid Miden bech32 address character.");
    return index;
  });

  if (bech32Polymod([...bech32HrpExpand(hrp), ...values]) !== BECH32M_CONST) {
    throw new Error("Invalid Miden bech32 checksum.");
  }

  // Miden account addresses encode one address-type byte followed by the 15-byte account ID.
  const bytes = convertBits(values.slice(0, -6), 5, 8);
  if (bytes.length !== 16 || bytes[0] !== 232) {
    throw new Error("Expected a Miden account ID address.");
  }

  return bytes
    .slice(1)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function normalizeMidenAccountHex(value: string) {
  const normalized = value.trim().replace(/^0x/, "");
  if (isMidenAccountHex(normalized)) return normalized.toLowerCase();
  return midenBech32AccountToHex(value);
}

export function midenAccountToBridgeDestination(value: string) {
  const midenAccount = normalizeMidenAccountHex(value);
  return `0x00000000${midenAccount}00` as `0x${string}`;
}

export function buildSepoliaDepositTransaction({
  amountEth,
  midenAccountId,
}: {
  amountEth: string;
  midenAccountId: string;
}) {
  const amountWei = parseEther(amountEth);
  if (amountWei <= BigInt(0)) {
    throw new Error("Enter an amount greater than zero.");
  }

  const destinationAddress = midenAccountToBridgeDestination(midenAccountId);
  const data = encodeFunctionData({
    abi: bridgeAssetAbi,
    functionName: "bridgeAsset",
    args: [
      AGGLAYER_BALI.destinationNetworkId,
      destinationAddress,
      amountWei,
      AGGLAYER_BALI.nativeTokenAddress,
      true,
      "0x",
    ],
  });

  return {
    to: AGGLAYER_BALI.sepoliaBridgeAddress,
    data,
    value: toHex(amountWei),
    gas: toHex(AGGLAYER_BALI.gasLimit),
    destinationAddress,
  };
}

export function bridgeStatusUrl(destinationAddress: string) {
  return `${AGGLAYER_BALI.bridgeServiceApi}/bridges/${destinationAddress}?limit=1&offset=0`;
}
