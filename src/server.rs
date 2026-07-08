use actix_cors::Cors;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::config::{self, normalize_server_type, AppConfig, McpServerConfig};
use crate::connection::{ConnectionManager, ServerConnectionConfig};
use crate::mcp::types::Tool;
use crate::openapi;

pub struct AppState {
    pub api_key: Option<String>,
    pub connections: Arc<Mutex<HashMap<String, Arc<Mutex<ConnectionManager>>>>>,
    pub tools_map: Arc<Mutex<HashMap<String, Vec<Tool>>>>,
}

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub cors_allow_origins: Vec<String>,
    pub config_path: Option<String>,
    pub server_type: Option<String>,
    pub server_command: Option<Vec<String>>,
    pub name: String,
    pub description: String,
    pub path_prefix: String,
    pub strict_auth: bool,
    pub headers: Option<HashMap<String, String>>,
    pub hot_reload: bool,
}

fn build_cors(origins: &[String]) -> Cors {
    let mut cors = Cors::default()
        .allow_any_method()
        .allow_any_header()
        .supports_credentials();

    if origins.is_empty() || origins.contains(&"*".to_string()) {
        cors = cors.allow_any_origin();
    } else {
        for origin in origins {
            cors = cors.allowed_origin(origin);
        }
    }

    cors
}

async fn init_connections_from_config(
    config: &AppConfig,
) -> HashMap<String, Arc<Mutex<ConnectionManager>>> {
    let mut connections = HashMap::new();

    for (name, server_cfg) in &config.mcp_servers {
        info!("Initializing connection for server: {}", name);
        match create_connection_manager(name, server_cfg) {
            Ok(mgr) => {
                let mgr = Arc::new(Mutex::new(mgr));
                connections.insert(name.clone(), mgr);
                info!("Connection manager created for server: {}", name);
            }
            Err(e) => {
                error!("Failed to create connection for server '{}': {}", name, e);
            }
        }
    }

    connections
}

fn create_connection_manager(
    name: &str,
    server_cfg: &McpServerConfig,
) -> anyhow::Result<ConnectionManager> {
    let server_type = normalize_server_type(server_cfg.server_type.as_deref().unwrap_or("stdio"));

    let conn_config = if server_type == "stdio" {
        let command = server_cfg
            .command
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Missing command for stdio server '{}'", name))?;
        let args = server_cfg.args.clone().unwrap_or_default();
        let env = server_cfg.env.clone().unwrap_or_default();

        ServerConnectionConfig {
            server_type: "stdio".to_string(),
            command: Some(command),
            args,
            env,
            url: None,
            headers: None,
        }
    } else {
        let url = server_cfg
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Missing URL for remote server '{}'", name))?;

        ServerConnectionConfig {
            server_type: server_type.to_string(),
            command: None,
            args: vec![url.clone()],
            env: HashMap::new(),
            url: Some(url),
            headers: server_cfg.headers.clone(),
        }
    };

    Ok(ConnectionManager::new(conn_config))
}

async fn handle_docs_page(req: HttpRequest) -> HttpResponse {
    let path = req.path().to_string();
    let base = path.trim_end_matches("/docs").trim_end_matches('/');
    let spec_url = format!("{}/openapi.json", base);

    let html = format!(
        r##"<!DOCTYPE html>
<html>
<head>
    <title>MCPO API Docs</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link rel="stylesheet" type="text/css" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css">
    <style>html{{box-sizing:border-box;overflow:-moz-scrollbars-vertical;overflow-y:scroll}}*,*:before,*:after{{box-sizing:inherit}}body{{margin:0;background:#fafafa}}</style>
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
<script>window.onload=function(){{window.ui=SwaggerUIBundle({{url:"{}",dom_id:"#swagger-ui",presets:[SwaggerUIBundle.presets.apis,SwaggerUIBundle.SwaggerUIStandalonePreset],layout:"BaseLayout"}})}}</script>
</body>
</html>"##,
        spec_url
    );

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

async fn handle_openapi_spec(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let path = req.path();
    let server_name = extract_server_name(path);

    // Try to connect lazily if tools not yet loaded
    {
        let tools_map = state.tools_map.lock().await;
        if !tools_map.contains_key(&server_name) {
            drop(tools_map);
            let connections = state.connections.lock().await;
            if let Some(conn_arc) = connections.get(&server_name) {
                let mut conn = conn_arc.lock().await;
                if conn.session().is_none() {
                    match conn.connect().await {
                        Ok(session) => {
                            let mut tools_map = state.tools_map.lock().await;
                            tools_map.insert(server_name.clone(), session.tools.clone());
                        }
                        Err(e) => {
                            tracing::error!("Failed to connect to '{}': {}", server_name, e);
                        }
                    }
                }
            }
        }
    }

    let tools_map = state.tools_map.lock().await;
    if let Some(tools) = tools_map.get(&server_name) {
        let host = req
            .headers()
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost");

        let spec = openapi::generate_openapi_spec(
            &server_name,
            &format!("{} MCP Server API", server_name),
            tools,
            host,
            "",
        );

        HttpResponse::Ok().json(spec)
    } else {
        HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Server '{}' not found or not connected", server_name)
        }))
    }
}

