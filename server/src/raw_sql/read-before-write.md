# Raw SQL Exemption Zone

This directory contains raw SQL that **cannot** be expressed through SeaORM.

The only current exemption is **SQLite FTS5** (full-text search virtual tables,
triggers, and MATCH queries). SeaORM has zero FTS5 support — there is no ORM
alternative.

## Rules

1. **Do NOT add new files** without explicit user authorization.
2. **Do NOT modify existing files** without explicit user authorization.
3. Every `.rs` file must explain in its `//!` header why ORM cannot do the job.
4. Only SQLite extension features that are impossible to express in SeaORM belong
   here. Normal CRUD, queries, and schema operations MUST use SeaORM.
5. `check_db_contracts.sh` whitelists this directory — raw SQL checks are
   intentionally skipped for files under `raw_sql/`.

## Current Files

| File | Purpose |
|------|---------|
| `fts.rs` | FTS5 virtual table creation, triggers, and BM25 search queries |
