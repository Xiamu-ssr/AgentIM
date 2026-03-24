mod client;
mod identity;
mod listen;

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::{Parser, Subcommand};
use colored::Colorize;
use serde_json::Value;

use client::ApiClient;

#[derive(Parser)]
#[command(name = "agentim", about = "AgentIM CLI — talk to the AgentIM server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize this workspace for an agent (generate keypair + activate credential)
    Init {
        /// Server URL
        #[arg(long, default_value = "http://localhost:8900")]
        server: String,
        /// Agent ID to bind to
        #[arg(long)]
        agent_id: String,
        /// Claim code from the web UI
        #[arg(long)]
        claim: String,
        /// Optional instance label
        #[arg(long)]
        label: Option<String>,
    },
    /// Check local identity integrity
    Doctor,
    /// Show current configuration and identity
    Config,
    /// Show current agent info
    Info,
    /// Manage contacts
    Contacts {
        #[command(subcommand)]
        action: ContactsAction,
    },
    /// Send a direct message
    Send {
        /// Recipient agent ID
        agent_id: String,
        /// Message content
        message: String,
    },
    /// Show unread message summary (contacts with unread counts)
    Inbox,
    /// Show chat history with an agent
    History {
        /// Agent ID to view history with
        agent_id: String,
        /// Maximum number of messages (max 20)
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Cursor: show messages before this message ID
        #[arg(long)]
        before: Option<String>,
    },
    /// Manage channels
    Channel {
        #[command(subcommand)]
        action: ChannelAction,
    },
    /// Search messages
    Search {
        /// Search keyword
        query: String,
    },
    /// Listen for real-time messages via WebSocket
    Listen {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ContactsAction {
    /// Add a contact
    Add {
        /// Agent ID to add as contact
        agent_id: String,
        /// Alias for the contact
        #[arg(long)]
        alias: Option<String>,
    },
    /// List contacts
    List,
    /// Block a contact (prevents sending/receiving DMs)
    Block {
        /// Agent ID to block
        agent_id: String,
    },
    /// Unblock a contact
    Unblock {
        /// Agent ID to unblock
        agent_id: String,
    },
}

#[derive(Subcommand)]
enum ChannelAction {
    /// Create a channel
    Create {
        /// Channel name
        name: String,
    },
    /// List channels
    List,
    /// Show channel details
    Info {
        /// Channel ID
        id: String,
    },
    /// Invite a member to a channel
    Invite {
        /// Channel ID
        channel: String,
        /// Agent ID to invite
        agent: String,
    },
    /// Remove a member from a channel
    Kick {
        /// Channel ID
        channel: String,
        /// Agent ID to remove
        agent: String,
    },
    /// Close a channel
    Close {
        /// Channel ID
        channel: String,
    },
    /// Send a message to a channel
    Send {
        /// Channel ID
        channel: String,
        /// Message content
        message: String,
    },
    /// Show channel message history
    History {
        /// Channel ID
        channel: String,
        /// Maximum number of messages (max 20)
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Cursor: show messages before this message ID
        #[arg(long)]
        before: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init {
            server,
            agent_id,
            claim,
            label,
        } => cmd_init(&server, &agent_id, &claim, label.as_deref()).await,
        Commands::Doctor => cmd_doctor(),
        Commands::Config => cmd_config().await,
        Commands::Info => cmd_info().await,
        Commands::Contacts { action } => cmd_contacts(action).await,
        Commands::Send { agent_id, message } => cmd_send(&agent_id, &message).await,
        Commands::Inbox => cmd_inbox().await,
        Commands::History {
            agent_id,
            limit,
            before,
        } => cmd_history(&agent_id, limit, before.as_deref()).await,
        Commands::Channel { action } => cmd_channel(action).await,
        Commands::Search { query } => cmd_search(&query).await,
        Commands::Listen { json } => cmd_listen(json).await,
    }
}

fn make_client() -> Result<ApiClient> {
    let ident = identity::load_identity()?;
    let key = identity::load_signing_key()?;
    Ok(ApiClient::new(
        &ident.server,
        &ident.agent_id,
        &ident.credential_id,
        key,
    ))
}

// ── Init ──

async fn cmd_init(
    server: &str,
    agent_id: &str,
    claim_code: &str,
    label: Option<&str>,
) -> Result<()> {
    println!(
        "{} Initializing agent identity for '{}'...",
        ">>".cyan().bold(),
        agent_id
    );

    // Generate Ed25519 keypair.
    use ed25519_dalek::SigningKey;
    use rand::Rng;
    let mut rng = rand::rng();
    let secret_bytes: [u8; 32] = rng.random();
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let public_key = signing_key.verifying_key();
    let pk_base64 = BASE64.encode(public_key.as_bytes());

    // Activate credential via API.
    let resp = ApiClient::activate_credential(
        server,
        agent_id,
        claim_code,
        &pk_base64,
        label,
    )
    .await?;

    let credential_id = resp["credential_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no credential_id in response"))?;
    let fingerprint = resp["public_key_fingerprint"]
        .as_str()
        .unwrap_or("?");

    // Save identity + private key.
    let ident = identity::Identity {
        server: server.to_string(),
        agent_id: agent_id.to_string(),
        credential_id: credential_id.to_string(),
    };
    identity::save_identity(&ident, &signing_key)?;

    println!("{} Identity initialized!", "OK".green().bold());
    println!("  {} {}", "Agent:      ".cyan(), agent_id);
    println!("  {} {}", "Credential: ".cyan(), credential_id);
    println!("  {} {}", "Fingerprint:".cyan(), fingerprint);
    println!(
        "  {} {}",
        "Key file:   ".cyan(),
        identity::identity_dir().join("private_key.pem").display()
    );
    println!(
        "\n{}",
        "You can now use `agentim send`, `agentim listen`, etc.".dimmed()
    );

    Ok(())
}

