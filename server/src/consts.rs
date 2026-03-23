/// Default HTTP port for the server.
pub const DEFAULT_PORT: u16 = 8900;

/// Default data directory name (relative to home dir).
pub const DEFAULT_DATA_DIR_NAME: &str = ".agentim";

/// SQLite database filename.
pub const DB_FILENAME: &str = "agentim.db";

/// Maximum number of agents per user.
pub const MAX_AGENTS_PER_USER: usize = 50;

/// Default page size for paginated queries.
#[allow(dead_code)]
pub const DEFAULT_PAGE_SIZE: u32 = 50;

/// Maximum page size for paginated queries.
#[allow(dead_code)]
pub const MAX_PAGE_SIZE: u32 = 100;

/// Minimum length for agent ID.
pub const AGENT_ID_MIN_LEN: usize = 3;

/// Maximum length for agent ID.
pub const AGENT_ID_MAX_LEN: usize = 50;

/// Session cookie name.
#[allow(dead_code)]
pub const SESSION_COOKIE_NAME: &str = "agentim_session";

/// Challenge nonce time-to-live (seconds). LLM agents are slow, 5 min.
#[allow(dead_code)]
pub const CHALLENGE_NONCE_TTL_SECS: u64 = 300;

/// JWT access token time-to-live (seconds).
#[allow(dead_code)]
pub const ACCESS_TOKEN_TTL_SECS: u64 = 600;

/// Claim code time-to-live (seconds).
#[allow(dead_code)]
pub const CLAIM_CODE_TTL_SECS: u64 = 600;

/// Claim code prefix.
#[allow(dead_code)]
pub const CLAIM_CODE_PREFIX: &str = "clm_";

/// Number of random bytes for claim code generation.
#[allow(dead_code)]
pub const CLAIM_CODE_RANDOM_BYTES: usize = 32;
