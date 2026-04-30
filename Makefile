.PHONY: genesis

genesis:
	docker compose --profile genesis up --build --abort-on-container-exit --exit-code-from miden-node-genesis miden-node-genesis
