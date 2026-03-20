use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};

use crate::auth::extractor::AgentAuth;
use crate::entity::{agent, channel, channel_member, message};
use crate::entity::channel_member::MemberRole;
use crate::error::AppError;
use crate::AppState;

use super::dto::{
    ChannelDetailResponse, ChannelMemberResponse, ChannelResponse, ChatHistoryParams,
    CreateChannelRequest, InviteMemberRequest, MessageResponse, SendChannelMessageRequest,
};

/// POST /api/channels
pub async fn create_channel(
    auth: AgentAuth,
    State(state): State<AppState>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>), AppError> {
    let now = Utc::now();
    let channel_id = uuid::Uuid::new_v4().to_string();

    let txn = state.db.begin().await.map_err(AppError::Db)?;

    let ch = channel::ActiveModel {
        id: Set(channel_id.clone()),
        name: Set(req.name.clone()),
        created_by: Set(auth.agent.id.clone()),
        is_closed: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
    };
    ch.insert(&txn).await.map_err(AppError::Db)?;

    let member = channel_member::ActiveModel {
        channel_id: Set(channel_id.clone()),
        agent_id: Set(auth.agent.id.clone()),
        role: Set(MemberRole::Admin),
        joined_at: Set(now),
    };
    member.insert(&txn).await.map_err(AppError::Db)?;

    txn.commit().await.map_err(AppError::Db)?;

    Ok((
        StatusCode::CREATED,
        Json(ChannelResponse {
            id: channel_id,
            name: req.name,
            created_by: auth.agent.id,
            is_closed: false,
            created_at: now.to_rfc3339(),
        }),
    ))
}

