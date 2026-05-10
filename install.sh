#!/usr/bin/env bash
set -e

echo "Installing Trytet Universal Execution Primitive..."
# In a real environment, this would download pre-built binaries from GitHub Releases
# e.g., curl -sL https://github.com/trytet/trytet/releases/latest/download/tet-x86_64-linux.tar.gz | tar xz

echo "Compiling CLI from source (local MVP mode)..."
cargo build --release --bin tet

echo "Installing CLI to ~/.local/bin/tet..."
mkdir -p ~/.local/bin
cp target/release/tet ~/.local/bin/tet

echo "Compiling Cartridges to wasm32-wasip1..."
for d in crates/*; do
  if [ -d "$d" ]; then
    (cd "$d" && cargo component build --release)
  fi
done

echo "Installing Cartridges to ~/.trytet/cartridges..."
mkdir -p ~/.trytet/cartridges
cp crates/*/target/wasm32-wasip1/release/*.wasm ~/.trytet/cartridges/

echo "✅ Trytet installed successfully!"
echo "Make sure ~/.local/bin is in your PATH. Run 'tet --help' to get started."
