.PHONY: genesis e2e

genesis:
	docker compose --profile genesis up --build --abort-on-container-exit --exit-code-from miden-node-genesis miden-node-genesis

e2e:
	RUN_E2E=1 cargo test --test e2e -- --test-threads=1
