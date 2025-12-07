#!/bin/sh
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ASCII Art
echo "${CYAN}"
echo " __  __                 _____ _                 _ "
echo "|  \/  |               / ____| |               | |"
echo "| \  / | ___ _ __ ___ | |    | | ___  _   _  __| |"
echo "| |\/| |/ _ \ '_ \` _ \| |    | |/ _ \| | | |/ _\` |"
echo "| |  | |  __/ | | | | | |____| | (_) | |_| | (_| |"
echo "|_|  |_|\___|_| |_| |_|\_____|_|\___/ \__,_|\__,_|"
echo "${NC}"
echo "       Distributed In-Memory Cloud Storage        "
echo ""

log_info() {
    echo "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo "${RED}[ERROR]${NC} $1"
}

# Detect OS and Architecture
log_info "Detecting system architecture..."
OS="$(uname -s)"
ARCH="$(uname -m)"

# Fetch latest version from GitHub API
log_info "Checking for latest version..."
VERSION=$(curl -s https://api.github.com/repos/vibhanshu2001/memcloud/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$VERSION" ]; then
    log_warn "Could not fetch latest version. Defaulting to v0.1.0"
    VERSION="v0.1.0"
else
    log_info "Latest version is $VERSION"
fi

case "$OS" in
  Linux)
    ASSET_URL="https://github.com/vibhanshu2001/memcloud/releases/download/${VERSION}/memcloud-x86_64-unknown-linux-gnu.tar.gz"
    ;;
  Darwin)
    if [ "$ARCH" = "x86_64" ]; then
        ASSET_URL="https://github.com/vibhanshu2001/memcloud/releases/download/${VERSION}/memcloud-x86_64-apple-darwin.tar.gz"
    elif [ "$ARCH" = "arm64" ]; then
        ASSET_URL="https://github.com/vibhanshu2001/memcloud/releases/download/${VERSION}/memcloud-aarch64-apple-darwin.tar.gz"
    else
        log_error "Unsupported architecture: $ARCH"
        exit 1
    fi
    ;;
  *)
    log_error "Unsupported OS: $OS"
    exit 1
    ;;
esac

echo ""
log_info "Found compatible build for ${OS} (${ARCH})"
log_info "Downloading MemCloud ${VERSION}..."
curl -L -o memcloud.tar.gz "$ASSET_URL"

echo ""
log_info "Extracting and installing..."
mkdir -p /tmp/memcloud_install
tar -xzf memcloud.tar.gz -C /tmp/memcloud_install

# Move binaries to /usr/local/bin (requires sudo)
log_info "Moving binaries to /usr/local/bin (sudo access required)..."
if sudo mv /tmp/memcloud_install/memnode /usr/local/bin/ && sudo mv /tmp/memcloud_install/memcli /usr/local/bin/; then
    # Clean up
    rm memcloud.tar.gz
    rm -rf /tmp/memcloud_install

    echo ""
    log_success "MemCloud installed successfully! 🚀"
    echo ""
    echo "To start the daemon, run:"
    echo "  ${CYAN}memnode --name \"MyNode\"${NC}"
    echo ""
else
    log_error "Failed to move binaries. Please check your permissions."
    exit 1
fi
