use std::fs;

use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, Schema, Statement,
};
use tracing::info;

use crate::config::AppConfig;
use crate::entity;

/// Initialize the database: create data directory, connect, set pragmas, create tables.
pub async fn init_db(config: &AppConfig) -> anyhow::Result<DatabaseConnection> {
    let data_dir = config.resolved_data_dir();
    fs::create_dir_all(&data_dir)?;

    let db_path = config.db_path();
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    info!("Connecting to database: {}", db_path.display());
    let db = Database::connect(&db_url).await?;

    // Enable WAL mode for better concurrent read performance.
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA journal_mode=WAL",
    ))
    .await?;

    create_all_tables(&db).await?;

    info!("Database initialized");
    Ok(db)
}

/// Create all tables from SeaORM entities.
/// Called on startup — uses IF NOT EXISTS so it's safe to re-run.
async fn create_all_tables(db: &DatabaseConnection) -> anyhow::Result<()> {
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);

    // Create tables from entities (order matters for logical FK dependencies).
    let stmts = [
        schema.create_table_from_entity(entity::user::Entity),
        schema.create_table_from_entity(entity::agent::Entity),
        schema.create_table_from_entity(entity::contact::Entity),
        schema.create_table_from_entity(entity::message::Entity),
        schema.create_table_from_entity(entity::message_read::Entity),
        schema.create_table_from_entity(entity::channel::Entity),
        schema.create_table_from_entity(entity::channel_member::Entity),
    ];

    for stmt in stmts {
        db.execute(builder.build(&stmt)).await?;
    }

    // Create FTS5 virtual table and sync triggers (raw SQL — SeaORM has no FTS5 support).
    create_fts_table(db).await?;

    Ok(())
}

/// Create the FTS5 virtual table and triggers for automatic sync with messages table.
///
/// This is the sole location for raw SQL in the project. FTS5 virtual tables
/// cannot be expressed in SeaORM entity-first mode.
async fn create_fts_table(db: &DatabaseConnection) -> anyhow::Result<()> {
    let raw_sqls = [
        // FTS5 virtual table for full-text search with BM25 ranking.
        // unicode61 provides character-level matching; sufficient for MVP.
        // v0.2 can add jieba tokenizer for better Chinese segmentation.
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content,
            content='messages',
            content_rowid='rowid',
            tokenize='unicode61'
        )",
        // Trigger: auto-index new messages.
        "CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
        END",
        // Trigger: auto-remove deleted messages from index.
        "CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.rowid, old.content);
        END",
        // Trigger: auto-update modified messages in index.
        "CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.rowid, old.content);
            INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
        END",
    ];

    for sql in raw_sqls {
        db.execute(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
    }

    Ok(())
}

#[cfg(test)]
pub async fn test_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA journal_mode=WAL",
    ))
    .await
    .unwrap();

    create_all_tables(&db).await.unwrap();
    db
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    use super::*;

    #[tokio::test]
    async fn tables_created_successfully() {
        let db = test_db().await;

        // Insert a user to verify the table exists.
        let user = entity::user::ActiveModel {
            id: Set("test-user-id".to_string()),
            github_id: Set(12345),
            github_name: Set("testuser".to_string()),
            avatar_url: Set(None),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        };
        let result = user.insert(&db).await;
        assert!(result.is_ok());

        // Verify we can query it back.
        let found = entity::user::Entity::find_by_id("test-user-id")
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().github_name, "testuser");
    }

    #[tokio::test]
    async fn agent_table_works() {
        let db = test_db().await;

        // Need a user first (logical FK).
        let user = entity::user::ActiveModel {
            id: Set("u1".to_string()),
            github_id: Set(1),
            github_name: Set("user1".to_string()),
            avatar_url: Set(None),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        };
        user.insert(&db).await.unwrap();

        let agent = entity::agent::ActiveModel {
            id: Set("alice-bot".to_string()),
            user_id: Set("u1".to_string()),
            name: Set("Alice Bot".to_string()),
            token_hash: Set("fakehash123".to_string()),
            avatar_url: Set(None),
            bio: Set(Some("A test bot".to_string())),
            status: Set(entity::agent::AgentStatus::Active),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        };
        let result = agent.insert(&db).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn fts5_table_exists() {
        let db = test_db().await;

        // FTS5 table should be queryable.
        let result = db
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT * FROM messages_fts LIMIT 1",
            ))
            .await;
        assert!(result.is_ok());
    }
}
