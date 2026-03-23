use anyhow::{Context, Result, bail};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value;
use std::sync::Mutex;

pub struct ApiClient {
    base_url: String,
    agent_id: String,
    credential_id: String,
    signing_key: SigningKey,
    http: reqwest::Client,
    /// Cached JWT access token.
    cached_jwt: Mutex<Option<CachedToken>>,
}

struct CachedToken {
    token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[allow(dead_code)]
impl ApiClient {
    pub fn new(
        base_url: &str,
        agent_id: &str,
        credential_id: &str,
        signing_key: SigningKey,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent_id: agent_id.to_string(),
            credential_id: credential_id.to_string(),
            signing_key,
            http: reqwest::Client::new(),
            cached_jwt: Mutex::new(None),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Get a valid JWT, using cache if not expired, otherwise doing challenge/verify.
    async fn get_jwt(&self) -> Result<String> {
        // Check cache (with 30s safety margin).
        {
            let cache = self.cached_jwt.lock().unwrap();
            if let Some(ref ct) = *cache {
                let margin = chrono::Duration::seconds(30);
                if ct.expires_at - margin > chrono::Utc::now() {
                    return Ok(ct.token.clone());
                }
            }
        }

        // Do challenge/verify.
        let jwt = self.authenticate().await?;
        Ok(jwt)
    }

    /// Perform challenge/verify handshake and cache the resulting JWT.
    async fn authenticate(&self) -> Result<String> {
        // 1. Request challenge nonce.
        let challenge_body = serde_json::json!({
            "agent_id": self.agent_id,
            "credential_id": self.credential_id,
        });
        let resp = self
            .http
            .post(self.url("/api/auth/challenge"))
            .json(&challenge_body)
            .send()
            .await
            .context("failed to request challenge")?;
        let challenge: Value = self.check_response(resp).await?;

        let nonce = challenge["nonce"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no nonce in challenge response"))?;

        // 2. Sign the nonce with Ed25519.
        let signature = self.signing_key.sign(nonce.as_bytes());
        let sig_base64 = BASE64.encode(signature.to_bytes());

        // 3. Verify (exchange signature for JWT).
        let verify_body = serde_json::json!({
            "agent_id": self.agent_id,
            "credential_id": self.credential_id,
            "nonce": nonce,
            "signature": sig_base64,
        });
        let resp = self
            .http
            .post(self.url("/api/auth/verify"))
            .json(&verify_body)
            .send()
            .await
            .context("failed to verify signature")?;
        let verify_resp: Value = self.check_response(resp).await?;

        let token = verify_resp["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no access_token in verify response"))?
            .to_string();

        let expires_at_str = verify_resp["expires_at"]
            .as_str()
            .unwrap_or("");
        let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now() + chrono::Duration::minutes(9));

        // Cache the JWT.
        {
            let mut cache = self.cached_jwt.lock().unwrap();
            *cache = Some(CachedToken {
                token: token.clone(),
                expires_at,
            });
        }

        Ok(token)
    }

    fn auth_header_from_jwt(jwt: &str) -> String {
        format!("Bearer {}", jwt)
    }

    async fn check_response(&self, resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if status.is_success() {
            if body.is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_str(&body).context("failed to parse response JSON")
        } else {
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or(body);
            bail!("HTTP {} — {}", status.as_u16(), detail)
        }
    }

    async fn check_status(&self, resp: reqwest::Response) -> Result<()> {
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or(body);
            bail!("HTTP {} — {}", status.as_u16(), detail)
        }
    }

    // ── Agents ──

