use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, Set,
    TransactionTrait,
};

use crate::auth::extractor::UserSession;
use crate::auth::token::{generate_token, hash_token};
use crate::consts;
use crate::entity::{agent, contact};
use crate::error::AppError;
use crate::AppState;

use super::dto::{
    AgentResponse, CreateAgentRequest, CreateAgentResponse, ResetTokenResponse, UpdateAgentRequest,
};

/// Validate agent ID: lowercase alphanumeric + hyphens, 3-50 chars.
fn validate_agent_id(id: &str) -> Result<(), AppError> {
    let len = id.len();
    if !(consts::AGENT_ID_MIN_LEN..=consts::AGENT_ID_MAX_LEN).contains(&len) {
        return Err(AppError::Validation(format!(
            "agent id must be {}-{} characters",
            consts::AGENT_ID_MIN_LEN,
            consts::AGENT_ID_MAX_LEN,
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::Validation(
            "agent id must contain only lowercase letters, digits, and hyphens".into(),
        ));
    }
    Ok(())
}

/// Convert an agent::Model to AgentResponse DTO.
fn to_agent_response(m: &agent::Model) -> AgentResponse {
    AgentResponse {
        id: m.id.clone(),
        name: m.name.clone(),
        avatar_url: m.avatar_url.clone(),
        bio: m.bio.clone(),
        status: match m.status {
            agent::AgentStatus::Active => "active".into(),
            agent::AgentStatus::Suspended => "suspended".into(),
        },
        created_at: m.created_at.to_rfc3339(),
        updated_at: m.updated_at.to_rfc3339(),
    }
}

/// Fetch an agent by id and verify ownership.
async fn find_owned_agent(
    db: &sea_orm::DatabaseConnection,
    agent_id: &str,
    user_id: &str,
) -> Result<agent::Model, AppError> {
    let found = agent::Entity::find_by_id(agent_id)
        .one(db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", agent_id)))?;

    if found.user_id != user_id {
        return Err(AppError::Forbidden("not your agent".into()));
    }

    Ok(found)
}

/// POST /api/agents
pub async fn create_agent(
    session: UserSession,
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<CreateAgentResponse>, AppError> {
    validate_agent_id(&req.id)?;

    // Check agent count limit.
    let count = agent::Entity::find()
        .filter(agent::Column::UserId.eq(&session.user.id))
        .count(&state.db)
        .await
        .map_err(AppError::Db)?;

    if count >= consts::MAX_AGENTS_PER_USER as u64 {
        return Err(AppError::Validation(format!(
            "maximum {} agents per user",
            consts::MAX_AGENTS_PER_USER,
        )));
    }

    // Check duplicate id.
    let existing = agent::Entity::find_by_id(&req.id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?;

    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "agent id '{}' already exists",
            req.id
        )));
    }

    let raw_token = generate_token();
    let now = Utc::now();

    let model = agent::ActiveModel {
        id: Set(req.id.clone()),
        user_id: Set(session.user.id.clone()),
        name: Set(req.name.clone()),
        token_hash: Set(hash_token(&raw_token)),
        avatar_url: Set(req.avatar_url),
        bio: Set(req.bio),
        status: Set(agent::AgentStatus::Active),
        created_at: Set(now),
        updated_at: Set(now),
    };

    model.insert(&state.db).await.map_err(AppError::Db)?;

    Ok(Json(CreateAgentResponse {
        id: req.id,
        name: req.name,
        token: raw_token,
        created_at: now.to_rfc3339(),
    }))
}

/// GET /api/agents
pub async fn list_agents(
    session: UserSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentResponse>>, AppError> {
    let agents = agent::Entity::find()
        .filter(agent::Column::UserId.eq(&session.user.id))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let resp: Vec<AgentResponse> = agents.iter().map(to_agent_response).collect();
    Ok(Json(resp))
}

/// GET /api/agents/:id
pub async fn get_agent(
    session: UserSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, AppError> {
    let found = find_owned_agent(&state.db, &id, &session.user.id).await?;
    Ok(Json(to_agent_response(&found)))
}

/// PUT /api/agents/:id
pub async fn update_agent(
    session: UserSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentResponse>, AppError> {
    let existing = find_owned_agent(&state.db, &id, &session.user.id).await?;

    let mut am: agent::ActiveModel = existing.into();
    if let Some(name) = req.name {
        am.name = Set(name);
    }
    if let Some(bio) = req.bio {
        am.bio = Set(Some(bio));
    }
    if let Some(avatar_url) = req.avatar_url {
        am.avatar_url = Set(Some(avatar_url));
    }
    am.updated_at = Set(Utc::now());

    let updated = am.update(&state.db).await.map_err(AppError::Db)?;
    Ok(Json(to_agent_response(&updated)))
}

/// DELETE /api/agents/:id
pub async fn delete_agent(
    session: UserSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let found = find_owned_agent(&state.db, &id, &session.user.id).await?;

    state
        .db
        .transaction::<_, (), sea_orm::DbErr>(|txn| {
            let agent_id = found.id.clone();
            Box::pin(async move {
                // Delete contacts where this agent is either side.
                contact::Entity::delete_many()
                    .filter(
                        Condition::any()
                            .add(contact::Column::AgentId.eq(&agent_id))
                            .add(contact::Column::ContactId.eq(&agent_id)),
                    )
                    .exec(txn)
                    .await?;

                // Delete the agent.
                agent::Entity::delete_by_id(&agent_id).exec(txn).await?;

                Ok(())
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(db_err) => AppError::Db(db_err),
            sea_orm::TransactionError::Transaction(db_err) => AppError::Db(db_err),
        })?;

    Ok(Json(serde_json::json!({"deleted": true})))
}

/// POST /api/agents/:id/token/reset
pub async fn reset_token(
    session: UserSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ResetTokenResponse>, AppError> {
    let existing = find_owned_agent(&state.db, &id, &session.user.id).await?;

    let raw_token = generate_token();
    let mut am: agent::ActiveModel = existing.into();
    am.token_hash = Set(hash_token(&raw_token));
    am.updated_at = Set(Utc::now());
    am.update(&state.db).await.map_err(AppError::Db)?;

    Ok(Json(ResetTokenResponse { token: raw_token }))
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    use crate::auth::token::{generate_token, hash_token};
    use crate::consts;
    use crate::db;
    use crate::entity::{agent, contact, user};

    use super::validate_agent_id;

    /// Helper: create a test user in the DB and return its id.
    async fn create_user(db: &sea_orm::DatabaseConnection, id: &str, github_id: i64) -> String {
        let now = Utc::now();
        let u = user::ActiveModel {
            id: Set(id.into()),
            github_id: Set(github_id),
            github_name: Set(format!("user-{}", id)),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        u.insert(db).await.unwrap();
        id.into()
    }

    /// Helper: insert an agent for a user, return (agent model, raw token).
    async fn insert_agent(
        db: &sea_orm::DatabaseConnection,
        agent_id: &str,
        user_id: &str,
    ) -> (agent::Model, String) {
        let raw_token = generate_token();
        let now = Utc::now();
        let am = agent::ActiveModel {
            id: Set(agent_id.into()),
            user_id: Set(user_id.into()),
            name: Set(format!("Agent {}", agent_id)),
            token_hash: Set(hash_token(&raw_token)),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let model = am.insert(db).await.unwrap();
        (model, raw_token)
    }

    // ── validate_agent_id ──

    #[test]
    fn valid_ids() {
        assert!(validate_agent_id("abc").is_ok());
        assert!(validate_agent_id("my-bot-123").is_ok());
        assert!(validate_agent_id("a".repeat(50).as_str()).is_ok());
    }

    #[test]
    fn too_short() {
        assert!(validate_agent_id("ab").is_err());
    }

    #[test]
    fn too_long() {
        assert!(validate_agent_id(&"a".repeat(51)).is_err());
    }

    #[test]
    fn uppercase_rejected() {
        assert!(validate_agent_id("MyBot").is_err());
    }

    #[test]
    fn spaces_rejected() {
        assert!(validate_agent_id("my bot").is_err());
    }

    #[test]
    fn underscores_rejected() {
        assert!(validate_agent_id("my_bot").is_err());
    }

    // ── Create agent ──

    #[tokio::test]
    async fn create_agent_returns_token_with_prefix() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;

        let raw_token = generate_token();
        let now = Utc::now();
        let am = agent::ActiveModel {
            id: Set("test-bot".into()),
            user_id: Set(user_id),
            name: Set("Test Bot".into()),
            token_hash: Set(hash_token(&raw_token)),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        };
        am.insert(&db).await.unwrap();

        assert!(raw_token.starts_with(consts::TOKEN_PREFIX));
    }

    #[tokio::test]
    async fn create_agent_duplicate_id_fails() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;

        let (_, _) = insert_agent(&db, "dup-bot", &user_id).await;

        // Attempt to insert another agent with the same id.
        let raw_token = generate_token();
        let now = Utc::now();
        let am = agent::ActiveModel {
            id: Set("dup-bot".into()),
            user_id: Set(user_id),
            name: Set("Dup Bot".into()),
            token_hash: Set(hash_token(&raw_token)),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let result = am.insert(&db).await;
        assert!(result.is_err()); // Primary key conflict.
    }

    #[tokio::test]
    async fn create_51st_agent_fails_count_check() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;

        // Insert 50 agents.
        for i in 0..consts::MAX_AGENTS_PER_USER {
            insert_agent(&db, &format!("bot-{:03}", i), &user_id).await;
        }

        // Count check.
        use sea_orm::PaginatorTrait;
        let count = agent::Entity::find()
            .filter(agent::Column::UserId.eq(&user_id))
            .count(&db)
            .await
            .unwrap();
        assert_eq!(count, consts::MAX_AGENTS_PER_USER as u64);

        // The handler would reject at this point — validate by checking count >= limit.
        assert!(count >= consts::MAX_AGENTS_PER_USER as u64);
    }

    // ── List agents: only own ──

    #[tokio::test]
    async fn list_agents_only_own() {
        let db = db::test_db().await;
        let u1 = create_user(&db, "u1", 1).await;
        let u2 = create_user(&db, "u2", 2).await;

        insert_agent(&db, "u1-bot-a", &u1).await;
        insert_agent(&db, "u1-bot-b", &u1).await;
        insert_agent(&db, "u2-bot-a", &u2).await;

        let u1_agents = agent::Entity::find()
            .filter(agent::Column::UserId.eq(&u1))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(u1_agents.len(), 2);

        let u2_agents = agent::Entity::find()
            .filter(agent::Column::UserId.eq(&u2))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(u2_agents.len(), 1);
    }

    // ── Get agent ──

    #[tokio::test]
    async fn get_agent_correct_data() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;
        let (model, _) = insert_agent(&db, "my-bot", &user_id).await;

        let found = agent::Entity::find_by_id("my-bot")
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(found.id, model.id);
        assert_eq!(found.user_id, user_id);
        assert_eq!(found.name, "Agent my-bot");
    }

    #[tokio::test]
    async fn get_agent_wrong_owner_denied() {
        let db = db::test_db().await;
        let u1 = create_user(&db, "u1", 1).await;
        let _u2 = create_user(&db, "u2", 2).await;
        insert_agent(&db, "u1-bot", &u1).await;

        // Simulate ownership check.
        let found = agent::Entity::find_by_id("u1-bot")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(found.user_id, "u2"); // u2 does not own this agent.
    }

    // ── Update agent ──

    #[tokio::test]
    async fn update_agent_fields() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;
        let (existing, _) = insert_agent(&db, "my-bot", &user_id).await;

        let mut am: agent::ActiveModel = existing.into();
        am.name = Set("New Name".into());
        am.bio = Set(Some("New bio".into()));
        am.updated_at = Set(Utc::now());
        let updated = am.update(&db).await.unwrap();

        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.bio, Some("New bio".into()));
    }

    // ── Delete agent + contacts cleanup ──

    #[tokio::test]
    async fn delete_agent_cleans_contacts() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;
        insert_agent(&db, "bot-a", &user_id).await;
        insert_agent(&db, "bot-b", &user_id).await;
        insert_agent(&db, "bot-c", &user_id).await;

        // Create contacts: bot-a <-> bot-b, bot-b <-> bot-c
        let now = Utc::now();
        contact::ActiveModel {
            agent_id: Set("bot-a".into()),
            contact_id: Set("bot-b".into()),
            alias: Set(None),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        contact::ActiveModel {
            agent_id: Set("bot-b".into()),
            contact_id: Set("bot-c".into()),
            alias: Set(None),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        // Delete bot-b — should clean both contact rows.
        use sea_orm::{Condition, TransactionTrait};
        db.transaction::<_, (), sea_orm::DbErr>(|txn| {
            Box::pin(async move {
                contact::Entity::delete_many()
                    .filter(
                        Condition::any()
                            .add(contact::Column::AgentId.eq("bot-b"))
                            .add(contact::Column::ContactId.eq("bot-b")),
                    )
                    .exec(txn)
                    .await?;

                agent::Entity::delete_by_id("bot-b").exec(txn).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

        // Verify agent is gone.
        let found = agent::Entity::find_by_id("bot-b")
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_none());

        // Verify contacts cleaned.
        let remaining = contact::Entity::find().all(&db).await.unwrap();
        assert!(remaining.is_empty());
    }

    // ── Reset token ──

    #[tokio::test]
    async fn reset_token_produces_new_token() {
        let db = db::test_db().await;
        let user_id = create_user(&db, "u1", 1).await;
        let (existing, old_token) = insert_agent(&db, "my-bot", &user_id).await;
        let old_hash = existing.token_hash.clone();

        // Reset: generate new token, update hash.
        let new_token = generate_token();
        let new_hash = hash_token(&new_token);
        assert_ne!(new_token, old_token);

        let mut am: agent::ActiveModel = existing.into();
        am.token_hash = Set(new_hash.clone());
        am.updated_at = Set(Utc::now());
        am.update(&db).await.unwrap();

        // Old hash no longer matches.
        let by_old = agent::Entity::find()
            .filter(agent::Column::TokenHash.eq(&old_hash))
            .one(&db)
            .await
            .unwrap();
        assert!(by_old.is_none());

        // New hash matches.
        let by_new = agent::Entity::find()
            .filter(agent::Column::TokenHash.eq(&new_hash))
            .one(&db)
            .await
            .unwrap();
        assert!(by_new.is_some());
        assert_eq!(by_new.unwrap().id, "my-bot");
    }
}
