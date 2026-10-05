.PHONY: check check-unit check-release-regression check-regression check-store check-http check-docs check-links check-cli check-architecture check-web require-test-database

# Use the repository pin even when a distribution cargo precedes rustup in PATH.
RUST_TOOLCHAIN := $(shell sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)
CARGO := rustup run $(RUST_TOOLCHAIN) cargo
LYCHEE ?= lychee

# Routine CI scope: Rust compilation/build, rules/local components, and two real-PG stack cases.
RUST_BUILD_FEATURES := job/catalog-prepare,job/polymarket-history,job/native-paper-test,job/native-sandbox-test,server/native-codex

check: check-unit check-release-regression

# Check all targets/features; actually link only production lib/bin targets.
# Integration/example linking and doctests are retained in check-regression.
check-unit:
	$(CARGO) check --locked --workspace --all-targets --all-features
	$(CARGO) build --locked --workspace --lib --bins --features $(RUST_BUILD_FEATURES)
	bash tests/native/rust_unit.sh $(CARGO)

check-release-regression: require-test-database
	bash tests/native/portfolio_release_regression.sh $(CARGO)

# Former full default Rust suite, without the routine classification filters.
# Native Codex, portable CLI, systemd and filesystem setup: rust-regression.yml.
# OCI/native-science acceptance remains in its existing focused workflows.
check-regression: require-test-database
	$(CARGO) check --locked --workspace --all-targets --all-features
	$(CARGO) build --locked --workspace --all-targets --features $(RUST_BUILD_FEATURES)
	$(CARGO) test --locked --no-fail-fast --workspace --features $(RUST_BUILD_FEATURES) -- --test-threads=1

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
	npm --prefix apps/web run check:generated
	npm --prefix apps/web run test:validator-adapter
	npm --prefix apps/web run generate
	git diff --exit-code -- apps/web/src/generated/
	test -z "$$(git ls-files --others --exclude-standard -- apps/web/src/generated/)"
	npm --prefix apps/web run typecheck
	npm --prefix apps/web test
	cd apps/web && node node_modules/vite/bin/vite.js build
	node apps/web/scripts/measure-validator-build.mjs --assert-modular

require-test-database:
	@test -n "$$DATABASE_URL" || { printf '%s\n' 'DATABASE_URL is required: use only a disposable PostgreSQL18 + PGMQ1.10.0 test instance.' >&2; exit 1; }

