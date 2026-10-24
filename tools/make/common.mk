SHELL := /bin/bash
# Prefix for multi-command recipe lines: stop on the first failing command,
# unset variable or pipe stage, as GitHub Actions' `bash -eo pipefail` steps do.
# Written per line because .SHELLFLAGS needs GNU make 3.82 and macOS ships 3.81.
STRICT := set -euo pipefail;

ROOT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/../..)
CORE_MANIFEST := $(ROOT_DIR)/core/Cargo.toml
API_DIR := $(ROOT_DIR)/api

RUST_TOOLCHAIN ?= 1.85.1
CARGO := cargo +$(RUST_TOOLCHAIN)
CARGO_ARGS := --manifest-path $(CORE_MANIFEST) --locked
CARGO_OFFLINE ?= 1
CARGO_NETWORK_FLAGS := $(if $(filter 1 true yes,$(CARGO_OFFLINE)),--offline,)

PYTHON ?= python3
NODE ?= node
NPM ?= npm
TEST_TIMEOUT ?= 120
TIMEOUT_BIN := $(shell command -v timeout 2>/dev/null || command -v gtimeout 2>/dev/null || true)
TIMEOUT_PREFIX := $(TIMEOUT_BIN) $(TEST_TIMEOUT)
TEST_BUILD_FLAGS ?=
PACKAGE ?=
MOUNT_BUILD_JSON ?= $(ROOT_DIR)/core/target/mount-test-build.json
MOUNT_EXECUTABLES ?= $(ROOT_DIR)/core/target/mount-test-executables.tsv

VERSION = $(shell git -C "$(ROOT_DIR)" describe --tags --always --dirty 2>/dev/null || echo dev)

define log_target
printf '\033[0;32m==> %s\033[0m\n' "$@"
endef

define require_command
command -v $(1) >/dev/null 2>&1 || { echo "required command not found: $(1)" >&2; exit 1; }
endef

# Every test run needs a wall clock ceiling; refuse to run without one.
define require_timeout
test -n "$(TIMEOUT_BIN)" || { echo 'timeout or gtimeout is required to cap test runs at $(TEST_TIMEOUT) s; on macOS run: brew install coreutils' >&2; exit 1; }
endef

# Flags for a literal $(MAKE) over the same command groups, for ordered steps.
SUBMAKE_FLAGS = --no-print-directory $(addprefix -f ,$(MAKEFILE_LIST))

.PHONY: help
help: ## Show the available LayerFS development commands
	@printf '\033[1;34mLayerFS development commands\033[0m\n\n'
	@awk 'BEGIN {FS = ":.*##"} /^##@/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5)} /^[a-zA-Z0-9_.-]+:.*##/ {printf "  \033[36m%-24s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

.PHONY: doctor
doctor: ## Check required and optional local development tools
	@$(call log_target)
	@$(STRICT) \
	for command in make cargo rustup $(PYTHON); do \
		$(call require_command,$$command); \
	done; \
	$(CARGO) --version; \
	rustup toolchain list | grep -E '^$(RUST_TOOLCHAIN)([- ]|$$)' >/dev/null || { \
		echo "Rust toolchain $(RUST_TOOLCHAIN) is not installed" >&2; exit 1; \
	}; \
	for component in rustfmt clippy; do \
		rustup component list --toolchain "$(RUST_TOOLCHAIN)" --installed | grep -E "^$$component([ -]|$$)" >/dev/null || { \
			echo "Rust component $$component is not installed for $(RUST_TOOLCHAIN)" >&2; exit 1; \
		}; \
	done; \
	for command in $(NODE) $(NPM) docker; do \
		if command -v "$$command" >/dev/null 2>&1; then printf 'optional %-8s: available\n' "$$command"; else printf 'optional %-8s: missing\n' "$$command"; fi; \
	done

.PHONY: version
version: ## Print repository and toolchain versions
	@$(call log_target)
	@printf 'source:  %s\n' '$(VERSION)'
	@$(CARGO) --version
	@$(PYTHON) --version
	@if command -v $(NODE) >/dev/null 2>&1; then $(NODE) --version; else echo 'node: unavailable'; fi

.PHONY: clean
clean: ## Remove all Cargo build output and the staged addon (prefer tools/cleanup.py for worktrees)
	@$(call log_target)
	@$(CARGO) clean --manifest-path $(CORE_MANIFEST)
	@rm -f $(API_DIR)/binding/layerfs.node

.PHONY: setup
setup: deps-rust js-install ## Prepare Rust dependencies and JavaScript workspaces
