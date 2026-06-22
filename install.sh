#!/usr/bin/env bash
set -euo pipefail

REPO="bneb/trytet"
BIN="tet"
VERSION="${TRYTET_VERSION:-latest}"
INSTALL_DIR="${TRYTET_INSTALL_DIR:-$HOME/.local/bin}"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac
case "$OS" in
    linux|darwin) ;;
    *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

TARBALL="tet-${OS}-${ARCH}.tar.gz"
if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/${REPO}/releases/latest/download/${TARBALL}"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${TARBALL}"
fi

echo "Installing Trytet CLI..."

# Try downloading pre-built binary first
if command -v curl &>/dev/null; then
    HTTP_CODE=$(curl -sLf -o /tmp/${TARBALL} -w "%{http_code}" "$URL")
    if [ "$HTTP_CODE" = "200" ] && [ -s "/tmp/${TARBALL}" ]; then
        # SHA-256 verification against release SHA256SUMS
        SHA256SUMS_URL="${URL%/*}/SHA256SUMS"
        if command -v sha256sum &>/dev/null; then
            SHA256_CMD="sha256sum"
        elif command -v shasum &>/dev/null; then
            SHA256_CMD="shasum -a 256"
        fi
        if [ -n "${SHA256_CMD:-}" ]; then
            HTTP_SUMS=$(curl -sLf -o "/tmp/tet_SHA256SUMS" -w "%{http_code}" "$SHA256SUMS_URL" 2>/dev/null || true)
            if [ "$HTTP_SUMS" = "200" ] && [ -s "/tmp/tet_SHA256SUMS" ]; then
                COMPUTED=$($SHA256_CMD "/tmp/${TARBALL}" | cut -d' ' -f1)
                EXPECTED=$(grep -F "${TARBALL}" "/tmp/tet_SHA256SUMS" | head -1 | cut -d' ' -f1)
                if [ -z "$EXPECTED" ]; then
                    echo "error: checksum for ${TARBALL} not found in SHA256SUMS" >&2
                    rm -f "/tmp/${TARBALL}" "/tmp/tet_SHA256SUMS"
                    exit 1
                fi
                if [ "$COMPUTED" != "$EXPECTED" ]; then
                    echo "error: checksum mismatch for ${TARBALL}" >&2
                    echo "  expected: ${EXPECTED}" >&2
                    echo "  actual:   ${COMPUTED}" >&2
                    rm -f "/tmp/${TARBALL}" "/tmp/tet_SHA256SUMS"
                    exit 1
                fi
                echo "  SHA-256 checksum verified"
            else
                echo "warning: unable to fetch SHA256SUMS, skipping verification" >&2
            fi
            rm -f "/tmp/tet_SHA256SUMS"
        fi

        mkdir -p "$INSTALL_DIR"
        tar xzf "/tmp/${TARBALL}" -C "$INSTALL_DIR"
        chmod +x "$INSTALL_DIR/$BIN"
        rm -f "/tmp/${TARBALL}"

        # Set up cartridge directory
        CARTRIDGE_DIR="${TRYTET_CARTRIDGE_DIR:-$HOME/.trytet/cartridges}"
        mkdir -p "$CARTRIDGE_DIR"
        # If cartridges were in the tarball, copy them
        if [ -d "$INSTALL_DIR/cartridges" ] && [ -n "$(ls -A "$INSTALL_DIR/cartridges" 2>/dev/null)" ]; then
            cp "$INSTALL_DIR/cartridges"/*.wasm "$CARTRIDGE_DIR/" 2>/dev/null || true
        fi

        # PATH check
        if ! echo "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
            SHELL_NAME=$(basename "${SHELL:-sh}")
            echo "⚠  $INSTALL_DIR is not in your PATH."
            case "$SHELL_NAME" in
                zsh)  echo "   Add this to ~/.zshrc:"; echo '   export PATH="$HOME/.local/bin:$PATH"' ;;
                bash) echo "   Add this to ~/.bashrc:"; echo '   export PATH="$HOME/.local/bin:$PATH"' ;;
                fish) echo "   Run: fish_add_path $INSTALL_DIR" ;;
            esac
        fi

        echo ""
        echo "✅ Trytet CLI installed to $INSTALL_DIR/$BIN"
        echo "   Run 'tet --help' to get started."
        echo "   Cartridge directory: $CARTRIDGE_DIR"
        exit 0
    fi
    rm -f "/tmp/${TARBALL}"
fi

# Fallback: compile from source
echo "⚠ Pre-built binary not available. Compiling from source..."
if ! command -v cargo &>/dev/null; then
    echo "Rust toolchain required. Install from https://rustup.rs"
    exit 1
fi

TEMPDIR=$(mktemp -d)
if ! git clone --depth 1 "https://github.com/${REPO}.git" "$TEMPDIR" 2>/dev/null; then
    echo "Cannot clone repository. Try building from a local checkout with: cargo build --release --bin tet"
    rm -rf "$TEMPDIR"
    exit 1
fi
cd "$TEMPDIR"

cargo build --release --bin tet
mkdir -p "$INSTALL_DIR"
cp target/release/tet "$INSTALL_DIR/tet"

# Build cartridge .wasm files
CARTRIDGE_DIR="${TRYTET_CARTRIDGE_DIR:-$HOME/.trytet/cartridges}"
mkdir -p "$CARTRIDGE_DIR"
if command -v cargo-component &>/dev/null; then
    echo "Building cartridge .wasm files..."
    for crate in js-evaluator regex-evaluator jmespath-cartridge; do
        if [ -d "crates/$crate" ]; then
            (cd "crates/$crate" && cargo component build --release 2>&1) || true
            WASM_FILE=$(find "crates/$crate" -name "*.wasm" -path "*/wasm32*/*" 2>/dev/null | head -1)
            if [ -n "$WASM_FILE" ]; then
                cp "$WASM_FILE" "$CARTRIDGE_DIR/"
                echo "  → $(basename "$WASM_FILE")"
            fi
        fi
    done
fi

rm -rf "$TEMPDIR"
echo ""
echo "✅ Trytet CLI compiled and installed to $INSTALL_DIR/tet"
echo "   Cartridge directory: $CARTRIDGE_DIR"
echo "   Run 'tet --help' to get started."
