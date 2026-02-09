#!/usr/bin/env bash
set -euo pipefail

REPO="chem-william/slurm_jd"
INSTALL_DIR="$HOME/.local/bin"
BINARY="jobs_done"

info()  { printf '\033[1;34m%s\033[0m\n' "$*"; }
warn()  { printf '\033[1;33m%s\033[0m\n' "$*"; }
error() { printf '\033[1;31m%s\033[0m\n' "$*" >&2; }

# --- Platform check ---
ARCH="$(uname -m)"
OS="$(uname -s)"

if [ "$OS" != "Linux" ] || [ "$ARCH" != "x86_64" ]; then
    error "Unsupported platform: $OS $ARCH"
    error "Only Linux x86_64 is supported."
    exit 1
fi

# --- Pick a download tool ---
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1"; }
    fetch_header() { curl -fsSI "$1" 2>/dev/null; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO- "$1"; }
    fetch_header() { wget -qS --spider "$1" 2>&1; }
else
    error "Neither curl nor wget found. Please install one and try again."
    exit 1
fi

# --- Resolve the latest release tag ---
info "Fetching latest release..."
REDIRECT_URL="$(fetch_header "https://github.com/$REPO/releases/latest" \
    | grep -i '^location:' | tail -1 | tr -d '\r' | awk '{print $2}')"

if [ -z "${REDIRECT_URL:-}" ]; then
    error "Could not determine latest release. Check your internet connection."
    exit 1
fi

TAG="$(basename "$REDIRECT_URL")"
info "Latest release: $TAG"

# --- Download & extract ---
ASSET="${BINARY}-${TAG}-x86_64-unknown-linux-musl.tar.gz"
URL="https://github.com/$REPO/releases/download/${TAG}/${ASSET}"

info "Downloading $ASSET..."
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

fetch "$URL" > "$TMPDIR/$ASSET"

mkdir -p "$INSTALL_DIR"
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"
install -m 755 "$TMPDIR/${BINARY}-${TAG}-x86_64-unknown-linux-musl/${BINARY}" "$INSTALL_DIR/$BINARY"

info "Installed $BINARY to $INSTALL_DIR/$BINARY"

# --- PATH check ---
PATH_ADDED=false
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    warn "$INSTALL_DIR is not on your PATH."
    printf 'Add export PATH="%s:$PATH" to ~/.bashrc? [y/N] ' "$INSTALL_DIR"
    read -r answer
    if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
        printf '\n# Added by jobs_done installer\nexport PATH="%s:$PATH"\n' "$INSTALL_DIR" >> "$HOME/.bashrc"
        info "Updated ~/.bashrc"
        PATH_ADDED=true
    fi
fi

# --- Optional shell integrations ---
printf '\n'
info "Optional shell integrations:"
printf '\n'

# Login hook — show jobs since last session on each login
printf 'Add "jobs_done" to ~/.bash_profile (show jobs on login)? [y/N] '
read -r answer
LOGIN_HOOK=false
if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
    printf '\n# Show finished SLURM jobs since last session on login\njobs_done\n' >> "$HOME/.bash_profile"
    info "Updated ~/.bash_profile"
    LOGIN_HOOK=true
fi

# Alias — quick shortcut for last 24h
printf 'Add alias jd="jobs_done --day" to ~/.bashrc? [y/N] '
read -r answer
ALIAS_ADDED=false
if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
    printf '\n# Shortcut: show SLURM jobs from the last 24 hours\nalias jd="jobs_done --day"\n' >> "$HOME/.bashrc"
    info "Updated ~/.bashrc"
    ALIAS_ADDED=true
fi

# --- Summary ---
printf '\n'
info "=== Installation complete ==="
printf '  Binary:  %s\n' "$INSTALL_DIR/$BINARY"

if $PATH_ADDED || $LOGIN_HOOK || $ALIAS_ADDED; then
    printf '\n'
    info "Shell changes made:"
    $PATH_ADDED  && printf '  ~/.bashrc       — added %s to PATH\n' "$INSTALL_DIR"
    $LOGIN_HOOK  && printf '  ~/.bash_profile — jobs_done runs on login\n'
    $ALIAS_ADDED && printf '  ~/.bashrc       — alias jd="jobs_done --day"\n'
    printf '\n'
    info "Run 'source ~/.bashrc' (and/or 'source ~/.bash_profile') to apply changes,"
    info "or start a new shell session."
else
    printf '\n'
    info "No shell changes were made."
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
        warn "Remember to add $INSTALL_DIR to your PATH."
    fi
fi
