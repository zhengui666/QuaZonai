.PHONY: check native
check:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo test --locked --workspace
native:
	cargo run --locked -p job -- verify-native --output $(OUTPUT)
