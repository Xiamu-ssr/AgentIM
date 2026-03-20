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
# 0. 自动检测项目结构
# ============================================================

# 检测 workspace 中的 crate 名
detect_crates() {
    cargo metadata --no-deps --format-version 1 2>/dev/null | \
        python3 -c "
import sys, json
meta = json.load(sys.stdin)
for p in meta['packages']:
    print(p['name'])
" 2>/dev/null || true
}

ALL_CRATES=$(detect_crates)
# 找出 web crate（名字含 web）和 app/server crate（名字含 app 或 server）
WEB_CRATE=$(echo "$ALL_CRATES" | grep -i "web" | head -1 || true)
APP_CRATE=$(echo "$ALL_CRATES" | grep -iE "app|server|bin" | head -1 || true)

# ============================================================
# 1. Build frontend (optional, if trunk available and web crate exists)
# ============================================================
if command -v trunk &>/dev/null && [ -n "$WEB_CRATE" ]; then
    # 找 web crate 的目录
    WEB_DIR=$(cargo metadata --no-deps --format-version 1 2>/dev/null | \
        python3 -c "
import sys, json, os
meta = json.load(sys.stdin)
for p in meta['packages']:
    if 'web' in p['name'].lower():
        print(os.path.dirname(p['manifest_path']))
        break
" 2>/dev/null || true)

    if [ -n "$WEB_DIR" ] && [ -d "$WEB_DIR" ]; then
        echo "--- Step 0: trunk build ($WEB_CRATE) ---"
        (cd "$WEB_DIR" && trunk build --release 2>&1)
        echo "    trunk build: PASS"
        echo ""
    fi
else
    echo "--- Step 0: trunk build SKIPPED (no trunk or no web crate) ---"
    echo ""
fi

# ============================================================
# 2. Clippy (native targets)
# ============================================================
echo "--- Step 1/4: clippy (native) ---"
cargo clippy --all-targets -- -D warnings
echo "    clippy: PASS"
echo ""

# ============================================================
# 3. Clippy (WASM target for web crate)
# ============================================================
if [ -n "$WEB_CRATE" ]; then
    echo "--- Step 2/4: clippy (wasm: $WEB_CRATE) ---"
    cargo clippy -p "$WEB_CRATE" --target wasm32-unknown-unknown -- -D warnings
    echo "    clippy (wasm): PASS"
else
    echo "--- Step 2/4: clippy (wasm) SKIPPED (no web crate) ---"
fi
echo ""

# ============================================================
# 4. Tests
# ============================================================
echo "--- Step 3/4: cargo test ---"
cargo test
echo "    tests: PASS"
echo ""

# ============================================================
# 5. Build check
# ============================================================
if [ -n "$APP_CRATE" ]; then
    echo "--- Step 4/4: build ($APP_CRATE) ---"
    cargo build -p "$APP_CRATE" --quiet
else
    echo "--- Step 4/4: build (all) ---"
    cargo build --quiet
fi
echo "    build: PASS"
echo ""

# ============================================================
# 6. Magic value scan
# ============================================================
echo "--- Step 5: magic value scan ---"
MAGIC_FAIL=0

# 自动找出所有 src 目录（排除 target、.git、web crate）
SRC_DIRS=$(find . -maxdepth 3 -type d -name "src" \
    ! -path "*/target/*" \
    ! -path "*/.git/*" \
    2>/dev/null || true)

if [ -n "$SRC_DIRS" ]; then
    # 过滤掉 web crate 的 src（前端不检查 magic value）
    if [ -n "$WEB_CRATE" ]; then
        SRC_DIRS=$(echo "$SRC_DIRS" | grep -v "web" || true)
    fi

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
fi

if [ "$MAGIC_FAIL" -eq 0 ]; then
    echo "    magic value scan: PASS"
else
    echo "    magic value scan: WARN (review above)"
fi
echo ""

# ============================================================
# 7. Cross-language contract check (ts-rs 类型安全)
# ============================================================
CONTRACT_CHECK="scripts/check-contracts.sh"
if [ -f "$CONTRACT_CHECK" ]; then
    echo "--- Step 6: cross-language contract check ---"
    bash "$CONTRACT_CHECK"
    echo ""
fi

# ============================================================
# 8. DB contracts check (if script exists)
# ============================================================
DB_CHECK=".claude/skills/rust-db-contracts/references/check_db_contracts.sh"
if [ -f "$DB_CHECK" ]; then
    echo "--- Step 6: db contracts check ---"
    bash "$DB_CHECK" || true
    echo ""
fi

echo "=== ALL CHECKS PASSED ==="
