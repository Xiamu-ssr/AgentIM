use axum::extract::{Query, State};
use axum::response::Redirect;
use axum::Json;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;

use crate::auth::extractor::UserSession;
use crate::entity::user;
use crate::error::AppError;
use crate::AppState;

use super::dto::MeResponse;

// ── GitHubClient trait ──

#[async_trait::async_trait]
pub trait GitHubClient: Send + Sync {
    /// Exchange an OAuth authorization code for an access token.
    async fn exchange_code(&self, code: &str) -> Result<String, AppError>;

    /// Fetch user info from GitHub using an access token.
    async fn get_user_info(&self, access_token: &str) -> Result<GitHubUser, AppError>;
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub avatar_url: Option<String>,
}

// ── RealGitHubClient ──

pub struct RealGitHubClient {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
}

#[async_trait::async_trait]
impl GitHubClient for RealGitHubClient {
    async fn exchange_code(&self, code: &str) -> Result<String, AppError> {
        let client = reqwest::Client::new();
        let resp = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": self.client_id,
                "client_secret": self.client_secret,
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("GitHub token exchange failed: {e}")))?;

        let token_resp: GitHubTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("GitHub token parse failed: {e}")))?;

        Ok(token_resp.access_token)
    }

    async fn get_user_info(&self, access_token: &str) -> Result<GitHubUser, AppError> {
        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "AgentIM")
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("GitHub user fetch failed: {e}")))?;

        let gh_user: GitHubUser = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("GitHub user parse failed: {e}")))?;

        Ok(gh_user)
    }
}

// ── Query params ──

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

// ── Handlers ──

/// GET /api/auth/github -- Initiate GitHub OAuth flow.
pub async fn github_auth(State(state): State<AppState>) -> Result<Redirect, AppError> {
    if state.config.github_client_id.is_empty() {
        return Err(AppError::Internal("GitHub OAuth not configured".into()));
    }

    let csrf_state = uuid::Uuid::new_v4().to_string();

    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&state={}&scope=read:user",
        state.config.github_client_id, csrf_state,
    );

    Ok(Redirect::temporary(&url))
}

