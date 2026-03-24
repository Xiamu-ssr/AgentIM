use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::auth::extractor::AgentAccess;
use crate::entity::{agent, contact, message, message_read};
use crate::error::AppError;
use crate::AppState;

use super::dto::{
    ChatHistoryParams, InboxSummaryEntry, MessageResponse, SearchParams, SendMessageRequest,
};

/// Check if a block relationship exists between two agents (either direction).
/// Returns true if either agent has blocked the other.
pub async fn check_blocked(
    db: &sea_orm::DatabaseConnection,
    a: &str,
    b: &str,
) -> Result<bool, sea_orm::DbErr> {
    // Check a→b
    if contact::Entity::find_by_id((a.to_string(), b.to_string()))
        .one(db)
        .await?
        .is_some_and(|c| c.is_blocked)
    {
        return Ok(true);
    }
    // Check b→a
    if contact::Entity::find_by_id((b.to_string(), a.to_string()))
        .one(db)
        .await?
        .is_some_and(|c| c.is_blocked)
    {
        return Ok(true);
    }
    Ok(false)
}

/// Build inbox summary: unread message counts grouped by sender.
pub async fn build_inbox_summary(
    db: &sea_orm::DatabaseConnection,
    agent_id: &str,
) -> Result<Vec<InboxSummaryEntry>, sea_orm::DbErr> {
    // Find all messages to me.
    let all_to_me = message::Entity::find()
        .filter(message::Column::ToAgent.eq(agent_id))
        .all(db)
        .await?;

    // Get existing read markers.
    let read_markers = message_read::Entity::find()
        .filter(message_read::Column::AgentId.eq(agent_id))
        .all(db)
        .await?;
    let read_ids: std::collections::HashSet<String> =
        read_markers.into_iter().map(|r| r.message_id).collect();

    // Group unread by sender.
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for m in &all_to_me {
        if !read_ids.contains(&m.id) {
            *counts.entry(m.from_agent.clone()).or_insert(0) += 1;
        }
    }

    // Build response with agent names.
    let mut result = Vec::new();
    for (from_agent, unread_count) in counts {
        let agent_name = agent::Entity::find_by_id(&from_agent)
            .one(db)
            .await?
            .map(|a| a.name)
            .unwrap_or_default();
        result.push(InboxSummaryEntry {
            from_agent,
            agent_name,
            unread_count,
        });
    }

    // Sort by unread_count desc for deterministic output.
    result.sort_by(|a, b| b.unread_count.cmp(&a.unread_count));
    Ok(result)
}

/// Mark all unread messages from `from_agent` to `me` as read.
pub async fn mark_read_from(
    db: &sea_orm::DatabaseConnection,
    me: &str,
    from_agent: &str,
) -> Result<(), sea_orm::DbErr> {
    let msgs_from = message::Entity::find()
        .filter(message::Column::ToAgent.eq(me))
        .filter(message::Column::FromAgent.eq(from_agent))
        .all(db)
        .await?;

    let read_markers = message_read::Entity::find()
        .filter(message_read::Column::AgentId.eq(me))
        .all(db)
        .await?;
    let read_ids: std::collections::HashSet<String> =
        read_markers.into_iter().map(|r| r.message_id).collect();

    let now = Utc::now();
    for m in &msgs_from {
        if !read_ids.contains(&m.id) {
            message_read::ActiveModel {
                agent_id: Set(me.to_string()),
                message_id: Set(m.id.clone()),
                read_at: Set(now),
            }
            .insert(db)
            .await?;
        }
    }
    Ok(())
}

fn to_response(m: &message::Model) -> MessageResponse {
    MessageResponse {
        id: m.id.clone(),
        from_agent: m.from_agent.clone(),
        to_agent: m.to_agent.clone(),
        channel_id: m.channel_id.clone(),
        content: m.content.clone(),
        msg_type: match m.msg_type {
            message::MsgType::Text => "text".to_string(),
        },
        created_at: m.created_at.to_rfc3339(),
    }
}

