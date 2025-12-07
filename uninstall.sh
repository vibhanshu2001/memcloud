#!/bin/sh
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Banner
echo "${MAGENTA}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                                                              ║"
echo "║        ⛅  M E M C L O U D   U N I N S T A L L E R  🧹        ║"
echo "║                                                              ║"
echo "║      'Borrowed RAM is returned... balance restored.'         ║"
echo "║                                                              ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo "${NC}"

# Logging helpers
log_info() {
    echo "${BLUE}➤${NC} $1"
}

log_success() {
    echo "${GREEN}✔${NC} $1"
}

log_warn() {
    echo "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo "${RED}✖${NC} $1"
}

echo ""
log_info "Initiating MemCloud cleanup protocol..."
sleep 0.4

# Stop running daemon
log_info "Checking for active MemNode daemon..."
if [ -f "$HOME/.memcloud/memnode.pid" ]; then
    PID=$(cat "$HOME/.memcloud/memnode.pid")
    if kill -0 "$PID" 2>/dev/null; then
        kill "$PID" 2>/dev/null || true
        log_warn "MemNode daemon (PID: $PID) gracefully retired."
    fi
    rm -f "$HOME/.memcloud/memnode.pid"
else
    log_info "No active daemon found. (Clean slate 👍)"
fi

# Remove binaries
echo ""
log_info "Disintegrating MemCloud binaries from your system paths..."

if [ -f /usr/local/bin/memnode ]; then
    sudo rm -f /usr/local/bin/memnode
    log_info "Removed memnode binary."
fi

if [ -f /usr/local/bin/memcli ]; then
    sudo rm -f /usr/local/bin/memcli
    log_info "Removed memcli binary."
fi

# Remove Unix socket
if [ -S /tmp/memcloud.sock ]; then
    rm -f /tmp/memcloud.sock
    log_info "Evaporated stale /tmp/memcloud.sock"
fi

# Ask to remove config directory
echo ""
echo "${CYAN}✨ Optional Cleanup:${NC} Remove MemCloud config/state (~/.memcloud)? [y/N]"
read -r response < /dev/tty
if [ "$response" = "y" ] || [ "$response" = "Y" ]; then
    rm -rf "$HOME/.memcloud"
    log_info "Config directory wiped. Fresh as new RAM."
else
    log_warn "Preserving ~/.memcloud for archaeology or future reinstalls."
fi

# Outro
echo ""
log_success "MemCloud has been fully uninstalled. Your RAM sovereignty is restored. 🧠👑"
echo ""
echo "If you were using the JS SDK, remove it with:"
echo "  ${CYAN}npm uninstall memcloud${NC}"
echo ""
echo "${MAGENTA}Thanks for trying MemCloud — see you in the distributed future!${NC}"
echo ""
