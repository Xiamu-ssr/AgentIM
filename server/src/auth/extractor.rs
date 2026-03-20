use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::entity::agent;
use crate::error::AppError;
use crate::AppState;

use super::token::hash_token;

/// Axum extractor for agent authentication via Bearer token.
///
/// Reads `Authorization: Bearer <token>`, hashes the token,
/// looks up the agent by token_hash, rejects if not found or suspended.
#[allow(dead_code)]
pub struct AgentAuth {
    pub agent: agent::Model,
}

impl FromRequestParts<AppState> for AgentAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing Authorization header".into()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("invalid Authorization format".into()))?;

        let token_hash = hash_token(token);

        let found = agent::Entity::find()
            .filter(
                agent::Column::TokenHash.eq(&token_hash),
            )
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Unauthorized("invalid token".into()))?;

        if found.status == agent::AgentStatus::Suspended {
            return Err(AppError::Forbidden("agent is suspended".into()));
        }

        Ok(AgentAuth { agent: found })
    }
}

/// Axum extractor for human user authentication via session.
///
/// Reads the session cookie, extracts user_id, loads the user.
/// Used for Web frontend endpoints (agent management, OAuth).
pub struct UserSession {
    pub user: crate::entity::user::Model,
}

impl FromRequestParts<AppState> for UserSession {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Get session from tower-sessions.
        let session = parts
            .extensions
            .get::<tower_sessions::Session>()
            .ok_or_else(|| AppError::Unauthorized("no session".into()))?
            .clone();

        let user_id: String = session
            .get("user_id")
            .await
            .map_err(|_| AppError::Internal("session read error".into()))?
            .ok_or_else(|| AppError::Unauthorized("not logged in".into()))?;

        let user = crate::entity::user::Entity::find_by_id(&user_id)
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Unauthorized("user not found".into()))?;

        Ok(UserSession { user })
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, Set};

    use crate::auth::token::generate_token;
    use crate::db;
    use crate::entity::{agent, user};

    use super::hash_token;

    /// Helper: create a test user + agent, return (agent model, raw token).
    async fn setup_agent(
        db: &sea_orm::DatabaseConnection,
    ) -> (agent::Model, String) {
        let u = user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        };
        u.insert(db).await.unwrap();

        let raw_token = generate_token();
        let a = agent::ActiveModel {
            id: Set("test-agent".into()),
            user_id: Set("u1".into()),
            name: Set("Test Agent".into()),
            token_hash: Set(hash_token(&raw_token)),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(Utc::now()),
            updated_at: Set(Utc::now()),
        };
        let agent = a.insert(db).await.unwrap();
        (agent, raw_token)
    }

    #[tokio::test]
    async fn agent_auth_finds_by_token_hash() {
        let db = db::test_db().await;
        let (_agent, raw_token) = setup_agent(&db).await;

        // Verify the agent can be found by hashing the token.
        let hash = hash_token(&raw_token);
        let found = agent::Entity::find()
            .filter(
                <agent::Column as sea_orm::ColumnTrait>::eq(
                    &agent::Column::TokenHash,
                    &hash,
                ),
            )
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "test-agent");
    }

    #[tokio::test]
    async fn wrong_token_finds_nothing() {
        let db = db::test_db().await;
        let _ = setup_agent(&db).await;

        let hash = hash_token("aim_wrong_token");
        let found = agent::Entity::find()
            .filter(
                <agent::Column as sea_orm::ColumnTrait>::eq(
                    &agent::Column::TokenHash,
                    &hash,
                ),
            )
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_none());
    }
}
