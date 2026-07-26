//! # Homelab MCP
//!
//! MCP adapter LLMs.

use auth::AuthKeys;
use axum::{middleware, Router};
use homelab_core::{
    tools::{opnsense, proxmox},
    Config, HomelabClient,
};
use homelab_mcp::osrs_market::{
    ActivitySnapshot, HistoryPoint, ItemMapping, LatestPrice, OsrsMarketClient, TimeSeriesLookback,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Content, ErrorCode, ErrorData as McpError};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{model::CallToolResult, tool, tool_router};
use rmcp::{tool_handler, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use state::AppState;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::info;
mod auth;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present — silently no-ops if missing (e.g. prod, where vars come from the environment directly).
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    // Boot boundary: load runtime configuration before constructing the adapter.
    let config_path = std::env::var("HOMELAB_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let config = Config::load(config_path)?;
    let profile = std::env::var("PROFILE").expect("No profile is configured...");
    // Public hostname this server is reachable at (e.g. via Cloudflare Tunnel) — added to
    // rmcp's Host-header allowlist alongside the loopback defaults so the DNS-rebinding
    // guard doesn't reject legitimate traffic arriving through the tunnel.
    let public_host = std::env::var("MCP_PUBLIC_HOST").ok();

    let auth_keys = if profile == "local" {
        None
    } else {
        let cf_url = std::env::var("CF_CERTS_URL").expect("CF_CERTS_URL not set");
        let auth_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Some(Arc::new(
            auth_client
                .get(&cf_url)
                .send()
                .await?
                .json::<AuthKeys>()
                .await?,
        ))
    };

    let app_state = Arc::new(AppState { auth_keys, profile });

    // Core dependency: HomelabClient knows how to call configured HTTP endpoints.
    let client = HomelabClient::new(config);
    // Public third-party API client. It is owned at the MCP boundary, not in core.
    let osrs_market = Arc::new(OsrsMarketClient::new()?);

    info!("Starting homelab-mcp server...");

    // with_allowed_hosts replaces rmcp's default list outright, so the loopback
    // entries have to be repeated here alongside the public tunnel hostname.
    let mut allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Some(host) = &public_host {
        allowed_hosts.push(host.clone());
    }

    //build mcp service
    let mcp_service = StreamableHttpService::new(
        move || Ok(HomelabMcp::new(client.clone(), osrs_market.clone())), // factory — called per session
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts),
    );

    //build router
    let app = Router::new()
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth::require_cf_jwt, // <- pass the function itself, axum calls it per-request
        ))
        .with_state(app_state);

    // bind and serve listener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8787").await?;
    axum::serve(listener, app).await?;

    // Keep serving until the MCP transport shuts down.
    // service.waiting().await?;

    info!("Shutting down...");
    Ok(())
}

// MCP adapter state. This is the "controller" object for MCP calls.
// It owns the core client and the generated dispatch table for tool methods.
pub struct HomelabMcp {
    // Core dependency used by tool methods to reach Proxmox/OPNsense/etc.
    client: HomelabClient,
    // Public OSRS Wiki client, shared across MCP sessions for its short-lived caches.
    osrs_market: Arc<OsrsMarketClient>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MarketLookupRequest {
    #[schemars(description = "OSRS item ID, e.g. 4151 for Abyssal whip")]
    item_id: u32,
}

#[derive(Debug, Serialize)]
struct MarketLookupResponse {
    item: Option<ItemMapping>,
    latest: LatestPrice,
    disclaimer: &'static str,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MarketHistoryRequest {
    #[schemars(description = "OSRS item ID")]
    item_id: u32,
    #[schemars(description = "One of: 6h, 24h, 7d, 30d, 6m, 1y")]
    lookback: String,
}

#[derive(Debug, Serialize)]
struct MarketHistoryResponse {
    item_id: u32,
    lookback: String,
    points: Vec<HistoryPoint>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MarketActivityRequest {
    #[schemars(
        description = "One or more OSRS item IDs to select from the bulk activity response"
    )]
    item_ids: Vec<u32>,
    #[schemars(description = "Aggregation window: 5m or 1h")]
    window: String,
}

#[derive(Debug, Serialize)]
struct MarketActivityResponse {
    window: String,
    items: BTreeMap<u32, ActivitySnapshot>,
}

// ServerHandler tells rmcp this struct is an MCP server.
// #[tool_handler] wires MCP list_tools/call_tool to the router.
#[tool_handler(
    name = "homelab-rs",
    version = "0.1.0",
    instructions = "Homelab automation tools backed by configured HTTP endpoints."
)]
impl ServerHandler for HomelabMcp {}

fn parse_lookback(value: &str) -> std::result::Result<TimeSeriesLookback, McpError> {
    match value {
        "6h" => Ok(TimeSeriesLookback::Hours6),
        "24h" => Ok(TimeSeriesLookback::Hours24),
        "7d" => Ok(TimeSeriesLookback::Days7),
        "30d" => Ok(TimeSeriesLookback::Days30),
        "6m" => Ok(TimeSeriesLookback::Months6),
        "1y" => Ok(TimeSeriesLookback::Year1),
        _ => Err(McpError::new(
            ErrorCode::INVALID_PARAMS,
            "lookback must be one of: 6h, 24h, 7d, 30d, 6m, 1y".to_string(),
            None,
        )),
    }
}