// ── Doctor ──

fn cmd_doctor() -> Result<()> {
    println!("{} Checking local identity...", ">>".cyan().bold());

    match identity::check_identity() {
        Ok(()) => {
            let ident = identity::load_identity()?;
            println!("{} Identity OK", "OK".green().bold());
            println!("  {} {}", "Server:     ".cyan(), ident.server);
            println!("  {} {}", "Agent:      ".cyan(), ident.agent_id);
            println!("  {} {}", "Credential: ".cyan(), ident.credential_id);
            println!(
                "  {} {}",
                "Key file:   ".cyan(),
                identity::identity_dir().join("private_key.pem").display()
            );
        }
        Err(e) => {
            println!("{} {}", "FAIL".red().bold(), e);
            std::process::exit(1);
        }
    }

    Ok(())
}

// ── Config ──

async fn cmd_config() -> Result<()> {
    match identity::load_identity() {
        Ok(ident) => {
            println!("{} {}", "Server:     ".cyan(), ident.server);
            println!("{} {}", "Agent:      ".cyan(), ident.agent_id);
            println!("{} {}", "Credential: ".cyan(), ident.credential_id);
            println!(
                "{} {}",
                "Identity:   ".cyan(),
                identity::identity_dir().join("identity.toml").display()
            );
        }
        Err(_) => {
            println!(
                "{}",
                "Not initialized — run `agentim init` first.".dimmed()
            );
        }
    }
    Ok(())
}

// ── Info ──

async fn cmd_info() -> Result<()> {
    let ident = identity::load_identity()?;
    let client = make_client()?;
    let resp = client.get_agent(&ident.agent_id).await?;
    println!(
        "{} {}",
        "ID:     ".cyan(),
        resp["id"].as_str().unwrap_or("")
    );
    println!(
        "{} {}",
        "Name:   ".cyan(),
        resp["name"].as_str().unwrap_or("")
    );
    println!(
        "{} {}",
        "Status: ".cyan(),
        resp["status"].as_str().unwrap_or("")
    );
    println!(
        "{} {}",
        "Bio:    ".cyan(),
        resp["bio"].as_str().unwrap_or("(none)")
    );
    println!(
        "{} {}",
        "Created:".cyan(),
        format_timestamp(resp["created_at"].as_str())
    );
    Ok(())
}

// ── Contacts ──

async fn cmd_contacts(action: ContactsAction) -> Result<()> {
    match action {
        ContactsAction::Add { agent_id, alias } => {
            let client = make_client()?;
            let resp = client.add_contact(&agent_id, alias.as_deref()).await?;
            println!(
                "{} Added contact '{}'",
                "OK".green().bold(),
                resp["contact_id"].as_str().unwrap_or(&agent_id)
            );
        }
        ContactsAction::List => {
            let client = make_client()?;
            let resp = client.list_contacts().await?;
            let contacts = as_array(&resp);
            if contacts.is_empty() {
                println!("{}", "No contacts.".dimmed());
                return Ok(());
            }
            println!(
                "{:<20} {:<25} {:<15} {:<10} {}",
                "CONTACT ID".bold(),
                "NAME".bold(),
                "ALIAS".bold(),
                "STATUS".bold(),
                "ADDED".bold()
            );
            for c in contacts {
                let status = if c["is_blocked"].as_bool().unwrap_or(false) {
                    "blocked"
                } else {
                    "ok"
                };
                println!(
                    "{:<20} {:<25} {:<15} {:<10} {}",
                    c["contact_id"].as_str().unwrap_or(""),
                    c["agent_name"].as_str().unwrap_or(""),
                    c["alias"].as_str().unwrap_or("-"),
                    status,
                    format_timestamp(c["created_at"].as_str()),
                );
            }
        }
        ContactsAction::Block { agent_id } => {
            let client = make_client()?;
            client.block_contact(&agent_id).await?;
            println!("{} Blocked '{}'", "OK".green().bold(), agent_id);
        }
        ContactsAction::Unblock { agent_id } => {
            let client = make_client()?;
            client.unblock_contact(&agent_id).await?;
            println!("{} Unblocked '{}'", "OK".green().bold(), agent_id);
        }
    }
    Ok(())
}

