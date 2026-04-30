#!/usr/bin/env bash
set -euo pipefail

data_dir="${MIDEN_NODE_DATA_DIR:-/var/lib/miden/data}"
rpc_url="${MIDEN_NODE_RPC_URL:-http://0.0.0.0:57291}"

if [[ ! -f "${data_dir}/genesis.dat" ]]; then
  echo "miden genesis not found in ${data_dir}; run 'make genesis' first" >&2
  exit 1
fi

exec miden-node bundled start \
  --data-directory "${data_dir}" \
  --rpc.url "${rpc_url}"
