use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub server_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "disabledTools", alias = "disabled_tools")]
    pub disabled_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub server_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_loopback: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

pub fn load_config(path: &str) -> Result<AppConfig> {
    let content =
        std::fs::read_to_string(path).context(format!("Failed to read config file: {}", path))?;
    let config: AppConfig =
        serde_json::from_str(&content).context("Failed to parse config file as JSON")?;

    if config.mcp_servers.is_empty() {
        return Err(anyhow::anyhow!("No 'mcpServers' found in config file."));
    }

    for (name, server) in &config.mcp_servers {
        validate_server_config(name, server)?;
    }

    Ok(config)
}

pub fn validate_server_config(name: &str, config: &McpServerConfig) -> Result<()> {
    let server_type = config.server_type.as_deref().unwrap_or("stdio");

    if matches!(server_type, "sse" | "streamable-http") {
        if config.url.is_none() {
            return Err(anyhow::anyhow!(
                "Server '{}' of type '{}' requires a 'url' field",
                name,
                server_type
            ));
        }
    } else if config.command.is_some() {
    } else {
        return Err(anyhow::anyhow!(
            "Server '{}' must have either 'command' for stdio or 'type' and 'url' for remote servers",
            name
        ));
    }

    let _disabled = &config.disabled_tools;

    Ok(())
}

pub fn normalize_server_type(server_type: &str) -> &str {
    match server_type {
        "streamable_http" | "streamablehttp" | "streamable-http" => "streamable-http",
        _ => server_type,
    }
}
