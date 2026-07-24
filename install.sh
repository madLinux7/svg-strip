#!/bin/sh
set -e

REPO="madLinux7/svg-strip"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BINARY="svg-strip"

# Detect OS and match the names used by the release workflow.
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$OS" in
  linux)  OS="linux" ;;
  darwin) OS="macos" ;;
  *) echo "Error: unsupported OS: $OS" >&2; exit 1 ;;
esac

# Detect architecture and match the names used by the release workflow.
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Error: unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

echo "Fetching latest release..."
TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
if [ -z "$TAG" ]; then
  echo "Error: could not determine latest release" >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/${BINARY}-${OS}-${ARCH}"
echo "Downloading ${BINARY} ${TAG} for ${OS}/${ARCH}..."

TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT

if ! curl -fSL --progress-bar "$URL" -o "$TMP"; then
  echo "Error: download failed. Check that a binary exists for ${OS}/${ARCH}." >&2
  exit 1
fi

chmod +x "$TMP"
mkdir -p "$INSTALL_DIR"
mv "$TMP" "${INSTALL_DIR}/${BINARY}"

echo "${BINARY} ${TAG} installed to ${INSTALL_DIR}/${BINARY}"

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *) echo "Warning: ${INSTALL_DIR} is not in your PATH. Add it with:"
     echo "  export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac
