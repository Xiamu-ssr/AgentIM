#!/usr/bin/env bash
# ============================================================
# check.sh —— AgentIM quality gate
#
# 用法: bash scripts/check.sh
# ============================================================
set -euo pipefail

PROJECT_NAME="AgentIM"

echo "=== ${PROJECT_NAME} quality gate ==="
echo ""

# ============================================================
# 1. Clippy
# ============================================================
echo "--- Step 1/6: clippy ---"
cargo clippy --all-targets -- -D warnings
echo "    clippy: PASS"
echo ""

# ============================================================
# 2. Tests
# ============================================================
echo "--- Step 2/6: cargo test ---"
cargo test
echo "    tests: PASS"
echo ""

# ============================================================
# 3. Build
# ============================================================
echo "--- Step 3/6: build ---"
cargo build --quiet
echo "    build: PASS"
echo ""

# ============================================================
# 4. Magic value scan
# ============================================================
echo "--- Step 4/6: magic value scan ---"
MAGIC_FAIL=0

# 搜索所有 Rust src 目录（排除 target、.git、frontend）
SRC_DIRS=$(find . -maxdepth 3 -type d -name "src" \
    ! -path "*/target/*" \
    ! -path "*/.git/*" \
    ! -path "*/frontend/*" \
    2>/dev/null || true)

if [ -n "$SRC_DIRS" ]; then
    # String literals that look like hardcoded config defaults
    STRING_HITS=$(echo "$SRC_DIRS" | tr '\n' ' ' | xargs -I{} find {} -name '*.rs' \
        ! -path '*/io/*' \
        ! -name 'consts.rs' \
        -exec awk '/^#\[cfg\(test\)\]/{exit} /"aim_"|"agentim"|"localhost:8900"/{print FILENAME":"NR": "$0}' {} \; \
        2>/dev/null || true)

    if [ -n "$STRING_HITS" ]; then
        echo "    WARN: potential magic strings outside isolation zone:"
        echo "$STRING_HITS" | head -20
        MAGIC_FAIL=1
    fi
fi

if [ "$MAGIC_FAIL" -eq 0 ]; then
    echo "    magic value scan: PASS"
else
    echo "    magic value scan: WARN (review above)"
fi
echo ""

# ============================================================
# 5. Frontend typecheck
# ============================================================
if [ -d "frontend" ]; then
    echo "--- Step 5/6: frontend typecheck ---"
    (
        cd frontend
        npx tsc --noEmit
    )
    echo "    frontend typecheck: PASS"
    echo ""
fi

# ============================================================
# 6. Cross-language contract check (ts-rs)
# ============================================================
CONTRACT_CHECK="scripts/check-contracts.sh"
if [ -f "$CONTRACT_CHECK" ]; then
    echo "--- Step 6: cross-language contract check ---"
    bash "$CONTRACT_CHECK"
    echo ""
fi

# ============================================================
# 7. DB contracts check (if script exists)
# ============================================================
DB_CHECK=".claude/skills/rust-db-contracts/references/check_db_contracts.sh"
if [ -f "$DB_CHECK" ]; then
    echo "--- Step 7: db contracts check ---"
    bash "$DB_CHECK" server/src
    echo ""
fi

echo "=== ALL CHECKS PASSED ==="