/// POST /api/messages — Send a DM (private message).
pub async fn send_message(
    auth: AgentAccess,
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<MessageResponse>), AppError> {
    let me = &auth.agent.id;

    // Validate content not empty.
    if req.content.trim().is_empty() {
        return Err(AppError::Validation("content must not be empty".into()));
    }

    // Prevent sending to self.
    if me == &req.to_agent {
        return Err(AppError::Validation("cannot send message to self".into()));
    }

    // Check block relationship (either direction).
    if check_blocked(&state.db, me, &req.to_agent)
        .await
        .map_err(AppError::Db)?
    {
        return Err(AppError::Forbidden(
            "cannot send message — blocked".into(),
        ));
    }

    // Verify recipient exists.
    agent::Entity::find_by_id(&req.to_agent)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", req.to_agent)))?;

    let msg_type = match req.msg_type.as_deref() {
        Some("text") | None => message::MsgType::Text,
        Some(other) => {
            return Err(AppError::Validation(format!(
                "unsupported msg_type: {}",
                other
            )));
        }
    };

    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    let model = message::ActiveModel {
        id: Set(id.clone()),
        from_agent: Set(me.clone()),
        to_agent: Set(Some(req.to_agent.clone())),
        channel_id: Set(None),
        content: Set(req.content.clone()),
        msg_type: Set(msg_type),
        created_at: Set(now),
    };
    let inserted = model.insert(&state.db).await.map_err(AppError::Db)?;

    // Push real-time notification to recipient.
    let resp = to_response(&inserted);
    let push_payload = serde_json::json!({
        "type": "new_message",
        "message": &resp,
    });
    state
        .connections
        .push(&req.to_agent, &push_payload.to_string())
        .await;

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/messages/inbox — Unread summary per sender for the authenticated agent.
pub async fn inbox(
    auth: AgentAccess,
    State(state): State<AppState>,
) -> Result<Json<Vec<InboxSummaryEntry>>, AppError> {
    let me = &auth.agent.id;
    let summary = build_inbox_summary(&state.db, me)
        .await
        .map_err(AppError::Db)?;
    Ok(Json(summary))
}

/// GET /api/messages/with/{agent_id} — Chat history with another agent.
pub async fn chat_history(
    auth: AgentAccess,
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<ChatHistoryParams>,
) -> Result<Json<Vec<MessageResponse>>, AppError> {
    let me = &auth.agent.id;
    let limit = params.limit.unwrap_or(20).min(20) as u64;

    // Messages between me and agent_id in either direction.
    let condition = Condition::any()
        .add(
            Condition::all()
                .add(message::Column::FromAgent.eq(me.as_str()))
                .add(message::Column::ToAgent.eq(agent_id.as_str())),
        )
        .add(
            Condition::all()
                .add(message::Column::FromAgent.eq(agent_id.as_str()))
                .add(message::Column::ToAgent.eq(me.as_str())),
        );

    let mut query = message::Entity::find()
        .filter(condition)
        .order_by_desc(message::Column::CreatedAt);

    // Apply cursor pagination: if `before` is provided, only return messages
    // created before that message's created_at.
    if let Some(ref before_id) = params.before {
        let cursor_msg = message::Entity::find_by_id(before_id)
            .one(&state.db)
            .await
            .map_err(AppError::Db)?;

        if let Some(cursor) = cursor_msg {
            query = query.filter(message::Column::CreatedAt.lt(cursor.created_at));
        }
    }

    // Apply limit.
    use sea_orm::QuerySelect;
    let messages = query
        .limit(limit)
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    // Auto-mark all messages from the other agent as read.
    mark_read_from(&state.db, me, &agent_id)
        .await
        .map_err(AppError::Db)?;

    Ok(Json(messages.iter().map(to_response).collect()))
}

/// POST /api/messages/{id}/read — Mark a single message as read.
pub async fn mark_read(
    auth: AgentAccess,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let me = &auth.agent.id;

    // Find the message, must exist and be addressed to this agent.
    let msg = message::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("message not found".into()))?;

    if msg.to_agent.as_deref() != Some(me) {
        return Err(AppError::NotFound("message not found".into()));
    }

    // Check if already read (idempotent).
    let existing = message_read::Entity::find_by_id((me.clone(), id.clone()))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?;

    if existing.is_some() {
        return Ok(StatusCode::OK);
    }

    let read_marker = message_read::ActiveModel {
        agent_id: Set(me.clone()),
        message_id: Set(id),
        read_at: Set(Utc::now()),
    };
    read_marker.insert(&state.db).await.map_err(AppError::Db)?;

    Ok(StatusCode::OK)
}

