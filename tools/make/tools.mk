##@ Repository checks

.PHONY: deps-rust
deps-rust: ## Fetch locked Rust dependencies with network access
	@$(call log_target)
	@$(call require_command,cargo)
	@$(CARGO) fetch --manifest-path $(CORE_MANIFEST) --locked

.PHONY: test-python-core
test-python-core: ## Test the core Python development tools
	@$(call log_target)
	@$(PYTHON) -B -m unittest discover -s core/tools -p 'test_*.py'

.PHONY: test-python
test-python: test-python-core ## Test all repository Python development tools
	@$(PYTHON) -B -m unittest discover -s tools -p 'test_*.py'

.PHONY: check-boundary
check-boundary: ## Check core crate boundaries and retirement guards
	@$(call log_target)
	@$(PYTHON) -B core/tools/check_product_boundary.py --skip-sdk

.PHONY: check-boundary-all
check-boundary-all: ## Check core and JavaScript binding boundaries
	@$(call log_target)
	@$(PYTHON) -B core/tools/check_product_boundary.py

.PHONY: check-fuser
check-fuser: ## Check the authorized fuser source and lockfile provenance
	@$(call log_target)
	@$(PYTHON) -B core/tools/check_fuser_integrity.py

.PHONY: production-loc
production-loc: ## Report first-party production source line counts
	@$(call log_target)
	@$(PYTHON) -B tools/production_loc.py

.PHONY: check
check: check-boundary check-fuser test-python ## Run fast repository guard checks

.PHONY: ci
ci: fmt-check check clippy-core test-core-fast ## Run the CI fast-lane checks locally, plus all Python tool tests

.PHONY: lint
lint: fmt-check check-boundary check-fuser ## Run formatting and repository lint guards

.PHONY: test
test: test-python test-core-fast test-js-standin ## Run core and JavaScript tests without Docker

.PHONY: docker-check
docker-check: ## Verify that a local Docker Engine is reachable
	@$(call log_target)
	@$(call require_command,docker)
	@docker info >/dev/null
	@echo 'Docker Engine: ready'