fn osrs_json_response<T: Serialize>(value: &T) -> std::result::Result<CallToolResult, McpError> {
    let json = serde_json::to_string_pretty(value).map_err(|error| {
        McpError::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to serialize OSRS market data: {error}"),
            None,
        )
    })?;
    Ok(CallToolResult::success(vec![Content::text(format!(
        "```json\n{json}\n```"
    ))]))
}

#[tool_router]
impl HomelabMcp {
    pub fn new(client: HomelabClient, osrs_market: Arc<OsrsMarketClient>) -> Self {
        Self {
            client,
            osrs_market,
        }
    }

    #[tool(
        name = "osrs_market_lookup",
        description = "Gets item metadata and the latest observed GE high/low trades. Observations are not executable quotes."
    )]
    async fn osrs_market_lookup(
        &self,
        Parameters(request): Parameters<MarketLookupRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        let (latest, mapping) = tokio::try_join!(
            self.osrs_market.lookup(request.item_id),
            self.osrs_market.mapping()
        )
        .map_err(|error| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to look up OSRS market data: {error}"),
                None,
            )
        })?;
        let item = mapping.into_iter().find(|item| item.id == request.item_id);
        let response = MarketLookupResponse {
            item,
            latest,
            disclaimer: "High and low are separate observed trades and may have different timestamps; they are not an executable spread.",
        };

        osrs_json_response(&response)
    }

    #[tool(
        name = "osrs_market_history",
        description = "Gets aggregate OSRS GE price and volume history for one item and a documented lookback period."
    )]
    async fn osrs_market_history(
        &self,
        Parameters(request): Parameters<MarketHistoryRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        let lookback = parse_lookback(&request.lookback)?;
        let points = self
            .osrs_market
            .history(request.item_id, lookback)
            .await
            .map_err(|error| {
                McpError::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to fetch OSRS market history: {error}"),
                    None,
                )
            })?;

        osrs_json_response(&MarketHistoryResponse {
            item_id: request.item_id,
            lookback: request.lookback,
            points,
        })
    }

    #[tool(
        name = "osrs_market_activity",
        description = "Gets selected item activity from the bulk 5-minute or hourly OSRS GE price and volume snapshot."
    )]
    async fn osrs_market_activity(
        &self,
        Parameters(request): Parameters<MarketActivityRequest>,
    ) -> std::result::Result<CallToolResult, McpError> {
        if request.item_ids.is_empty() {
            return Err(McpError::new(
                ErrorCode::INVALID_PARAMS,
                "item_ids must contain at least one item ID".to_string(),
                None,
            ));
        }

        let activity = match request.window.as_str() {
            "5m" => self.osrs_market.activity_5m().await,
            "1h" => self.osrs_market.activity_1h().await,
            _ => {
                return Err(McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    "window must be either '5m' or '1h'".to_string(),
                    None,
                ));
            }
        }
        .map_err(|error| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to fetch OSRS market activity: {error}"),
                None,
            )
        })?;
        let items = request
            .item_ids
            .into_iter()
            .filter_map(|item_id| {
                activity
                    .get(&item_id)
                    .cloned()
                    .map(|value| (item_id, value))
            })
            .collect();

        osrs_json_response(&MarketActivityResponse {
            window: request.window,
            items,
        })
    }

    // MCP entrypoint: external clients call tool name "scan_nodes".
    #[tool(name = "scan_nodes", description = "Scans proxmox nodes")]
    async fn scan_nodes(&self) -> std::result::Result<CallToolResult, McpError> {
        // Adapter -> core: call the real domain/tool function.
        let nodes = proxmox::scan_nodes(&self.client).await.map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to scan nodes: {}", e),
                None,
            )
        })?;

        // Core result -> MCP content: serialize the domain data for the protocol response.
        let json = serde_json::to_string_pretty(&nodes).map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to serialize nodes: {}", e),
                None,
            )
        })?;

        // MCP response: return one text content block containing JSON.
        Ok(CallToolResult::success(vec![Content::text(format!(
            "```json\n{}\n```",
            json
        ))]))
    }

    #[tool(name = "scan_cluster", description = "Scans proxmox cluster")]
    async fn scan_cluster(&self) -> std::result::Result<CallToolResult, McpError> {
        // Adapter -> core: call the real domain/tool function.
        let nodes = proxmox::scan_cluster(&self.client).await.map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to scan clusters: {}", e),
                None,
            )
        })?;

        // Core result -> MCP content: serialize the domain data for the protocol response.
        let json = serde_json::to_string_pretty(&nodes).map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to serialize clusters: {}", e),
                None,
            )
        })?;

        // MCP response: return one text content block containing JSON.
        Ok(CallToolResult::success(vec![Content::text(format!(
            "```json\n{}\n```",
            json
        ))]))
    }

    #[tool(
        name = "get_dhcp_leases",
        description = "Gets DHCP leases from OPNsense"
    )]
    async fn get_dhcp_leases(&self) -> std::result::Result<CallToolResult, McpError> {
        let leases = opnsense::get_dhcp_leases(&self.client).await.map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to get DHCP leases: {}", e),
                None,
            )
        })?;

        let json = serde_json::to_string_pretty(&leases).map_err(|e| {
            McpError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to serialize leases: {}", e),
                None,
            )
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "```json\n{}\n```",
            json
        ))]))
    }
}
