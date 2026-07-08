#![allow(dead_code)]

mod auth;
mod config;
mod connection;
mod mcp;
mod openapi;
mod server;
mod watcher;

use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "mcpo")]
#[command(version = "0.1.0")]
#[command(about = "A simple, secure MCP-to-OpenAPI proxy server written in Rust", long_about = None)]
struct Cli {
    #[arg(short = 'H', long, default_value = "0.0.0.0", help = "Host address")]
    host: String,

    #[arg(short = 'p', long, default_value = "8000", help = "Port number")]
    port: u16,

    #[arg(long, help = "CORS allowed origins (comma-separated)")]
    cors_allow_origins: Option<String>,

    #[arg(short = 'k', long, help = "API key for authentication")]
    api_key: Option<String>,

    #[arg(
        long,
        default_value = "false",
        help = "API key protects all endpoints and documentation"
    )]
    strict_auth: bool,

    #[arg(
        long = "type",
        visible_alias = "server-type",
        default_value = "stdio",
        help = "Server type (stdio, sse, streamable-http)"
    )]
    server_type: String,

    #[arg(short = 'c', long, help = "Config file path")]
    config: Option<String>,

    #[arg(
        short = 'n',
        long,
        default_value = "MCP OpenAPI Proxy",
        help = "Server name"
    )]
    name: String,

    #[arg(
        short = 'd',
        long,
        default_value = "Automatically generated API from MCP Tool Schemas",
        help = "Server description"
    )]
    description: String,

    #[arg(
        long = "root-path",
        default_value = "",
        help = "Root path for reverse proxy"
    )]
    root_path: String,

    #[arg(long = "path-prefix", default_value = "/", help = "URL prefix")]
    path_prefix: String,

    #[arg(short = 'H', long = "header", help = "Headers in JSON format")]
    headers: Option<String>,

    #[arg(
        long,
        default_value = "false",
        help = "Enable hot reload for config file changes"
    )]
    hot_reload: bool,

    #[arg(
        long,
        default_value = "info",
        help = "Log level (trace, debug, info, warn, error)"
    )]
    log_level: String,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    server_command: Vec<String>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = EnvFilter::try_new(&cli.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let cors_allow_origins: Vec<String> = cli
        .cors_allow_origins
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let headers: Option<std::collections::HashMap<String, String>> = cli
        .headers
        .as_ref()
        .and_then(|h| serde_json::from_str(h).ok());

    // Handle -- separator for server command
    let server_command = if cli.config.is_none() && !cli.server_command.is_empty() {
        let command_str = cli.server_command.join(" ");

        // Handle the case where clap doesn't consume -- correctly
        if let Some(idx) = std::env::args().position(|a| a == "--") {
            let server_args: Vec<String> = std::env::args().skip(idx + 1).collect();
            if !server_args.is_empty() {
                Some(server_args)
            } else {
                None
            }
        } else if !command_str.is_empty() {
            Some(cli.server_command.clone())
        } else {
            None
        }
    } else {
        None
    };

    info!("Starting MCPO-RS v{}", env!("CARGO_PKG_VERSION"));

    let server_config = server::ServerConfig {
        host: cli.host,
        port: cli.port,
        api_key: cli.api_key,
        cors_allow_origins,
        config_path: cli.config,
        server_type: if server_command.is_some() {
            Some(cli.server_type)
        } else {
            None
        },
        server_command,
        name: cli.name,
        description: cli.description,
        path_prefix: cli.path_prefix,
        strict_auth: cli.strict_auth,
        headers,
        hot_reload: cli.hot_reload,
    };

    server::serve(server_config).await
}
