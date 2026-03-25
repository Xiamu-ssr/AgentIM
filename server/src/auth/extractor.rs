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

/// Axum extractor that accepts EITHER JWT Bearer OR session cookie + X-Agent-Id.
///
/// This is the primary extractor for agent-level endpoints (contacts, messages, channels).
/// It allows both CLI (JWT) and Web frontend (session proxy) access.
///
/// Priority:
/// 1. `Authorization: Bearer <jwt>` → JWT verification (same as AgentAuth)
/// 2. Session cookie + `X-Agent-Id` header → verify user owns the agent
/// 3. Neither → 401
pub struct AgentAccess {
    pub agent: agent::Model,
    #[allow(dead_code)]
    pub credential_id: Option<String>,
}

impl FromRequestParts<AppState> for AgentAccess {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Path 1: Try JWT Bearer token.
        if let Some(header) = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
        {
            let token = header
                .strip_prefix("Bearer ")
                .ok_or_else(|| AppError::Unauthorized("invalid Authorization format".into()))?;

            let claims = verify_jwt(token, &state.jwt_secret)
                .map_err(AppError::Unauthorized)?;

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

            let cred = agent_credential::Entity::find_by_id(&claims.cid)
                .one(&state.db)
                .await
                .map_err(AppError::Db)?
                .ok_or_else(|| AppError::Unauthorized("credential not found".into()))?;

            if cred.status != agent_credential::CredentialStatus::Active {
                return Err(AppError::Unauthorized("credential revoked".into()));
            }

            return Ok(AgentAccess {
                agent: found,
                credential_id: Some(claims.cid),
            });
        }

