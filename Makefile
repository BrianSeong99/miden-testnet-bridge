.PHONY: genesis e2e e2e-local-node

# Local-node targets are legacy/manual only. The supported E2E path uses
# public Miden testnet plus Anvil.
genesis:
	docker compose --profile local-node up --build --abort-on-container-exit --exit-code-from miden-node-genesis miden-node-genesis

e2e:
	RUSTFLAGS="-C debug-assertions=no" RUN_E2E=1 cargo test --test e2e -- --test-threads=1

e2e-local-node:
	MIDEN_RPC_URL=http://miden-node:57291 RUSTFLAGS="-C debug-assertions=no" RUN_E2E=1 cargo test --test e2e -- --test-threads=1
