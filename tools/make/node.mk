##@ JavaScript SDK

.PHONY: js-check
js-check: ## Check that Node.js and npm are available
	@$(call log_target)
	@$(call require_command,$(NODE))
	@$(call require_command,$(NPM))
	@$(NODE) --version
	@$(NPM) --version

.PHONY: js-install
js-install: js-check ## Link the JavaScript workspaces without changing package locks
	@$(call log_target)
	@$(NPM) --prefix $(API_DIR) install --no-audit --no-fund --package-lock=false
	@if test -f $(API_DIR)/agent/cli/package.json; then \
		$(NPM) --prefix $(API_DIR)/agent/cli install --no-audit --no-fund --package-lock=false; \
	fi

.PHONY: build-native-debug
build-native-debug: js-install ## Build the debug layerfs.node addon on macOS or Linux
	@$(call log_target)
	@$(NPM) --prefix $(API_DIR) run build:native -- --debug

.PHONY: build-native-release
build-native-release: js-install ## Build the release layerfs.node addon on macOS or Linux
	@$(call log_target)
	@$(NPM) --prefix $(API_DIR) run build:native

.PHONY: test-js-standin
test-js-standin: js-install ## Run JavaScript SDK and CLI stand-in tests without Docker
	@$(call log_target)
	@$(NODE) api/agent/sdk/test/calls.mjs
	@$(NODE) api/agent/sdk/test/exec_output.mjs
	@for case_name in surface lifecycle copy sandbox worker-route transport-order; do \
		$(NODE) api/agent/sdk/test/packaging.mjs "$$case_name"; \
	done
	@for case_name in on off failure layerfs-env start ids no-content store daemon-host; do \
		$(NODE) api/agent/sdk/test/trace.mjs "$$case_name"; \
	done
	@$(NODE) --test api/agent/cli/test/cli.test.mjs

.PHONY: test-js-native
test-js-native: build-native-debug ## Run stand-in tests plus the real addon dispatch case
	@$(call log_target)
	@$(NODE) api/agent/sdk/test/packaging.mjs dispatch

.PHONY: test-js
test-js: test-js-standin test-js-native ## Run JavaScript tests that do not require Docker or a Store volume
