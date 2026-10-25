##@ Docker and E2E

RESOURCES ?=
PLATFORM ?= linux
PROFILE ?= debug
JOBS ?= 2
E2E_TIMEOUT ?= 90
BUNDLE ?=
E2E_SCRIPT ?= api/agent/sdk/test/verify/calls_real.mjs
E2E_ARGS ?= inner store
OWNER ?=
BUILD_IMAGE ?=
RUNTIME_IMAGE ?=
SANDBOX_IMAGE ?=

.PHONY: worktree-register
worktree-register: docker-check ## Register isolated Cargo and Docker resources
	@$(call log_target)
	@test -n "$(OWNER)" -a -n "$(BUILD_IMAGE)" -a -n "$(RUNTIME_IMAGE)" -a -n "$(SANDBOX_IMAGE)" || { \
		echo 'set OWNER, BUILD_IMAGE, RUNTIME_IMAGE and SANDBOX_IMAGE' >&2; exit 2; \
	}
	@$(PYTHON) tools/worktree.py register --worktree "$(ROOT_DIR)" --owner "$(OWNER)" \
		--build-image "$(BUILD_IMAGE)" --runtime-image "$(RUNTIME_IMAGE)" --sandbox-image "$(SANDBOX_IMAGE)"

.PHONY: worktree-prepare
worktree-prepare: docker-check ## Prepare a registered worktree's locked dependencies
	@$(call log_target)
	@test -n "$(RESOURCES)" || { echo 'set RESOURCES=/absolute/path/resources.json' >&2; exit 2; }
	@$(PYTHON) tools/worktree.py prepare --resources "$(RESOURCES)" --platform "$(PLATFORM)"

.PHONY: worktree-build
worktree-build: docker-check ## Build a sealed daemon and SDK bundle for E2E
	@$(call log_target)
	@test -n "$(RESOURCES)" || { echo 'set RESOURCES=/absolute/path/resources.json' >&2; exit 2; }
	@$(PYTHON) tools/worktree.py build --resources "$(RESOURCES)" --platform "$(PLATFORM)" \
		--profile "$(PROFILE)" --jobs "$(JOBS)"

.PHONY: worktree-run
worktree-run: docker-check ## Run a real SDK scenario in an isolated Docker E2E environment
	@$(call log_target)
	@test -n "$(RESOURCES)" -a -n "$(BUNDLE)" || { \
		echo 'set RESOURCES=/absolute/path/resources.json and BUNDLE=/absolute/path/sealed-bundle' >&2; exit 2; \
	}
	@$(PYTHON) tools/worktree.py run --resources "$(RESOURCES)" --bundle "$(BUNDLE)" \
		--timeout "$(E2E_TIMEOUT)" -- $(NODE) "$(E2E_SCRIPT)" $(E2E_ARGS)

.PHONY: e2e
e2e: worktree-run ## Alias for worktree-run
