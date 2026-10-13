.PHONY: check check-ts check-web check-science check-core check-native-full check-deploy check-links

RUST_TOOLCHAIN := $(shell sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)
CARGO := rustup run $(RUST_TOOLCHAIN) cargo
PROFILE ?= debug
CARGO_PROFILE_FLAG := $(if $(filter release,$(PROFILE)),--release,)
SCIENCE_FEATURES := job/native-paper-test,job/native-sandbox-test,job/catalog-prepare,job/polymarket-history
QZ_JOB_BINARY ?= $(abspath $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)/$(PROFILE)/job)
LYCHEE ?= lychee

check: check-ts check-web check-deploy

check-ts:
	npm --prefix apps/service run typecheck
	npm --prefix apps/service test
	npm --prefix apps/research run typecheck
	npm --prefix apps/research test
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
	$(CARGO) test --locked $(CARGO_PROFILE_FLAG) -p job --features $(SCIENCE_FEATURES) --lib --test factors --test science_cli --test simulation --test allocation --test validation --test sealed --test native_sandbox_observer --test native_paper_target -- --test-threads=1

# Must use the matching built kernel. The service's integrated native test owns
# the real idea -> trials -> shared-account backtest -> portfolio -> receipt flow.
check-core:
	test -x "$(QZ_JOB_BINARY)"
	@directory=$$(mktemp -d "$${PI_SCRATCH_DIR:-$${RUNNER_TEMP:-$${TMPDIR:-/tmp}}}/qz-native-core.XXXXXX"); \
	trap 'rm -rf -- "$$directory"' EXIT; \
	binaries=$$(dirname "$(QZ_JOB_BINARY)"); \
	"$$binaries/browser_catalog_fixture" "$$directory/catalog" "$$binaries/catalog-prepare" && \
	QZ_JOB_BINARY="$(QZ_JOB_BINARY)" QZ_CATALOG_PREPARE_BINARY="$$binaries/catalog-prepare" QZ_BROWSER_FIXTURE="$$directory/catalog/prepared.json" npm --prefix apps/service run test:native

# Manual/formal-release science coverage only, on the retained three crates.
check-native-full:
	$(CARGO) clippy --locked --workspace --all-targets --all-features -- -D warnings
	$(CARGO) test --locked --workspace --all-features --no-fail-fast -- --test-threads=1

check-deploy:
	for script in deploy/install.sh deploy/docker/manage.sh deploy/docker/entrypoint.sh deploy/docker/native-build.sh deploy/docker/deploy.sh deploy/docker/update.sh; do bash -n "$$script" || exit; done
	node --test deploy/install.test.mjs deploy/docker/manage_shell.test.mjs deploy/docker/workflow.test.mjs runtimes/native/native-files.test.mjs runtimes/native/native-image-contract.test.mjs
	python3 -B -m unittest discover -s deploy -p package_test.py
	python3 -B -m unittest discover -s deploy/docker -p release_test.py

check-links:
	git ls-files -z --cached --others --exclude-standard -- '*.md' | xargs -0 $(LYCHEE) --config .lychee.toml --
