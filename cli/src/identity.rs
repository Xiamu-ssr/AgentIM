//! Local identity management.
//!
//! Each workspace has `.agentim/` containing:
//! - `identity.toml` — agent_id, credential_id, server URL
//! - `private_key.pem` — Ed25519 signing key (0600)

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

const DIR_NAME: &str = ".agentim";
const IDENTITY_FILE: &str = "identity.toml";
const KEY_FILE: &str = "private_key.pem";

#[derive(Debug, Serialize, Deserialize)]
pub struct Identity {
    pub server: String,
    pub agent_id: String,
    pub credential_id: String,
}

/// Return the `.agentim/` directory in the current working directory.
pub fn identity_dir() -> PathBuf {
    PathBuf::from(DIR_NAME)
}

/// Load identity from `.agentim/identity.toml`.
pub fn load_identity() -> Result<Identity> {
    let dir = identity_dir();
    let path = dir.join(IDENTITY_FILE);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("no identity found at {} — run `agentim init`", path.display()))?;
    let identity: Identity = toml::from_str(&content).context("failed to parse identity.toml")?;
    Ok(identity)
}

/// Load the Ed25519 signing key from `.agentim/private_key.pem`.
pub fn load_signing_key() -> Result<SigningKey> {
    let dir = identity_dir();
    let path = dir.join(KEY_FILE);
    let pem = fs::read_to_string(&path)
        .with_context(|| format!("no private key found at {} — run `agentim init`", path.display()))?;
    let key = SigningKey::from_pkcs8_pem(&pem)
        .context("failed to parse private key PEM")?;
    Ok(key)
}

/// Save identity + keypair to `.agentim/`.
pub fn save_identity(
    identity: &Identity,
    signing_key: &SigningKey,
) -> Result<()> {
    let dir = identity_dir();
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;

    // Save identity.toml
    let identity_path = dir.join(IDENTITY_FILE);
    let content = toml::to_string_pretty(identity).context("failed to serialize identity")?;
    fs::write(&identity_path, content)
        .with_context(|| format!("failed to write {}", identity_path.display()))?;

    // Save private_key.pem
    let key_path = dir.join(KEY_FILE);
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let pem = signing_key
        .to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
        .context("failed to encode private key to PEM")?;
    fs::write(&key_path, pem.as_bytes())
        .with_context(|| format!("failed to write {}", key_path.display()))?;

    // Set file permissions to 0600 on unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .context("failed to set private key permissions")?;
    }

    Ok(())
}

/// Check integrity of the local identity.
pub fn check_identity() -> Result<()> {
    let dir = identity_dir();
    if !dir.exists() {
        bail!("{} directory not found — run `agentim init`", dir.display());
    }

    let identity_path = dir.join(IDENTITY_FILE);
    if !identity_path.exists() {
        bail!("{} not found", identity_path.display());
    }

    let key_path = dir.join(KEY_FILE);
    if !key_path.exists() {
        bail!("{} not found", key_path.display());
    }

    // Check file permissions on unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&key_path)?.permissions().mode() & 0o777;
        if mode != 0o600 {
            bail!("{} has permissions {:o}, expected 600", key_path.display(), mode);
        }
    }

    // Try to load both.
    let _identity = load_identity()?;
    let _key = load_signing_key()?;

    Ok(())
}
