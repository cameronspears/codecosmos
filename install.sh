#!/bin/bash
set -e

# Cosmos installer - detects your system and installs the right binary

REPO="cameronspears/cosmos"
INSTALL_DIR="/usr/local/bin"

echo ""
echo "  Installing cosmos..."
echo ""

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin)
    # macOS: use the .pkg installer
    echo "  Detected: macOS"
    echo ""
    
    TMP_DIR=$(mktemp -d)
    PKG_PATH="$TMP_DIR/cosmos-macos-installer.pkg"
    
    echo "  Downloading installer..."
    curl -fsSL "https://github.com/$REPO/releases/latest/download/cosmos-macos-installer.pkg" -o "$PKG_PATH"
    
    echo "  Running installer (you may be prompted for your password)..."
    sudo installer -pkg "$PKG_PATH" -target /
    
    rm -rf "$TMP_DIR"
    
    echo ""
    echo "  Done! Run 'cosmos' in any project folder to get started."
    echo ""
    exit 0
    ;;
  linux)
    case "$ARCH" in
      x86_64) ARTIFACT="cosmos-linux-x64.tar.gz" ;;
      aarch64) ARTIFACT="cosmos-linux-arm64.tar.gz" ;;
      *) echo "  Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  *)
    echo "  Unsupported OS: $OS"
    echo "  Please download manually from: https://github.com/$REPO/releases"
    exit 1
    ;;
esac

URL="https://github.com/$REPO/releases/latest/download/$ARTIFACT"

echo "  Detected: $OS ($ARCH)"
echo ""

# Download and extract
TMP_DIR=$(mktemp -d)
cd "$TMP_DIR"

echo "  Downloading..."
curl -fsSL "$URL" | tar xz

# Install
echo "  Installing to $INSTALL_DIR..."
if [ -w "$INSTALL_DIR" ]; then
  mv cosmos "$INSTALL_DIR/"
else
  sudo mv cosmos "$INSTALL_DIR/"
fi

# Cleanup
cd - > /dev/null
rm -rf "$TMP_DIR"

echo ""
echo "  Done! Run 'cosmos' in any project folder to get started."
echo ""
