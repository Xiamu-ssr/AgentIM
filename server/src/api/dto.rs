use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Requests ──

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct CreateAgentRequest {
    pub id: String,
    pub name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

// ── Responses ──

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct CreateAgentResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

// ── Contacts ──

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct AddContactRequest {
    pub contact_id: String,
    pub alias: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ContactResponse {
    pub contact_id: String,
    pub alias: Option<String>,
    pub agent_name: String,
    pub is_blocked: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct InboxSummaryEntry {
    pub from_agent: String,
    pub agent_name: String,
    pub unread_count: u32,
}

// ── Messages ──

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct SendMessageRequest {
    pub to_agent: String,
    pub content: String,
    pub msg_type: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct MessageResponse {
    pub id: String,
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub channel_id: Option<String>,
    pub content: String,
    pub msg_type: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ChatHistoryParams {
    pub before: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct SearchParams {
    pub q: String,
}

// ── Auth ──

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct MeResponse {
    pub id: String,
    pub github_name: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
}

// ── Channels ──

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct CreateChannelRequest {
    pub name: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ChannelResponse {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub is_closed: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ChannelDetailResponse {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub is_closed: bool,
    pub created_at: String,
    pub members: Vec<ChannelMemberResponse>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ChannelMemberResponse {
    pub agent_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct InviteMemberRequest {
    pub agent_id: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct SendChannelMessageRequest {
    pub content: String,
    pub msg_type: Option<String>,
}

// ── Credentials ──

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ClaimCodeResponse {
    pub claim_code: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ActivateCredentialRequest {
    pub claim_code: String,
    pub public_key: String,
    pub instance_label: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ActivateCredentialResponse {
    pub credential_id: String,
    pub public_key_fingerprint: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ChallengeRequest {
    pub agent_id: String,
    pub credential_id: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct ChallengeResponse {
    pub nonce: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct VerifyRequest {
    pub agent_id: String,
    pub credential_id: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct VerifyResponse {
    pub access_token: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../frontend/src/api/types.generated.ts")]
pub struct AuthEventResponse {
    pub id: String,
    pub credential_id: Option<String>,
    pub event_type: String,
    pub success: bool,
    pub reason: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use crate::consts;

    /// Generate `constants.generated.ts` — the cross-language constants file.
    /// Runs alongside ts-rs type generation during `cargo test`.
    #[test]
    fn export_constants() {
        let content = format!(
            r#"// This file was generated by cargo test. Do not edit manually.

export const AGENT_ID_MIN_LEN = {min};
export const AGENT_ID_MAX_LEN = {max};
export const AGENT_ID_PATTERN = /^[a-z0-9-]+$/;
export const HEADER_AGENT_ID = "{header}";
"#,
            min = consts::AGENT_ID_MIN_LEN,
            max = consts::AGENT_ID_MAX_LEN,
            header = consts::HEADER_AGENT_ID,
        );
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../frontend/src/api/constants.generated.ts");
        std::fs::write(out, content).unwrap();
    }
}
