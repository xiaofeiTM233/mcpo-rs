use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::types::{CallToolResult, InitializeResult, ListToolsResult, Tool};

#[derive(Clone)]
pub struct McpSession {
    pub tools: Vec<Tool>,
    pub initialize_result: InitializeResult,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send_request_raw(&mut self, method: &str, params: Option<Value>) -> Result<Value>;
    async fn send_notification(&mut self, method: &str, params: Option<Value>) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}

pub struct McpClient {
    transport: Box<dyn McpTransport>,
    request_id: i64,
}

impl McpClient {
    pub fn new(transport: Box<dyn McpTransport>) -> Self {
        McpClient {
            transport,
            request_id: 1,
        }
    }

    fn next_id(&mut self) -> i64 {
        let id = self.request_id;
        self.request_id += 1;
        id
    }

    async fn call(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let response = self.transport.send_request_raw(method, params).await?;
        extract_result(response)
    }

    pub async fn initialize(&mut self) -> Result<InitializeResult> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "mcpo-rs",
                "version": "0.1.0"
            }
        });
        let result = self.call("initialize", Some(params)).await?;
        let init_result: InitializeResult = serde_json::from_value(result)?;

        let notif_params = serde_json::json!({});
        self.transport
            .send_notification("notifications/initialized", Some(notif_params))
            .await?;

        Ok(init_result)
    }

    pub async fn list_tools(&mut self) -> Result<Vec<Tool>> {
        let result = self.call("tools/list", None).await?;
        let list_result: ListToolsResult = serde_json::from_value(result)?;
        Ok(list_result.tools)
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments.unwrap_or(serde_json::json!({}))
        });
        let result = self.call("tools/call", Some(params)).await?;
        let call_result: CallToolResult = serde_json::from_value(result)?;
        Ok(call_result)
    }

    pub async fn close(&mut self) -> Result<()> {
        self.transport.close().await
    }
}

fn extract_result(response: Value) -> Result<Value> {
    if let Some(err) = response.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown RPC error");
        return Err(anyhow::anyhow!("MCP RPC error: {}", msg));
    }
    response
        .get("result")
        .cloned()
        .context("JSON-RPC response missing 'result' field")
}
