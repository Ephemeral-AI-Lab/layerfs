.DEFAULT_GOAL := _run

# Resolve the repository root from this file so `make -f path/to/Makefile`
# works from any directory; command groups run with the root as cwd.
REPO_ROOT := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))

_run:
	@$(MAKE) --no-print-directory --warn-undefined-variables -C "$(REPO_ROOT)" \
		-f tools/make/common.mk \
		-f tools/make/tools.mk \
		-f tools/make/rust.mk \
		-f tools/make/node.mk \
		-f tools/make/e2e.mk \
		$(if $(MAKECMDGOALS),$(MAKECMDGOALS),help)

.PHONY: _run

# Every requested goal is delegated to _run; the empty recipe keeps make quiet.
$(if $(MAKECMDGOALS),$(MAKECMDGOALS): %: _run ; @:)
