.PHONY: check check-ts check-web check-science check-core check-native-full check-source check-deploy generate-native-schema

RUST_TOOLCHAIN := $(shell sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)
CARGO := rustup run $(RUST_TOOLCHAIN) cargo
PROFILE ?= debug
CARGO_PROFILE_FLAG := $(if $(filter release,$(PROFILE)),--release,)
SCIENCE_FEATURES := job/native-sandbox-test,job/catalog-prepare,job/hyperliquid-offline
QZ_JOB_BINARY ?= $(abspath $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)/$(PROFILE)/job)

check: check-ts check-web check-deploy

generate-native-schema:
	@schema_tmp=$$(mktemp contracts/generated/.domain-v1.XXXXXX); \
	trap 'rm -f "$$schema_tmp"' EXIT; \
	$(CARGO) run --locked -p contracts --example generate > "$$schema_tmp" && mv "$$schema_tmp" contracts/generated/domain-v1.openapi.json

check-ts:
	npm --prefix apps/service run typecheck
	npm --prefix apps/service test
	npm --prefix apps/research run typecheck
	npm --prefix apps/research test
	npm --prefix apps/nautilus run typecheck
	npm --prefix apps/nautilus test
	npm --prefix apps/cli run typecheck
	npm --prefix apps/cli test

check-web:
	npm --prefix apps/web run typecheck
	npm --prefix apps/web test
	npm --prefix apps/web run build

# A single profile/feature set; no repeated all-target check + build + test.
check-science:
	$(CARGO) fmt --all -- --check
	$(CARGO) test --locked $(CARGO_PROFILE_FLAG) -p contracts -p domain --lib --tests
	$(CARGO) test --locked $(CARGO_PROFILE_FLAG) -p job --features $(SCIENCE_FEATURES) --lib --tests -- --test-threads=1

# Use the matching built kernel for real capital sizing and existing account
# protocols; original-market strategy replay is covered by check-science.
check-core:
	test -x "$(QZ_JOB_BINARY)"
	QZ_JOB_BINARY="$(QZ_JOB_BINARY)" npm --prefix apps/service run test:native
	npm --prefix apps/nautilus test
	node --experimental-strip-types --test apps/research/test/remote-native.test.ts apps/research/test/remote-admission.test.ts apps/research/test/native-scan-recovery.test.ts

# Manual/formal-release science coverage only, on the retained three crates.
check-native-full:
	$(CARGO) clippy --locked --workspace --all-targets --all-features -- -D warnings
	$(CARGO) test --locked --workspace --all-features --no-fail-fast -- --test-threads=1

check-source:
	python3 -B -m unittest discover -s runtimes/data -v

check-deploy: check-source
	for script in deploy/install.sh deploy/docker/manage.sh deploy/docker/entrypoint.sh deploy/docker/native-build.sh deploy/docker/deploy.sh deploy/docker/update.sh; do bash -n "$$script" || exit; done
	node --test deploy/install.test.mjs deploy/docker/manage_shell.test.mjs deploy/docker/runtime-update.test.mjs deploy/docker/pm-maintenance.test.mjs deploy/docker/native-build.test.mjs deploy/docker/native-glue.test.mjs deploy/docker/workflow.test.mjs runtimes/native/native-files.test.mjs runtimes/native/native-image-contract.test.mjs
	python3 -B -m unittest discover -s deploy -p package_test.py
	python3 -B -m unittest discover -s deploy/docker -p release_test.py
