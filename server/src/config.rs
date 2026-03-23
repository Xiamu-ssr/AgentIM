use std::path::PathBuf;

use clap::Parser;

use crate::consts;

/// AgentIM Server configuration.
#[derive(Parser, Debug, Clone)]
#[command(name = "agentim-server", about = "AgentIM Server — IM for AI Agents")]
pub struct AppConfig {
    /// Data directory for SQLite and runtime files.
    #[arg(long, env = "AGENTIM_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    /// HTTP port to listen on.
    #[arg(long, env = "AGENTIM_PORT", default_value_t = consts::DEFAULT_PORT)]
    pub port: u16,

    /// GitHub OAuth client ID.
    #[arg(long, env = "GITHUB_CLIENT_ID", default_value = "")]
    pub github_client_id: String,

    /// GitHub OAuth client secret.
    #[arg(long, env = "GITHUB_CLIENT_SECRET", default_value = "")]
    pub github_client_secret: String,

    /// Public web URL to redirect to after OAuth succeeds.
    #[arg(long, env = "AGENTIM_WEB_BASE_URL")]
    pub web_base_url: Option<String>,

    /// Whether session cookies require HTTPS.
    #[arg(
        long,
        env = "AGENTIM_SESSION_COOKIE_SECURE",
        default_value_t = false
    )]
    pub session_cookie_secure: bool,
}

impl AppConfig {
    /// Resolve the data directory to an absolute path.
    /// Priority: --data-dir > AGENTIM_DATA_DIR > ~/.agentim/
    pub fn resolved_data_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.data_dir {
            dir.clone()
        } else {
            dirs::home_dir()
                .expect("cannot determine home directory")
                .join(consts::DEFAULT_DATA_DIR_NAME)
        }
    }

    /// Full path to the SQLite database file.
    pub fn db_path(&self) -> PathBuf {
        self.resolved_data_dir().join(consts::DB_FILENAME)
    }

    /// Where the browser should land after OAuth succeeds.
    pub fn auth_redirect_url(&self) -> &str {
        self.web_base_url.as_deref().unwrap_or("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_is_under_home() {
        let config = AppConfig {
            data_dir: None,
            port: 8900,
            github_client_id: String::new(),
            github_client_secret: String::new(),
            web_base_url: None,
            session_cookie_secure: false,
        };
        let dir = config.resolved_data_dir();
        assert!(dir.ends_with(consts::DEFAULT_DATA_DIR_NAME));
    }

    #[test]
    fn custom_data_dir_is_respected() {
        let config = AppConfig {
            data_dir: Some(PathBuf::from("/tmp/agentim-test")),
            port: 8900,
            github_client_id: String::new(),
            github_client_secret: String::new(),
            web_base_url: None,
            session_cookie_secure: false,
        };
        assert_eq!(config.resolved_data_dir(), PathBuf::from("/tmp/agentim-test"));
    }

    #[test]
    fn db_path_is_under_data_dir() {
        let config = AppConfig {
            data_dir: Some(PathBuf::from("/tmp/agentim-test")),
            port: 8900,
            github_client_id: String::new(),
            github_client_secret: String::new(),
            web_base_url: None,
            session_cookie_secure: false,
        };
        assert_eq!(
            config.db_path(),
            PathBuf::from("/tmp/agentim-test/agentim.db")
        );
    }

    #[test]
    fn auth_redirect_defaults_to_root() {
        let config = AppConfig {
            data_dir: None,
            port: 8900,
            github_client_id: String::new(),
            github_client_secret: String::new(),
            web_base_url: None,
            session_cookie_secure: false,
        };
        assert_eq!(config.auth_redirect_url(), "/");
    }

    #[test]
    fn auth_redirect_uses_configured_web_base_url() {
        let config = AppConfig {
            data_dir: None,
            port: 8900,
            github_client_id: String::new(),
            github_client_secret: String::new(),
            web_base_url: Some("http://127.0.0.1:3000".into()),
            session_cookie_secure: false,
        };
        assert_eq!(config.auth_redirect_url(), "http://127.0.0.1:3000");
    }
}
