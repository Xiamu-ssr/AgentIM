mod client;
mod config;
mod listen;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use serde_json::Value;

use client::ApiClient;
use config::Config;

#[derive(Parser)]
#[command(name = "agentim", about = "AgentIM CLI — talk to the AgentIM server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage CLI configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Open browser for GitHub OAuth login
    Login,
    /// Manage agents
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
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
    /// Show inbox messages
    Inbox {
        /// Show all recent messages (not just unread)
        #[arg(long)]
        all: bool,
    },
    /// Show chat history with an agent
    History {
        /// Agent ID to view history with
        agent_id: String,
        /// Maximum number of messages
        #[arg(long, default_value = "20")]
        limit: u32,
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
enum ConfigAction {
    /// Set a config value
    Set {
        /// Config key (server, token)
        key: String,
        /// Config value
        value: String,
    },
    /// Show current config
    Show,
}

#[derive(Subcommand)]
enum AgentAction {
    /// Create a new agent
    Create {
        /// Agent ID (lowercase alphanumeric + hyphens)
        id: String,
        /// Display name
        #[arg(long)]
        name: String,
        /// Bio/description
        #[arg(long)]
        bio: Option<String>,
    },
    /// List your agents
    List,
    /// Set current agent
    Use {
        /// Agent ID to use
        id: String,
    },
    /// Show current agent info
    Info,
    /// Delete an agent
    Delete {
        /// Agent ID to delete
        id: String,
    },
    /// Reset agent token
    ResetToken {
        /// Agent ID
        id: String,
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
    /// Remove a contact
    Remove {
        /// Agent ID to remove
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
        /// Maximum number of messages
        #[arg(long, default_value = "20")]
        limit: u32,
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
        Commands::Config { action } => cmd_config(action).await,
        Commands::Login => cmd_login().await,
        Commands::Agent { action } => cmd_agent(action).await,
        Commands::Contacts { action } => cmd_contacts(action).await,
        Commands::Send { agent_id, message } => cmd_send(&agent_id, &message).await,
        Commands::Inbox { all } => cmd_inbox(all).await,
        Commands::History { agent_id, limit } => cmd_history(&agent_id, limit).await,
        Commands::Channel { action } => cmd_channel(action).await,
        Commands::Search { query } => cmd_search(&query).await,
        Commands::Listen { json } => cmd_listen(json).await,
    }
}

fn make_client() -> Result<ApiClient> {
    let cfg = Config::load()?;
    Ok(ApiClient::new(&cfg.server, cfg.token.as_deref()))
}

// ── Config ──

async fn cmd_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Set { key, value } => {
            let mut cfg = Config::load()?;
            match key.as_str() {
                "server" => cfg.server = value.clone(),
                "token" => cfg.token = Some(value.clone()),
                other => anyhow::bail!("unknown config key '{}' — valid keys: server, token", other),
            }
            cfg.save()?;
            println!("{} {} = {}", "Set".green(), key, value);
        }
        ConfigAction::Show => {
            let cfg = Config::load()?;
            let path = Config::path()?;
            println!("{} {}", "Config file:".cyan(), path.display());
            println!("{} {}", "Server:     ".cyan(), cfg.server);
            println!(
                "{} {}",
                "Agent:      ".cyan(),
                cfg.current_agent.as_deref().unwrap_or("(none)")
            );
            println!(
                "{} {}",
                "Token:      ".cyan(),
                match &cfg.token {
                    Some(t) if t.len() > 8 => format!("{}...{}", &t[..4], &t[t.len() - 4..]),
                    Some(t) => format!("{}...", &t[..t.len().min(4)]),
                    None => "(none)".to_string(),
                }
            );
        }
    }
    Ok(())
}

// ── Login ──

async fn cmd_login() -> Result<()> {
    let cfg = Config::load()?;
    let url = format!("{}/api/auth/github", cfg.server);
    println!("{}", "Open this URL in your browser to log in:".cyan());
    println!("\n  {}\n", url.bold());
    println!(
        "After login, get your agent token and run:\n  {}",
        "agentim config set token <YOUR_TOKEN>".green()
    );
    Ok(())
}

// ── Agent ──

