# Makefile for the project

.PHONY: help test test-contracts test-frontend test-pipeline fuzz clean

help:
	@echo "Available targets:"
	@echo "  make test            - Run all tests (contracts, frontend, pipeline)"
	@echo "  make test-contracts  - Run Rust contract tests (cargo test)"
	@echo "  make test-frontend   - Run frontend tests (vitest)"
	@echo "  make test-pipeline   - Run pipeline tests"
	@echo "  make fuzz            - Run fuzz targets"
	@echo "  make clean           - Clean build artifacts"

test: test-contracts test-frontend test-pipeline

# Run the Rust contract test suite.
test-contracts:
	cargo test --workspace

# Run the frontend test suite via the package.json script (vitest).
test-frontend:
	cd frontend && npm test

# Run the pipeline test suite via the package.json script.
test-pipeline:
	cd pipeline && npm test

# Run fuzz targets where applicable.
fuzz:
	cargo fuzz run --all-targets

clean:
	cargo clean
	rm -rf frontend/node_modules pipeline/node_modules
