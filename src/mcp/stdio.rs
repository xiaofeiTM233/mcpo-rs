use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use super::client::McpTransport;

pub struct StdioTransport {
    process: Child,
    reader: Option<BufReader<tokio::process::ChildStdout>>,
    stdin: Option<tokio::process::ChildStdin>,
}

impl StdioTransport {
    pub async fn new(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut process = cmd.spawn().context("Failed to spawn MCP server process")?;

        let stdout = process.stdout.take().context("Failed to capture stdout")?;
        let stdin = process.stdin.take().context("Failed to capture stdin")?;

        let reader = Some(BufReader::new(stdout));

        Ok(StdioTransport {
            process,
            reader,
            stdin: Some(stdin),
        })
    }

    async fn read_response(&mut self) -> Result<Value> {
        let reader = self.reader.as_mut().context("Reader not available")?;
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .context("Failed to read from stdout")?;

        if line.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response from MCP server"));
        }

        let response: Value =
            serde_json::from_str(line.trim()).context("Failed to parse JSON response")?;
        Ok(response)
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send_request_raw(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let request = crate::mcp::types::JsonRpcRequest::new(1, method, params);
        let json = serde_json::to_string(&request)?;

        let stdin = self.stdin.as_mut().context("Stdin not available")?;
        stdin
            .write_all(format!("{}\n", json).as_bytes())
            .await
            .context("Failed to write request")?;
        stdin.flush().await.context("Failed to flush stdin")?;

        self.read_response().await
    }

    async fn send_notification(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let request = crate::mcp::types::JsonRpcRequest::notification(method, params);
        let json = serde_json::to_string(&request)?;

        let stdin = self.stdin.as_mut().context("Stdin not available")?;
        stdin
            .write_all(format!("{}\n", json).as_bytes())
            .await
            .context("Failed to write notification")?;
        stdin.flush().await.context("Failed to flush stdin")?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        let _ = self.process.kill().await;
        self.process.wait().await.ok();
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.process.start_kill();
    }
}