        // Path 2: Try session cookie + X-Agent-Id header.
        let agent_id = parts
            .headers
            .get(crate::consts::HEADER_AGENT_ID)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("unauthorized".into()))?
            .to_string();

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

        let found = agent::Entity::find_by_id(&agent_id)
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", agent_id)))?;

        if found.user_id != user_id {
            return Err(AppError::Forbidden("not your agent".into()));
        }

        if found.status == agent::AgentStatus::Suspended {
            return Err(AppError::Forbidden("agent is suspended".into()));
        }

        Ok(AgentAccess {
            agent: found,
            credential_id: None,
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
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Json;
    use axum::Router;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use tower::ServiceExt;

    use crate::auth::token::create_jwt;
    use crate::config::AppConfig;
    use crate::consts;
    use crate::db;
    use crate::entity::{agent, agent_credential, user};
    use crate::AppState;

    use super::AgentAccess;

    const TEST_JWT_SECRET: &str = "test-jwt-secret-for-unit-tests";

    fn test_config() -> AppConfig {
        AppConfig {
            data_dir: None,
            port: 8900,
            github_client_id: "test_client_id".into(),
            github_client_secret: "test_client_secret".into(),
            web_base_url: None,
            session_cookie_secure: false,
        }
    }

    /// A simple handler that uses AgentAccess and returns the agent ID.
    async fn agent_access_handler(auth: AgentAccess) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "agent_id": auth.agent.id,
            "has_credential": auth.credential_id.is_some(),
        }))
    }

    fn build_test_app(state: AppState) -> Router {
        let session_store = tower_sessions::MemoryStore::default();
        let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
            .with_name(consts::SESSION_COOKIE_NAME);

        Router::new()
            .route("/test", get(agent_access_handler))
            .with_state(state)
            .layer(session_layer)
    }

    fn build_test_app_with_session(
        state: AppState,
    ) -> (Router, tower_sessions::MemoryStore) {
        let session_store = tower_sessions::MemoryStore::default();
        let session_layer = tower_sessions::SessionManagerLayer::new(session_store.clone())
            .with_name(consts::SESSION_COOKIE_NAME);

        let app = Router::new()
            .route("/test", get(agent_access_handler))
            .route(
                "/api/auth/github/callback",
                get(crate::api::auth::github_callback),
            )
            .with_state(state)
            .layer(session_layer);

        (app, session_store)
    }

    fn test_state(db: sea_orm::DatabaseConnection) -> AppState {
        use crate::api::auth::{GitHubClient, GitHubUser};
        use crate::error::AppError;

        struct MockGitHub;
        #[async_trait::async_trait]
        impl GitHubClient for MockGitHub {
            async fn exchange_code(&self, _: &str) -> Result<String, AppError> {
                Ok("mock".into())
            }
            async fn get_user_info(&self, _: &str) -> Result<GitHubUser, AppError> {
                Ok(GitHubUser {
                    id: 1,
                    login: "testuser".into(),
                    avatar_url: None,
                })
            }
        }

        AppState {
            db,
            config: test_config(),
            github_client: Arc::new(MockGitHub),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: TEST_JWT_SECRET.into(),
            challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

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

    // ── AgentAccess tests ──

    #[tokio::test]
    async fn agent_access_via_jwt() {
        let db = db::test_db().await;
        let (_, _, jwt) = setup_authed_agent(&db).await;
        let state = test_state(db);
        let app = build_test_app(state);

        let req = Request::builder()
            .uri("/test")
            .header("Authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["agent_id"], "test-agent");
        assert_eq!(json["has_credential"], true);
    }

    #[tokio::test]
    async fn agent_access_via_session_proxy() {
        let db = db::test_db().await;
        // Create user + agent (no credential needed for session proxy).
        let now = Utc::now();
        user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        agent::ActiveModel {
            id: Set("web-agent".into()),
            user_id: Set("u1".into()),
            name: Set("Web Agent".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let state = test_state(db);
        let (app, _store) = build_test_app_with_session(state);

        // Step 1: Log in via mock GitHub OAuth to get a session cookie.
        let req = Request::builder()
            .uri("/api/auth/github/callback?code=test&state=test")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        let cookie = resp
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        let cookie_value = cookie.split(';').next().unwrap();

        // Step 2: Use session + X-Agent-Id to access agent data.
        let req = Request::builder()
            .uri("/test")
            .header("cookie", cookie_value)
            .header("X-Agent-Id", "web-agent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["agent_id"], "web-agent");
        assert_eq!(json["has_credential"], false);
    }

    #[tokio::test]
    async fn agent_access_wrong_owner_rejected() {
        let db = db::test_db().await;
        let now = Utc::now();

        // User u1 (will be logged in).
        user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        // User u2 owns this agent.
        user::ActiveModel {
            id: Set("u2".into()),
            github_id: Set(2),
            github_name: Set("otheruser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        agent::ActiveModel {
            id: Set("other-agent".into()),
            user_id: Set("u2".into()),
            name: Set("Other Agent".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let state = test_state(db);
        let (app, _store) = build_test_app_with_session(state);

        // Log in as u1.
        let req = Request::builder()
            .uri("/api/auth/github/callback?code=test&state=test")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let cookie = resp
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        let cookie_value = cookie.split(';').next().unwrap();

        // Try to access u2's agent → 403.
        let req = Request::builder()
            .uri("/test")
            .header("cookie", cookie_value)
            .header("X-Agent-Id", "other-agent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn agent_access_no_auth_rejected() {
        let db = db::test_db().await;
        let state = test_state(db);
        let app = build_test_app(state);

        // No JWT, no session, no X-Agent-Id → 401.
        let req = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn agent_access_suspended_rejected() {
        let db = db::test_db().await;
        let now = Utc::now();

        user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        agent::ActiveModel {
            id: Set("sus-agent".into()),
            user_id: Set("u1".into()),
            name: Set("Suspended Agent".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Suspended),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let state = test_state(db);
        let (app, _store) = build_test_app_with_session(state);

        // Log in as u1.
        let req = Request::builder()
            .uri("/api/auth/github/callback?code=test&state=test")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let cookie = resp
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        let cookie_value = cookie.split(';').next().unwrap();

        // Try to access suspended agent → 403.
        let req = Request::builder()
            .uri("/test")
            .header("cookie", cookie_value)
            .header("X-Agent-Id", "sus-agent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