/// GET /api/auth/github/callback -- Handle OAuth callback from GitHub.
pub async fn github_callback(
    session: tower_sessions::Session,
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Result<Redirect, AppError> {
    if state.config.github_client_id.is_empty() {
        return Err(AppError::Internal("GitHub OAuth not configured".into()));
    }

    if params.state.is_empty() {
        return Err(AppError::Unauthorized("invalid OAuth state".into()));
    }

    // Exchange code for access token.
    let access_token = state.github_client.exchange_code(&params.code).await?;

    // Fetch user info from GitHub.
    let gh_user = state.github_client.get_user_info(&access_token).await?;

    // Upsert user: match on github_id, update name + avatar on re-login.
    let now = Utc::now();
    let existing = user::Entity::find()
        .filter(user::Column::GithubId.eq(gh_user.id))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?;

    let user_id = if let Some(existing) = existing {
        let uid = existing.id.clone();
        let mut am: user::ActiveModel = existing.into();
        am.github_name = Set(gh_user.login);
        am.avatar_url = Set(gh_user.avatar_url);
        am.updated_at = Set(now);
        am.update(&state.db).await.map_err(AppError::Db)?;
        uid
    } else {
        let uid = uuid::Uuid::new_v4().to_string();
        let am = user::ActiveModel {
            id: Set(uid.clone()),
            github_id: Set(gh_user.id),
            github_name: Set(gh_user.login),
            avatar_url: Set(gh_user.avatar_url),
            created_at: Set(now),
            updated_at: Set(now),
        };
        am.insert(&state.db).await.map_err(AppError::Db)?;
        uid
    };

    // Store user_id in session.
    session
        .insert("user_id", &user_id)
        .await
        .map_err(|e| AppError::Internal(format!("session write error: {e}")))?;

    Ok(Redirect::temporary(state.config.auth_redirect_url()))
}

/// GET /api/auth/me -- Get current authenticated user info.
pub async fn me(session: UserSession) -> Result<Json<MeResponse>, AppError> {
    Ok(Json(MeResponse {
        id: session.user.id,
        github_name: session.user.github_name,
        avatar_url: session.user.avatar_url,
        created_at: session.user.created_at.to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};
    use tower::ServiceExt;

    use crate::api;
    use crate::config::AppConfig;
    use crate::consts;
    use crate::db;
    use crate::entity::user;
    use crate::AppState;

    use super::*;

    // ── MockGitHubClient ──

    struct MockGitHubClient {
        user: GitHubUser,
    }

    #[async_trait::async_trait]
    impl GitHubClient for MockGitHubClient {
        async fn exchange_code(&self, _code: &str) -> Result<String, AppError> {
            Ok("mock_access_token".into())
        }

        async fn get_user_info(&self, _access_token: &str) -> Result<GitHubUser, AppError> {
            Ok(GitHubUser {
                id: self.user.id,
                login: self.user.login.clone(),
                avatar_url: self.user.avatar_url.clone(),
            })
        }
    }

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

    fn build_app(state: AppState) -> Router {
        let session_store = tower_sessions::MemoryStore::default();
        let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
            .with_name(consts::SESSION_COOKIE_NAME);

        Router::new()
            .merge(api::api_router())
            .with_state(state)
            .layer(session_layer)
    }

    // ── Tests ──

    #[tokio::test]
    async fn github_auth_returns_redirect_to_github() {
        let db = db::test_db().await;
        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 123,
                login: "testuser".into(),
                avatar_url: Some("https://example.com/avatar.png".into()),
            },
        };
        let state = AppState {
            db,
            config: test_config(),
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        let app = build_app(state);

        let req = Request::builder()
            .uri("/api/auth/github")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        let location = resp
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.starts_with("https://github.com/login/oauth/authorize"));
        assert!(location.contains("client_id=test_client_id"));
    }

    #[tokio::test]
    async fn github_auth_fails_when_not_configured() {
        let db = db::test_db().await;
        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 123,
                login: "testuser".into(),
                avatar_url: None,
            },
        };
        let mut config = test_config();
        config.github_client_id = String::new();
        let state = AppState {
            db,
            config,
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        let app = build_app(state);

        let req = Request::builder()
            .uri("/api/auth/github")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn me_returns_401_without_session() {
        let db = db::test_db().await;
        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 123,
                login: "testuser".into(),
                avatar_url: None,
            },
        };
        let state = AppState {
            db,
            config: test_config(),
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        let app = build_app(state);

        let req = Request::builder()
            .uri("/api/auth/me")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn me_returns_user_with_valid_session() {
        let db = db::test_db().await;
        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 456,
                login: "sessionuser".into(),
                avatar_url: Some("https://example.com/avatar.png".into()),
            },
        };
        let state = AppState {
            db: db.clone(),
            config: test_config(),
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        // Create a user in the DB.
        let now = Utc::now();
        let u = user::ActiveModel {
            id: Set("user-abc".into()),
            github_id: Set(456),
            github_name: Set("sessionuser".into()),
            avatar_url: Set(Some("https://example.com/avatar.png".into())),
            created_at: Set(now),
            updated_at: Set(now),
        };
        u.insert(&db).await.unwrap();

        // Build app with shared session store.
        let session_store = tower_sessions::MemoryStore::default();
        let session_layer = tower_sessions::SessionManagerLayer::new(session_store.clone())
            .with_name(consts::SESSION_COOKIE_NAME);

        let app = Router::new()
            .merge(api::api_router())
            .with_state(state)
            .layer(session_layer);

        // Step 1: Trigger callback to create a session.
        let req = Request::builder()
            .uri("/api/auth/github/callback?code=testcode&state=teststate")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        // Extract the session cookie.
        let cookie = resp
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        let cookie_value = cookie.split(';').next().unwrap();

        // Step 2: Use the session cookie to call /api/auth/me.
        let req = Request::builder()
            .uri("/api/auth/me")
            .header("cookie", cookie_value)
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let me_resp: MeResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(me_resp.github_name, "sessionuser");
        assert_eq!(
            me_resp.avatar_url,
            Some("https://example.com/avatar.png".into())
        );
    }

    #[tokio::test]
    async fn callback_redirects_to_configured_web_base_url() {
        let db = db::test_db().await;
        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 990,
                login: "redirect-user".into(),
                avatar_url: None,
            },
        };
        let mut config = test_config();
        config.web_base_url = Some("http://127.0.0.1:3000".into());

        let state = AppState {
            db,
            config,
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        let app = build_app(state);

        let req = Request::builder()
            .uri("/api/auth/github/callback?code=testcode&state=teststate")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            resp.headers().get("location").unwrap(),
            "http://127.0.0.1:3000"
        );
    }

    #[tokio::test]
    async fn callback_creates_new_user_on_first_login() {
        let db = db::test_db().await;
        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 789,
                login: "newuser".into(),
                avatar_url: Some("https://example.com/new.png".into()),
            },
        };
        let state = AppState {
            db: db.clone(),
            config: test_config(),
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        let app = build_app(state);

        let req = Request::builder()
            .uri("/api/auth/github/callback?code=testcode&state=teststate")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        // Verify user was created in DB.
        let found = user::Entity::find()
            .filter(user::Column::GithubId.eq(789i64))
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.github_name, "newuser");
        assert_eq!(found.avatar_url, Some("https://example.com/new.png".into()));
    }

    #[tokio::test]
    async fn callback_updates_existing_user_on_re_login() {
        let db = db::test_db().await;

        // Create existing user with old name.
        let now = Utc::now();
        let u = user::ActiveModel {
            id: Set("existing-user".into()),
            github_id: Set(999),
            github_name: Set("oldname".into()),
            avatar_url: Set(Some("https://example.com/old.png".into())),
            created_at: Set(now),
            updated_at: Set(now),
        };
        u.insert(&db).await.unwrap();

        let mock_client = MockGitHubClient {
            user: GitHubUser {
                id: 999,
                login: "newname".into(),
                avatar_url: Some("https://example.com/new.png".into()),
            },
        };
        let state = AppState {
            db: db.clone(),
            config: test_config(),
            github_client: Arc::new(mock_client),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: "test-jwt-secret".into(),
        };

        let app = build_app(state);

        let req = Request::builder()
            .uri("/api/auth/github/callback?code=testcode&state=teststate")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        // Verify user was updated.
        let found = user::Entity::find_by_id("existing-user")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.github_name, "newname");
        assert_eq!(
            found.avatar_url,
            Some("https://example.com/new.png".into())
        );
    }
}
