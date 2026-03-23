use axum::extract::{Path, State};
use axum::Json;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

use crate::auth::extractor::UserSession;
use crate::auth::token::{
    create_jwt, generate_challenge_nonce, generate_claim_code, hash_claim_code,
    public_key_fingerprint,
};
use crate::consts;
use crate::entity::{agent, agent_credential, auth_event, claim_token};
use crate::error::AppError;
use crate::AppState;

use super::dto::{
    ActivateCredentialRequest, ActivateCredentialResponse, AuthEventResponse, ChallengeRequest,
    ChallengeResponse, ClaimCodeResponse, VerifyRequest, VerifyResponse,
};

/// Fetch an agent by id and verify ownership (same as agents.rs helper).
async fn find_owned_agent(
    db: &sea_orm::DatabaseConnection,
    agent_id: &str,
    user_id: &str,
) -> Result<agent::Model, AppError> {
    let found = agent::Entity::find_by_id(agent_id)
        .one(db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", agent_id)))?;

    if found.user_id != user_id {
        return Err(AppError::Forbidden("not your agent".into()));
    }

    Ok(found)
}

/// POST /api/agents/{id}/claim
///
/// Owner generates a one-time claim code for an agent.
/// Revokes any existing active claim codes for this agent.
pub async fn generate_claim(
    session: UserSession,
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<ClaimCodeResponse>, AppError> {
    let agent = find_owned_agent(&state.db, &agent_id, &session.user.id).await?;

    // Determine purpose: enroll if no active credential exists, recover otherwise.
    let has_active_cred = agent_credential::Entity::find()
        .filter(agent_credential::Column::AgentId.eq(&agent.id))
        .filter(agent_credential::Column::Status.eq(agent_credential::CredentialStatus::Active))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?;

    let purpose = if has_active_cred.is_some() {
        claim_token::ClaimPurpose::Recover
    } else {
        claim_token::ClaimPurpose::Enroll
    };

    // Revoke any existing active claim codes for this agent.
    let active_claims = claim_token::Entity::find()
        .filter(claim_token::Column::AgentId.eq(&agent.id))
        .filter(claim_token::Column::Status.eq(claim_token::ClaimStatus::Active))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    for claim in active_claims {
        let mut am: claim_token::ActiveModel = claim.into();
        am.status = Set(claim_token::ClaimStatus::Revoked);
        am.update(&state.db).await.map_err(AppError::Db)?;
    }

    // Generate new claim code.
    let raw_code = generate_claim_code();
    let code_hash = hash_claim_code(&raw_code);
    let now = Utc::now();
    let expires_at = now + Duration::seconds(consts::CLAIM_CODE_TTL_SECS as i64);

    let claim = claim_token::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        agent_id: Set(agent.id),
        user_id: Set(session.user.id),
        code_hash: Set(code_hash),
        purpose: Set(purpose),
        status: Set(claim_token::ClaimStatus::Active),
        created_at: Set(now),
        expires_at: Set(expires_at),
        used_at: Set(None),
        created_from_ip: Set(None),
    };
    claim.insert(&state.db).await.map_err(AppError::Db)?;

    Ok(Json(ClaimCodeResponse {
        claim_code: raw_code,
        expires_at: expires_at.to_rfc3339(),
    }))
}

/// POST /api/agents/{id}/credentials/activate
///
/// Agent uses a claim code + Ed25519 public key to activate a new credential.
/// Replaces any existing active credential for this agent.
pub async fn activate_credential(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(req): Json<ActivateCredentialRequest>,
) -> Result<Json<ActivateCredentialResponse>, AppError> {
    // Verify the agent exists.
    let agent = agent::Entity::find_by_id(&agent_id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", agent_id)))?;

    // Find matching active claim code.
    let code_hash = hash_claim_code(&req.claim_code);
    let claim = claim_token::Entity::find()
        .filter(claim_token::Column::AgentId.eq(&agent.id))
        .filter(claim_token::Column::CodeHash.eq(&code_hash))
        .filter(claim_token::Column::Status.eq(claim_token::ClaimStatus::Active))
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::Unauthorized("invalid or expired claim code".into()))?;

    // Check expiry.
    let now = Utc::now();
    if now > claim.expires_at {
        // Mark as expired.
        let mut am: claim_token::ActiveModel = claim.into();
        am.status = Set(claim_token::ClaimStatus::Expired);
        am.update(&state.db).await.map_err(AppError::Db)?;
        return Err(AppError::Unauthorized("claim code expired".into()));
    }

    // Validate public key: must be valid base64 and 32 bytes (Ed25519).
    let pk_bytes = BASE64
        .decode(&req.public_key)
        .map_err(|_| AppError::Validation("invalid base64 public key".into()))?;

    if pk_bytes.len() != 32 {
        return Err(AppError::Validation(
            "public key must be 32 bytes (Ed25519)".into(),
        ));
    }

    // Verify it's a valid Ed25519 public key.
    let _vk = VerifyingKey::from_bytes(
        pk_bytes
            .as_slice()
            .try_into()
            .map_err(|_| AppError::Validation("invalid Ed25519 public key".into()))?,
    )
    .map_err(|_| AppError::Validation("invalid Ed25519 public key".into()))?;

    let fingerprint = public_key_fingerprint(&pk_bytes);

    // Replace any existing active credential for this agent.
    let new_cred_id = uuid::Uuid::new_v4().to_string();

    let active_creds = agent_credential::Entity::find()
        .filter(agent_credential::Column::AgentId.eq(&agent.id))
        .filter(agent_credential::Column::Status.eq(agent_credential::CredentialStatus::Active))
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    for old_cred in active_creds {
        let mut am: agent_credential::ActiveModel = old_cred.into();
        am.status = Set(agent_credential::CredentialStatus::Replaced);
        am.revoke_reason = Set(Some("replaced".into()));
        am.revoked_at = Set(Some(now));
        am.replaced_by_id = Set(Some(new_cred_id.clone()));
        am.update(&state.db).await.map_err(AppError::Db)?;
    }

    // Create new credential.
    let cred = agent_credential::ActiveModel {
        id: Set(new_cred_id.clone()),
        agent_id: Set(agent.id.clone()),
        public_key: Set(req.public_key),
        public_key_fp: Set(fingerprint.clone()),
        status: Set(agent_credential::CredentialStatus::Active),
        revoke_reason: Set(None),
        instance_label: Set(req.instance_label),
        issued_at: Set(now),
        last_used_at: Set(None),
        revoked_at: Set(None),
        replaced_by_id: Set(None),
    };
    cred.insert(&state.db).await.map_err(AppError::Db)?;

    // Mark claim code as used.
    let mut claim_am: claim_token::ActiveModel = claim.into();
    claim_am.status = Set(claim_token::ClaimStatus::Used);
    claim_am.used_at = Set(Some(now));
    claim_am.update(&state.db).await.map_err(AppError::Db)?;

    // Clear reauth_required flag on agent.
    let mut agent_am: agent::ActiveModel = agent.into();
    agent_am.reauth_required = Set(false);
    agent_am.updated_at = Set(now);
    agent_am.update(&state.db).await.map_err(AppError::Db)?;

    Ok(Json(ActivateCredentialResponse {
        credential_id: new_cred_id,
        public_key_fingerprint: fingerprint,
    }))
}

/// POST /api/auth/challenge
///
/// Agent requests a challenge nonce for authentication.
pub async fn challenge(
    State(state): State<AppState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, AppError> {
    // Verify agent exists.
    let _agent = agent::Entity::find_by_id(&req.agent_id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound(format!("agent '{}' not found", req.agent_id)))?;

    // Verify credential exists and is active.
    let _cred = agent_credential::Entity::find_by_id(&req.credential_id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::NotFound("credential not found".into()))?;

    if _cred.status != agent_credential::CredentialStatus::Active {
        return Err(AppError::Unauthorized("credential not active".into()));
    }

    if _cred.agent_id != req.agent_id {
        return Err(AppError::Unauthorized(
            "credential does not belong to agent".into(),
        ));
    }

    // Generate nonce and store in memory.
    let nonce = generate_challenge_nonce();
    let now = Utc::now();
    let expires_at = now + Duration::seconds(consts::CHALLENGE_NONCE_TTL_SECS as i64);

    let key = format!("{}:{}", req.agent_id, req.credential_id);
    let entry = crate::ChallengeEntry {
        nonce: nonce.clone(),
        expires_at,
    };

    {
        let mut store = state.challenges.write().await;
        store.insert(key, entry);
    }

    // Record auth event.
    let event = auth_event::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        agent_id: Set(req.agent_id),
        credential_id: Set(Some(req.credential_id)),
        event_type: Set("challenge_issued".into()),
        success: Set(true),
        reason: Set(None),
        source_ip: Set(None),
        client_name: Set(None),
        client_version: Set(None),
        instance_label: Set(None),
        created_at: Set(now),
    };
    event.insert(&state.db).await.map_err(AppError::Db)?;

    Ok(Json(ChallengeResponse {
        nonce,
        expires_at: expires_at.to_rfc3339(),
    }))
}

/// POST /api/auth/verify
///
/// Agent submits a signed challenge to get a JWT access token.
pub async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, AppError> {
    let now = Utc::now();

    // Look up the challenge nonce.
    let key = format!("{}:{}", req.agent_id, req.credential_id);
    let entry = {
        let mut store = state.challenges.write().await;
        store.remove(&key)
    };

    let entry = entry.ok_or_else(|| AppError::Unauthorized("no pending challenge".into()))?;

    // Verify nonce matches.
    if entry.nonce != req.nonce {
        record_auth_event(
            &state,
            &req.agent_id,
            Some(&req.credential_id),
            "auth_failed",
            false,
            Some("nonce mismatch"),
        )
        .await;
        let _ = crate::risk::assess_risk(&state.db, &req.agent_id, &req.credential_id).await;
        return Err(AppError::Unauthorized("invalid nonce".into()));
    }

    // Verify nonce not expired.
    if now > entry.expires_at {
        record_auth_event(
            &state,
            &req.agent_id,
            Some(&req.credential_id),
            "auth_failed",
            false,
            Some("nonce expired"),
        )
        .await;
        let _ = crate::risk::assess_risk(&state.db, &req.agent_id, &req.credential_id).await;
        return Err(AppError::Unauthorized("challenge expired".into()));
    }

    // Load credential.
    let cred = agent_credential::Entity::find_by_id(&req.credential_id)
        .one(&state.db)
        .await
        .map_err(AppError::Db)?
        .ok_or_else(|| AppError::Unauthorized("credential not found".into()))?;

    if cred.status != agent_credential::CredentialStatus::Active {
        record_auth_event(
            &state,
            &req.agent_id,
            Some(&req.credential_id),
            "auth_failed",
            false,
            Some("credential not active"),
        )
        .await;
        return Err(AppError::Unauthorized("credential not active".into()));
    }

    if cred.agent_id != req.agent_id {
        return Err(AppError::Unauthorized(
            "credential does not belong to agent".into(),
        ));
    }

    // Decode public key and verify signature.
    let pk_bytes = BASE64
        .decode(&cred.public_key)
        .map_err(|_| AppError::Internal("stored public key is invalid base64".into()))?;

    let vk = VerifyingKey::from_bytes(
        pk_bytes
            .as_slice()
            .try_into()
            .map_err(|_| AppError::Internal("stored public key is invalid".into()))?,
    )
    .map_err(|_| AppError::Internal("stored public key is invalid Ed25519".into()))?;

    let sig_bytes = BASE64
        .decode(&req.signature)
        .map_err(|_| AppError::Unauthorized("invalid base64 signature".into()))?;

    let signature = Signature::from_bytes(
        sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| AppError::Unauthorized("signature must be 64 bytes".into()))?,
    );

    // The signed message is the nonce.
    if vk.verify(req.nonce.as_bytes(), &signature).is_err() {
        record_auth_event(
            &state,
            &req.agent_id,
            Some(&req.credential_id),
            "auth_failed",
            false,
            Some("invalid signature"),
        )
        .await;
        let _ = crate::risk::assess_risk(&state.db, &req.agent_id, &req.credential_id).await;
        return Err(AppError::Unauthorized("invalid signature".into()));
    }

    // Signature verified! Issue JWT.
    let access_token = create_jwt(&req.agent_id, &req.credential_id, &state.jwt_secret)
        .map_err(AppError::Internal)?;

    let token_expires_at = now + Duration::seconds(consts::ACCESS_TOKEN_TTL_SECS as i64);

    // Update last_used_at on credential.
    let mut cred_am: agent_credential::ActiveModel = cred.into();
    cred_am.last_used_at = Set(Some(now));
    cred_am.update(&state.db).await.map_err(AppError::Db)?;

    // Record successful auth event.
    record_auth_event(
        &state,
        &req.agent_id,
        Some(&req.credential_id),
        "token_issued",
        true,
        None,
    )
    .await;

    Ok(Json(VerifyResponse {
        access_token,
        expires_at: token_expires_at.to_rfc3339(),
    }))
}

