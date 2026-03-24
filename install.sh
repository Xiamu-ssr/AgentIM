#!/usr/bin/env sh
# AgentIM CLI installer
# Usage: curl -sSL https://raw.githubusercontent.com/Xiamu-ssr/AgentIM/main/install.sh | sh
set -eu

REPO="Xiamu-ssr/AgentIM"
INSTALL_DIR="$HOME/.agentim/bin"
BINARY_NAME="agentim"

# ── Detect platform ──

detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)   os="linux" ;;
    Darwin)  os="darwin" ;;
    *)       echo "Error: unsupported OS '$os'"; exit 1 ;;
  esac

  case "$arch" in
    x86_64|amd64)   arch="amd64" ;;
    aarch64|arm64)   arch="arm64" ;;
    *)               echo "Error: unsupported architecture '$arch'"; exit 1 ;;
  esac

  echo "${os}-${arch}"
}

# ── Resolve latest version ──

get_latest_version() {
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
  elif command -v wget > /dev/null 2>&1; then
    wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
  else
    echo "Error: curl or wget is required" >&2
    exit 1
  fi
}

# ── Download and install ──

main() {
  platform="$(detect_platform)"
  echo "Detected platform: ${platform}"

  echo "Fetching latest release..."
  version="$(get_latest_version)"
  if [ -z "$version" ]; then
    echo "Error: could not determine latest version. Is there a GitHub release?"
    echo ""
    echo "If no release exists yet, build from source instead:"
    echo "  git clone https://github.com/${REPO}.git"
    echo "  cd AgentIM && cargo install --path cli"
    exit 1
  fi
  echo "Latest version: ${version}"

  archive="agentim-cli-${platform}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${version}/${archive}"

  echo "Downloading ${url}..."
  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  if command -v curl > /dev/null 2>&1; then
    curl -fSL "$url" -o "${tmpdir}/${archive}"
  else
    wget -q "$url" -O "${tmpdir}/${archive}"
  fi

  echo "Extracting..."
  tar -xzf "${tmpdir}/${archive}" -C "${tmpdir}"

  # Install binary
  mkdir -p "${INSTALL_DIR}"
  mv "${tmpdir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
  chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

  echo ""
  echo "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"

  # Check if already in PATH
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*)
      echo ""
      echo "Ready! Run: agentim --help"
      ;;
    *)
      echo ""
      echo "Add to your PATH by adding this to your shell profile:"
      echo ""
      echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
      echo ""
      echo "Then run: agentim --help"
      ;;
  esac
}

main