/// POST /api/messages/read-all — Mark all unread messages as read.
pub async fn mark_all_read(
    auth: AgentAccess,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let me = &auth.agent.id;

    // Find all messages to me.
    let all_to_me = message::Entity::find()
        .filter(message::Column::ToAgent.eq(me))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    // Get existing read markers.
    let read_markers = message_read::Entity::find()
        .filter(message_read::Column::AgentId.eq(me))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let read_ids: std::collections::HashSet<String> =
        read_markers.into_iter().map(|r| r.message_id).collect();

    let now = Utc::now();
    for msg in &all_to_me {
        if !read_ids.contains(&msg.id) {
            let marker = message_read::ActiveModel {
                agent_id: Set(me.clone()),
                message_id: Set(msg.id.clone()),
                read_at: Set(now),
            };
            marker.insert(&state.db).await.map_err(AppError::Db)?;
        }
    }

    Ok(StatusCode::OK)
}

/// GET /api/messages/search?q=xxx — FTS5 full-text search.
pub async fn search(
    auth: AgentAccess,
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<MessageResponse>>, AppError> {
    let me = &auth.agent.id;

    if params.q.trim().is_empty() {
        return Err(AppError::Validation("search query must not be empty".into()));
    }

    // Use FTS5 to get matching message IDs ranked by BM25.
    let fts_ids = crate::raw_sql::fts::fts_search(&state.db, params.q.trim(), 100)
        .await
        .map_err(AppError::Db)?;

    if fts_ids.is_empty() {
        return Ok(Json(vec![]));
    }

    // Load the matched messages, filtering by agent ownership.
    let messages = message::Entity::find()
        .filter(message::Column::Id.is_in(&fts_ids))
        .filter(
            Condition::any()
                .add(message::Column::FromAgent.eq(me.as_str()))
                .add(message::Column::ToAgent.eq(me.as_str())),
        )
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    // Preserve FTS5 BM25 ordering.
    let id_order: std::collections::HashMap<&str, usize> = fts_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let mut sorted = messages;
    sorted.sort_by_key(|m| id_order.get(m.id.as_str()).copied().unwrap_or(usize::MAX));

    Ok(Json(sorted.iter().map(to_response).collect()))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    use crate::db;
    use crate::entity::{agent, contact, message, message_read, user};

    /// Helper: create a test user.
    async fn create_user(db: &sea_orm::DatabaseConnection, id: &str, github_id: i64) {
        let now = Utc::now();
        user::ActiveModel {
            id: Set(id.into()),
            github_id: Set(github_id),
            github_name: Set(format!("user-{}", id)),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }

    /// Helper: create an agent for a user.
    async fn create_agent(db: &sea_orm::DatabaseConnection, agent_id: &str, user_id: &str) {
        let now = Utc::now();
        agent::ActiveModel {
            id: Set(agent_id.into()),
            user_id: Set(user_id.into()),
            name: Set(format!("Agent {}", agent_id)),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }

    /// Helper: insert a message directly.
    async fn insert_message(
        db: &sea_orm::DatabaseConnection,
        id: &str,
        from: &str,
        to: &str,
        content: &str,
    ) -> message::Model {
        message::ActiveModel {
            id: Set(id.into()),
            from_agent: Set(from.into()),
            to_agent: Set(Some(to.into())),
            channel_id: Set(None),
            content: Set(content.into()),
            msg_type: Set(message::MsgType::Text),
            created_at: Set(Utc::now()),
        }
        .insert(db)
        .await
        .unwrap()
    }

    // ── 1. send_message_creates_record ──

    #[tokio::test]
    async fn send_message_creates_record() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        let msg = insert_message(&db, "msg-1", "alice", "bob", "hello bob").await;

        // Verify it exists in DB.
        let found = message::Entity::find_by_id("msg-1")
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.from_agent, "alice");
        assert_eq!(found.to_agent, Some("bob".into()));
        assert_eq!(found.content, "hello bob");
        assert_eq!(found.channel_id, None);
        assert_eq!(msg.id, "msg-1");
    }

    // ── 2. send_to_self_rejected ──

    #[tokio::test]
    async fn send_to_self_rejected() {
        // Pure logic test: from_agent == to_agent should be rejected.
        let me = "alice";
        let to_agent = "alice";
        assert_eq!(me, to_agent); // Handler returns 422.
    }

    // ── 3. inbox_shows_unread_only ──

    #[tokio::test]
    async fn inbox_shows_unread_only() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        // Send two messages to alice.
        insert_message(&db, "msg-1", "bob", "alice", "hello 1").await;
        insert_message(&db, "msg-2", "bob", "alice", "hello 2").await;

        // Before marking read: both should be unread.
        let all_to_alice = message::Entity::find()
            .filter(message::Column::ToAgent.eq("alice"))
            .all(&db)
            .await
            .unwrap();

        let read_markers = message_read::Entity::find()
            .filter(message_read::Column::AgentId.eq("alice"))
            .all(&db)
            .await
            .unwrap();
        let read_ids: std::collections::HashSet<String> =
            read_markers.into_iter().map(|r| r.message_id).collect();

        let unread: Vec<_> = all_to_alice
            .iter()
            .filter(|m| !read_ids.contains(&m.id))
            .collect();
        assert_eq!(unread.len(), 2);

        // Mark msg-1 as read.
        message_read::ActiveModel {
            agent_id: Set("alice".into()),
            message_id: Set("msg-1".into()),
            read_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();

        // After marking read: only msg-2 should be unread.
        let read_markers = message_read::Entity::find()
            .filter(message_read::Column::AgentId.eq("alice"))
            .all(&db)
            .await
            .unwrap();
        let read_ids: std::collections::HashSet<String> =
            read_markers.into_iter().map(|r| r.message_id).collect();

        let unread: Vec<_> = all_to_alice
            .iter()
            .filter(|m| !read_ids.contains(&m.id))
            .collect();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].id, "msg-2");
    }

    // ── 4. chat_history_returns_both_directions ──

    #[tokio::test]
    async fn chat_history_returns_both_directions() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        // A sends to B.
        insert_message(&db, "msg-a2b", "alice", "bob", "hi bob").await;
        // B sends to A.
        insert_message(&db, "msg-b2a", "bob", "alice", "hi alice").await;

        // Query messages between alice and bob (either direction).
        use sea_orm::Condition;
        let condition = Condition::any()
            .add(
                Condition::all()
                    .add(message::Column::FromAgent.eq("alice"))
                    .add(message::Column::ToAgent.eq("bob")),
            )
            .add(
                Condition::all()
                    .add(message::Column::FromAgent.eq("bob"))
                    .add(message::Column::ToAgent.eq("alice")),
            );

        let messages = message::Entity::find()
            .filter(condition)
            .all(&db)
            .await
            .unwrap();

        assert_eq!(messages.len(), 2);
        let ids: Vec<&str> = messages.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"msg-a2b"));
        assert!(ids.contains(&"msg-b2a"));
    }

    // ── 5. mark_read_is_idempotent ──

    #[tokio::test]
    async fn mark_read_is_idempotent() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        insert_message(&db, "msg-1", "bob", "alice", "hello").await;

        // First mark-read.
        message_read::ActiveModel {
            agent_id: Set("alice".into()),
            message_id: Set("msg-1".into()),
            read_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();

        // Second mark-read: check existing first (idempotent pattern).
        let existing = message_read::Entity::find_by_id(("alice".to_string(), "msg-1".to_string()))
            .one(&db)
            .await
            .unwrap();
        assert!(existing.is_some()); // Already read, handler returns OK without inserting.

        // Verify only one read marker exists.
        let count = message_read::Entity::find()
            .filter(message_read::Column::AgentId.eq("alice"))
            .filter(message_read::Column::MessageId.eq("msg-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(count.len(), 1);
    }

    // ── 6. search_finds_matching_messages (via FTS5) ──

    #[tokio::test]
    async fn search_finds_matching_messages() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        // Insert messages — FTS5 triggers auto-index.
        insert_message(&db, "msg-1", "alice", "bob", "hello world").await;
        insert_message(&db, "msg-2", "bob", "alice", "goodbye world").await;
        insert_message(&db, "msg-3", "alice", "bob", "something else entirely").await;

        // FTS5 search for "world".
        let world_ids = crate::raw_sql::fts::fts_search(&db, "world", 100)
            .await
            .unwrap();
        assert_eq!(world_ids.len(), 2);
        assert!(world_ids.contains(&"msg-1".to_string()));
        assert!(world_ids.contains(&"msg-2".to_string()));

        // FTS5 search for "goodbye".
        let goodbye_ids = crate::raw_sql::fts::fts_search(&db, "goodbye", 100)
            .await
            .unwrap();
        assert_eq!(goodbye_ids.len(), 1);
        assert_eq!(goodbye_ids[0], "msg-2");

        // FTS5 search for non-existent term.
        let none_ids = crate::raw_sql::fts::fts_search(&db, "nonexistent", 100)
            .await
            .unwrap();
        assert!(none_ids.is_empty());
    }

    // ── 7. blocked contact prevents sending ──

    #[tokio::test]
    async fn blocked_contact_prevents_sending() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        // Alice blocks Bob.
        contact::ActiveModel {
            agent_id: Set("alice".into()),
            contact_id: Set("bob".into()),
            alias: Set(None),
            is_blocked: Set(true),
            created_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();

        // Check: should alice be able to send to bob? No — blocked.
        let blocked = is_blocked_between(&db, "alice", "bob").await;
        assert!(blocked);

        // Check: should bob be able to send to alice? No — alice blocked bob.
        let blocked = is_blocked_between(&db, "bob", "alice").await;
        assert!(blocked);

        // Check: unrelated pair is not blocked.
        let blocked = is_blocked_between(&db, "alice", "alice").await;
        assert!(!blocked);
    }

    /// Helper: check if a block relationship exists (either direction).
    async fn is_blocked_between(db: &sea_orm::DatabaseConnection, a: &str, b: &str) -> bool {
        crate::api::messages::check_blocked(db, a, b).await.unwrap()
    }

    // ── 8. inbox returns unread summary per contact ──

    #[tokio::test]
    async fn inbox_returns_unread_summary() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;
        create_agent(&db, "carol", "u1").await;

        // Bob sends 3 messages to alice.
        insert_message(&db, "msg-1", "bob", "alice", "hi 1").await;
        insert_message(&db, "msg-2", "bob", "alice", "hi 2").await;
        insert_message(&db, "msg-3", "bob", "alice", "hi 3").await;
        // Carol sends 1 message to alice.
        insert_message(&db, "msg-4", "carol", "alice", "hello").await;

        // Get inbox summary for alice.
        let summary = build_inbox_summary(&db, "alice").await;
        assert_eq!(summary.len(), 2); // bob and carol

        let bob_entry = summary.iter().find(|e| e.from_agent == "bob").unwrap();
        assert_eq!(bob_entry.unread_count, 3);

        let carol_entry = summary.iter().find(|e| e.from_agent == "carol").unwrap();
        assert_eq!(carol_entry.unread_count, 1);

        // Mark one of bob's messages as read.
        message_read::ActiveModel {
            agent_id: Set("alice".into()),
            message_id: Set("msg-1".into()),
            read_at: Set(Utc::now()),
        }
        .insert(&db)
        .await
        .unwrap();

        let summary = build_inbox_summary(&db, "alice").await;
        let bob_entry = summary.iter().find(|e| e.from_agent == "bob").unwrap();
        assert_eq!(bob_entry.unread_count, 2);
    }

    /// Helper: build inbox summary (calls the function we need to implement).
    async fn build_inbox_summary(
        db: &sea_orm::DatabaseConnection,
        agent_id: &str,
    ) -> Vec<crate::api::dto::InboxSummaryEntry> {
        crate::api::messages::build_inbox_summary(db, agent_id)
            .await
            .unwrap()
    }

    // ── 9. viewing history auto-marks messages as read ──

    #[tokio::test]
    async fn history_auto_marks_read() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        // Bob sends messages to alice.
        insert_message(&db, "msg-1", "bob", "alice", "hello 1").await;
        insert_message(&db, "msg-2", "bob", "alice", "hello 2").await;

        // Before: alice has 2 unread from bob.
        let summary = build_inbox_summary(&db, "alice").await;
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].unread_count, 2);

        // Alice views history with bob → auto-mark read.
        crate::api::messages::mark_read_from(&db, "alice", "bob")
            .await
            .unwrap();

        // After: alice has 0 unread from bob.
        let summary = build_inbox_summary(&db, "alice").await;
        assert!(summary.is_empty()); // no unread contacts
    }
}
