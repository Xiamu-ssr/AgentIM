#!/usr/bin/env bash
# ============================================================
# check_db_contracts.sh —— Rust + SeaORM 显式契约检测脚本
#
# 用法: ./check_db_contracts.sh [src_dir]
# 默认 src_dir = ./src
#
# 集成到 CI: cargo build && ./check_db_contracts.sh
# ============================================================

set -euo pipefail

SRC_DIR="${1:-./src}"
ERRORS=0
WARNINGS=0

red()    { printf "\033[31m%s\033[0m\n" "$1"; }
yellow() { printf "\033[33m%s\033[0m\n" "$1"; }
green()  { printf "\033[32m%s\033[0m\n" "$1"; }

error()   { red "  ❌ $1"; ERRORS=$((ERRORS + 1)); }
warn()    { yellow "  ⚠ $1"; WARNINGS=$((WARNINGS + 1)); }
ok()      { green "  ✅ $1"; }

# ============================================================
# 自动检测 entity 目录（不硬编码路径）
# ============================================================
detect_entity_dir() {
    local dirs
    dirs=$(grep -rl "DeriveEntityModel" "$SRC_DIR" 2>/dev/null | xargs -I{} dirname {} | sort -u)
    echo "$dirs"
}

ENTITY_DIRS=$(detect_entity_dir)

echo "═══════════════════════════════════════"
echo "  🔒 Rust + SeaORM 显式契约检测"
echo "═══════════════════════════════════════"
echo ""

# ============================================================
# 1. 数据库操作只走 SeaORM
# ============================================================
echo "🔍 [1] 数据库操作是否只走 SeaORM"

# 1a. 禁止其他数据库库
other_db_libs=$(grep -rn \
    -e "use rusqlite" \
    -e "use sqlx" \
    -e "use diesel" \
    -e "use tokio_postgres" \
    -e "use mysql" \
    "$SRC_DIR" 2>/dev/null || true)

if [ -n "$other_db_libs" ]; then
    error "发现非 SeaORM 数据库库引用:"
    while IFS= read -r line; do red "     $line"; done <<< "$other_db_libs"
else
    ok "未发现其他数据库库"
fi

# 1b. 禁止裸 SQL
raw_sql=$(grep -rn \
    -e "execute_unprepared" \
    -e "query_raw" \
    --include="*.rs" \
    "$SRC_DIR" 2>/dev/null \
    | grep -v "///\|//\|#\[" \
    | grep -v "raw_sql/" \
    || true)

if [ -n "$raw_sql" ]; then
    error "发现裸 SQL 操作:"
    while IFS= read -r line; do red "     $line"; done <<< "$raw_sql"
else
    ok "未发现裸 SQL 操作"
fi

# 1c. 禁止硬编码 SQL 语句
hardcoded_sql=$(grep -rn \
    -E "(\"|\')\ *(INSERT INTO|SELECT .+ FROM|UPDATE .+ SET|DELETE FROM)" \
    --include="*.rs" \
    "$SRC_DIR" 2>/dev/null \
    | grep -v "///\|//\|#\[\|doc\|test\|migration" \
    | grep -v "raw_sql/" \
    || true)

if [ -n "$hardcoded_sql" ]; then
    error "发现硬编码 SQL 语句:"
    while IFS= read -r line; do red "     $line"; done <<< "$hardcoded_sql"
else
    ok "未发现硬编码 SQL 语句"
fi

echo ""

# ============================================================
# 2. 类型安全不被绕过
# ============================================================
echo "🔍 [2] 类型安全检查"

# 2a. col_expr 中禁止字符串字面量
col_expr_string=$(grep -rn \
    -e "col_expr.*Expr::value.*String" \
    -e "col_expr.*Expr::value.*\"" \
    --include="*.rs" \
    "$SRC_DIR" 2>/dev/null \
    | grep -v "///\|//" \
    || true)

if [ -n "$col_expr_string" ]; then
    error "col_expr 中使用了字符串字面量（应用 .set(ActiveModel) 替代）:"
    while IFS= read -r line; do red "     $line"; done <<< "$col_expr_string"
else
    ok "col_expr 未使用字符串字面量"
fi