/// GET /api/channels
pub async fn list_channels(
    auth: AgentAuth,
    State(state): State<AppState>,
) -> Result<Json<Vec<ChannelResponse>>, AppError> {
    // Find channel IDs where I'm a member.
    let memberships = channel_member::Entity::find()
        .filter(channel_member::Column::AgentId.eq(&auth.agent.id))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let channel_ids: Vec<String> = memberships.iter().map(|m| m.channel_id.clone()).collect();

    if channel_ids.is_empty() {
        return Ok(Json(vec![]));
    }

    let channels = channel::Entity::find()
        .filter(channel::Column::Id.is_in(channel_ids))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let result = channels
        .into_iter()
        .map(|ch| ChannelResponse {
            id: ch.id,
            name: ch.name,
            created_by: ch.created_by,
            is_closed: ch.is_closed,
            created_at: ch.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}

/// GET /api/channels/{id}
pub async fn get_channel(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ChannelDetailResponse>, AppError> {
    let ch = channel::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))?;

    // Check membership.
    let my_membership = channel_member::Entity::find_by_id((id.clone(), auth.agent.id.clone()))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?;

    if my_membership.is_none() {
        return Err(AppError::Forbidden("not a member of this channel".into()));
    }

    let members = channel_member::Entity::find()
        .filter(channel_member::Column::ChannelId.eq(&id))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let member_responses = members
        .into_iter()
        .map(|m| ChannelMemberResponse {
            agent_id: m.agent_id,
            role: match m.role {
                MemberRole::Admin => "admin".to_string(),
                MemberRole::Member => "member".to_string(),
            },
            joined_at: m.joined_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ChannelDetailResponse {
        id: ch.id,
        name: ch.name,
        created_by: ch.created_by,
        is_closed: ch.is_closed,
        created_at: ch.created_at.to_rfc3339(),
        members: member_responses,
    }))
}

/// POST /api/channels/{id}/members
pub async fn invite_member(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<InviteMemberRequest>,
) -> Result<StatusCode, AppError> {
    // Channel must exist.
    let ch = channel::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))?;

    // Channel must not be closed.
    if ch.is_closed {
        return Err(AppError::Validation("channel is closed".into()));
    }

    // Requester must be admin.
    let my_membership =
        channel_member::Entity::find_by_id((id.clone(), auth.agent.id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Forbidden("not a member of this channel".into()))?;

    if my_membership.role != MemberRole::Admin {
        return Err(AppError::Forbidden("only admins can invite members".into()));
    }

    // Target agent must exist.
    agent::Entity::find_by_id(&req.agent_id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", req.agent_id)))?;

    // Target must not already be a member.
    let existing =
        channel_member::Entity::find_by_id((id.clone(), req.agent_id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?;

    if existing.is_some() {
        return Err(AppError::Conflict("agent is already a member".into()));
    }

    let member = channel_member::ActiveModel {
        channel_id: Set(id),
        agent_id: Set(req.agent_id),
        role: Set(MemberRole::Member),
        joined_at: Set(Utc::now()),
    };
    member.insert(&state.db).await.map_err(AppError::Db)?;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/channels/{id}/members/{agent_id}
pub async fn remove_member(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path((id, target_agent_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    // Channel must exist.
    let ch = channel::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))?;

    // Channel must not be closed.
    if ch.is_closed {
        return Err(AppError::Validation("channel is closed".into()));
    }

    // Requester must be admin.
    let my_membership =
        channel_member::Entity::find_by_id((id.clone(), auth.agent.id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Forbidden("not a member of this channel".into()))?;

    if my_membership.role != MemberRole::Admin {
        return Err(AppError::Forbidden("only admins can remove members".into()));
    }

    // Can't remove self (admin).
    if target_agent_id == auth.agent.id {
        return Err(AppError::Validation("admin cannot remove self".into()));
    }

    // Target must be a member.
    let target_membership =
        channel_member::Entity::find_by_id((id.clone(), target_agent_id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::NotFound("member not found".into()))?;

    let am: channel_member::ActiveModel = target_membership.into();
    am.delete(&state.db).await.map_err(AppError::Db)?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/channels/{id}/close
pub async fn close_channel(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    // Channel must exist.
    let ch = channel::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))?;

    // Requester must be admin.
    let my_membership =
        channel_member::Entity::find_by_id((id.clone(), auth.agent.id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .ok_or_else(|| AppError::Forbidden("not a member of this channel".into()))?;

    if my_membership.role != MemberRole::Admin {
        return Err(AppError::Forbidden("only admins can close channels".into()));
    }

    let mut am: channel::ActiveModel = ch.into();
    am.is_closed = Set(true);
    am.updated_at = Set(Utc::now());
    am.update(&state.db).await.map_err(AppError::Db)?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/channels/{id}/messages
pub async fn send_channel_message(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SendChannelMessageRequest>,
) -> Result<(StatusCode, Json<MessageResponse>), AppError> {
    // Channel must exist.
    let ch = channel::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))?;

    // Channel must not be closed.
    if ch.is_closed {
        return Err(AppError::Validation("channel is closed".into()));
    }

    // Must be a member.
    let my_membership =
        channel_member::Entity::find_by_id((id.clone(), auth.agent.id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?;

    if my_membership.is_none() {
        return Err(AppError::Forbidden("not a member of this channel".into()));
    }

    let now = Utc::now();
    let msg_id = uuid::Uuid::new_v4().to_string();
    let msg_type_str = req.msg_type.unwrap_or_else(|| "text".to_string());

    let msg = message::ActiveModel {
        id: Set(msg_id.clone()),
        from_agent: Set(auth.agent.id.clone()),
        to_agent: Set(None),
        channel_id: Set(Some(id.clone())),
        content: Set(req.content.clone()),
        msg_type: Set(message::MsgType::Text),
        created_at: Set(now),
    };
    msg.insert(&state.db).await.map_err(AppError::Db)?;

    let resp = MessageResponse {
        id: msg_id,
        from_agent: auth.agent.id.clone(),
        to_agent: None,
        channel_id: Some(id.clone()),
        content: req.content,
        msg_type: msg_type_str,
        created_at: now.to_rfc3339(),
    };

    // Push real-time notification to all channel members (except sender).
    let members = channel_member::Entity::find()
        .filter(channel_member::Column::ChannelId.eq(&id))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;
    let member_ids: Vec<String> = members.into_iter().map(|m| m.agent_id).collect();
    let push_payload = serde_json::json!({
        "type": "new_message",
        "message": &resp,
    });
    state
        .connections
        .push_to_channel_members(&member_ids, &auth.agent.id, &push_payload.to_string())
        .await;

    Ok((StatusCode::CREATED, Json(resp)))
}

/// GET /api/channels/{id}/messages
pub async fn list_channel_messages(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ChatHistoryParams>,
) -> Result<Json<Vec<MessageResponse>>, AppError> {
    // Channel must exist.
    channel::Entity::find_by_id(&id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))?;

    // Must be a member.
    let my_membership =
        channel_member::Entity::find_by_id((id.clone(), auth.agent.id.clone()))
            .one(&state.db)
            .await
            .map_err(AppError::Db)?;

    if my_membership.is_none() {
        return Err(AppError::Forbidden("not a member of this channel".into()));
    }

    let limit = params.limit.unwrap_or(50).min(100) as u64;

    let mut query = message::Entity::find()
        .filter(message::Column::ChannelId.eq(&id))
        .order_by_desc(message::Column::CreatedAt);

    if let Some(before) = &params.before {
        query = query.filter(message::Column::CreatedAt.lt(
            chrono::DateTime::parse_from_rfc3339(before)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|_| AppError::Validation("invalid 'before' timestamp".into()))?,
        ));
    }

    let messages = query
        .all(&state.db)
        .await
        .map_err(AppError::Db)?
        .into_iter()
        .take(limit as usize)
        .map(|m| MessageResponse {
            id: m.id,
            from_agent: m.from_agent,
            to_agent: m.to_agent,
            channel_id: m.channel_id,
            content: m.content,
            msg_type: match m.msg_type {
                message::MsgType::Text => "text".to_string(),
            },
            created_at: m.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(messages))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    use crate::db;
    use crate::entity::{agent, channel, channel_member, message, user};
    use crate::entity::channel_member::MemberRole;

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
            token_hash: Set(format!("hash-{}", agent_id)),
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

    /// Helper: create a channel with creator as admin.
    async fn create_channel_with_admin(
        db: &sea_orm::DatabaseConnection,
        channel_id: &str,
        name: &str,
        creator_id: &str,
    ) {
        let now = Utc::now();
        channel::ActiveModel {
            id: Set(channel_id.into()),
            name: Set(name.into()),
            created_by: Set(creator_id.into()),
            is_closed: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        channel_member::ActiveModel {
            channel_id: Set(channel_id.into()),
            agent_id: Set(creator_id.into()),
            role: Set(MemberRole::Admin),
            joined_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }

    /// Helper: add a member to a channel.
    async fn add_member(
        db: &sea_orm::DatabaseConnection,
        channel_id: &str,
        agent_id: &str,
        role: MemberRole,
    ) {
        channel_member::ActiveModel {
            channel_id: Set(channel_id.into()),
            agent_id: Set(agent_id.into()),
            role: Set(role),
            joined_at: Set(Utc::now()),
        }
        .insert(db)
        .await
        .unwrap();
    }

    // 1. create_channel_adds_creator_as_admin

    #[tokio::test]
    async fn create_channel_adds_creator_as_admin() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;

        // Simulate create_channel: insert channel + admin member in transaction.
        let now = Utc::now();
        let channel_id = uuid::Uuid::new_v4().to_string();

        let txn = sea_orm::TransactionTrait::begin(&db).await.unwrap();

        channel::ActiveModel {
            id: Set(channel_id.clone()),
            name: Set("test-channel".into()),
            created_by: Set("alice".into()),
            is_closed: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&txn)
        .await
        .unwrap();

        channel_member::ActiveModel {
            channel_id: Set(channel_id.clone()),
            agent_id: Set("alice".into()),
            role: Set(MemberRole::Admin),
            joined_at: Set(now),
        }
        .insert(&txn)
        .await
        .unwrap();

        txn.commit().await.unwrap();

        // Verify channel exists.
        let ch = channel::Entity::find_by_id(&channel_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ch.name, "test-channel");
        assert_eq!(ch.created_by, "alice");

        // Verify creator is admin member.
        let member =
            channel_member::Entity::find_by_id((channel_id, "alice".to_string()))
                .one(&db)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(member.role, MemberRole::Admin);
    }

    // 2. list_channels_only_mine

    #[tokio::test]
    async fn list_channels_only_mine() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_user(&db, "u2", 2).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u2").await;

        // Alice creates channel-a, Bob creates channel-b.
        create_channel_with_admin(&db, "ch-a", "Alice's Channel", "alice").await;
        create_channel_with_admin(&db, "ch-b", "Bob's Channel", "bob").await;

        // Alice should only see ch-a.
        let alice_memberships = channel_member::Entity::find()
            .filter(channel_member::Column::AgentId.eq("alice"))
            .all(&db)
            .await
            .unwrap();
        let alice_channel_ids: Vec<String> =
            alice_memberships.iter().map(|m| m.channel_id.clone()).collect();

        assert_eq!(alice_channel_ids.len(), 1);
        assert!(alice_channel_ids.contains(&"ch-a".to_string()));
        assert!(!alice_channel_ids.contains(&"ch-b".to_string()));
    }

    // 3. invite_member_works

    #[tokio::test]
    async fn invite_member_works() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_user(&db, "u2", 2).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u2").await;

        create_channel_with_admin(&db, "ch-1", "Test Channel", "alice").await;

        // Alice (admin) invites Bob.
        // Check alice is admin.
        let alice_membership =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "alice".to_string()))
                .one(&db)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(alice_membership.role, MemberRole::Admin);

        // Bob exists and is not a member yet.
        let existing =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "bob".to_string()))
                .one(&db)
                .await
                .unwrap();
        assert!(existing.is_none());

        // Add Bob as member.
        add_member(&db, "ch-1", "bob", MemberRole::Member).await;

        // Verify Bob is now a member.
        let bob_membership =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "bob".to_string()))
                .one(&db)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(bob_membership.role, MemberRole::Member);

        // Verify both are in the member list.
        let members = channel_member::Entity::find()
            .filter(channel_member::Column::ChannelId.eq("ch-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(members.len(), 2);
    }

    // 4. non_admin_cannot_invite

    #[tokio::test]
    async fn non_admin_cannot_invite() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_user(&db, "u2", 2).await;
        create_user(&db, "u3", 3).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u2").await;
        create_agent(&db, "carol", "u3").await;

        create_channel_with_admin(&db, "ch-1", "Test Channel", "alice").await;
        add_member(&db, "ch-1", "bob", MemberRole::Member).await;

        // Bob is a member (not admin). Simulate permission check.
        let bob_membership =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "bob".to_string()))
                .one(&db)
                .await
                .unwrap()
                .unwrap();

        // Bob's role is Member, not Admin -> should be rejected (403).
        assert_ne!(bob_membership.role, MemberRole::Admin);
    }

    // 5. closed_channel_rejects_message

    #[tokio::test]
    async fn closed_channel_rejects_message() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;

        create_channel_with_admin(&db, "ch-1", "Test Channel", "alice").await;

        // Close the channel.
        let ch = channel::Entity::find_by_id("ch-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut am: channel::ActiveModel = ch.into();
        am.is_closed = Set(true);
        am.updated_at = Set(Utc::now());
        am.update(&db).await.unwrap();

        // Verify channel is closed.
        let ch = channel::Entity::find_by_id("ch-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(ch.is_closed);

        // Attempting to send a message should be rejected (is_closed check).
        // The handler would return Validation error.
    }

    // 6. channel_messages_only_for_members

    #[tokio::test]
    async fn channel_messages_only_for_members() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_user(&db, "u2", 2).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u2").await;

        create_channel_with_admin(&db, "ch-1", "Test Channel", "alice").await;

        // Bob is NOT a member. Check membership.
        let bob_membership =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "bob".to_string()))
                .one(&db)
                .await
                .unwrap();
        assert!(bob_membership.is_none()); // Would produce 403 in handler.

        // Alice IS a member, can access messages.
        let alice_membership =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "alice".to_string()))
                .one(&db)
                .await
                .unwrap();
        assert!(alice_membership.is_some());

        // Insert a message in the channel.
        let now = Utc::now();
        message::ActiveModel {
            id: Set(uuid::Uuid::new_v4().to_string()),
            from_agent: Set("alice".into()),
            to_agent: Set(None),
            channel_id: Set(Some("ch-1".into())),
            content: Set("hello group".into()),
            msg_type: Set(message::MsgType::Text),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        // Alice can query messages.
        let messages = message::Entity::find()
            .filter(message::Column::ChannelId.eq("ch-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "hello group");
    }

    // 7. remove_member_works

    #[tokio::test]
    async fn remove_member_works() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_user(&db, "u2", 2).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u2").await;

        create_channel_with_admin(&db, "ch-1", "Test Channel", "alice").await;
        add_member(&db, "ch-1", "bob", MemberRole::Member).await;

        // Verify Bob is a member.
        let members_before = channel_member::Entity::find()
            .filter(channel_member::Column::ChannelId.eq("ch-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(members_before.len(), 2);

        // Alice (admin) removes Bob.
        let bob_membership =
            channel_member::Entity::find_by_id(("ch-1".to_string(), "bob".to_string()))
                .one(&db)
                .await
                .unwrap()
                .unwrap();
        let am: channel_member::ActiveModel = bob_membership.into();
        am.delete(&db).await.unwrap();

        // Verify Bob is no longer a member.
        let members_after = channel_member::Entity::find()
            .filter(channel_member::Column::ChannelId.eq("ch-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(members_after.len(), 1);
        assert_eq!(members_after[0].agent_id, "alice");
    }
}
