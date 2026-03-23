//! # FTS5 Full-Text Search — Raw SQL Exemption
//!
//! ## Why raw SQL?
//! SQLite FTS5 virtual tables, triggers, and MATCH queries have zero SeaORM
//! support. There is no ORM abstraction for `CREATE VIRTUAL TABLE ... USING fts5`,
//! `MATCH`, or `bm25()`. This is the only raw SQL in the project.
//!
//! ## What this file does
//! - `create_fts_tables()` — creates the FTS5 virtual table and auto-sync triggers
//! - `fts_search()` — performs full-text search with BM25 ranking
//!
//! See `raw_sql/read-before-write.md` for the exemption policy.

// TODO: Step 5 will implement FTS5 here.