# 2b. Entity 文件中禁止 serde_json::Value（弱类型 JSON）
if [ -n "$ENTITY_DIRS" ]; then
    json_value_in_entity=$(grep -rn "serde_json::Value" $ENTITY_DIRS 2>/dev/null \
        | grep -v "///\|//" \
        || true)

    if [ -n "$json_value_in_entity" ]; then
        warn "Entity 中使用了 serde_json::Value（建议用强类型 struct）:"
        while IFS= read -r line; do yellow "     $line"; done <<< "$json_value_in_entity"
    else
        ok "Entity 中未使用弱类型 JSON"
    fi
fi

# 2c. Entity 文件中禁止 NaiveDateTime
if [ -n "$ENTITY_DIRS" ]; then
    naive_dt_in_entity=$(grep -rn "NaiveDateTime" $ENTITY_DIRS 2>/dev/null \
        | grep -v "///\|//" \
        || true)

    if [ -n "$naive_dt_in_entity" ]; then
        error "Entity 中使用了 NaiveDateTime（应用 DateTimeUtc）:"
        while IFS= read -r line; do red "     $line"; done <<< "$naive_dt_in_entity"
    else
        ok "Entity 中未使用 NaiveDateTime"
    fi
fi

echo ""

# ============================================================
# 3. 业务规范
# ============================================================
echo "🔍 [3] 业务规范检查"

# 3a. 状态变更是否检查 can_transition_to
#
# 精确区分两类 Set(Status):
#   - 初始创建 INSERT（`status: Set(Active)` 在 struct 字面量里）→ 无需检查
#   - UPDATE 状态转换（`am.status = Set(Revoked)` 赋值给已有 model）→ 必须检查
#
# 判断方法:
#   - `am.status = Set(` 或 `model.status = Set(` 是赋值 → UPDATE 语义
#   - `status: Set(` 是 struct 字面量 → INSERT 语义（跳过）
#
# 额外排除:
#   - 测试代码（用 awk 在 #[cfg(test)] 处截断每个文件）
#   - db.rs 中的测试辅助函数
#   - raw_sql/ 豁免区中的测试 fixture

