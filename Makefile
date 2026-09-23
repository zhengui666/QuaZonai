.PHONY: check check-unit check-store check-http check-docs check-links check-cli check-architecture check-web require-test-database

# Use the repository pin even when a distribution cargo precedes rustup in PATH.
RUST_TOOLCHAIN := $(shell sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)
CARGO := rustup run $(RUST_TOOLCHAIN) cargo
LYCHEE ?= lychee

# Full check requires a disposable test database.
check: require-test-database check-docs
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --locked --workspace --all-targets --features server/native-codex,runtime/native-oci -- -D warnings
	$(CARGO) test --locked --workspace --features server/native-codex

# Unit and native computation tests without a PostgreSQL instance.
check-unit:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --locked --workspace --all-targets --features server/native-codex,runtime/native-oci -- -D warnings
	$(CARGO) test --locked --workspace --exclude store --exclude server

check-store: require-test-database
	$(CARGO) test --locked -p store

check-http: require-test-database
	$(CARGO) test --locked -p server --features native-codex

check-docs: check-links check-cli

# Paths include hidden governance docs, but not ignored worktrees or build output.
check-links:
	git ls-files -z --cached --others --exclude-standard -- '*.md' | xargs -0 $(LYCHEE) --config .lychee.toml --

check-cli:
	$(CARGO) test --locked -p server --test client_help --test client_skill
	$(CARGO) test --locked -p server --bin server agent_schema

check-architecture:
	$(CARGO) test --locked -p contracts --test architecture

check-web:
	npm --prefix apps/web run generate
	git diff --exit-code -- apps/web/src/generated/
	npm --prefix apps/web run typecheck
	npm --prefix apps/web test
	cd apps/web && node node_modules/vite/bin/vite.js build

require-test-database:
	@test -n "$$DATABASE_URL" || { printf '%s\n' 'DATABASE_URL is required: use only a disposable PostgreSQL18 + PGMQ1.10.0 test instance.' >&2; exit 1; }