async fn handle_tool_call(
    req: HttpRequest,
    body: web::Json<Value>,
    state: web::Data<AppState>,
) -> HttpResponse {
    let path = req.path();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let (server_name, tool_name) = if segments.len() >= 2 {
        (segments[0].to_string(), segments[1..].join("_"))
    } else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "Invalid path"}));
    };

    let connections = state.connections.lock().await;

    if let Some(conn_arc) = connections.get(&server_name) {
        let mut conn = conn_arc.lock().await;

        if conn.session().is_none() {
            info!("Establishing connection to MCP server: {}", server_name);
            match conn.connect().await {
                Ok(session) => {
                    let mut tools_map = state.tools_map.lock().await;
                    tools_map.insert(server_name.clone(), session.tools.clone());
                    info!(
                        "Connected to MCP server: {} ({} tools)",
                        server_name,
                        session.tools.len()
                    );
                }
                Err(e) => {
                    error!("Failed to connect to MCP server '{}': {}", server_name, e);
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": format!("Failed to connect to MCP server: {}", e)
                    }));
                }
            }
        }

        let args = body.into_inner();
        match conn.call_tool(&tool_name, args).await {
            Ok(result) => {
                let content = result.get("content").and_then(|c| c.as_array());
                let is_error = result
                    .get("isError")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);

                if is_error {
                    let msg = content
                        .and_then(|arr| arr.first())
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("Unknown error");
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "message": msg
                    }));
                }

                let response = if let Some(items) = content {
                    let items: Vec<_> = items
                        .iter()
                        .map(|c| {
                            if let Some(text) = c.get("text") {
                                text.clone()
                            } else {
                                c.clone()
                            }
                        })
                        .collect();
                    if items.len() == 1 {
                        items.into_iter().next().unwrap()
                    } else {
                        Value::Array(items)
                    }
                } else {
                    result
                };

                HttpResponse::Ok().json(response)
            }
            Err(e) => {
                error!(
                    "Error calling tool '{}' on '{}': {}",
                    tool_name, server_name, e
                );
                HttpResponse::InternalServerError().json(serde_json::json!({
                    "message": format!("Tool execution error: {}", e)
                }))
            }
        }
    } else {
        HttpResponse::NotFound().json(serde_json::json!({
            "error": format!("Server '{}' not found", server_name)
        }))
    }
}

fn extract_server_name(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() >= 2 && segments.last() == Some(&"openapi.json") {
        segments[segments.len() - 2].to_string()
    } else {
        segments.first().unwrap_or(&"unknown").to_string()
    }
}

