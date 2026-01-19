use crate::slack::SlackBot;
use std::time::Duration;
use std::env;
use tokio::time;
use chrono::{DateTime, Utc};
use chrono_tz::US::Eastern;
use serde::Deserialize;
use std::fs;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};

pub mod slack;

#[derive(Debug, Deserialize)]
struct EndpointConfig {
    endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize)]
struct Endpoint {
    name: String, 
    url: String,
    chain_id: i32, 
    auth_key: Option<String>,
} 

#[derive(Debug, Clone, Default )]
struct MonitorConfig {

    endpoint_monitor_index: usize 

}

const ONE_HOUR:u64 = 3600 ;

const ONE_DAY:u64 = 86400;

const ALERTED_BIDS_FILE: &str = "alerted_bids.txt";

fn load_alerted_bids() -> HashSet<String> {
    let mut alerted = HashSet::new();
    if let Ok(file) = fs::File::open(ALERTED_BIDS_FILE) {
        let reader = BufReader::new(file);
        for line in reader.lines().flatten() {
            if !line.trim().is_empty() {
                alerted.insert(line.trim().to_string());
            }
        }
    }
    alerted
}

fn save_alerted_bid(chain_id: i32, bid_id: &str) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ALERTED_BIDS_FILE)
        .expect("Failed to open alerted bids file");
    writeln!(file, "{}:{}", chain_id, bid_id).expect("Failed to write to alerted bids file");
}

fn make_bid_key(chain_id: i32, bid_id: &str) -> String {
    format!("{}:{}", chain_id, bid_id)
}