/// GET /api/agents/{id}/auth-events
///
/// Owner views recent authentication events for their agent.
pub async fn list_auth_events(
    session: UserSession,
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Vec<AuthEventResponse>>, AppError> {
    let _agent = find_owned_agent(&state.db, &agent_id, &session.user.id).await?;

    let events = auth_event::Entity::find()
        .filter(auth_event::Column::AgentId.eq(&agent_id))
        .order_by_desc(auth_event::Column::CreatedAt)
        .all(&state.db)
        .await
        .map_err(AppError::Db)?;

    let responses: Vec<AuthEventResponse> = events
        .into_iter()
        .map(|e| AuthEventResponse {
            id: e.id,
            credential_id: e.credential_id,
            event_type: e.event_type,
            success: e.success,
            reason: e.reason,
            created_at: e.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(responses))
}

/// Helper to record an auth event (best-effort, ignores DB errors).
async fn record_auth_event(
    state: &AppState,
    agent_id: &str,
    credential_id: Option<&str>,
    event_type: &str,
    success: bool,
    reason: Option<&str>,
) {
    let event = auth_event::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        agent_id: Set(agent_id.to_string()),
        credential_id: Set(credential_id.map(|s| s.to_string())),
        event_type: Set(event_type.to_string()),
        success: Set(success),
        reason: Set(reason.map(|s| s.to_string())),
        source_ip: Set(None),
        client_name: Set(None),
        client_version: Set(None),
        instance_label: Set(None),
        created_at: Set(Utc::now()),
    };
    let _ = event.insert(&state.db).await;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::extract::{Path, State};
    use axum::Json;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use ed25519_dalek::Signer;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use tokio::sync::RwLock;

    use crate::auth::token::{generate_claim_code, hash_claim_code, verify_jwt};
    use crate::consts;
    use crate::db;
    use crate::entity::{agent, agent_credential, auth_event, claim_token, user};
    use crate::AppState;

    const TEST_JWT_SECRET: &str = "test-jwt-secret-for-credentials";

    /// Helper: create a test user.
    async fn create_user(db: &sea_orm::DatabaseConnection, id: &str) {
        let now = Utc::now();
        let u = user::ActiveModel {
            id: Set(id.into()),
            github_id: Set(1),
            github_name: Set("testuser".into()),
            avatar_url: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        u.insert(db).await.unwrap();
    }

    /// Helper: create a test agent.
    async fn create_agent(
        db: &sea_orm::DatabaseConnection,
        agent_id: &str,
        user_id: &str,
    ) -> agent::Model {
        let now = Utc::now();
        let a = agent::ActiveModel {
            id: Set(agent_id.into()),
            user_id: Set(user_id.into()),
            name: Set("Test Agent".into()),
            reauth_required: Set(false),
            avatar_url: Set(None),
            bio: Set(None),
            status: Set(agent::AgentStatus::Active),
            created_at: Set(now),
            updated_at: Set(now),
        };
        a.insert(db).await.unwrap()
    }

    /// Helper: build a test AppState.
    fn test_state(db: sea_orm::DatabaseConnection) -> AppState {
        use crate::config::AppConfig;
        AppState {
            db,
            config: AppConfig {
                data_dir: None,
                port: 8900,
                github_client_id: String::new(),
                github_client_secret: String::new(),
                web_base_url: None,
                session_cookie_secure: false,
            },
            github_client: Arc::new(MockGitHubClient),
            connections: crate::ws::ConnectionRegistry::new(),
            jwt_secret: TEST_JWT_SECRET.into(),
            challenges: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    struct MockGitHubClient;

    #[async_trait::async_trait]
    impl crate::api::auth::GitHubClient for MockGitHubClient {
        async fn exchange_code(&self, _code: &str) -> Result<String, crate::error::AppError> {
            Ok("mock".into())
        }
        async fn get_user_info(
            &self,
            _access_token: &str,
        ) -> Result<crate::api::auth::GitHubUser, crate::error::AppError> {
            Ok(crate::api::auth::GitHubUser {
                id: 1,
                login: "mock".into(),
                avatar_url: None,
            })
        }
    }

    /// Helper: generate an Ed25519 keypair, return (signing_key, public_key_base64).
    fn generate_keypair() -> (SigningKey, String) {
        use rand::Rng;
        let mut rng = rand::rng();
        let secret_bytes: [u8; 32] = rng.random();
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let public_key = signing_key.verifying_key();
        let pk_base64 = BASE64.encode(public_key.as_bytes());
        (signing_key, pk_base64)
    }

    /// Helper: create a claim code for an agent and return the raw code.
    async fn create_claim_code(
        db: &sea_orm::DatabaseConnection,
        agent_id: &str,
        user_id: &str,
    ) -> String {
        let raw_code = generate_claim_code();
        let code_hash = hash_claim_code(&raw_code);
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(consts::CLAIM_CODE_TTL_SECS as i64);

        let claim = claim_token::ActiveModel {
            id: Set(uuid::Uuid::new_v4().to_string()),
            agent_id: Set(agent_id.into()),
            user_id: Set(user_id.into()),
            code_hash: Set(code_hash),
            purpose: Set(claim_token::ClaimPurpose::Enroll),
            status: Set(claim_token::ClaimStatus::Active),
            created_at: Set(now),
            expires_at: Set(expires_at),
            used_at: Set(None),
            created_from_ip: Set(None),
        };
        claim.insert(db).await.unwrap();
        raw_code
    }

    /// Helper: activate a credential and return (credential_id, signing_key).
    async fn activate_cred(
        db: &sea_orm::DatabaseConnection,
        agent_id: &str,
        public_key_base64: &str,
    ) -> String {
        let now = Utc::now();
        let pk_bytes = BASE64.decode(public_key_base64).unwrap();
        let fingerprint = crate::auth::token::public_key_fingerprint(&pk_bytes);
        let cred_id = uuid::Uuid::new_v4().to_string();

        let cred = agent_credential::ActiveModel {
            id: Set(cred_id.clone()),
            agent_id: Set(agent_id.into()),
            public_key: Set(public_key_base64.into()),
            public_key_fp: Set(fingerprint),
            status: Set(agent_credential::CredentialStatus::Active),
            revoke_reason: Set(None),
            instance_label: Set(None),
            issued_at: Set(now),
            last_used_at: Set(None),
            revoked_at: Set(None),
            replaced_by_id: Set(None),
        };
        cred.insert(db).await.unwrap();
        cred_id
    }

    // ── Test: claim code generation flow ──

    #[tokio::test]
    async fn claim_generates_code_and_stores_hash() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let raw_code = create_claim_code(&db, "test-agent", "u1").await;

        // Code has correct prefix.
        assert!(raw_code.starts_with(consts::CLAIM_CODE_PREFIX));

        // Hash is stored in DB.
        let code_hash = hash_claim_code(&raw_code);
        let found = claim_token::Entity::find()
            .filter(claim_token::Column::AgentId.eq("test-agent"))
            .filter(claim_token::Column::CodeHash.eq(&code_hash))
            .one(&db)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().status, claim_token::ClaimStatus::Active);
    }

    // ── Test: activate credential via claim code ──

    #[tokio::test]
    async fn activate_credential_full_flow() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let raw_code = create_claim_code(&db, "test-agent", "u1").await;
        let (_signing_key, pk_base64) = generate_keypair();

        let state = test_state(db.clone());

        // Call activate.
        let req = super::super::dto::ActivateCredentialRequest {
            claim_code: raw_code.clone(),
            public_key: pk_base64.clone(),
            instance_label: Some("test-instance".into()),
        };

        let result = super::activate_credential(
            State(state),
            Path("test-agent".into()),
            Json(req),
        )
        .await
        .unwrap();

        let resp = result.0;
        assert!(!resp.credential_id.is_empty());
        assert_eq!(resp.public_key_fingerprint.len(), 16);

        // Claim code should now be used.
        let code_hash = hash_claim_code(&raw_code);
        let claim = claim_token::Entity::find()
            .filter(claim_token::Column::CodeHash.eq(&code_hash))
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.status, claim_token::ClaimStatus::Used);

        // Credential should be active.
        let cred = agent_credential::Entity::find_by_id(&resp.credential_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cred.status, agent_credential::CredentialStatus::Active);
        assert_eq!(cred.instance_label, Some("test-instance".into()));
    }

    // ── Test: activate replaces old credential ──

    #[tokio::test]
    async fn activate_replaces_old_credential() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        // Create first credential directly.
        let (_sk1, pk1) = generate_keypair();
        let old_cred_id = activate_cred(&db, "test-agent", &pk1).await;

        // Create claim code and activate new credential.
        let raw_code = create_claim_code(&db, "test-agent", "u1").await;
        let (_sk2, pk2) = generate_keypair();

        let state = test_state(db.clone());
        let req = super::super::dto::ActivateCredentialRequest {
            claim_code: raw_code,
            public_key: pk2,
            instance_label: None,
        };

        let result = super::activate_credential(
            State(state),
            Path("test-agent".into()),
            Json(req),
        )
        .await
        .unwrap();

        // Old credential should be replaced.
        let old_cred = agent_credential::Entity::find_by_id(&old_cred_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old_cred.status, agent_credential::CredentialStatus::Replaced);
        assert_eq!(old_cred.replaced_by_id, Some(result.0.credential_id.clone()));
    }

    // ── Test: invalid claim code rejected ──

    #[tokio::test]
    async fn activate_invalid_claim_code_rejected() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let (_sk, pk) = generate_keypair();
        let state = test_state(db.clone());

        let req = super::super::dto::ActivateCredentialRequest {
            claim_code: "clm_invalid_code_that_does_not_exist".into(),
            public_key: pk,
            instance_label: None,
        };

        let result = super::activate_credential(
            State(state),
            Path("test-agent".into()),
            Json(req),
        )
        .await;

        assert!(result.is_err());
    }

    // ── Test: challenge/verify full flow ──

    #[tokio::test]
    async fn challenge_verify_full_flow() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let (signing_key, pk_base64) = generate_keypair();
        let cred_id = activate_cred(&db, "test-agent", &pk_base64).await;

        let state = test_state(db.clone());

        // Step 1: Request challenge.
        let challenge_req = super::super::dto::ChallengeRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
        };

        let challenge_resp =
            super::challenge(State(state.clone()), Json(challenge_req))
                .await
                .unwrap();

        let nonce = &challenge_resp.0.nonce;
        assert!(!nonce.is_empty());

        // Step 2: Sign the nonce.
        let signature = signing_key.sign(nonce.as_bytes());
        let sig_base64 = BASE64.encode(signature.to_bytes());

        // Step 3: Verify.
        let verify_req = super::super::dto::VerifyRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
            nonce: nonce.clone(),
            signature: sig_base64,
        };

        let verify_resp =
            super::verify(State(state.clone()), Json(verify_req))
                .await
                .unwrap();

        let access_token = &verify_resp.0.access_token;
        assert!(!access_token.is_empty());

        // Verify the JWT is valid.
        let claims = verify_jwt(access_token, TEST_JWT_SECRET).unwrap();
        assert_eq!(claims.sub, "test-agent");
        assert_eq!(claims.cid, cred_id);

        // Verify auth event was recorded.
        let events = auth_event::Entity::find()
            .filter(auth_event::Column::AgentId.eq("test-agent"))
            .filter(auth_event::Column::EventType.eq("token_issued"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].success);
    }

    // ── Test: wrong signature rejected ──

    #[tokio::test]
    async fn verify_wrong_signature_rejected() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let (_signing_key, pk_base64) = generate_keypair();
        let cred_id = activate_cred(&db, "test-agent", &pk_base64).await;

        let state = test_state(db.clone());

        // Request challenge.
        let challenge_req = super::super::dto::ChallengeRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
        };

        let challenge_resp =
            super::challenge(State(state.clone()), Json(challenge_req))
                .await
                .unwrap();

        let nonce = &challenge_resp.0.nonce;

        // Sign with a DIFFERENT key.
        let (wrong_sk, _) = generate_keypair();
        let wrong_sig = wrong_sk.sign(nonce.as_bytes());
        let wrong_sig_base64 = BASE64.encode(wrong_sig.to_bytes());

        let verify_req = super::super::dto::VerifyRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id,
            nonce: nonce.clone(),
            signature: wrong_sig_base64,
        };

        let result = super::verify(State(state), Json(verify_req)).await;
        assert!(result.is_err());
    }

    // ── Test: nonce replay rejected ──

    #[tokio::test]
    async fn verify_nonce_replay_rejected() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let (signing_key, pk_base64) = generate_keypair();
        let cred_id = activate_cred(&db, "test-agent", &pk_base64).await;

        let state = test_state(db.clone());

        // Request challenge.
        let challenge_req = super::super::dto::ChallengeRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
        };

        let challenge_resp =
            super::challenge(State(state.clone()), Json(challenge_req))
                .await
                .unwrap();

        let nonce = challenge_resp.0.nonce.clone();
        let signature = signing_key.sign(nonce.as_bytes());
        let sig_base64 = BASE64.encode(signature.to_bytes());

        // First verify — should succeed.
        let verify_req = super::super::dto::VerifyRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
            nonce: nonce.clone(),
            signature: sig_base64.clone(),
        };

        let result = super::verify(State(state.clone()), Json(verify_req)).await;
        assert!(result.is_ok());

        // Second verify with same nonce — should fail (nonce consumed).
        let verify_req2 = super::super::dto::VerifyRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id,
            nonce,
            signature: sig_base64,
        };

        let result2 = super::verify(State(state), Json(verify_req2)).await;
        assert!(result2.is_err());
    }

    // ── Test: revoked credential cannot get challenge ──

    #[tokio::test]
    async fn challenge_revoked_credential_rejected() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let (_sk, pk_base64) = generate_keypair();
        let cred_id = activate_cred(&db, "test-agent", &pk_base64).await;

        // Revoke the credential.
        let cred = agent_credential::Entity::find_by_id(&cred_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut am: agent_credential::ActiveModel = cred.into();
        am.status = Set(agent_credential::CredentialStatus::Revoked);
        am.update(&db).await.unwrap();

        let state = test_state(db.clone());

        let challenge_req = super::super::dto::ChallengeRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id,
        };

        let result = super::challenge(State(state), Json(challenge_req)).await;
        assert!(result.is_err());
    }

    // ── Test: credential last_used_at updated after verify ──

    #[tokio::test]
    async fn verify_updates_last_used_at() {
        let db = db::test_db().await;
        create_user(&db, "u1").await;
        create_agent(&db, "test-agent", "u1").await;

        let (signing_key, pk_base64) = generate_keypair();
        let cred_id = activate_cred(&db, "test-agent", &pk_base64).await;

        // Verify last_used_at is None initially.
        let cred = agent_credential::Entity::find_by_id(&cred_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(cred.last_used_at.is_none());

        let state = test_state(db.clone());

        // Challenge + verify.
        let challenge_req = super::super::dto::ChallengeRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
        };
        let challenge_resp =
            super::challenge(State(state.clone()), Json(challenge_req))
                .await
                .unwrap();

        let nonce = &challenge_resp.0.nonce;
        let signature = signing_key.sign(nonce.as_bytes());
        let sig_base64 = BASE64.encode(signature.to_bytes());

        let verify_req = super::super::dto::VerifyRequest {
            agent_id: "test-agent".into(),
            credential_id: cred_id.clone(),
            nonce: nonce.clone(),
            signature: sig_base64,
        };

        let _ = super::verify(State(state), Json(verify_req))
            .await
            .unwrap();

        // Verify last_used_at is now set.
        let cred = agent_credential::Entity::find_by_id(&cred_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(cred.last_used_at.is_some());
    }
}