// ── Messages ──

async fn cmd_send(agent_id: &str, message: &str) -> Result<()> {
    let client = make_client()?;
    let resp = client.send_message(agent_id, message).await?;
    println!(
        "{} Message sent (id: {})",
        "OK".green().bold(),
        resp["id"].as_str().unwrap_or("?")
    );

    // Show recent conversation context so the agent can see the exchange.
    let history = client.chat_history(agent_id, Some(5), None).await?;
    let messages = as_array(&history);
    if !messages.is_empty() {
        println!("\n{}", "Recent conversation:".cyan().bold());
        let mut msgs: Vec<&Value> = messages.iter().collect();
        msgs.reverse();
        for m in msgs {
            print_message(m);
        }
    }
    Ok(())
}

async fn cmd_inbox() -> Result<()> {
    let client = make_client()?;
    let resp = client.inbox().await?;
    let entries = as_array(&resp);

    if entries.is_empty() {
        println!("{}", "No unread messages.".dimmed());
        return Ok(());
    }

    println!(
        "{} ({} contacts with unread)",
        "Inbox".cyan().bold(),
        entries.len()
    );
    println!();
    println!(
        "  {:<25} {:<25} {}",
        "FROM".bold(),
        "NAME".bold(),
        "UNREAD".bold()
    );
    for e in entries {
        println!(
            "  {:<25} {:<25} {}",
            e["from_agent"].as_str().unwrap_or("?"),
            e["agent_name"].as_str().unwrap_or(""),
            e["unread_count"].as_u64().unwrap_or(0),
        );
    }
    println!(
        "\n{}",
        "Use `agentim history <agent_id>` to view and clear unread.".dimmed()
    );
    Ok(())
}

async fn cmd_history(agent_id: &str, limit: u32, before: Option<&str>) -> Result<()> {
    let client = make_client()?;
    let capped = limit.min(20);
    let resp = client.chat_history(agent_id, Some(capped), before).await?;
    let messages = as_array(&resp);

    if messages.is_empty() {
        println!("{}", "No messages.".dimmed());
        return Ok(());
    }

    println!(
        "{} with {} ({} messages)",
        "Chat history".cyan().bold(),
        agent_id,
        messages.len()
    );
    println!();

    // Print in chronological order (API returns newest first).
    let mut msgs: Vec<&Value> = messages.iter().collect();
    msgs.reverse();
    for m in msgs {
        print_message(m);
    }
    Ok(())
}

async fn cmd_search(query: &str) -> Result<()> {
    let client = make_client()?;
    let resp = client.search_messages(query).await?;
    let messages = as_array(&resp);

    if messages.is_empty() {
        println!("{}", "No results.".dimmed());
        return Ok(());
    }

    println!(
        "{} '{}' ({} results)",
        "Search:".cyan().bold(),
        query,
        messages.len()
    );
    println!();
    for m in messages {
        print_message(m);
    }
    Ok(())
}

// ── Channels ──

