.PHONY: genesis e2e e2e-local-node sandbox sandbox-down sandbox-reset sandbox-logs sandbox-status sepolia sepolia-down sepolia-reset sepolia-logs bridgectl

# Local-node targets are legacy/manual only. The default documented builder
# path uses public Miden testnet plus Sepolia native ETH.
genesis:
	docker compose --profile local-node up --build --abort-on-container-exit --exit-code-from miden-node-genesis miden-node-genesis

e2e:
	RUSTFLAGS="-C debug-assertions=no" RUN_E2E=1 cargo test --test e2e -- --test-threads=1

e2e-local-node:
	MIDEN_RPC_URL=http://miden-node:57291 RUSTFLAGS="-C debug-assertions=no" RUN_E2E=1 cargo test --test e2e -- --test-threads=1

sandbox:
	@if [ ! -f .env ]; then cp .env.sepolia.example .env; fi
	@if grep -q '^MIDEN_MASTER_SEED_HEX=replace-with-32-byte-hex-seed' .env; then \
		seed="$$(openssl rand -hex 32)"; \
		perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$$seed/" .env; \
		echo "Generated MIDEN_MASTER_SEED_HEX=$$seed"; \
	fi
	@if grep -Eq 'replace-with|sepolia.infura.io/v3/replace-me' .env; then \
		echo "Fill MASTER_MNEMONIC, funded SOLVER_PRIVATE_KEY, funded DEMO_EVM_FUNDED_PRIVATE_KEY, and MIDEN_MASTER_SEED_HEX in .env before running make sandbox"; \
		exit 1; \
	fi
	docker compose --env-file .env up -d --build --wait --wait-timeout 600
	@port="$$(grep -E '^BRIDGE_HTTP_PORT=' .env | tail -1 | cut -d= -f2)"; port="$${port:-8080}"; \
		ui_port="$$(grep -E '^LAB_UI_HTTP_PORT=' .env | tail -1 | cut -d= -f2)"; ui_port="$${ui_port:-3000}"; \
		echo "Bridge API: http://localhost:$$port"; \
		echo "Lab UI:     http://localhost:$$ui_port"; \
		echo "Legacy lab: http://localhost:$$port/lab"
	@echo "CLI:        ./bin/bridgectl status"

sandbox-down:
	docker compose --env-file .env down --remove-orphans

sandbox-reset:
	docker compose --env-file .env down --volumes --remove-orphans

sandbox-logs:
	docker compose --env-file .env logs -f bridge

sandbox-status:
	./bin/bridgectl status

sepolia:
	@if [ ! -f .env ]; then cp .env.sepolia.example .env; fi
	@if grep -q '^MIDEN_MASTER_SEED_HEX=replace-with-32-byte-hex-seed' .env; then \
		seed="$$(openssl rand -hex 32)"; \
		perl -0pi -e "s/MIDEN_MASTER_SEED_HEX=.*/MIDEN_MASTER_SEED_HEX=$$seed/" .env; \
		echo "Generated MIDEN_MASTER_SEED_HEX=$$seed"; \
	fi
	@if grep -Eq 'replace-with|sepolia.infura.io/v3/replace-me' .env; then \
		echo "Fill MASTER_MNEMONIC, funded SOLVER_PRIVATE_KEY, funded DEMO_EVM_FUNDED_PRIVATE_KEY, and MIDEN_MASTER_SEED_HEX in .env before running make sepolia"; \
		exit 1; \
	fi
	docker compose --env-file .env up -d --build --wait --wait-timeout 600
	@port="$$(grep -E '^BRIDGE_HTTP_PORT=' .env | tail -1 | cut -d= -f2)"; port="$${port:-8080}"; \
		ui_port="$$(grep -E '^LAB_UI_HTTP_PORT=' .env | tail -1 | cut -d= -f2)"; ui_port="$${ui_port:-3000}"; \
		echo "Bridge API: http://localhost:$$port"; \
		echo "Lab UI:     http://localhost:$$ui_port"; \
		echo "Status:     ./bin/bridgectl status"

sepolia-down:
	docker compose --env-file .env down --remove-orphans

sepolia-reset:
	docker compose --env-file .env down --volumes --remove-orphans

sepolia-logs:
	docker compose --env-file .env logs -f bridge

bridgectl:
	./bin/bridgectl status
