//! # Risk monitoring module
//!
//! Basic risk assessment for authentication events.
//! Rules:
//! - > 5 consecutive failures for a credential → High → auto-revoke
//! - Otherwise → Low

use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::entity::{agent_credential, auth_event};

/// Risk level assessment result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    High,
}

/// Max consecutive auth failures before auto-revoking credential.
const MAX_CONSECUTIVE_FAILURES: usize = 5;

/// Time window for checking recent auth events (1 hour).
const RISK_WINDOW_SECS: i64 = 3600;

/// Assess risk for an agent/credential and take action if needed.
///
/// Returns the risk level. If High, the credential is automatically revoked
/// with status RiskRevoked.
pub async fn assess_risk(
    db: &DatabaseConnection,
    agent_id: &str,
    credential_id: &str,
) -> Result<RiskLevel, sea_orm::DbErr> {
    let since = Utc::now() - Duration::seconds(RISK_WINDOW_SECS);

    // Get recent auth events for this credential, newest first.
    let recent_events = auth_event::Entity::find()
        .filter(auth_event::Column::AgentId.eq(agent_id))
        .filter(auth_event::Column::CredentialId.eq(credential_id))
        .filter(auth_event::Column::CreatedAt.gt(since))
        .order_by_desc(auth_event::Column::CreatedAt)
        .all(db)
        .await?;

    // Count consecutive failures from the most recent event.
    let consecutive_failures = recent_events
        .iter()
        .take_while(|e| !e.success)
        .count();

    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
        // Auto-revoke the credential.
        let cred = agent_credential::Entity::find_by_id(credential_id)
            .one(db)
            .await?;

        if let Some(cred) = cred
            && cred.status == agent_credential::CredentialStatus::Active
        {
            let now = Utc::now();
            let mut am: agent_credential::ActiveModel = cred.into();
            am.status = Set(agent_credential::CredentialStatus::RiskRevoked);
            am.revoke_reason = Set(Some(format!(
                "auto-revoked: {} consecutive auth failures",
                consecutive_failures
            )));
            am.revoked_at = Set(Some(now));
            am.update(db).await?;

            // Set reauth_required on agent.
            let agent = crate::entity::agent::Entity::find_by_id(agent_id)
                .one(db)
                .await?;
            if let Some(agent) = agent {
                let mut agent_am: crate::entity::agent::ActiveModel = agent.into();
                agent_am.reauth_required = Set(true);
                agent_am.updated_at = Set(now);
                agent_am.update(db).await?;
            }
        }

        return Ok(RiskLevel::High);
    }

    Ok(RiskLevel::Low)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    use crate::db;
    use crate::entity::{agent, agent_credential, auth_event, user};

    use super::*;

    async fn setup(db: &sea_orm::DatabaseConnection) {
        let now = Utc::now();
        user::ActiveModel {
            id: Set("u1".into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();

        agent::ActiveModel {
            id: Set("test-agent".into()),
            user_id: Set("u1".into()),
            name: Set("Test Agent".into()),
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

        agent_credential::ActiveModel {
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
        }
        .insert(db)
        .await
        .unwrap();
    }

    async fn insert_event(db: &sea_orm::DatabaseConnection, idx: usize, success: bool) {
        let now = Utc::now();
        auth_event::ActiveModel {
            id: Set(format!("evt-{}", idx)),
            agent_id: Set("test-agent".into()),
            credential_id: Set(Some("cred-1".into())),
            event_type: Set("auth_attempt".into()),
            success: Set(success),
            reason: Set(None),
            source_ip: Set(None),
            client_name: Set(None),
            client_version: Set(None),
            instance_label: Set(None),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn no_events_is_low_risk() {
        let db = db::test_db().await;
        setup(&db).await;

        let level = assess_risk(&db, "test-agent", "cred-1").await.unwrap();
        assert_eq!(level, RiskLevel::Low);
    }

    #[tokio::test]
    async fn few_failures_is_low_risk() {
        let db = db::test_db().await;
        setup(&db).await;

        // 3 failures — below threshold.
        for i in 0..3 {
            insert_event(&db, i, false).await;
        }

        let level = assess_risk(&db, "test-agent", "cred-1").await.unwrap();
        assert_eq!(level, RiskLevel::Low);

        // Credential should still be active.
        let cred = agent_credential::Entity::find_by_id("cred-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cred.status, agent_credential::CredentialStatus::Active);
    }

    #[tokio::test]
    async fn consecutive_failures_triggers_high_risk() {
        let db = db::test_db().await;
        setup(&db).await;

        // 6 consecutive failures.
        for i in 0..6 {
            insert_event(&db, i, false).await;
        }

        let level = assess_risk(&db, "test-agent", "cred-1").await.unwrap();
        assert_eq!(level, RiskLevel::High);

        // Credential should be risk-revoked.
        let cred = agent_credential::Entity::find_by_id("cred-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cred.status, agent_credential::CredentialStatus::RiskRevoked);
        assert!(cred.revoke_reason.unwrap().contains("consecutive"));

        // Agent should have reauth_required.
        let agent = agent::Entity::find_by_id("test-agent")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(agent.reauth_required);
    }

    #[tokio::test]
    async fn success_breaks_failure_streak() {
        let db = db::test_db().await;
        setup(&db).await;

        // 4 failures, then 1 success, then 3 more failures.
        for i in 0..4 {
            insert_event(&db, i, false).await;
        }
        insert_event(&db, 100, true).await;
        for i in 4..7 {
            insert_event(&db, i, false).await;
        }

        // Most recent events: 3 failures (below threshold) — the success breaks the streak.
        let level = assess_risk(&db, "test-agent", "cred-1").await.unwrap();
        assert_eq!(level, RiskLevel::Low);
    }
}