    pub async fn create_agent(
        &self,
        id: &str,
        name: &str,
        bio: Option<&str>,
    ) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let mut body = serde_json::json!({ "id": id, "name": name });
        if let Some(b) = bio {
            body["bio"] = Value::String(b.to_string());
        }
        let resp = self
            .http
            .post(self.url("/api/agents"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn list_agents(&self) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url("/api/agents"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn get_agent(&self, id: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url(&format!("/api/agents/{}", id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn update_agent(
        &self,
        id: &str,
        name: Option<&str>,
        bio: Option<&str>,
    ) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let mut body = serde_json::json!({});
        if let Some(n) = name {
            body["name"] = Value::String(n.to_string());
        }
        if let Some(b) = bio {
            body["bio"] = Value::String(b.to_string());
        }
        let resp = self
            .http
            .put(self.url(&format!("/api/agents/{}", id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn delete_agent(&self, id: &str) -> Result<()> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .delete(self.url(&format!("/api/agents/{}", id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    // ── Contacts ──

    pub async fn add_contact(&self, contact_id: &str, alias: Option<&str>) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let mut body = serde_json::json!({ "contact_id": contact_id });
        if let Some(a) = alias {
            body["alias"] = Value::String(a.to_string());
        }
        let resp = self
            .http
            .post(self.url("/api/contacts"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn list_contacts(&self) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url("/api/contacts"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn remove_contact(&self, contact_id: &str) -> Result<()> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .delete(self.url(&format!("/api/contacts/{}", contact_id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    // ── Messages ──

    pub async fn send_message(&self, to_agent: &str, content: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let body = serde_json::json!({
            "to_agent": to_agent,
            "content": content,
        });
        let resp = self
            .http
            .post(self.url("/api/messages"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn inbox(&self) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url("/api/messages/inbox"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn chat_history(
        &self,
        agent_id: &str,
        limit: Option<u32>,
        before: Option<&str>,
    ) -> Result<Value> {
        let mut url = format!("/api/messages/with/{}", agent_id);
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        if let Some(b) = before {
            params.push(format!("before={}", b));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn mark_read(&self, message_id: &str) -> Result<()> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .post(self.url(&format!("/api/messages/{}/read", message_id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn mark_all_read(&self) -> Result<()> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .post(self.url("/api/messages/read-all"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn search_messages(&self, query: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url(&format!("/api/messages/search?q={}", urlencod(query))))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    // ── Channels ──

    pub async fn create_channel(&self, name: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let body = serde_json::json!({ "name": name });
        let resp = self
            .http
            .post(self.url("/api/channels"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn list_channels(&self) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url("/api/channels"))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn get_channel(&self, id: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url(&format!("/api/channels/{}", id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn invite_member(&self, channel_id: &str, agent_id: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let body = serde_json::json!({ "agent_id": agent_id });
        let resp = self
            .http
            .post(self.url(&format!("/api/channels/{}/members", channel_id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn remove_member(&self, channel_id: &str, agent_id: &str) -> Result<()> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .delete(self.url(&format!(
                "/api/channels/{}/members/{}",
                channel_id, agent_id
            )))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn close_channel(&self, channel_id: &str) -> Result<()> {
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .post(self.url(&format!("/api/channels/{}/close", channel_id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn send_channel_message(&self, channel_id: &str, content: &str) -> Result<Value> {
        let jwt = self.get_jwt().await?;
        let body = serde_json::json!({ "content": content });
        let resp = self
            .http
            .post(self.url(&format!("/api/channels/{}/messages", channel_id)))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn channel_messages(
        &self,
        channel_id: &str,
        limit: Option<u32>,
        before: Option<&str>,
    ) -> Result<Value> {
        let mut url = format!("/api/channels/{}/messages", channel_id);
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        if let Some(b) = before {
            params.push(format!("before={}", b));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }
        let jwt = self.get_jwt().await?;
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", Self::auth_header_from_jwt(&jwt))
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    /// Get a fresh JWT for WebSocket connection.
    pub async fn ws_url(&self) -> Result<String> {
        let jwt = self.get_jwt().await?;
        let ws_base = self
            .base_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        Ok(format!("{}/ws?token={}", ws_base, urlencod(&jwt)))
    }

    /// Activate a credential using a claim code (used during `init`).
    /// This is a static method that doesn't need an authenticated client.
    pub async fn activate_credential(
        base_url: &str,
        agent_id: &str,
        claim_code: &str,
        public_key_base64: &str,
        instance_label: Option<&str>,
    ) -> Result<Value> {
        let http = reqwest::Client::new();
        let url = format!(
            "{}/api/agents/{}/credentials/activate",
            base_url.trim_end_matches('/'),
            agent_id
        );
        let mut body = serde_json::json!({
            "claim_code": claim_code,
            "public_key": public_key_base64,
        });
        if let Some(label) = instance_label {
            body["instance_label"] = Value::String(label.to_string());
        }
        let resp = http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("failed to activate credential")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if status.is_success() {
            serde_json::from_str(&text).context("failed to parse activation response")
        } else {
            let detail = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or(text);
            bail!("HTTP {} — {}", status.as_u16(), detail)
        }
    }
}

/// Minimal percent-encoding for query parameters.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}
