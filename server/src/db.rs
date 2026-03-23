use std::fs;

use sea_orm::{sqlx::sqlite::SqliteJournalMode, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, Schema};
use tracing::info;

use crate::config::AppConfig;
use crate::entity;

/// Initialize the database: create data directory, connect, set pragmas, create tables.
pub async fn init_db(config: &AppConfig) -> anyhow::Result<DatabaseConnection> {
    let data_dir = config.resolved_data_dir();
    fs::create_dir_all(&data_dir)?;

    let db_path = config.db_path();
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let mut options = ConnectOptions::new(db_url);
    options.map_sqlx_sqlite_opts(|sqlx_options| {
        sqlx_options
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
    });

    info!("Connecting to database: {}", db_path.display());
    let db = Database::connect(options).await?;

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
    let mut stmts = [
        schema.create_table_from_entity(entity::user::Entity),
        schema.create_table_from_entity(entity::agent::Entity),
        schema.create_table_from_entity(entity::contact::Entity),
        schema.create_table_from_entity(entity::message::Entity),
        schema.create_table_from_entity(entity::message_read::Entity),
        schema.create_table_from_entity(entity::channel::Entity),
        schema.create_table_from_entity(entity::channel_member::Entity),
    ];

    for stmt in &mut stmts {
        stmt.if_not_exists();
        db.execute(builder.build(stmt)).await?;
    }

    Ok(())
}

#[cfg(test)]
pub async fn test_db() -> DatabaseConnection {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.map_sqlx_sqlite_opts(|sqlx_options| {
        sqlx_options
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
    });
    let db = Database::connect(options).await.unwrap();

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
    async fn create_all_tables_is_idempotent() {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.map_sqlx_sqlite_opts(|sqlx_options| {
            sqlx_options
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
        });
        let db = Database::connect(options).await.unwrap();

        create_all_tables(&db).await.unwrap();
        create_all_tables(&db).await.unwrap();
    }
}
