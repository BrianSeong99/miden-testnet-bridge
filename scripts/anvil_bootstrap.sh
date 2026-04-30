#!/usr/bin/env bash
set -euo pipefail

RPC_URL="${RPC_URL:-http://anvil:8545}"
STATE_DIR="${STATE_DIR:-/state}"
TOKEN_FILE="${STATE_DIR}/token-addresses.json"
BOOTSTRAP_PRIVATE_KEY="${BOOTSTRAP_PRIVATE_KEY:-0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80}"
DEPLOYER_ADDRESS="${DEPLOYER_ADDRESS:-0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266}"
SOLVER_PRIVATE_KEY="${SOLVER_PRIVATE_KEY:?SOLVER_PRIVATE_KEY is required}"
PROJECT_ROOT="${PROJECT_ROOT:-/workspace}"
SOLVER_ETH_FUND="${SOLVER_ETH_FUND:-100ether}"
USDC_SUPPLY="${USDC_SUPPLY:-1000000000000000}"
USDT_SUPPLY="${USDT_SUPPLY:-1000000000000000}"
BTC_SUPPLY="${BTC_SUPPLY:-100000000000000}"
SOLVER_TOKEN_FUND="${SOLVER_TOKEN_FUND:-1000000000000}"

mkdir -p "${STATE_DIR}"

for _ in $(seq 1 60); do
  if cast rpc eth_blockNumber --rpc-url "${RPC_URL}" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

USDC_ADDRESS="$(cast compute-address --nonce 0 "${DEPLOYER_ADDRESS}")"
USDT_ADDRESS="$(cast compute-address --nonce 1 "${DEPLOYER_ADDRESS}")"
BTC_ADDRESS="$(cast compute-address --nonce 2 "${DEPLOYER_ADDRESS}")"
SOLVER_ADDRESS="$(cast wallet address --private-key "${SOLVER_PRIVATE_KEY}")"

write_token_file() {
  cat >"${TOKEN_FILE}" <<EOF
{"usdc":"${USDC_ADDRESS}","usdt":"${USDT_ADDRESS}","btc":"${BTC_ADDRESS}"}
EOF
}

code_at() {
  cast code "$1" --rpc-url "${RPC_URL}" 2>/dev/null || true
}

if [[ "$(code_at "${USDC_ADDRESS}")" != "0x" ]] \
  && [[ "$(code_at "${USDT_ADDRESS}")" != "0x" ]] \
  && [[ "$(code_at "${BTC_ADDRESS}")" != "0x" ]]; then
  write_token_file
  exit 0
fi

forge create "${PROJECT_ROOT}/contracts/MockERC20.sol:MockERC20" \
  --root "${PROJECT_ROOT}" \
  --rpc-url "${RPC_URL}" \
  --private-key "${BOOTSTRAP_PRIVATE_KEY}" \
  --broadcast \
  --constructor-args "Mock USD Coin" "USDC" 6 "${USDC_SUPPLY}" >/dev/null

forge create "${PROJECT_ROOT}/contracts/MockERC20.sol:MockERC20" \
  --root "${PROJECT_ROOT}" \
  --rpc-url "${RPC_URL}" \
  --private-key "${BOOTSTRAP_PRIVATE_KEY}" \
  --broadcast \
  --constructor-args "Mock Tether USD" "USDT" 6 "${USDT_SUPPLY}" >/dev/null

forge create "${PROJECT_ROOT}/contracts/MockERC20.sol:MockERC20" \
  --root "${PROJECT_ROOT}" \
  --rpc-url "${RPC_URL}" \
  --private-key "${BOOTSTRAP_PRIVATE_KEY}" \
  --broadcast \
  --constructor-args "Mock Bitcoin" "BTC" 8 "${BTC_SUPPLY}" >/dev/null

cast send "${SOLVER_ADDRESS}" --value "${SOLVER_ETH_FUND}" --rpc-url "${RPC_URL}" \
  --private-key "${BOOTSTRAP_PRIVATE_KEY}" >/dev/null
cast send "${USDC_ADDRESS}" "transfer(address,uint256)" "${SOLVER_ADDRESS}" "${SOLVER_TOKEN_FUND}" \
  --rpc-url "${RPC_URL}" --private-key "${BOOTSTRAP_PRIVATE_KEY}" >/dev/null
cast send "${USDT_ADDRESS}" "transfer(address,uint256)" "${SOLVER_ADDRESS}" "${SOLVER_TOKEN_FUND}" \
  --rpc-url "${RPC_URL}" --private-key "${BOOTSTRAP_PRIVATE_KEY}" >/dev/null
cast send "${BTC_ADDRESS}" "transfer(address,uint256)" "${SOLVER_ADDRESS}" "${SOLVER_TOKEN_FUND}" \
  --rpc-url "${RPC_URL}" --private-key "${BOOTSTRAP_PRIVATE_KEY}" >/dev/null

write_token_file
