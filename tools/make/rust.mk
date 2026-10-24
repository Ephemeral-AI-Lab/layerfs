##@ Rust

.PHONY: fmt
fmt: ## Format all Rust workspace members
	@$(call log_target)
	@$(CARGO) fmt --manifest-path $(CORE_MANIFEST) --all

.PHONY: fmt-check
fmt-check: ## Check Rust formatting without changing files
	@$(call log_target)
	@$(CARGO) fmt --manifest-path $(CORE_MANIFEST) --all -- --check

.PHONY: build-core
build-core: ## Build all core crates except the Node addon
	@$(call log_target)
	@$(CARGO) build $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node

.PHONY: build-core-release
build-core-release: ## Build all core crates in release mode
	@$(call log_target)
	@$(CARGO) build $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node --release

.PHONY: build-daemon
build-daemon: ## Build the layerfs-daemon binary
	@$(call log_target)
	@$(CARGO) build $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) -p layerfs-daemon

.PHONY: check-core
check-core: ## Run Cargo check for all core crates
	@$(call log_target)
	@$(CARGO) check $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node

.PHONY: clippy-core
clippy-core: ## Run Clippy with warnings denied on core crates
	@$(call log_target)
	@$(CARGO) clippy $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node --all-targets -- -D warnings

.PHONY: test-package
test-package: ## Build and test one Rust package; use PACKAGE=layerfs-content
	@$(call log_target)
	@test -n "$(PACKAGE)" || { echo 'set PACKAGE, for example PACKAGE=layerfs-storage' >&2; exit 2; }
	@$(call require_timeout)
	@$(CARGO) test $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) -p $(PACKAGE) --all-targets --no-run
	@$(TIMEOUT_PREFIX) $(CARGO) test $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) -p $(PACKAGE) --all-targets

.PHONY: test-core
test-core: test-package ## Alias for a package-scoped Rust test run

.PHONY: build-test-fast
build-test-fast: ## Build the CI fast test selection
	@$(call log_target)
	@$(call require_command,$(PYTHON))
	@$(STRICT) targets="$$( $(PYTHON) -B core/tools/ci_tests.py targets fast )"; \
	$(CARGO) test $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node $$targets --no-run $(TEST_BUILD_FLAGS)

.PHONY: run-test-fast
run-test-fast: ## Run the CI fast test selection with cargo-nextest
	@$(call log_target)
	@$(call require_command,$(PYTHON))
	@$(call require_timeout)
	@command -v cargo-nextest >/dev/null 2>&1 || { echo 'cargo-nextest is required; install cargo-nextest or use test-package' >&2; exit 1; }
	@$(STRICT) targets="$$( $(PYTHON) -B core/tools/ci_tests.py targets fast )"; \
	filter="$$( $(PYTHON) -B core/tools/ci_tests.py filter fast )"; \
	$(TIMEOUT_PREFIX) $(CARGO) nextest run $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node $$targets --profile ci -E "$$filter"

.PHONY: test-core-fast
# Sequential sub-makes keep the build outside the timed run, even under -j.
test-core-fast: ## Build and run the CI fast test selection
	@$(MAKE) $(SUBMAKE_FLAGS) build-test-fast
	@$(MAKE) $(SUBMAKE_FLAGS) run-test-fast

.PHONY: build-test-nightly
build-test-nightly: ## Build the CI nightly test selection
	@$(call log_target)
	@$(call require_command,$(PYTHON))
	@$(STRICT) targets="$$( $(PYTHON) -B core/tools/ci_tests.py targets nightly )"; \
	$(CARGO) test $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node $$targets --no-run $(TEST_BUILD_FLAGS)

.PHONY: run-test-nightly
run-test-nightly: ## Run the CI nightly test selection with cargo-nextest
	@$(call log_target)
	@$(call require_command,$(PYTHON))
	@$(call require_timeout)
	@command -v cargo-nextest >/dev/null 2>&1 || { echo 'cargo-nextest is required; install cargo-nextest' >&2; exit 1; }
	@$(STRICT) targets="$$( $(PYTHON) -B core/tools/ci_tests.py targets nightly )"; \
	filter="$$( $(PYTHON) -B core/tools/ci_tests.py filter nightly )"; \
	$(TIMEOUT_PREFIX) $(CARGO) nextest run $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) --workspace --exclude layerfs-node $$targets --profile ci -E "$$filter"

.PHONY: test-core-nightly
# Sequential sub-makes keep the build outside the timed run, even under -j.
test-core-nightly: ## Build and run the CI nightly test selection
	@$(MAKE) $(SUBMAKE_FLAGS) build-test-nightly
	@$(MAKE) $(SUBMAKE_FLAGS) run-test-nightly

.PHONY: build-mount-tests
build-mount-tests: ## Build Linux FUSE tests and record their executables
	@$(call log_target)
	@$(call require_command,$(PYTHON))
	@mkdir -p "$$(dirname "$(MOUNT_BUILD_JSON)")" "$$(dirname "$(MOUNT_EXECUTABLES)")"
	@$(STRICT) targets="$$( $(PYTHON) -B core/tools/ci_tests.py mount-targets )"; \
	$(CARGO) test $(CARGO_ARGS) $(CARGO_NETWORK_FLAGS) -p layerfs-daemon $$targets --no-run --message-format json > "$(MOUNT_BUILD_JSON)"
	@$(PYTHON) -B core/tools/ci_tests.py executables < "$(MOUNT_BUILD_JSON)" > "$(MOUNT_EXECUTABLES)"
	@test -s "$(MOUNT_EXECUTABLES)"
	@wc -l < "$(MOUNT_EXECUTABLES)"

.PHONY: build
build: build-core build-native-debug ## Build the Rust core and debug Node addon

.PHONY: build-release
build-release: build-core-release build-native-release ## Build release Rust artifacts and the Node addon
