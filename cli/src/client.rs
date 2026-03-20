use anyhow::{Context, Result, bail};
use serde_json::Value;

pub struct ApiClient {
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

#[allow(dead_code)]
impl ApiClient {
    pub fn new(base_url: &str, token: Option<&str>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.map(|t| t.to_string()),
            http: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth_header(&self) -> Result<String> {
        match &self.token {
            Some(t) => Ok(format!("Bearer {}", t)),
            None => bail!("no token configured — run `agentim config set token <TOKEN>`"),
        }
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
        let mut body = serde_json::json!({ "id": id, "name": name });
        if let Some(b) = bio {
            body["bio"] = Value::String(b.to_string());
        }
        // create_agent uses session auth (cookie), but from CLI we try Bearer
        let resp = self
            .http
            .post(self.url("/api/agents"))
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn list_agents(&self) -> Result<Value> {
        let resp = self
            .http
            .get(self.url("/api/agents"))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn get_agent(&self, id: &str) -> Result<Value> {
        let resp = self
            .http
            .get(self.url(&format!("/api/agents/{}", id)))
            .header("Authorization", self.auth_header()?)
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
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn delete_agent(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/api/agents/{}", id)))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn reset_token(&self, id: &str) -> Result<Value> {
        let resp = self
            .http
            .post(self.url(&format!("/api/agents/{}/token/reset", id)))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    // ── Contacts ──

    pub async fn add_contact(&self, contact_id: &str, alias: Option<&str>) -> Result<Value> {
        let mut body = serde_json::json!({ "contact_id": contact_id });
        if let Some(a) = alias {
            body["alias"] = Value::String(a.to_string());
        }
        let resp = self
            .http
            .post(self.url("/api/contacts"))
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn list_contacts(&self) -> Result<Value> {
        let resp = self
            .http
            .get(self.url("/api/contacts"))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn remove_contact(&self, contact_id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/api/contacts/{}", contact_id)))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    // ── Messages ──

    pub async fn send_message(&self, to_agent: &str, content: &str) -> Result<Value> {
        let body = serde_json::json!({
            "to_agent": to_agent,
            "content": content,
        });
        let resp = self
            .http
            .post(self.url("/api/messages"))
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn inbox(&self) -> Result<Value> {
        let resp = self
            .http
            .get(self.url("/api/messages/inbox"))
            .header("Authorization", self.auth_header()?)
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
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn mark_read(&self, message_id: &str) -> Result<()> {
        let resp = self
            .http
            .post(self.url(&format!("/api/messages/{}/read", message_id)))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn mark_all_read(&self) -> Result<()> {
        let resp = self
            .http
            .post(self.url("/api/messages/read-all"))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn search_messages(&self, query: &str) -> Result<Value> {
        let resp = self
            .http
            .get(self.url(&format!("/api/messages/search?q={}", urlencod(query))))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    // ── Channels ──

    pub async fn create_channel(&self, name: &str) -> Result<Value> {
        let body = serde_json::json!({ "name": name });
        let resp = self
            .http
            .post(self.url("/api/channels"))
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn list_channels(&self) -> Result<Value> {
        let resp = self
            .http
            .get(self.url("/api/channels"))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn get_channel(&self, id: &str) -> Result<Value> {
        let resp = self
            .http
            .get(self.url(&format!("/api/channels/{}", id)))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn invite_member(&self, channel_id: &str, agent_id: &str) -> Result<Value> {
        let body = serde_json::json!({ "agent_id": agent_id });
        let resp = self
            .http
            .post(self.url(&format!("/api/channels/{}/members", channel_id)))
            .header("Authorization", self.auth_header()?)
            .json(&body)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    pub async fn remove_member(&self, channel_id: &str, agent_id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!(
                "/api/channels/{}/members/{}",
                channel_id, agent_id
            )))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn close_channel(&self, channel_id: &str) -> Result<()> {
        let resp = self
            .http
            .post(self.url(&format!("/api/channels/{}/close", channel_id)))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_status(resp).await
    }

    pub async fn send_channel_message(&self, channel_id: &str, content: &str) -> Result<Value> {
        let body = serde_json::json!({ "content": content });
        let resp = self
            .http
            .post(self.url(&format!("/api/channels/{}/messages", channel_id)))
            .header("Authorization", self.auth_header()?)
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
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", self.auth_header()?)
            .send()
            .await
            .context("failed to connect to server")?;
        self.check_response(resp).await
    }

    /// Get the WebSocket URL for the listen command.
    pub fn ws_url(&self) -> Result<String> {
        let token = self.token.as_ref().ok_or_else(|| {
            anyhow::anyhow!("no token configured — run `agentim config set token <TOKEN>`")
        })?;
        let ws_base = self
            .base_url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        Ok(format!("{}/ws?token={}", ws_base, urlencod(token)))
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
