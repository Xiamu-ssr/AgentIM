use anyhow::Result;
use colored::Colorize;
use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::client::ApiClient;

pub async fn run(client: &ApiClient, json_mode: bool) -> Result<()> {
    loop {
        // Get a fresh JWT-based WebSocket URL each reconnect.
        let url = client.ws_url().await?;

        eprintln!("{}", "Connecting to WebSocket...".cyan());

        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                eprintln!(
                    "{}",
                    "Connected. Listening for messages (Ctrl+C to stop)...".green()
                );

                let (_write, mut read) = ws_stream.split();

                while let Some(msg_result) = read.next().await {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            if json_mode {
                                println!("{}", text);
                            } else {
                                print_human_readable(&text);
                            }
                        }
                        Ok(Message::Close(_)) => {
                            eprintln!("{}", "Server closed connection.".yellow());
                            break;
                        }
                        Ok(Message::Ping(_)) => {
                            // tungstenite auto-responds with pong
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("{} {}", "WebSocket error:".red(), e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("{} {}", "Connection failed:".red(), e);
            }
        }

        eprintln!("{}", "Reconnecting in 3 seconds...".yellow());
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

fn print_human_readable(text: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        println!("{}", text);
        return;
    };

    let msg_type = v
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown");

    match msg_type {
        "new_message" => {
            if let Some(msg) = v.get("message") {
                let from = msg
                    .get("from_agent")
                    .and_then(|f| f.as_str())
                    .unwrap_or("?");
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let channel = msg.get("channel_id").and_then(|c| c.as_str());
                let time = format_time(msg.get("created_at").and_then(|t| t.as_str()));

                if let Some(ch) = channel {
                    println!(
                        "{} {} {} : {}",
                        time.dimmed(),
                        format!("[#{}]", ch).cyan(),
                        format!("<{}>", from).green(),
                        content
                    );
                } else {
                    println!(
                        "{} {} : {}",
                        time.dimmed(),
                        format!("<{}>", from).green(),
                        content
                    );
                }
            }
        }
        other => {
            println!(
                "{} {}",
                format!("[{}]", other).yellow(),
                serde_json::to_string_pretty(&v).unwrap_or_default()
            );
        }
    }
}

fn format_time(ts: Option<&str>) -> String {
    let Some(ts) = ts else {
        return "??:??".to_string();
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        let local = dt.with_timezone(&chrono::Local);
        local.format("%H:%M").to_string()
    } else {
        ts.chars().take(16).collect()
    }
}
