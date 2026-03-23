#!/usr/bin/env bash
# ============================================================
# dev.sh — 本地开发：同时启动后端和前端
#
# 用法:
#   bash scripts/dev.sh
#
# 环境变量:
#   AGENTIM_PORT=8900
#   AGENTIM_FRONTEND_PORT=3000
#   AGENTIM_FRONTEND_HOST=127.0.0.1
# ============================================================
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f ".env.local" ]; then
  set -a
  # shellcheck disable=SC1091
  source ".env.local"
  set +a
fi

export RUST_LOG="${RUST_LOG:-agentim_server=debug}"

PORT="${AGENTIM_PORT:-8900}"
FRONTEND_PORT="${AGENTIM_FRONTEND_PORT:-3000}"
FRONTEND_HOST="${AGENTIM_FRONTEND_HOST:-127.0.0.1}"
API_BASE_URL="${NEXT_PUBLIC_API_BASE_URL:-http://127.0.0.1:${PORT}}"
OAUTH_STATUS="not set (optional)"
if [ -n "${GITHUB_CLIENT_ID:-}" ]; then
  OAUTH_STATUS="configured"
fi

echo "=== AgentIM dev stack ==="
echo "  Backend:   http://127.0.0.1:${PORT}"
echo "  Frontend:  http://${FRONTEND_HOST}:${FRONTEND_PORT}"
echo "  API Base:  ${API_BASE_URL}"
echo "  Data:      ${AGENTIM_DATA_DIR:-~/.agentim/}"
echo "  OAuth:     ${OAUTH_STATUS}"
echo ""

cleanup() {
  trap - EXIT INT TERM
  if [ -n "${BACKEND_PID:-}" ]; then
    kill "${BACKEND_PID}" >/dev/null 2>&1 || true
  fi
  if [ -n "${FRONTEND_PID:-}" ]; then
    kill "${FRONTEND_PID}" >/dev/null 2>&1 || true
  fi
}

trap cleanup EXIT INT TERM

cargo run -p agentim-server -- --port "$PORT" &
BACKEND_PID=$!

(
  cd frontend
  NEXT_PUBLIC_API_BASE_URL="${API_BASE_URL}" \
    npm run dev -- --hostname "${FRONTEND_HOST}" --port "${FRONTEND_PORT}"
) &
FRONTEND_PID=$!

while kill -0 "${BACKEND_PID}" >/dev/null 2>&1 && kill -0 "${FRONTEND_PID}" >/dev/null 2>&1; do
  sleep 1
done

wait "${BACKEND_PID}" >/dev/null 2>&1 || true
wait "${FRONTEND_PID}" >/dev/null 2>&1 || true
