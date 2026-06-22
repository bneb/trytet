.PHONY: build release cartridges tarball test clippy doctor clean

# Build the tet binary (native, debug)
build:
	cargo build --bin tet

# Build the tet binary (native, release)
release:
	cargo build --release --bin tet
	@echo "Binary: target/release/tet"

# Build all cartridge .wasm files with cargo-component
# Only builds the 3 proven cartridges (JS, Regex, JMESPath)
cartridges:
	@mkdir -p dist/cartridges
	@for crate in js-evaluator regex-evaluator jmespath-cartridge; do \
		echo "=== Building $$crate ==="; \
		(cd crates/$$crate && cargo component build --release) || exit 1; \
		find crates/$$crate -name "*.wasm" -path "*/wasm32*/*" -exec cp {} dist/cartridges/ \; 2>/dev/null || \
			find target -name "*.wasm" -path "*/wasm32*/*" -newer Cargo.toml -exec cp {} dist/cartridges/ \; 2>/dev/null; \
	done
	@echo "Cartridges built to dist/cartridges/"
	@ls -la dist/cartridges/ 2>/dev/null || echo "No .wasm files found — check cargo-component output"

# Create release tarball: tet binary + cartridges + install script
tarball: release cartridges
	@mkdir -p dist
	@cp target/release/tet dist/
	@cp install.sh dist/
	@cd dist && tar czf tet-darwin-$(shell uname -m).tar.gz tet cartridges/ install.sh
	@echo "Tarball: dist/tet-darwin-$(shell uname -m).tar.gz"
	@ls -lh dist/tet-darwin-*.tar.gz

# Run tests and lint
test:
	cargo test -- --test-threads=4

clippy:
	cargo clippy --all-targets

# Quick sanity check that the install is functional
doctor:
	@echo "=== Trytet Doctor ==="
	@echo -n "Binary: " && (command -v tet >/dev/null && echo "✓" || echo "✗ (not in PATH, run: make release)")
	@echo -n "Cargo build: " && (cargo build --bin tet >/dev/null 2>&1 && echo "✓" || echo "✗")
	@echo -n "Cartridge dir: " && (ls ~/.trytet/cartridges/*.wasm >/dev/null 2>&1 && echo "✓" || echo "✗ (run: make cartridges)")
	@echo -n "Rust toolchain: " && (rustc --version >/dev/null 2>&1 && echo "✓" || echo "✗")
	@echo -n "cargo-component: " && (cargo component --version >/dev/null 2>&1 && echo "✓" || echo "✗")

clean:
	cargo clean
	rm -rf dist