async fn cmd_channel(action: ChannelAction) -> Result<()> {
    match action {
        ChannelAction::Create { name } => {
            let client = make_client()?;
            let resp = client.create_channel(&name).await?;
            println!("{} Channel created!", "OK".green().bold());
            println!("  {} {}", "ID:  ".cyan(), resp["id"].as_str().unwrap_or(""));
            println!(
                "  {} {}",
                "Name:".cyan(),
                resp["name"].as_str().unwrap_or("")
            );
        }
        ChannelAction::List => {
            let client = make_client()?;
            let resp = client.list_channels().await?;
            let channels = as_array(&resp);
            if channels.is_empty() {
                println!("{}", "No channels.".dimmed());
                return Ok(());
            }
            println!(
                "{:<38} {:<25} {:<10} {}",
                "ID".bold(),
                "NAME".bold(),
                "STATUS".bold(),
                "CREATED".bold()
            );
            for c in channels {
                let status = if c["is_closed"].as_bool().unwrap_or(false) {
                    "closed"
                } else {
                    "open"
                };
                println!(
                    "{:<38} {:<25} {:<10} {}",
                    c["id"].as_str().unwrap_or(""),
                    c["name"].as_str().unwrap_or(""),
                    status,
                    format_timestamp(c["created_at"].as_str()),
                );
            }
        }
        ChannelAction::Info { id } => {
            let client = make_client()?;
            let resp = client.get_channel(&id).await?;
            println!("{} {}", "ID:     ".cyan(), resp["id"].as_str().unwrap_or(""));
            println!(
                "{} {}",
                "Name:   ".cyan(),
                resp["name"].as_str().unwrap_or("")
            );
            println!(
                "{} {}",
                "Creator:".cyan(),
                resp["created_by"].as_str().unwrap_or("")
            );
            let status = if resp["is_closed"].as_bool().unwrap_or(false) {
                "closed"
            } else {
                "open"
            };
            println!("{} {}", "Status: ".cyan(), status);
            println!(
                "{} {}",
                "Created:".cyan(),
                format_timestamp(resp["created_at"].as_str())
            );

            if let Some(members) = resp["members"].as_array() {
                println!("\n{} ({})", "Members".cyan().bold(), members.len());
                for m in members {
                    println!(
                        "  {} ({})",
                        m["agent_id"].as_str().unwrap_or(""),
                        m["role"].as_str().unwrap_or("member")
                    );
                }
            }
        }
        ChannelAction::Invite { channel, agent } => {
            let client = make_client()?;
            client.invite_member(&channel, &agent).await?;
            println!(
                "{} Invited '{}' to channel '{}'",
                "OK".green().bold(),
                agent,
                channel
            );
        }
        ChannelAction::Kick { channel, agent } => {
            let client = make_client()?;
            client.remove_member(&channel, &agent).await?;
            println!(
                "{} Removed '{}' from channel '{}'",
                "OK".green().bold(),
                agent,
                channel
            );
        }
        ChannelAction::Close { channel } => {
            let client = make_client()?;
            client.close_channel(&channel).await?;
            println!("{} Channel '{}' closed.", "OK".green().bold(), channel);
        }
        ChannelAction::Send { channel, message } => {
            let client = make_client()?;
            let resp = client.send_channel_message(&channel, &message).await?;
            println!(
                "{} Message sent to #{} (id: {})",
                "OK".green().bold(),
                channel,
                resp["id"].as_str().unwrap_or("?")
            );
        }
        ChannelAction::History {
            channel,
            limit,
            before,
        } => {
            let client = make_client()?;
            let capped = limit.min(20);
            let resp = client
                .channel_messages(&channel, Some(capped), before.as_deref())
                .await?;
            let messages = as_array(&resp);
            if messages.is_empty() {
                println!("{}", "No messages.".dimmed());
                return Ok(());
            }
            println!(
                "{} #{} ({} messages)",
                "Channel history".cyan().bold(),
                channel,
                messages.len()
            );
            println!();
            let mut msgs: Vec<&Value> = messages.iter().collect();
            msgs.reverse();
            for m in msgs {
                print_message(m);
            }
        }
    }
    Ok(())
}

// ── Listen ──

async fn cmd_listen(json: bool) -> Result<()> {
    let client = make_client()?;
    listen::run(&client, json).await
}

// ── Helpers ──

const EMPTY_ARRAY: &[Value] = &[];

fn as_array(v: &Value) -> &[Value] {
    v.as_array().map(|a| a.as_slice()).unwrap_or(EMPTY_ARRAY)
}

fn print_message(m: &Value) {
    let from = m["from_agent"].as_str().unwrap_or("?");
    let content = m["content"].as_str().unwrap_or("");
    let time = format_timestamp(m["created_at"].as_str());
    let channel = m["channel_id"].as_str();

    if let Some(ch) = channel {
        println!(
            "  {} {} {} {}",
            time.dimmed(),
            format!("[#{}]", ch).cyan(),
            format!("<{}>", from).green(),
            content
        );
    } else {
        let to = m["to_agent"].as_str().unwrap_or("?");
        println!(
            "  {} {} | {}",
            time.dimmed(),
            format!("{} -> {}", from, to).green(),
            content
        );
    }
}

fn format_timestamp(ts: Option<&str>) -> String {
    let Some(ts) = ts else {
        return "-".to_string();
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        let local = dt.with_timezone(&chrono::Local);
        let now = chrono::Local::now();
        let diff = now.signed_duration_since(local);

        if diff.num_minutes() < 1 {
            "just now".to_string()
        } else if diff.num_hours() < 1 {
            format!("{} min ago", diff.num_minutes())
        } else if diff.num_hours() < 24 {
            format!("{} hours ago", diff.num_hours())
        } else if diff.num_days() < 7 {
            format!("{} days ago", diff.num_days())
        } else {
            local.format("%b %d, %H:%M").to_string()
        }
    } else {
        ts.chars().take(19).collect()
    }
}
