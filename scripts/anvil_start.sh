#!/bin/sh
set -eu

state_file="/state/anvil.json"

set -- \
  anvil \
  --host 0.0.0.0 \
  --chain-id 271828 \
  --block-time 1 \
  --mnemonic "test test test test test test test test test test test junk" \
  --balance 100000 \
  --dump-state "$state_file"

if [ -f "$state_file" ]; then
  set -- "$@" --load-state "$state_file"
fi

exec "$@"
