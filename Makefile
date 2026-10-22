.DEFAULT_GOAL := help

CARGO ?= cargo
PYTHON ?= python3
ARGS ?=

.PHONY: help all build debug release build-release run run-release fmt fmt-check \
	check lint lint-all test test-all test-doc doc policy verify

# Keep the full verification sequence ordered even with make -j.
.NOTPARALLEL: verify

help: ## Show available targets (the default).
	@printf '%s\n' 'Usage: make <target>' ''
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\n%s\n' 'Overrides: CARGO="cargo +stable", PYTHON=python3, ARGS="--help" (run targets).'

all: build

build: ## Build Sonicfast in debug mode (alias: debug).
	$(CARGO) build --locked --bin sonicfast

debug: build

release: ## Build Sonicfast in release mode (alias: build-release).
	$(CARGO) build --locked --release --bin sonicfast

build-release: release

run: ## Build and run Sonicfast in debug mode; accepts ARGS.
	$(CARGO) run --locked --bin sonicfast -- $(ARGS)

run-release: ## Build and run Sonicfast in release mode; accepts ARGS.
	$(CARGO) run --locked --release --bin sonicfast -- $(ARGS)

fmt: ## Format all Rust code.
	$(CARGO) fmt --all

fmt-check: ## Check Rust formatting without changing files.
	$(CARGO) fmt --all --check

check: ## Type-check all targets with default features.
	$(CARGO) check --locked --all-targets

lint: ## Run Clippy on all targets, denying warnings.
	$(CARGO) clippy --locked --all-targets -- -D warnings

lint-all: ## Run Clippy with all features, denying warnings.
	$(CARGO) clippy --locked --all-targets --all-features -- -D warnings

test: ## Run all target tests with default features.
	$(CARGO) test --locked --all-targets

test-all: ## Run all target tests with all features.
	$(CARGO) test --locked --all-targets --all-features

test-doc: ## Run documentation tests with all features.
	$(CARGO) test --locked --all-features --doc

doc: ## Build all-feature documentation, denying warnings.
	RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" $(CARGO) doc --locked --all-features --no-deps

policy: ## Check dependency sources, review evidence, policy, launchers and release/docs regressions.
	$(PYTHON) .github/scripts/check-dependency-sources.py
	$(PYTHON) .github/scripts/check-review-evidence.py
	$(PYTHON) .github/scripts/test-policy-checks.py
	$(PYTHON) packaging/test-launchers.py
	$(PYTHON) packaging/test-release.py
	$(PYTHON) packaging/test-release-names.py
	$(PYTHON) packaging/flatpak/test-metainfo.py
	$(PYTHON) .github/scripts/test-docs.py

verify: fmt-check policy lint lint-all test test-all test-doc doc ## Run all CONTRIBUTING.md checks without changing files.