# 只匹配赋值形式的状态变更（UPDATE 语义）
status_updates=$(
    find "$SRC_DIR" -name '*.rs' ! -path '*/raw_sql/*' \
        -exec awk '
            /^#\[cfg\(test\)\]/ { exit }
            /\.status[[:space:]]*=[[:space:]]*Set\(/ { print FILENAME ":" NR ": " $0 }
        ' {} \; 2>/dev/null \
    | grep -v "///\|//" \
    || true
)

if [ -n "$status_updates" ]; then
    has_unchecked=false
    while IFS= read -r line; do
        file=$(echo "$line" | cut -d: -f1)
        lineno=$(echo "$line" | cut -d: -f2)

        # 检查该行前 15 行内是否有 can_transition_to 调用
        start=$((lineno - 15))
        [ "$start" -lt 1 ] && start=1
        nearby_check=$(sed -n "${start},${lineno}p" "$file" 2>/dev/null \
            | grep -c "can_transition_to" || true)
        nearby_check=$((nearby_check + 0))

        if [ "$nearby_check" -eq 0 ]; then
            warn "状态变更未见 can_transition_to 检查: $line"
            has_unchecked=true
        fi
    done <<< "$status_updates"
    if [ "$has_unchecked" = false ]; then
        ok "状态变更均有流转检查"
    fi
else
    ok "未发现状态变更操作（或已过滤）"
fi

# 3b. 软删除表查询是否过滤
# [FIX] 同样用 here-string 避免子 shell 问题
if [ -n "$ENTITY_DIRS" ]; then
    soft_delete_entities=$(grep -rl "is_deleted" $ENTITY_DIRS 2>/dev/null | sort -u || true)

    for entity_file in $soft_delete_entities; do
        [ -f "$entity_file" ] || continue
        entity_name=$(basename "$entity_file" .rs)
        [ "$entity_name" = "mod" ] && continue

        entity_finds=$(grep -rn "${entity_name}::Entity::find" "$SRC_DIR" 2>/dev/null || true)

        if [ -n "$entity_finds" ]; then
            while IFS= read -r line; do
                file=$(echo "$line" | cut -d: -f1)
                lineno=$(echo "$line" | cut -d: -f2)

                has_filter=$(sed -n "${lineno},$((lineno + 10))p" "$file" 2>/dev/null \
                    | grep -c "is_deleted\|IsDeleted" || echo "0")

                if [ "$has_filter" -eq 0 ]; then
                    warn "查询软删除表 ${entity_name} 但未见 is_deleted 过滤: $line"
                fi
            done <<< "$entity_finds"
        fi
    done
fi

echo ""

# ============================================================
# 4. Entity 文件完整性
# ============================================================
echo "🔍 [4] Entity 文件完整性"

if [ -z "$ENTITY_DIRS" ]; then
    yellow "  ⚠ 未检测到 Entity 文件（无 DeriveEntityModel），跳过"
else
    for dir in $ENTITY_DIRS; do
        for file in "$dir"/*.rs; do
            [ -f "$file" ] || continue
            fname=$(basename "$file")
            [ "$fname" = "mod.rs" ] && continue
            [ "$fname" = "prelude.rs" ] && continue

            # 4a. 文件级 doc comment
            if ! head -3 "$file" | grep -q "^//!"; then
                error "$fname 缺少文件级 doc comment（//! 开头）"
            fi

            # 4b. 状态枚举是否有 can_transition_to
            has_enum=$(grep -c "DeriveActiveEnum" "$file" 2>/dev/null || true)
            has_enum=$((has_enum + 0))
            if [ "$has_enum" -gt 0 ]; then
                has_transition=$(grep -c "can_transition_to" "$file" 2>/dev/null || true)
                has_transition=$((has_transition + 0))
                if [ "$has_transition" -eq 0 ]; then
                    has_status_enum=$(grep -E "enum.*(Status|State)" "$file" || true)
                    if [ -n "$has_status_enum" ]; then
                        warn "$fname 有状态枚举但未定义 can_transition_to()"
                    fi
                fi
            fi

            # 4c. 字段注释覆盖度
            pub_fields=$(grep -c "pub " "$file" 2>/dev/null || true)
            pub_fields=$((pub_fields + 0))
            doc_comments=$(grep -c "/// " "$file" 2>/dev/null || true)
            doc_comments=$((doc_comments + 0))
            if [ "$pub_fields" -gt 4 ] && [ "$doc_comments" -lt 3 ]; then
                warn "$fname 字段注释偏少（$doc_comments 个注释 / $pub_fields 个字段）"
            fi
        done
    done
fi

echo ""

# ============================================================
# 5. FTS5 全文搜索检查（AgentIM 专项）
# ============================================================
echo "🔍 [5] FTS5 全文搜索使用检查"

# 有 FTS5 表但用 LIKE '%xxx%' 做模糊搜索的，应走 FTS5
#
# 只检测 SeaORM 的 .contains() 调用（出现在 Column 枚举的 filter 链中），
# 不匹配 Rust 标准库的 Vec::contains / str::contains / Range::contains 等。
# SeaORM 模式: `Column::Xxx.contains(` 或 `.col(Column::Xxx).contains(`

# 1) LIKE 硬编码
like_search=$(grep -rn \
    -E "LIKE\s+'%|like\s+'%" \
    --include="*.rs" \
    "$SRC_DIR" 2>/dev/null \
    | grep -v "///\|//\|#\[" \
    | grep -v "raw_sql/" \
    || true)

# 2) SeaORM .contains() — 只匹配 Column 枚举前缀的调用
seaorm_contains=$(
    find "$SRC_DIR" -name '*.rs' ! -path '*/raw_sql/*' \
        -exec awk '
            /^#\[cfg\(test\)\]/ { exit }
            /Column::.*\.contains\(/ { print FILENAME ":" NR ": " $0 }
        ' {} \; 2>/dev/null \
    | grep -v "///\|//" \
    || true
)

fts_violations=""
[ -n "$like_search" ] && fts_violations="${fts_violations}${like_search}"$'\n'
[ -n "$seaorm_contains" ] && fts_violations="${fts_violations}${seaorm_contains}"$'\n'
fts_violations=$(echo "$fts_violations" | sed '/^$/d')

if [ -n "$fts_violations" ]; then
    warn "发现 LIKE/SeaORM contains 模糊搜索（如有 FTS5 表，应优先走全文搜索）:"
    while IFS= read -r line; do yellow "     $line"; done <<< "$fts_violations"
else
    ok "未发现绕过 FTS5 的模糊搜索"
fi

echo ""

# ============================================================
# 汇总
# ============================================================
echo "═══════════════════════════════════════"
if [ "$ERRORS" -gt 0 ]; then
    red "  ❌ 发现 $ERRORS 个错误，$WARNINGS 个警告"
    exit 1
elif [ "$WARNINGS" -gt 0 ]; then
    yellow "  ⚠ 无错误，$WARNINGS 个警告（建议修复）"
    exit 0
else
    green "  ✅ 所有检查通过"
    exit 0
fi
