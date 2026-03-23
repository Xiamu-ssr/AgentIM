//! WebSocket 实时推送模块
//!
//! 提供 WebSocket 连接管理和消息推送能力：
//! - agent 通过 `ws://<host>/ws?token=<bearer_token>` 建立连接
//! - 服务端在消息发送时推送通知给在线接收者
//! - 支持同一 agent 多连接（多设备/多窗口）

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use sea_orm::EntityTrait;
use serde::Deserialize;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use crate::entity::agent;
use crate::error::AppError;
use crate::AppState;

/// Per-connection sender handle.
pub type WsSender = mpsc::UnboundedSender<String>;

/// Registry mapping agent_id → list of active WebSocket senders.
/// Wrapped in Arc<RwLock<...>> for concurrent access.
#[derive(Clone, Default)]
pub struct ConnectionRegistry {
    inner: Arc<RwLock<HashMap<String, Vec<WsSender>>>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new connection for an agent. Returns the receiver end.
    pub async fn register(&self, agent_id: &str) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut map = self.inner.write().await;
        map.entry(agent_id.to_string()).or_default().push(tx);
        rx
    }

    /// Remove closed senders for an agent. Called on disconnect.
    pub async fn cleanup(&self, agent_id: &str) {
        let mut map = self.inner.write().await;
        if let Some(senders) = map.get_mut(agent_id) {
            senders.retain(|tx| !tx.is_closed());
            if senders.is_empty() {
                map.remove(agent_id);
            }
        }
    }

    /// Push a JSON message to all connections of a given agent.
    /// Returns the number of connections that received the message.
    pub async fn push(&self, agent_id: &str, json_msg: &str) -> usize {
        let map = self.inner.read().await;
        if let Some(senders) = map.get(agent_id) {
            let mut delivered = 0;
            for tx in senders {
                if tx.send(json_msg.to_string()).is_ok() {
                    delivered += 1;
                }
            }
            delivered
        } else {
            0
        }
    }

    /// Push a message to all members of a channel (except the sender).
    pub async fn push_to_channel_members(
        &self,
        member_ids: &[String],
        exclude: &str,
        json_msg: &str,
    ) -> usize {
        let map = self.inner.read().await;
        let mut total = 0;
        for member_id in member_ids {
            if member_id == exclude {
                continue;
            }
            if let Some(senders) = map.get(member_id) {
                for tx in senders {
                    if tx.send(json_msg.to_string()).is_ok() {
                        total += 1;
                    }
                }
            }
        }
        total
    }

    /// Get the number of online agents (for testing/monitoring).
    #[allow(dead_code)]
    pub async fn online_count(&self) -> usize {
        let map = self.inner.read().await;
        map.len()
    }
}

// ── WebSocket handler ──

#[derive(Deserialize)]
pub struct WsParams {
    pub token: String,
}

/// GET /ws?token=<bearer_token> — WebSocket upgrade endpoint.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
) -> Result<impl IntoResponse, AppError> {
    // Transitional: authenticate by agent ID. Step 2 replaces with JWT.
    let found = agent::Entity::find_by_id(&params.token)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::Unauthorized("invalid token".into()))?;

    if found.status == agent::AgentStatus::Suspended {
        return Err(AppError::Forbidden("agent is suspended".into()));
    }

    let agent_id = found.id.clone();
    let registry = state.connections.clone();

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, agent_id, registry)))
}

async fn handle_ws(socket: WebSocket, agent_id: String, registry: ConnectionRegistry) {
    info!("WebSocket connected: {}", agent_id);

    let mut rx = registry.register(&agent_id).await;
    let (mut ws_sender, mut ws_receiver) = socket.split();

    use futures_util::SinkExt;
    use futures_util::StreamExt;

    // Spawn a task to forward messages from registry → WebSocket.
    let send_agent_id = agent_id.clone();
    let send_registry = registry.clone();
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
        send_registry.cleanup(&send_agent_id).await;
    });

    // Read loop: we don't expect client messages, but we need to detect disconnect.
    let recv_agent_id = agent_id.clone();
    let recv_registry = registry.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(_) => {} // axum auto-responds with pong
                _ => {}                // Ignore other messages
            }
        }
        recv_registry.cleanup(&recv_agent_id).await;
    });

    // Wait for either task to finish.
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    registry.cleanup(&agent_id).await;
    info!("WebSocket disconnected: {}", agent_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_register_and_push() {
        let registry = ConnectionRegistry::new();
        let mut rx = registry.register("agent-1").await;

        let delivered = registry.push("agent-1", r#"{"type":"test"}"#).await;
        assert_eq!(delivered, 1);

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg, r#"{"type":"test"}"#);
    }

    #[tokio::test]
    async fn push_to_unregistered_agent_delivers_nothing() {
        let registry = ConnectionRegistry::new();
        let delivered = registry.push("nonexistent", "hello").await;
        assert_eq!(delivered, 0);
    }

    #[tokio::test]
    async fn multiple_connections_all_receive() {
        let registry = ConnectionRegistry::new();
        let mut rx1 = registry.register("agent-1").await;
        let mut rx2 = registry.register("agent-1").await;

        let delivered = registry.push("agent-1", "broadcast").await;
        assert_eq!(delivered, 2);

        assert_eq!(rx1.try_recv().unwrap(), "broadcast");
        assert_eq!(rx2.try_recv().unwrap(), "broadcast");
    }

    #[tokio::test]
    async fn cleanup_removes_closed_senders() {
        let registry = ConnectionRegistry::new();
        let rx = registry.register("agent-1").await;
        drop(rx); // Close the receiver → sender is closed.

        registry.cleanup("agent-1").await;
        assert_eq!(registry.online_count().await, 0);
    }

    #[tokio::test]
    async fn push_to_channel_members_excludes_sender() {
        let registry = ConnectionRegistry::new();
        let mut rx_a = registry.register("alice").await;
        let mut rx_b = registry.register("bob").await;

        let members = vec!["alice".to_string(), "bob".to_string()];
        let delivered = registry
            .push_to_channel_members(&members, "alice", "msg")
            .await;

        assert_eq!(delivered, 1); // Only bob receives.
        assert!(rx_a.try_recv().is_err()); // Alice excluded.
        assert_eq!(rx_b.try_recv().unwrap(), "msg");
    }
}
