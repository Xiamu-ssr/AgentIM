mod api;
mod auth;
mod config;
mod consts;
mod db;
mod entity;
mod error;
mod frontend;
pub mod ws;

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use clap::Parser;
use config::AppConfig;
use sea_orm::DatabaseConnection;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tracing::info;

use api::auth::{GitHubClient, RealGitHubClient};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: AppConfig,
    pub github_client: Arc<dyn GitHubClient>,
    pub connections: ws::ConnectionRegistry,
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
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

    let state = AppState {
        db,
        config: config.clone(),
        github_client,
        connections,
    };

    let session_store = tower_sessions::MemoryStore::default();
    let session_layer = tower_sessions::SessionManagerLayer::new(session_store)
        .with_name(consts::SESSION_COOKIE_NAME);

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/ws", get(ws::ws_handler))
        .merge(api::api_router())
        .with_state(state)
        .fallback(frontend::static_handler)
        .layer(session_layer);

    let addr = format!("0.0.0.0:{}", config.port);
    info!("AgentIM server listening on {}", addr);
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
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
