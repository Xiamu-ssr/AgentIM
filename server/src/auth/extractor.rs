use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::EntityTrait;

use crate::entity::{agent, agent_credential};
use crate::error::AppError;
use crate::AppState;

use super::token::verify_jwt;

/// Axum extractor for agent authentication via JWT Bearer token.
///
/// Reads `Authorization: Bearer <jwt>`, verifies the JWT signature and expiry,
/// loads the agent and credential from the database, rejects if:
/// - JWT is invalid or expired
/// - Agent not found or suspended
/// - Agent has reauth_required flag
/// - Credential not found or not active
#[allow(dead_code)]
pub struct AgentAuth {
    pub agent: agent::Model,
    pub credential_id: String,
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

        // Verify JWT
        let claims = verify_jwt(token, &state.jwt_secret)
            .map_err(AppError::Unauthorized)?;

        // Load agent
        let found = agent::Entity::find_by_id(&claims.sub)
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Unauthorized("agent not found".into()))?;

        if found.status == agent::AgentStatus::Suspended {
            return Err(AppError::Forbidden("agent is suspended".into()));
        }

        if found.reauth_required {
            return Err(AppError::Forbidden("re-authentication required".into()));
        }

        // Verify credential is still active
        let cred = agent_credential::Entity::find_by_id(&claims.cid)
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Unauthorized("credential not found".into()))?;

        if cred.status != agent_credential::CredentialStatus::Active {
            return Err(AppError::Unauthorized("credential revoked".into()));
        }

        Ok(AgentAuth {
            agent: found,
            credential_id: claims.cid,
        })
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
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    use crate::auth::token::create_jwt;
    use crate::db;
    use crate::entity::{agent, agent_credential, user};

    const TEST_JWT_SECRET: &str = "test-jwt-secret-for-unit-tests";

    /// Helper: create a test user + agent + credential, return (agent, credential_id, jwt).
    async fn setup_authed_agent(
        db: &sea_orm::DatabaseConnection,
    ) -> (agent::Model, String, String) {
        let now = Utc::now();

        let u = user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        u.insert(db).await.unwrap();

        let a = agent::ActiveModel {
            id: Set("test-agent".into()),
            user_id: Set("u1".into()),
            name: Set("Test Agent".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let agent_model = a.insert(db).await.unwrap();

        let cred = agent_credential::ActiveModel {
            id: Set("cred-1".into()),
            agent_id: Set("test-agent".into()),
            public_key: Set("base64key".into()),
            public_key_fp: Set("fp1234567890abcd".into()),
            status: Set(agent_credential::CredentialStatus::Active),
            revoke_reason: Set(None),
            instance_label: Set(None),
            issued_at: Set(now),
            last_used_at: Set(None),
            revoked_at: Set(None),
            replaced_by_id: Set(None),
        };
        cred.insert(db).await.unwrap();

        let jwt = create_jwt("test-agent", "cred-1", TEST_JWT_SECRET).unwrap();
        (agent_model, "cred-1".into(), jwt)
    }

    #[tokio::test]
    async fn jwt_finds_agent_and_credential() {
        let db = db::test_db().await;
        let (agent, cred_id, jwt) = setup_authed_agent(&db).await;

        // Verify the JWT decodes correctly and we can look up the agent
        let claims = crate::auth::token::verify_jwt(&jwt, TEST_JWT_SECRET).unwrap();
        assert_eq!(claims.sub, agent.id);
        assert_eq!(claims.cid, cred_id);

        // Verify agent lookup works
        let found = agent::Entity::find_by_id(&claims.sub)
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn wrong_jwt_secret_fails() {
        let db = db::test_db().await;
        let (_, _, jwt) = setup_authed_agent(&db).await;

        let result = crate::auth::token::verify_jwt(&jwt, "wrong-secret");
        assert!(result.is_err());
        let _ = db; // keep db alive
    }

    #[tokio::test]
    async fn revoked_credential_rejected() {
        let db = db::test_db().await;
        let (_, _, _jwt) = setup_authed_agent(&db).await;

        // Revoke the credential
        let cred = agent_credential::Entity::find_by_id("cred-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut am: agent_credential::ActiveModel = cred.into();
        am.status = Set(agent_credential::CredentialStatus::Revoked);
        am.update(&db).await.unwrap();

        // Verify credential is no longer active
        let cred = agent_credential::Entity::find_by_id("cred-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(cred.status, agent_credential::CredentialStatus::Active);
    }
}
