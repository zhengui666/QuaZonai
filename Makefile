SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help

.PHONY: help install-dev format lint typecheck test test-unit test-integration compile migrate preflight up down logs ps build verify-compose ci

help:
	@printf '%s\n' \
	  'install-dev      Install backend development and Codex runtime dependencies' \
	  'format           Format Python sources' \
	  'lint             Run Python lint checks' \
	  'typecheck        Run Python type checks' \
	  'test             Run Python tests' \
	  'compile          Compile Python sources' \
	  'migrate          Run database preflight and Alembic upgrade' \
	  'up/down/logs/ps  Operate the Core Compose stack' \
	  'build            Build the backend image' \
	  'ci               Run the local CI-equivalent checks'

install-dev:
	python -m pip install -e 'backend[dev,research,agent]'

format:
	ruff format backend/src backend/tests

lint:
	ruff check backend/src backend/tests

typecheck:
	mypy --config-file backend/pyproject.toml backend/src

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

ci: compile lint typecheck test verify-compose
