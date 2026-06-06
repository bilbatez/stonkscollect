# StonksCollect dev tasks.
# Coverage gates are 100%; main.rs / main.tsx bootstrap is excluded.

BACKEND := backend
FRONTEND := frontend
COV_IGNORE := main\.rs

.PHONY: test test-backend test-frontend cov cov-backend cov-frontend lint e2e up down build

test: test-backend test-frontend

test-backend:
	cd $(BACKEND) && cargo test

test-frontend:
	cd $(FRONTEND) && npm run test:run

cov: cov-backend cov-frontend

# Fails build if backend logic coverage < 100% (bootstrap excluded).
cov-backend:
	cd $(BACKEND) && cargo llvm-cov --ignore-filename-regex '$(COV_IGNORE)' \
		--fail-under-lines 100 --fail-under-functions 100

# Vitest enforces 100% thresholds from vite.config.ts.
cov-frontend:
	cd $(FRONTEND) && npm run coverage

lint:
	cd $(BACKEND) && cargo clippy --all-targets -- -D warnings
	cd $(FRONTEND) && npm run lint

e2e:
	cd $(FRONTEND) && npm run e2e

build:
	docker compose build

up:
	docker compose up --build

down:
	docker compose down
