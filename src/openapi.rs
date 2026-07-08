use serde_json::{json, Value};

use crate::mcp::types::Tool;

pub fn generate_openapi_spec(
    server_name: &str,
    description: &str,
    tools: &[Tool],
    host: &str,
    path_prefix: &str,
) -> Value {
    let base_path = path_prefix.trim_end_matches('/');
    let tool_path_prefix = if base_path.is_empty() || base_path == "/" {
        String::new()
    } else {
        format!("{}/{}", base_path, server_name)
    };

    let servers_url = if host.contains("://") {
        format!("{}/{}", host.trim_end_matches('/'), server_name)
    } else {
        format!("http://{}/{}", host, server_name)
    };

    let mut paths = serde_json::Map::new();
    let mut schemas = serde_json::Map::new();

    for tool in tools {
        let route = format!("{}/{}", tool_path_prefix, tool.name);
        let input_schema = tool.input_schema.clone();

        let schema_name = format!("{}_params", tool.name);
        let mut schema = json!({
            "type": "object",
            "properties": {}
        });

        let props = input_schema.get("properties");
        if let Some(properties) = props {
            let required: Vec<String> = input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            schema["properties"] = properties.clone();

            if let Some(defs) = input_schema.get("$defs") {
                if let Some(defs_obj) = defs.as_object() {
                    for (def_name, def_schema) in defs_obj {
                        schemas.insert(def_name.clone(), def_schema.clone());
                    }
                }
            }

            if !required.is_empty() {
                schema["required"] = json!(required);
            }
        }

        schemas.insert(schema_name.clone(), schema.clone());

        let mut post_op = json!({
            "summary": tool.name.replace('_', " "),
            "description": tool.description.clone().unwrap_or_default(),
            "operationId": format!("{}_{}", server_name, tool.name),
            "requestBody": {
                "required": false,
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": format!("#/components/schemas/{}", schema_name)
                        }
                    }
                }
            },
            "responses": {
                "200": {
                    "description": "Successful response",
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object"
                            }
                        }
                    }
                },
                "400": {
                    "description": "Bad request"
                },
                "500": {
                    "description": "Internal server error"
                }
            }
        });

        if let Some(tags) = post_op.get_mut("tags") {
            if let Some(arr) = tags.as_array_mut() {
                arr.push(json!(server_name));
            }
        } else {
            post_op["tags"] = json!([server_name]);
        }

        let mut path_obj = serde_json::Map::new();
        path_obj.insert("post".to_string(), post_op);
        paths.insert(route, Value::Object(path_obj));
    }

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": format!("{} MCP Server", server_name),
            "description": description,
            "version": "1.0.0"
        },
        "servers": [
            {
                "url": servers_url,
                "description": format!("{} server", server_name)
            }
        ],
        "paths": paths,
        "components": {
            "schemas": schemas
        }
    })
}
