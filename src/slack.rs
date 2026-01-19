

use reqwest::Client;
use std::error::Error;
use serde_json::json;

 
 /*
  // Get the bot token from environment variable
    let token = env::var("SLACK_OAUTH_TOKEN")
        .expect("SLACK_OAUTH_TOKEN environment variable must be set");

    // Create the bot instance
    let bot = SlackBot::new(token);

    // Send a simple message
    bot.send_message("#general", "Hello from my Rust bot! 🦀").await?;

    // Send a rich message with attachment
    let attachment = json!([
        {
            "color": "good",
            "title": "Bot Status",
            "text": "The Rust bot is running smoothly!",
            "fields": [
                {
                    "title": "Language",
                    "value": "Rust",
                    "short": true
                },
                {
                    "title": "Status",
                    "value": "Active",
                    "short": true
                }
            ],
            "footer": "Rust Bot",
            "ts": chrono::Utc::now().timestamp()
        }
    ]);

    bot.send_rich_message("#general", "Bot Status Update", Some(attachment)).await?;
    Ok(())

 
*/



pub struct SlackBot {
    client: Client,
    token: String,
}

impl SlackBot {
    pub fn new(token: String) -> Self {
        Self {
            client: Client::new(),
            token,
        }
    }

    pub async fn send_message(&self, channel: &str, text: &str) -> Result<(), Box<dyn Error>> {
        let url = "https://slack.com/api/chat.postMessage";
        
        let payload = json!({
            "channel": channel,
            "text": text
        });

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            let response_body: serde_json::Value = response.json().await?;
            
            if response_body["ok"].as_bool().unwrap_or(false) {
                println!("Message sent successfully!");
            } else {
                let error = response_body["error"].as_str().unwrap_or("Unknown error");
                eprintln!("Slack API error: {}", error);
            }
        } else {
            eprintln!("HTTP error: {}", response.status());
        }

        Ok(())
    }

    pub async fn send_rich_message(
        &self,
        channel: &str,
        text: &str,
        attachments: Option<serde_json::Value>,
    ) -> Result<(), Box<dyn Error>> {
        let url = "https://slack.com/api/chat.postMessage";
        
        let mut payload = json!({
            "channel": channel,
            "text": text
        });

        if let Some(attachments) = attachments {
            payload["attachments"] = attachments;
        }

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            let response_body: serde_json::Value = response.json().await?;
            
            if response_body["ok"].as_bool().unwrap_or(false) {
                println!("Rich message sent successfully!");
            } else {
                let error = response_body["error"].as_str().unwrap_or("Unknown error");
                eprintln!("Slack API error: {}", error);
            }
        } else {
            eprintln!("HTTP error: {}", response.status());
        }

        Ok(())
    }
}