pub async fn serve(config: ServerConfig) -> anyhow::Result<()> {
    let api_key = config.api_key.clone();
    let host = config.host.clone();
    let port = config.port;
    let cors_origins = config.cors_allow_origins.clone();
    let name = config.name.clone();

    let app_config = if let Some(ref config_path) = config.config_path {
        info!("Loading config from: {}", config_path);
        config::load_config(config_path)?
    } else if let Some(ref server_command) = config.server_command {
        let server_type = config.server_type.as_deref().unwrap_or("stdio");
        info!("Single server mode: {} {:?}", server_type, server_command);

        let mut servers = HashMap::new();
        let server_cfg = if server_type == "stdio" {
            McpServerConfig {
                command: Some(server_command[0].clone()),
                args: Some(server_command[1..].to_vec()),
                env: None,
                url: None,
                server_type: Some("stdio".to_string()),
                headers: config.headers.clone(),
                disabled_tools: None,
                oauth: None,
            }
        } else {
            McpServerConfig {
                command: None,
                args: None,
                env: None,
                url: Some(server_command[0].clone()),
                server_type: Some(server_type.to_string()),
                headers: config.headers.clone(),
                disabled_tools: None,
                oauth: None,
            }
        };

        servers.insert(name.clone(), server_cfg);
        AppConfig {
            mcp_servers: servers,
        }
    } else {
        return Err(anyhow::anyhow!(
            "Either --config or a server command must be provided"
        ));
    };

    let connections = init_connections_from_config(&app_config).await;

    let mut tools_map: HashMap<String, Vec<Tool>> = HashMap::new();
    for (server_name, conn_arc) in &connections {
        let mut conn = conn_arc.lock().await;
        match conn.connect().await {
            Ok(session) => {
                let disabled: Vec<String> = app_config
                    .mcp_servers
                    .get(server_name)
                    .and_then(|c| c.disabled_tools.clone())
                    .unwrap_or_default();

                let filtered_tools: Vec<Tool> = session
                    .tools
                    .into_iter()
                    .filter(|t| !disabled.contains(&t.name))
                    .collect();

                info!(
                    "Connected to '{}': {} tools available ({} disabled)",
                    server_name,
                    filtered_tools.len(),
                    disabled.len()
                );
                tools_map.insert(server_name.clone(), filtered_tools);
            }
            Err(e) => {
                warn!("Failed to connect to '{}': {}", server_name, e);
            }
        }
    }

    let state = web::Data::new(AppState {
        api_key,
        connections: Arc::new(Mutex::new(connections)),
        tools_map: Arc::new(Mutex::new(tools_map)),
    });

    let app_config_for_server = app_config;
    let servers_list = app_config_for_server
        .mcp_servers
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    let app_config_for_scope = app_config_for_server.clone();

    info!("Starting MCPO-RS Server...");
    info!("  Name: {}", name);
    info!("  Host: {}:{}", host, port);
    info!(
        "  API Key: {}",
        if state.api_key.as_deref().is_none_or(|k| k.is_empty()) {
            "Not Provided"
        } else {
            "Provided"
        }
    );
    info!("  Servers: {:?}", servers_list);

    HttpServer::new(move || {
        let cors = build_cors(&cors_origins);
        let mut app = App::new()
            .wrap(cors)
            .app_data(state.clone());

        // Build scopes for each server inline to avoid type issues
        for server_name in app_config_for_scope.mcp_servers.keys() {
            let scope_path = format!("/{}", server_name);
            let scope = actix_web::web::scope(&scope_path)
                .app_data(state.clone())
                .route("/openapi.json", web::get().to(handle_openapi_spec))
                .route("/docs", web::get().to(handle_docs_page))
                .route("/{tool_name}", web::post().to(handle_tool_call));
            app = app.service(scope);
        }

        let servers_list_clone = servers_list.clone();
        app = app.service(
            web::scope("")
                .route("/", web::get().to(move || {
                    let servers = servers_list_clone.clone();
                    async move {
                        let mut links = String::new();
                        for s in &servers {
                            links.push_str(&format!(
                                r#"<li><a href="/{s}/docs">{s} API Docs</a></li>"#,
                                s = s
                            ));
                        }
                        let html = format!(
                            r##"<!DOCTYPE html>
<html>
<head>
    <title>MCPO-RS - MCP Servers</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
               max-width: 800px; margin: 40px auto; padding: 20px; background: #f5f5f5; }}
        h1 {{ color: #333; }}
        ul {{ list-style: none; padding: 0; }}
        li {{ margin: 10px 0; }}
        a {{ color: #2563eb; text-decoration: none; font-size: 18px; }}
        a:hover {{ text-decoration: underline; }}
        .card {{ background: white; border-radius: 8px; padding: 20px; margin: 10px 0; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }}
    </style>
</head>
<body>
    <h1>MCPO-RS - MCP-to-OpenAPI Proxy</h1>
    <p>Available MCP Servers:</p>
    <div class="card">
        <ul>{links}</ul>
    </div>
</body>
</html>"##
                        );
                        HttpResponse::Ok()
                            .content_type("text/html; charset=utf-8")
                            .body(html)
                    }
                }))
                .route(
                    "/health",
                    web::get().to(|| async { HttpResponse::Ok().json(serde_json::json!({"status": "ok"})) }),
                ),
        );

        app
    })
    .bind(format!("{}:{}", host, port))?
    .run()
    .await?;

    Ok(())
}
