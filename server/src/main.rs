mod api;
mod auth;
mod config;
mod consts;
mod db;
mod entity;
mod error;
mod frontend;
mod raw_sql;
mod risk;
pub mod ws;

use std::collections::HashMap;
use std::sync::Arc;

use axum::{http::header, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use clap::Parser;
use config::AppConfig;
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_sessions::cookie::SameSite;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use api::auth::{GitHubClient, RealGitHubClient};

/// A pending challenge nonce entry.
#[derive(Clone, Debug)]
pub struct ChallengeEntry {
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

/// In-memory store for challenge nonces, keyed by "{agent_id}:{credential_id}".
pub type ChallengeStore = Arc<RwLock<HashMap<String, ChallengeEntry>>>;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: AppConfig,
    pub github_client: Arc<dyn GitHubClient>,
    pub connections: ws::ConnectionRegistry,
    /// HMAC secret for JWT signing/verification. Generated at startup if not configured.
    pub jwt_secret: String,
    /// In-memory challenge nonce store for auth challenge/verify flow.
    pub challenges: ChallengeStore,
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

fn cors_layer() -> CorsLayer {
    let allowed_origins: Vec<_> = consts::CORS_DEV_ORIGINS
        .iter()
        .map(|o| o.parse().unwrap())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_credentials(true)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agentim_server=info".parse().unwrap()),
        )
        .init();

    let config = AppConfig::parse();
    let db = db::init_db(&config).await?;

    let github_client: Arc<dyn GitHubClient> = Arc::new(RealGitHubClient {
        client_id: config.github_client_id.clone(),
        client_secret: config.github_client_secret.clone(),
    });

    let connections = ws::ConnectionRegistry::new();

    // JWT secret: from env or generate a random one at startup.
    let jwt_secret = std::env::var("AGENTIM_JWT_SECRET").unwrap_or_else(|_| {
        use rand::Rng;
        let mut rng = rand::rng();
        let bytes: Vec<u8> = (0..consts::JWT_SECRET_BYTES)
            .map(|_| rng.random::<u8>())
            .collect();
        hex::encode(bytes)
    });

    let challenges: ChallengeStore = Arc::new(RwLock::new(HashMap::new()));

    let state = AppState {
        db,
        config: config.clone(),
        github_client,
        connections,
        jwt_secret,
        challenges,
    };

    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
        .with_name(consts::SESSION_COOKIE_NAME)
        .with_same_site(SameSite::Lax)
        .with_secure(config.session_cookie_secure);

    let global_governor = GovernorConfigBuilder::default()
        .key_extractor(SmartIpKeyExtractor)
        .per_second(consts::RATE_LIMIT_PER_SECOND)
        .burst_size(consts::RATE_LIMIT_BURST_SIZE)
        .finish()
        .unwrap();

    let governor_layer = GovernorLayer::new(Arc::new(global_governor));

    // Rate-limit only API + WS routes; static assets are unlimited.
    let api_routes = Router::new()
        .route("/api/health", get(health))
        .route("/ws", get(ws::ws_handler))
        .merge(api::api_router())
        .layer(governor_layer);

    let app = Router::new()
        .merge(api_routes)
        .with_state(state)
        .fallback(frontend::static_handler)
        .layer(session_layer)
        .layer(cors_layer());

    let addr = format!("0.0.0.0:{}", config.port);
    info!("AgentIM server listening on {}", addr);
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    info!("Shutdown signal received");
}
