use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::client::McpTransport;

pub struct StreamableHttpTransport {
    client: reqwest::Client,
    url: String,
    headers: std::collections::HashMap<String, String>,
    request_id: i64,
    session_id: Option<String>,
}

impl StreamableHttpTransport {
    pub async fn new(
        url: &str,
        headers: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Self> {
        Ok(StreamableHttpTransport {
            client: reqwest::Client::new(),
            url: url.to_string(),
            headers: headers.unwrap_or_default(),
            request_id: 1,
            session_id: None,
        })
    }

    fn next_id(&mut self) -> i64 {
        let id = self.request_id;
        self.request_id += 1;
        id
    }
}

#[async_trait]
impl McpTransport for StreamableHttpTransport {
    async fn send_request_raw(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let request = crate::mcp::types::JsonRpcRequest::new(self.next_id(), method, params);
        let body = serde_json::to_string(&request)?;

        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        if let Some(ref sid) = self.session_id {
            req = req.header("mcp-session-id", sid.as_str());
        }

        let response = req
            .body(body)
            .send()
            .await
            .context("Failed to send streamable HTTP request")?;

        if let Some(sid) = response.headers().get("mcp-session-id") {
            self.session_id = Some(sid.to_str().unwrap_or("").to_string());
        }

        let text = response.text().await?;

        if text.is_empty() {
            return Err(anyhow::anyhow!(
                "Empty response from streamable HTTP server"
            ));
        }

        let value: Value =
            serde_json::from_str(&text).context("Failed to parse streamable HTTP response")?;
        Ok(value)
    }

    async fn send_notification(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        self.send_request_raw(method, params).await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
