#!/usr/bin/env bash
set -euo pipefail

data_dir="${MIDEN_NODE_DATA_DIR:-/var/lib/miden/data}"
accounts_dir="${MIDEN_NODE_ACCOUNTS_DIR:-/var/lib/miden/accounts}"
genesis_file="${data_dir}/genesis.dat"

mkdir -p "${data_dir}" "${accounts_dir}"

if [[ -f "${genesis_file}" ]]; then
  echo "miden genesis already present at ${genesis_file}; skipping bootstrap"
  exit 0
fi

exec miden-node bundled bootstrap \
  --data-directory "${data_dir}" \
  --accounts-directory "${accounts_dir}"
