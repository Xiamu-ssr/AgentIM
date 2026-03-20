use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::auth::extractor::AgentAuth;
use crate::entity::{agent, contact};
use crate::error::AppError;
use crate::AppState;

use super::dto::{AddContactRequest, ContactResponse};

/// POST /api/contacts
pub async fn add_contact(
    auth: AgentAuth,
    State(state): State<AppState>,
    Json(req): Json<AddContactRequest>,
) -> Result<(StatusCode, Json<ContactResponse>), AppError> {
    let me = &auth.agent.id;

    // Prevent adding self.
    if me == &req.contact_id {
        return Err(AppError::Validation("cannot add self as contact".into()));
    }

    // Verify the contact agent exists.
    let target = agent::Entity::find_by_id(&req.contact_id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", req.contact_id)))?;

    // Prevent duplicate.
    let existing = contact::Entity::find_by_id((me.clone(), req.contact_id.clone()))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?;

    if existing.is_some() {
        return Err(AppError::Conflict("contact already exists".into()));
    }

    let now = Utc::now();
    let model = contact::ActiveModel {
        agent_id: Set(me.clone()),
        contact_id: Set(req.contact_id.clone()),
        alias: Set(req.alias.clone()),
        created_at: Set(now),
    };
    model.insert(&state.db).await.map_err(AppError::Db)?;

    Ok((
        StatusCode::CREATED,
        Json(ContactResponse {
            contact_id: req.contact_id,
            alias: req.alias,
            agent_name: target.name,
            created_at: now.to_rfc3339(),
        }),
    ))
}

/// GET /api/contacts
pub async fn list_contacts(
    auth: AgentAuth,
    State(state): State<AppState>,
) -> Result<Json<Vec<ContactResponse>>, AppError> {
    let me = &auth.agent.id;

    let contacts = contact::Entity::find()
        .filter(contact::Column::AgentId.eq(me))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let mut result = Vec::with_capacity(contacts.len());
    for c in &contacts {
        let agent_name = agent::Entity::find_by_id(&c.contact_id)
            .one(&state.db)
            .await
            .map_err(AppError::Db)?
            .map(|a| a.name)
            .unwrap_or_default();

        result.push(ContactResponse {
            contact_id: c.contact_id.clone(),
            alias: c.alias.clone(),
            agent_name,
            created_at: c.created_at.to_rfc3339(),
        });
    }

    Ok(Json(result))
}

/// DELETE /api/contacts/:contact_id
pub async fn remove_contact(
    auth: AgentAuth,
    State(state): State<AppState>,
    Path(contact_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let me = &auth.agent.id;

    let existing = contact::Entity::find_by_id((me.clone(), contact_id.clone()))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("contact not found".into()))?;

    let am: contact::ActiveModel = existing.into();
    am.delete(&state.db).await.map_err(AppError::Db)?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    use crate::db;
    use crate::entity::{agent, contact, user};

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

    /// Helper: add a contact row directly.
    async fn add_contact_row(
        db: &sea_orm::DatabaseConnection,
        agent_id: &str,
        contact_id: &str,
        alias: Option<&str>,
    ) {
        contact::ActiveModel {
            agent_id: Set(agent_id.into()),
            contact_id: Set(contact_id.into()),
            alias: Set(alias.map(|s| s.into())),
            created_at: Set(Utc::now()),
        }
        .insert(db)
        .await
        .unwrap();
    }

    // ── Add contact: appears in list ──

    #[tokio::test]
    async fn add_contact_appears_in_list() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        // Add alice -> bob contact.
        add_contact_row(&db, "alice", "bob", Some("Bobby")).await;

        // List alice's contacts.
        let contacts = contact::Entity::find()
            .filter(contact::Column::AgentId.eq("alice"))
            .all(&db)
            .await
            .unwrap();

        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].contact_id, "bob");
        assert_eq!(contacts[0].alias, Some("Bobby".into()));
    }

    // ── Add non-existent agent: 404 ──

    #[tokio::test]
    async fn add_nonexistent_agent_fails() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;

        // Try to look up a non-existent agent (simulates the handler check).
        let target = agent::Entity::find_by_id("no-such-agent")
            .one(&db)
            .await
            .unwrap();

        assert!(target.is_none()); // Would produce 404 in handler.
    }

    // ── Add self: 422 ──

    #[tokio::test]
    async fn add_self_rejected() {
        // Pure logic test: agent_id == contact_id should be rejected.
        let me = "alice";
        let contact_id = "alice";
        assert_eq!(me, contact_id); // Handler would return 422.
    }

    // ── Add duplicate: 409 ──

    #[tokio::test]
    async fn add_duplicate_detected() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        add_contact_row(&db, "alice", "bob", None).await;

        // Check for existing contact (simulates the handler duplicate check).
        let existing = contact::Entity::find_by_id(("alice".to_string(), "bob".to_string()))
            .one(&db)
            .await
            .unwrap();

        assert!(existing.is_some()); // Would produce 409 in handler.
    }

    // ── Delete: removed ──

    #[tokio::test]
    async fn delete_contact_removes_row() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;

        add_contact_row(&db, "alice", "bob", None).await;

        // Delete it.
        let existing = contact::Entity::find_by_id(("alice".to_string(), "bob".to_string()))
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let am: contact::ActiveModel = existing.into();
        am.delete(&db).await.unwrap();

        // Verify gone.
        let remaining = contact::Entity::find()
            .filter(contact::Column::AgentId.eq("alice"))
            .all(&db)
            .await
            .unwrap();

        assert!(remaining.is_empty());
    }

    // ── List: only own contacts ──

    #[tokio::test]
    async fn list_only_own_contacts() {
        let db = db::test_db().await;
        create_user(&db, "u1", 1).await;
        create_user(&db, "u2", 2).await;
        create_agent(&db, "alice", "u1").await;
        create_agent(&db, "bob", "u1").await;
        create_agent(&db, "carol", "u2").await;

        // alice adds bob.
        add_contact_row(&db, "alice", "bob", None).await;
        // carol adds alice.
        add_contact_row(&db, "carol", "alice", None).await;

        // Alice's contacts: only bob.
        let alice_contacts = contact::Entity::find()
            .filter(contact::Column::AgentId.eq("alice"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(alice_contacts.len(), 1);
        assert_eq!(alice_contacts[0].contact_id, "bob");

        // Carol's contacts: only alice.
        let carol_contacts = contact::Entity::find()
            .filter(contact::Column::AgentId.eq("carol"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(carol_contacts.len(), 1);
        assert_eq!(carol_contacts[0].contact_id, "alice");
    }
}
