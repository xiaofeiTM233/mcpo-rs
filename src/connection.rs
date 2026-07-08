use anyhow::Result;
use std::collections::HashMap;

use crate::mcp::client::{McpClient, McpSession};
use crate::mcp::sse::SseTransport;
use crate::mcp::stdio::StdioTransport;
use crate::mcp::streamable_http::StreamableHttpTransport;

#[derive(Debug, Clone)]
pub struct ServerConnectionConfig {
    pub server_type: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

pub struct ConnectionManager {
    config: ServerConnectionConfig,
    client: Option<McpClient>,
    session: Option<McpSession>,
}

impl ConnectionManager {
    pub fn new(config: ServerConnectionConfig) -> Self {
        ConnectionManager {
            config,
            client: None,
            session: None,
        }
    }

    pub async fn connect(&mut self) -> Result<McpSession> {
        let mut client = self.create_client().await?;
        let init_result = client.initialize().await?;
        let tools = client.list_tools().await?;

        let session = McpSession {
            tools,
            initialize_result: init_result,
        };

        self.client = Some(client);
        self.session = Some(session.clone());
        Ok(session)
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let client = self.client.as_mut().unwrap();
        let result = client.call_tool(name, Some(args)).await?;

        Ok(serde_json::to_value(result)?)
    }

    pub fn session(&self) -> Option<&McpSession> {
        self.session.as_ref()
    }

    async fn create_client(&mut self) -> Result<McpClient> {
        match self.config.server_type.as_str() {
            "stdio" => {
                let command = self.config.command.as_ref().unwrap();
                let transport =
                    StdioTransport::new(command, &self.config.args, &self.config.env).await?;
                Ok(McpClient::new(Box::new(transport)))
            }
            "sse" => {
                let url = self.config.url.as_ref().unwrap();
                let transport = SseTransport::new(url, self.config.headers.clone()).await?;
                Ok(McpClient::new(Box::new(transport)))
            }
            "streamable-http" | "streamable_http" | "streamablehttp" => {
                let url = self.config.url.as_ref().unwrap();
                let transport =
                    StreamableHttpTransport::new(url, self.config.headers.clone()).await?;
                Ok(McpClient::new(Box::new(transport)))
            }
            _ => Err(anyhow::anyhow!(
                "Unsupported server type: {}",
                self.config.server_type
            )),
        }
    }
}
