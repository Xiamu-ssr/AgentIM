pub mod agents;
pub mod auth;
pub mod channels;
pub mod contacts;
pub mod credentials;
pub mod dto;
pub mod messages;

use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::AppState;

/// Build the API router with all agent endpoints.
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/agents", post(agents::create_agent))
        .route("/api/agents", get(agents::list_agents))
        .route("/api/agents/{id}", get(agents::get_agent))
        .route("/api/agents/{id}", put(agents::update_agent))
        .route("/api/agents/{id}", delete(agents::delete_agent))
        .route("/api/contacts", post(contacts::add_contact))
        .route("/api/contacts", get(contacts::list_contacts))
        .route(
            "/api/contacts/{contact_id}",
            delete(contacts::remove_contact),
        )
        .route("/api/auth/github", get(auth::github_auth))
        .route(
            "/api/auth/github/callback",
            get(auth::github_callback),
        )
        .route("/api/auth/me", get(auth::me))
        // Credentials (claim + challenge/verify)
        .route(
            "/api/agents/{id}/claim",
            post(credentials::generate_claim),
        )
        .route(
            "/api/agents/{id}/credentials/activate",
            post(credentials::activate_credential),
        )
        .route("/api/auth/challenge", post(credentials::challenge))
        .route("/api/auth/verify", post(credentials::verify))
        .route(
            "/api/agents/{id}/auth-events",
            get(credentials::list_auth_events),
        )
        // Messages (DM)
        .route("/api/messages", post(messages::send_message))
        .route("/api/messages/inbox", get(messages::inbox))
        .route("/api/messages/with/{agent_id}", get(messages::chat_history))
        .route("/api/messages/{id}/read", post(messages::mark_read))
        .route("/api/messages/read-all", post(messages::mark_all_read))
        .route("/api/messages/search", get(messages::search))
        // Channels (Group)
        .route("/api/channels", post(channels::create_channel))
        .route("/api/channels", get(channels::list_channels))
        .route("/api/channels/{id}", get(channels::get_channel))
        .route(
            "/api/channels/{id}/members",
            post(channels::invite_member),
        )
        .route(
            "/api/channels/{id}/members/{agent_id}",
            delete(channels::remove_member),
        )
        .route(
            "/api/channels/{id}/close",
            post(channels::close_channel),
        )
        .route(
            "/api/channels/{id}/messages",
            post(channels::send_channel_message),
        )
        .route(
            "/api/channels/{id}/messages",
            get(channels::list_channel_messages),
        )
}