async fn cmd_agent(action: AgentAction) -> Result<()> {
    match action {
        AgentAction::Create { id, name, bio } => {
            let client = make_client()?;
            let resp = client.create_agent(&id, &name, bio.as_deref()).await?;
            println!("{} Agent created!", "OK".green().bold());
            println!("  {} {}", "ID:   ".cyan(), resp["id"].as_str().unwrap_or(""));
            println!("  {} {}", "Name: ".cyan(), resp["name"].as_str().unwrap_or(""));
            println!(
                "  {} {}",
                "Token:".cyan(),
                resp["token"].as_str().unwrap_or("").yellow()
            );
            println!(
                "\n{}",
                "Save this token! Run: agentim config set token <TOKEN>".dimmed()
            );
        }
        AgentAction::List => {
            let client = make_client()?;
            let resp = client.list_agents().await?;
            let agents = as_array(&resp);
            if agents.is_empty() {
                println!("{}", "No agents found.".dimmed());
                return Ok(());
            }
            println!(
                "{:<20} {:<25} {:<10} {}",
                "ID".bold(),
                "NAME".bold(),
                "STATUS".bold(),
                "CREATED".bold()
            );
            for a in agents {
                println!(
                    "{:<20} {:<25} {:<10} {}",
                    a["id"].as_str().unwrap_or(""),
                    a["name"].as_str().unwrap_or(""),
                    a["status"].as_str().unwrap_or(""),
                    format_timestamp(a["created_at"].as_str()),
                );
            }
        }
        AgentAction::Use { id } => {
            let mut cfg = Config::load()?;
            cfg.current_agent = Some(id.clone());
            cfg.save()?;
            println!("{} Current agent set to '{}'", "OK".green().bold(), id);
            println!(
                "{}",
                "Set the token: agentim config set token <TOKEN>".dimmed()
            );
        }
        AgentAction::Info => {
            let cfg = Config::load()?;
            let agent_id = cfg
                .current_agent
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no current agent — run `agentim agent use <id>`"))?;
            let client = make_client()?;
            let resp = client.get_agent(agent_id).await?;
            println!("{} {}", "ID:     ".cyan(), resp["id"].as_str().unwrap_or(""));
            println!("{} {}", "Name:   ".cyan(), resp["name"].as_str().unwrap_or(""));
            println!("{} {}", "Status: ".cyan(), resp["status"].as_str().unwrap_or(""));
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
        }
        AgentAction::Delete { id } => {
            let client = make_client()?;
            client.delete_agent(&id).await?;
            println!("{} Agent '{}' deleted.", "OK".green().bold(), id);
        }
        AgentAction::ResetToken { id } => {
            let client = make_client()?;
            let resp = client.reset_token(&id).await?;
            println!("{} Token reset!", "OK".green().bold());
            println!(
                "  {} {}",
                "New token:".cyan(),
                resp["token"].as_str().unwrap_or("").yellow()
            );
        }
    }
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
                "{:<20} {:<25} {:<15} {}",
                "CONTACT ID".bold(),
                "NAME".bold(),
                "ALIAS".bold(),
                "ADDED".bold()
            );
            for c in contacts {
                println!(
                    "{:<20} {:<25} {:<15} {}",
                    c["contact_id"].as_str().unwrap_or(""),
                    c["agent_name"].as_str().unwrap_or(""),
                    c["alias"].as_str().unwrap_or("-"),
                    format_timestamp(c["created_at"].as_str()),
                );
            }
        }
        ContactsAction::Remove { agent_id } => {
            let client = make_client()?;
            client.remove_contact(&agent_id).await?;
            println!("{} Removed contact '{}'", "OK".green().bold(), agent_id);
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
    Ok(())
}

async fn cmd_inbox(all: bool) -> Result<()> {
    let client = make_client()?;
    let resp = client.inbox().await?;
    let messages = as_array(&resp);

    if messages.is_empty() {
        if all {
            println!("{}", "No messages.".dimmed());
        } else {
            println!("{}", "No unread messages.".dimmed());
        }
        return Ok(());
    }

    let label = if all { "Messages" } else { "Unread messages" };
    println!("{} ({})", label.cyan().bold(), messages.len());
    println!();
    for m in messages {
        print_message(m);
    }
    Ok(())
}

async fn cmd_history(agent_id: &str, limit: u32) -> Result<()> {
    let client = make_client()?;
    let resp = client.chat_history(agent_id, Some(limit), None).await?;
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
        ChannelAction::History { channel, limit } => {
            let client = make_client()?;
            let resp = client
                .channel_messages(&channel, Some(limit), None)
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
