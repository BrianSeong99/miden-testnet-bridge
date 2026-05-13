.PHONY: genesis e2e e2e-local-node sandbox sandbox-down sandbox-reset sandbox-logs sandbox-status bridgectl

# Local-node targets are legacy/manual only. The supported E2E path uses
# public Miden testnet plus Anvil.
genesis:
	docker compose --profile local-node up --build --abort-on-container-exit --exit-code-from miden-node-genesis miden-node-genesis

e2e:
	RUSTFLAGS="-C debug-assertions=no" RUN_E2E=1 cargo test --test e2e -- --test-threads=1

e2e-local-node:
	MIDEN_RPC_URL=http://miden-node:57291 RUSTFLAGS="-C debug-assertions=no" RUN_E2E=1 cargo test --test e2e -- --test-threads=1

sandbox:
	@if [ ! -f .env ]; then cp .env.anvil.example .env; fi
	@if grep -q '^MIDEN_MASTER_SEED_HEX=replace-with-32-byte-hex-seed' .env; then \
		seed="$$(openssl rand -hex 32)"; \
		perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$$seed/" .env; \
		echo "Generated MIDEN_MASTER_SEED_HEX=$$seed"; \
	fi
	docker compose --env-file .env up -d --build --wait --wait-timeout 600
	@port="$$(grep -E '^BRIDGE_HTTP_PORT=' .env | tail -1 | cut -d= -f2)"; port="$${port:-8080}"; \
		echo "Bridge API: http://localhost:$$port"; \
		echo "Lab UI:     http://localhost:$$port/lab"
	@echo "CLI:        ./bin/bridgectl status"

sandbox-down:
	docker compose --env-file .env down --remove-orphans

sandbox-reset:
	docker compose --env-file .env down --volumes --remove-orphans

sandbox-logs:
	docker compose --env-file .env logs -f bridge

sandbox-status:
	./bin/bridgectl status

bridgectl:
	./bin/bridgectl status
