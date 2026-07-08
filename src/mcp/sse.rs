use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::client::McpTransport;

pub struct SseTransport {
    client: reqwest::Client,
    base_url: String,
    headers: std::collections::HashMap<String, String>,
    message_endpoint: Option<String>,
    request_id: i64,
}

impl SseTransport {
    pub async fn new(
        url: &str,
        headers: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Self> {
        let mut transport = SseTransport {
            client: reqwest::Client::new(),
            base_url: url.to_string(),
            headers: headers.unwrap_or_default(),
            message_endpoint: None,
            request_id: 1,
        };

        transport.connect().await?;
        Ok(transport)
    }

    async fn connect(&mut self) -> Result<()> {
        let mut req = self.client.get(&self.base_url);
        req = req.header("Accept", "text/event-stream");

        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let response = req
            .send()
            .await
            .context("Failed to connect to SSE endpoint")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "SSE connection failed with status: {}",
                response.status()
            ));
        }

        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        let mut event_type = String::new();
        let mut data = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read SSE chunk")?;
            let text = String::from_utf8_lossy(&chunk);

            for line in text.lines() {
                if line.is_empty() {
                    if event_type == "endpoint" {
                        self.message_endpoint = Some(data.trim().to_string());
                        return Ok(());
                    }
                    event_type.clear();
                    data.clear();
                } else if let Some(field_value) = line.strip_prefix("event: ") {
                    event_type = field_value.to_string();
                } else if let Some(field_value) = line.strip_prefix("data: ") {
                    data.push_str(field_value);
                }
            }
        }

        Err(anyhow::anyhow!(
            "SSE connection closed without endpoint event"
        ))
    }

    fn next_id(&mut self) -> i64 {
        let id = self.request_id;
        self.request_id += 1;
        id
    }

    async fn post_request(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let endpoint = self
            .message_endpoint
            .as_ref()
            .context("Message endpoint not available")?
            .clone();

        let request_id = self.next_id();
        let request = crate::mcp::types::JsonRpcRequest::new(request_id, method, params);
        let body = serde_json::to_string(&request)?;

        let mut req = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json");

        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let response = req
            .body(body)
            .send()
            .await
            .context("Failed to send SSE POST request")?;

        if !response.status().is_success() {
            if response.status().as_u16() == 405 {
                return Err(anyhow::anyhow!("SSE message endpoint returned 405 Method Not Allowed, the server may not support POST messaging"));
            }
            return Err(anyhow::anyhow!(
                "SSE POST failed with status: {}",
                response.status()
            ));
        }

        let text = response.text().await?;
        let value: Value =
            serde_json::from_str(&text).context("Failed to parse SSE POST response")?;
        Ok(value)
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    async fn send_request_raw(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        self.post_request(method, params).await
    }

    async fn send_notification(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        self.post_request(method, params).await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
