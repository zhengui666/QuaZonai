SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help

.PHONY: help install-dev format lint typecheck test test-unit test-integration compile migrate preflight up down logs ps build verify-compose rust-format rust-lint rust-test ci

help:
	@printf '%s\n' \
	  'install-dev      Install backend with development dependencies' \
	  'format           Format Python and Rust sources' \
	  'lint             Run Python lint checks' \
	  'typecheck        Run Python type checks' \
	  'test             Run Python tests' \
	  'compile          Compile Python sources' \
	  'migrate          Run database preflight and Alembic upgrade' \
	  'up/down/logs/ps  Operate the Core Compose stack' \
	  'build            Build the backend image' \
	  'rust-*           Run native risk crate checks' \
	  'ci               Run the local CI-equivalent checks'

install-dev:
	python -m pip install -e 'backend[dev]'

format:
	ruff format backend/src backend/tests
	cargo fmt --manifest-path native/quazonai_nautilus_risk/Cargo.toml

lint:
	ruff check backend/src backend/tests

typecheck:
	mypy backend/src/quazonai

test:
	pytest -q backend/tests

test-unit:
	pytest -q backend/tests/unit

test-integration:
	pytest -q backend/tests/integration

compile:
	python -m compileall -q backend/src

preflight:
	python -m quazonai.db.preflight

migrate:
	python -m quazonai.db.preflight
	alembic -c backend/alembic.ini upgrade head

up:
	docker compose --env-file .env up --build --remove-orphans

down:
	docker compose --env-file .env down --remove-orphans

logs:
	docker compose --env-file .env logs --follow --tail=200

ps:
	docker compose --env-file .env ps

build:
	docker build -f deploy/Dockerfile.backend -t quazonai-backend:local .

verify-compose:
	docker compose --env-file .env.example config --quiet

rust-format:
	cargo fmt --manifest-path native/quazonai_nautilus_risk/Cargo.toml --check

rust-lint:
	cargo clippy --manifest-path native/quazonai_nautilus_risk/Cargo.toml --all-targets -- -D warnings

rust-test:
	cargo test --manifest-path native/quazonai_nautilus_risk/Cargo.toml

ci: compile lint typecheck test rust-format rust-lint rust-test verify-compose
