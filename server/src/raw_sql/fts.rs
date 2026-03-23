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

use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};

/// Create the FTS5 virtual table and triggers for message search.
///
/// The FTS5 table mirrors the `messages.content` column. Three triggers keep
/// it in sync: INSERT, UPDATE, DELETE.
///
/// Safe to call multiple times — uses IF NOT EXISTS / IF NOT EXISTS equivalents.
pub async fn create_fts_tables(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    let backend = db.get_database_backend();

    // Create FTS5 virtual table.
    db.execute(Statement::from_string(
        backend,
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content,
            content='messages',
            content_rowid='rowid'
        )"
        .to_string(),
    ))
    .await?;

    // Trigger: after INSERT on messages, insert into FTS.
    db.execute(Statement::from_string(
        backend,
        "CREATE TRIGGER IF NOT EXISTS messages_fts_insert AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
        END"
        .to_string(),
    ))
    .await?;

    // Trigger: after DELETE on messages, delete from FTS.
    db.execute(Statement::from_string(
        backend,
        "CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.rowid, old.content);
        END"
        .to_string(),
    ))
    .await?;

    // Trigger: after UPDATE on messages, update FTS.
    db.execute(Statement::from_string(
        backend,
        "CREATE TRIGGER IF NOT EXISTS messages_fts_update AFTER UPDATE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.rowid, old.content);
            INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
        END"
        .to_string(),
    ))
    .await?;

    Ok(())
}

/// Search messages using FTS5 full-text search with BM25 ranking.
///
/// Returns a list of message IDs matching the query, ordered by relevance.
/// The caller is responsible for filtering by agent ownership.
pub async fn fts_search(
    db: &DatabaseConnection,
    query: &str,
    limit: u32,
) -> Result<Vec<String>, sea_orm::DbErr> {
    let backend = db.get_database_backend();

    // FTS5 MATCH query with BM25 ranking.
    // Join back to messages table to get the message ID.
    let results = db
        .query_all(Statement::from_sql_and_values(
            backend,
            "SELECT m.id FROM messages m
             INNER JOIN messages_fts fts ON m.rowid = fts.rowid
             WHERE messages_fts MATCH $1
             ORDER BY bm25(messages_fts)
             LIMIT $2",
            [query.into(), (limit as i64).into()],
        ))
        .await?;

    let mut ids = Vec::new();
    for row in results {
        let id: String = row.try_get("", "id")?;
        ids.push(id);
    }

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};

    use crate::db;
    use crate::entity::{agent, message, user};

    async fn setup_with_messages(db: &sea_orm::DatabaseConnection) {
        // Create user + agents.
        let now = Utc::now();
        user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        agent::ActiveModel {
            id: Set("alice".into()),
            user_id: Set("u1".into()),
            name: Set("Alice".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        agent::ActiveModel {
            id: Set("bob".into()),
            user_id: Set("u1".into()),
            name: Set("Bob".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        // Insert messages.
        message::ActiveModel {
            id: Set("msg-1".into()),
            from_agent: Set("alice".into()),
            to_agent: Set(Some("bob".into())),
            channel_id: Set(None),
            content: Set("hello world from alice".into()),
            msg_type: Set(message::MsgType::Text),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        message::ActiveModel {
            id: Set("msg-2".into()),
            from_agent: Set("bob".into()),
            to_agent: Set(Some("alice".into())),
            channel_id: Set(None),
            content: Set("goodbye world from bob".into()),
            msg_type: Set(message::MsgType::Text),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        message::ActiveModel {
            id: Set("msg-3".into()),
            from_agent: Set("alice".into()),
            to_agent: Set(Some("bob".into())),
            channel_id: Set(None),
            content: Set("something else entirely".into()),
            msg_type: Set(message::MsgType::Text),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn fts_search_finds_matching_messages() {
        let db = db::test_db().await;
        setup_with_messages(&db).await;

        let results = super::fts_search(&db, "world", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"msg-1".to_string()));
        assert!(results.contains(&"msg-2".to_string()));
    }

    #[tokio::test]
    async fn fts_search_returns_empty_for_no_match() {
        let db = db::test_db().await;
        setup_with_messages(&db).await;

        let results = super::fts_search(&db, "nonexistent", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn fts_search_respects_limit() {
        let db = db::test_db().await;
        setup_with_messages(&db).await;

        let results = super::fts_search(&db, "world", 1).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn fts_search_specific_term() {
        let db = db::test_db().await;
        setup_with_messages(&db).await;

        let results = super::fts_search(&db, "goodbye", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "msg-2");
    }
}
