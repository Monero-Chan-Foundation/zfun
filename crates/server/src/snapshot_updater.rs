//! Automatic snapshot updater
//!
//! Background task that periodically checks Zebra height and triggers
//! snapshot updates when needed.

use eyre::{Result, eyre};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tracing;

/// Configuration for automatic snapshot updates
#[derive(Debug, Clone)]
pub struct SnapshotUpdaterConfig {
    /// Enable automatic updates
    pub enabled: bool,

    /// Check interval (how often to poll Zebra)
    pub check_interval: Duration,

    /// Update threshold (trigger update when N blocks behind)
    pub update_threshold: u32,

    /// Current snapshot height (read from metadata)
    pub current_height: u32,

    /// Zebra RPC URL
    pub zebra_rpc_url: String,

    /// Path to update_snapshot.sh script
    pub update_script_path: String,
}

impl Default for SnapshotUpdaterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval: Duration::from_secs(3600), // 1 hour
            update_threshold: 1000,
            current_height: 0,
            zebra_rpc_url: "http://localhost:8232".to_string(),
            update_script_path: "./scripts/update_snapshot.sh".to_string(),
        }
    }
}

impl SnapshotUpdaterConfig {
    /// Load from environment variables
    pub fn from_env() -> Self {
        let enabled = std::env::var("SNAPSHOT_AUTO_UPDATE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let check_interval_secs = std::env::var("SNAPSHOT_CHECK_INTERVAL")
            .unwrap_or_else(|_| "3600".to_string())
            .parse::<u64>()
            .unwrap_or(3600);

        let update_threshold = std::env::var("SNAPSHOT_UPDATE_THRESHOLD")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u32>()
            .unwrap_or(1000);

        let zebra_rpc_url =
            std::env::var("ZEBRA_RPC_URL").unwrap_or_else(|_| "http://localhost:8232".to_string());

        let update_script_path = std::env::var("SNAPSHOT_UPDATE_SCRIPT")
            .unwrap_or_else(|_| "./scripts/update_snapshot.sh".to_string());

        // Load current snapshot height from compiled-in metadata
        let current_height = zfun_lib::SNAPSHOT.height;

        Self {
            enabled,
            check_interval: Duration::from_secs(check_interval_secs),
            update_threshold,
            current_height,
            zebra_rpc_url,
            update_script_path,
        }
    }
}

/// Snapshot updater background task
pub struct SnapshotUpdater {
    config: Arc<SnapshotUpdaterConfig>,
}

impl SnapshotUpdater {
    pub fn new(config: SnapshotUpdaterConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Start the background update task
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                tracing::error!("Snapshot updater task failed: {}", e);
            }
        })
    }

    /// Main background task loop
    async fn run(&self) -> Result<()> {
        tracing::info!(
            "Snapshot updater started (check every {}s, threshold: {} blocks)",
            self.config.check_interval.as_secs(),
            self.config.update_threshold
        );

        loop {
            sleep(self.config.check_interval).await;

            if let Err(e) = self.check_and_update().await {
                tracing::error!("Snapshot update check failed: {}", e);
                // Continue running despite errors
            }
        }
    }

    /// Check if update is needed and trigger if necessary
    async fn check_and_update(&self) -> Result<()> {
        // Get current Zebra height
        let zebra_height = self.get_zebra_height().await?;

        tracing::debug!(
            "Zebra height: {}, Snapshot height: {}, Diff: {}",
            zebra_height,
            self.config.current_height,
            zebra_height.saturating_sub(self.config.current_height)
        );

        // Check if update is needed
        let blocks_behind = zebra_height.saturating_sub(self.config.current_height);

        if blocks_behind >= self.config.update_threshold {
            tracing::info!(
                "Snapshot is {} blocks behind (threshold: {}), triggering update...",
                blocks_behind,
                self.config.update_threshold
            );

            self.trigger_update(zebra_height).await?;
        } else {
            tracing::debug!(
                "Snapshot is up to date ({} blocks behind threshold)",
                self.config.update_threshold.saturating_sub(blocks_behind)
            );
        }

        Ok(())
    }

    /// Get current Zebra height via RPC
    async fn get_zebra_height(&self) -> Result<u32> {
        let output = Command::new("./scripts/get_zebra_height.sh")
            .arg("--rpc-url")
            .arg(&self.config.zebra_rpc_url)
            .output()
            .await?;

        if !output.status.success() {
            return Err(eyre!(
                "Failed to get Zebra height: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let height_str = String::from_utf8_lossy(&output.stdout);
        let height = height_str.trim().parse::<u32>()?;

        Ok(height)
    }

    /// Trigger snapshot update
    async fn trigger_update(&self, height: u32) -> Result<()> {
        tracing::info!("Starting snapshot update to height {}", height);

        let output = Command::new(&self.config.update_script_path)
            .arg("--height")
            .arg(height.to_string())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("Snapshot update failed: {}", stderr);
            return Err(eyre!("Snapshot update failed: {}", stderr));
        }

        tracing::info!("Snapshot update completed successfully");
        Ok(())
    }
}

/// API handler for manual snapshot update trigger
pub async fn trigger_manual_update(height: Option<u32>) -> Result<String> {
    tracing::info!("Manual snapshot update triggered");

    let mut cmd = Command::new("./scripts/update_snapshot.sh");

    if let Some(h) = height {
        cmd.arg("--height").arg(h.to_string());
    }

    let output = cmd.output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("Update failed: {}", stderr));
    }

    Ok("Snapshot update started".to_string())
}

/// Get current snapshot update status
pub async fn get_update_status(config: &SnapshotUpdaterConfig) -> Result<serde_json::Value> {
    // Get Zebra height
    let zebra_height = Command::new("./scripts/get_zebra_height.sh")
        .output()
        .await
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u32>()
                    .ok()
            } else {
                None
            }
        });

    let blocks_behind = zebra_height.map(|h| h.saturating_sub(config.current_height));

    Ok(serde_json::json!({
        "enabled": config.enabled,
        "current_snapshot_height": config.current_height,
        "zebra_height": zebra_height,
        "blocks_behind": blocks_behind,
        "update_threshold": config.update_threshold,
        "check_interval_seconds": config.check_interval.as_secs(),
        "needs_update": blocks_behind.map(|b| b >= config.update_threshold),
    }))
}
