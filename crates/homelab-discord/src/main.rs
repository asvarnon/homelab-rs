//! # Homelab Discord
//!
//! Discord adapter for friends.

use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    info!("Starting homelab-discord bot...");
    
    // In a real implementation, we would start the serenity/poise client here.

    info!("Bot is running (placeholder). Press Ctrl+C to exit.");
    
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down...");
    Ok(())
}
