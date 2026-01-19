# Teller Loan Health Monitor

A Rust-based monitoring bot that tracks loan health across multiple blockchain networks by querying TellerV2 subgraphs and sending Slack alerts for overdue loans.

## Overview

This bot periodically queries GraphQL endpoints (The Graph subgraphs) to identify loans that have passed their due dates. When overdue loans are detected, it sends alerts to a configured Slack channel with relevant loan details.

## How It Works

1. **Hourly Polling**: The bot runs on a 1-hour interval, cycling through configured GraphQL endpoints
2. **Overdue Detection**: For each endpoint, it queries for accepted bids where:
   - `nextDueDate` is less than the current timestamp (past due)
   - `nextDueDate` is within the last 24 hours (recently overdue)
3. **Deduplication**: Alerted bid IDs are persisted to `alerted_bids.txt` to prevent duplicate notifications
4. **Slack Alerts**: When new overdue loans are found, detailed alerts are sent to `#webserver-alerts`

## Monitored Networks

The bot monitors TellerV2 subgraphs on:
- Ethereum Mainnet (chain ID: 1)
- Base (chain ID: 8453)
- Arbitrum (chain ID: 42161)
- Polygon (chain ID: 137)

## Configuration

### Environment Variables

Copy `.env.template` to `.env` and configure:

```bash
SLACK_OAUTH_TOKEN=       # Slack bot OAuth token for sending alerts
THEGRAPH_AUTH_TOKEN=     # The Graph API authentication token
```

### Endpoint Configuration

Endpoints are configured in `src/endpoints.ron`:

```ron
(
    endpoints: [
        (
            name: "TheGraph TellerV2 Mainnet",
            url: "https://gateway.thegraph.com/api/subgraphs/id/...",
            auth_key: Some("THEGRAPH_AUTH_TOKEN"),
            chain_id: 1
        ),
        // ... more endpoints
    ]
)
```

Each endpoint specifies:
- `name`: Human-readable identifier
- `url`: GraphQL endpoint URL
- `auth_key`: Optional environment variable name containing the auth token
- `chain_id`: Blockchain network identifier

## Alert Format

When an overdue loan is detected, Slack receives:

```
🚨 Overdue Loan Alert!
Timestamp: 2024-01-15 10:30:00 EST
Chain ID: 1
Bid ID: 12345
Borrower: 0x...
Principal: 1000000000000000000
Next Due Date: 1705312800
Status: Accepted
```

Endpoint failures also trigger alerts:

```
⚠️ GraphQL Endpoint Failed!
Timestamp: 2024-01-15 10:30:00 EST
Endpoint: TheGraph TellerV2 Mainnet https://...
Error: ...
```

## Building & Running

### Local Development

```bash
# Install dependencies and build
cargo build --release

# Run the bot
cargo run --release
```

### Docker

```bash
# Build the image
docker build -t loan-health-bot .

# Run the container
docker run --env-file .env loan-health-bot
```

## Project Structure

```
├── src/
│   ├── health_bot.rs    # Main bot logic and monitoring loop
│   ├── slack.rs         # Slack API integration
│   └── endpoints.ron    # Endpoint configuration
├── Cargo.toml           # Rust dependencies
├── Dockerfile           # Container build configuration
└── .env.template        # Environment variable template
```

## Dependencies

- `reqwest` - HTTP client for GraphQL queries
- `tokio` - Async runtime
- `serde` / `serde_json` - JSON serialization
- `ron` - Rusty Object Notation for configuration
- `chrono` / `chrono-tz` - Timestamp handling with timezone support
- `dotenvy` - Environment variable loading
