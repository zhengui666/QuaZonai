.PHONY: check check-unit check-store check-http require-test-database native

# Full check fails closed when a disposable test database was not provided.
check: require-test-database
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace

# Explicitly narrower entrypoint; it is not full Store/product acceptance.
check-unit:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace --exclude store --exclude server

check-store: require-test-database
	cargo test --locked -p store

check-http: require-test-database
	cargo test --locked -p server

require-test-database:
	@test -n "$$DATABASE_URL" || { printf '%s\n' 'DATABASE_URL is required: use only a disposable PostgreSQL18 + PGMQ1.10.0 test instance.' >&2; exit 1; }

native:
	@test -n "$(OUTPUT)" || { printf '%s\n' 'OUTPUT must name a new directory.' >&2; exit 1; }
	cargo run --locked -p job -- verify-native --output "$(OUTPUT)"
