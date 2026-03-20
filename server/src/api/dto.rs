use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Requests ──

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub id: String,
    pub name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

// ── Responses ──

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct CreateAgentResponse {
    pub id: String,
    pub name: String,
    pub token: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct ResetTokenResponse {
    pub token: String,
}

// ── Contacts ──

#[derive(Debug, Deserialize)]
pub struct AddContactRequest {
    pub contact_id: String,
    pub alias: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct ContactResponse {
    pub contact_id: String,
    pub alias: Option<String>,
    pub agent_name: String,
    pub created_at: String,
}

// ── Messages ──

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub to_agent: String,
    pub content: String,
    pub msg_type: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct MessageResponse {
    pub id: String,
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub channel_id: Option<String>,
    pub content: String,
    pub msg_type: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatHistoryParams {
    pub before: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
}

// ── Auth ──

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct MeResponse {
    pub id: String,
    pub github_name: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
}

// ── Channels ──

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct ChannelResponse {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub is_closed: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct ChannelDetailResponse {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub is_closed: bool,
    pub created_at: String,
    pub members: Vec<ChannelMemberResponse>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../frontend/src/api/types.generated.ts")]
pub struct ChannelMemberResponse {
    pub agent_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Deserialize)]
pub struct InviteMemberRequest {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SendChannelMessageRequest {
    pub content: String,
    pub msg_type: Option<String>,
}