fn format_bid_alert(bid: &serde_json::Value, chain_id: i32, timestamp: &str) -> String {
    let bid_id = bid.get("bidId").and_then(|v| v.as_str()).unwrap_or("unknown");
    let borrower = bid.get("borrowerAddress").and_then(|v| v.as_str()).unwrap_or("unknown");
    let principal_raw = bid.get("principal").and_then(|v| v.as_str()).unwrap_or("0");
    let lending_token_obj = bid.get("lendingToken");
    let lending_token = lending_token_obj
        .and_then(|v| v.get("symbol"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let decimals = lending_token_obj
        .and_then(|v| v.get("decimals"))
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0) as u32;
    let principal: f64 = principal_raw.parse().unwrap_or(0.0) / 10_f64.powi(decimals as i32);
    let next_due = bid.get("nextDueDate").and_then(|v| v.as_str()).unwrap_or("unknown");
    let status = bid.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");

    format!(
        "🚨 Overdue Loan Alert!\nTimestamp: {}\nChain ID: {}\nBid ID: {}\nBorrower: {}\nPrincipal Token: {}\nPrincipal Amount: {:.2}\nNext Due Date: {}\nStatus: {}",
        timestamp, chain_id, bid_id, borrower, lending_token, principal, next_due, status
    )
} 


impl MonitorConfig {

    fn get_monitor_index(&self) -> usize {

        self.endpoint_monitor_index
    }

    fn set_monitor_index(&mut self, new_index: usize) {
        self.endpoint_monitor_index = new_index; 
    }
}


#[tokio::main]
async fn main() {
    // Load environment variables from .env file if it exists
    dotenvy::dotenv().ok();

    println!("Starting periodic POST requests ...");

    // Create a shared index to track which endpoint to check next
    let endpoint_config =   Arc::new(Mutex::new(  MonitorConfig::default() ))   ;

    let mut interval = time::interval(Duration::from_secs( ONE_HOUR )); // 1 hour = 3600 seconds

    loop {
        interval.tick().await;

 
        pulse_monitor(Arc::clone(&endpoint_config)).await;
    }
}



async fn pulse_monitor(endpoint_config: Arc< Mutex<  MonitorConfig> > ) {
    // Read and parse the endpoints.ron file
    let config_content = match fs::read_to_string("src/endpoints.ron") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read endpoints.ron file: {}", e);
            return;
        }
    };

    let config: EndpointConfig = match ron::from_str(&config_content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to parse endpoints.ron file: {}", e);
            return;
        }
    };

    let client = reqwest::Client::new();


    let total_endpoints_count = config.endpoints.len(); 


    let endpoint_index = endpoint_config.lock().unwrap().get_monitor_index() .clone() ;

    if let Some(endpoint_data) = config.endpoints.get(endpoint_index) {
        println!("Querying endpoint {}: {}", endpoint_index, endpoint_data.url);

        let chain_id = endpoint_data.chain_id; 

        // Get auth token from environment if auth_key is specified
        let auth_token = endpoint_data.auth_key.as_ref().and_then(|key| {
            let env_var_name = format!("{}", key );
            match env::var(&env_var_name) {
                Ok(token) => {
                    println!("Using authentication for endpoint with key: {}", key);
                    Some(token)
                }
                Err(_) => {
                    eprintln!("Warning: auth_key '{}' specified but {} environment variable not set", key, env_var_name);
                    None
                }
            }
        });

         let current_timestamp = Utc::now().timestamp();
          let last_week = current_timestamp - (ONE_DAY as i64);
          let query_body = format!(r#"
          {{
            bids(
              where: {{
                nextDueDate_lt: "{}",
                nextDueDate_gt: "{}",
                status: "Accepted"
              }}
              first: 5
            ) {{
              id
              bidId
              nextDueDate
              borrowerAddress
              status
              principal
              lendingToken {{
                id
                symbol
                decimals
              }}
            }}
          }}
          "#, current_timestamp, last_week);   

        // Construct proper JSON body for GraphQL query
        let body = serde_json::json!({
            "query": query_body
        });

        println!("Query body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

        // Make the POST request
        match make_post_request(&client, &endpoint_data.url, body, auth_token.as_deref()).await {
            Ok(response) => {
                // Check if the response contains errors
                let has_errors = if let Ok(json_response) = serde_json::from_str::<serde_json::Value>(&response) {
                    json_response.get("errors").is_some()
                } else {
                    false
                };

                if has_errors {
                    eprintln!("✗ GraphQL query returned errors for endpoint: {}", endpoint_data.url);
                    eprintln!("Response: {}", response);

                    // Get current timestamp in New York time
                    let now_utc: DateTime<Utc> = Utc::now();
                    let now_ny = now_utc.with_timezone(&Eastern);
                    let timestamp = now_ny.format("%Y-%m-%d %H:%M:%S %Z").to_string();

                    let message = format!(
                        "⚠️ GraphQL Endpoint Failed!\nTimestamp: {}\nEndpoint: {} {}\nError: {}",
                        timestamp, endpoint_data.name, endpoint_data.url, response
                    );

                    send_slack_warning(&message).await;
                } else {
                    println!("✓ Successfully queried endpoint: {}", endpoint_data.url);

                    // Parse response and check for overdue bids
                    if let Ok(json_response) = serde_json::from_str::<serde_json::Value>(&response) {
                        if let Some(bids) = json_response.get("data").and_then(|d| d.get("bids")).and_then(|b| b.as_array()) {
                            if bids.is_empty() {
                                println!("No overdue bids found.");
                            } else {
                                println!("Found {} overdue bid(s), checking for new alerts...", bids.len());

                                let alerted_bids = load_alerted_bids();
                                let now_utc: DateTime<Utc> = Utc::now();
                                let now_ny = now_utc.with_timezone(&Eastern);
                                let timestamp = now_ny.format("%Y-%m-%d %H:%M:%S %Z").to_string();

                                for bid in bids {
                                    let bid_id = bid.get("bidId").and_then(|v| v.as_str()).unwrap_or("unknown");
                                    let bid_key = make_bid_key(chain_id, bid_id);

                                    // Skip if already alerted
                                    if alerted_bids.contains(&bid_key) {
                                        println!("Bid {} on chain {} already alerted, skipping.", bid_id, chain_id);
                                        continue;
                                    }

                                    let message = format_bid_alert(bid, chain_id, &timestamp);

                                    send_slack_warning(&message).await;
                                    save_alerted_bid(chain_id, bid_id);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Failed to query endpoint {}: {}", endpoint_data.url, e);

                // Get current timestamp in New York time
                let now_utc: DateTime<Utc> = Utc::now();
                let now_ny = now_utc.with_timezone(&Eastern);
                let timestamp = now_ny.format("%Y-%m-%d %H:%M:%S %Z").to_string();

                let message = format!(
                    "⚠️ GraphQL Endpoint Failed!\nTimestamp: {}\nEndpoint: {} {}\nError: {}",
                    timestamp, endpoint_data.name,  endpoint_data.url, e
                );

                send_slack_warning(&message).await;
            }
        }
    }

    // Always increment index to move to next endpoint, even if current one failed
    let mut next_endpoint_index = endpoint_index + 1;
    if next_endpoint_index >= total_endpoints_count {
        next_endpoint_index = 0;
    }

    endpoint_config.lock().unwrap().set_monitor_index(next_endpoint_index);

}

/*
async fn query_endpoint(endpoint_config: Arc< &MonitorConfig> ) {

}*/ 

async fn send_slack_warning(message: &str) {

    println!("sending slack warning ");

    let token = match env::var("SLACK_OAUTH_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            eprintln!("SLACK_OAUTH_TOKEN environment variable not set, skipping Slack notification");
            return;
        }
    };

    let bot = SlackBot::new(token);

    match bot.send_message("#webserver-alerts", message).await {
        Ok(_) => println!("Slack alert sent successfully"),
        Err(e) => eprintln!("Failed to send Slack alert: {}", e),
    }
}


/*
async fn get_cursor_block() -> Result<U256, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    
    //hit hasura for the cursor 
    let url = "https://hasura-mainnet.nfteller.org/v1/graphql";
    let body = serde_json::json!({
        "query": "query MyQuery { cursors { block_id block_num cursor id } }"
    });
    
    match make_post_request(&client, url, body).await {
        Ok(response) => {
            println!("Hasura GraphQL request successful:");
            println!("{}", response);
            
            // Parse the response to get the cursor block
            parse_cursor_response(&response)
        }
        Err(e) => {
            Err(e.into())
        }
    }
}*/

async fn make_post_request(client: &reqwest::Client, url: &str, body: serde_json::Value, auth_token: Option<&str>) -> Result<String, reqwest::Error> {

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body);

    // Add Bearer token if provided
    if let Some(token) = auth_token {
        request = request.bearer_auth(token);
    }

    let response = request.send().await?;

    let text = response.text().await?;
    Ok(text)
}

/*
async fn get_alchemy_block(client: &reqwest::Client) -> Result<U256, Box<dyn std::error::Error>> {
    let api_key = env::var("ALCHEMY_API_KEY")
        .map_err(|_| "ALCHEMY_API_KEY environment variable not set")?;
    
    let url = format!("https://eth-mainnet.g.alchemy.com/v2/{}", api_key);
    
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    });
    
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    
    let json: serde_json::Value = response.json().await?;
    
    if let Some(result) = json.get("result") {
        if let Some(block_hex) = result.as_str() {
            // Parse hex string to U256
            let block_number = U256::from_str_radix(block_hex, 16)?;
            return Ok(block_number);
        }
    }
    
    Err("Failed to parse block number from Alchemy response".into())
}

fn parse_cursor_response(response: &str) -> Result<U256, Box<dyn std::error::Error>> {
    let json: serde_json::Value = serde_json::from_str(response)?;
    
    // Navigate to data.cursors array
    if let Some(data) = json.get("data") {
        if let Some(cursors) = data.get("cursors") {
            if let Some(cursors_array) = cursors.as_array() {
                // Find the cursor with the highest block_num
                let mut max_block = U256::zero();
                for cursor in cursors_array {
                    if let Some(block_num) = cursor.get("block_num") {
                        let block_val = if let Some(num) = block_num.as_u64() {
                            U256::from(num)
                        } else if let Some(str_val) = block_num.as_str() {
                            U256::from_dec_str(str_val)?
                        } else {
                            continue;
                        };
                        
                        if block_val > max_block {
                            max_block = block_val;
                        }
                    }
                }
                if max_block > U256::zero() {
                    return Ok(max_block);
                }
            }
        }
    }
    
    Err("No cursors found or invalid response format".into())
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bid_alert_with_usdc() {
        let bid = serde_json::json!({
            "bidId": "12345",
            "borrowerAddress": "0xabc123def456",
            "principal": "1000000",
            "lendingToken": {
                "id": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "symbol": "USDC",
                "decimals": 6
            },
            "nextDueDate": "1704067200",
            "status": "Accepted"
        });

        let message = format_bid_alert(&bid, 1, "2024-01-01 12:00:00 EST");

        assert!(message.contains("🚨 Overdue Loan Alert!"));
        assert!(message.contains("Chain ID: 1"));
        assert!(message.contains("Bid ID: 12345"));
        assert!(message.contains("Borrower: 0xabc123def456"));
        assert!(message.contains("Principal Token: USDC"));
        assert!(message.contains("Principal Amount: 1.00"));
        assert!(message.contains("Status: Accepted"));
    }

    #[test]
    fn test_format_bid_alert_with_18_decimals() {
        let bid = serde_json::json!({
            "bidId": "99999",
            "borrowerAddress": "0xdeadbeef",
            "principal": "5000000000000000000",
            "lendingToken": {
                "id": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
                "symbol": "WETH",
                "decimals": 18
            },
            "nextDueDate": "1704153600",
            "status": "Accepted"
        });

        let message = format_bid_alert(&bid, 137, "2024-01-02 12:00:00 EST");

        assert!(message.contains("Chain ID: 137"));
        assert!(message.contains("Principal Token: WETH"));
        assert!(message.contains("Principal Amount: 5.00"));
    }

    #[test]
    fn test_format_bid_alert_with_missing_fields() {
        let bid = serde_json::json!({});

        let message = format_bid_alert(&bid, 1, "2024-01-01 12:00:00 EST");

        assert!(message.contains("Bid ID: unknown"));
        assert!(message.contains("Borrower: unknown"));
        assert!(message.contains("Principal Token: unknown"));
        assert!(message.contains("Principal Amount: 0.00"));
    }
}