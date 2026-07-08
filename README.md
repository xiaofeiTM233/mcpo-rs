# ⚡️ mcpo-rs

> MCP-to-OpenAPI proxy server, rewritten in Rust

将任意 MCP Server 暴露为 OpenAPI 兼容的 HTTP 服务，零配置、即时可用。

本项目参考 [open-webui/mcpo](https://github.com/open-webui/mcpo) 用 Rust 重写，代码由 AI 辅助生成。

## 与原版 mcpo 的区别

| | mcpo (Python) | mcpo-rs (Rust) |
| --- | --- | --- |
| 启动速度 | ~1s (含 Python 解释器初始化) | <10ms |
| 内存占用 | ~50MB+ | ~5MB |
| 二进制大小 | 不适用 (需 Python 运行时) | ~8.5MB (单文件) |
| 运行时依赖 | Python 3.11+、uv/pip | 无 |
| 跨平台 | 需 Python 环境 | 单二进制，Windows/Linux/macOS 通用 |

## 快速开始

### 安装

从 GitHub Releases 下载对应平台的二进制文件：

- `mcpo-linux-x86_64` — Linux x86_64
- `mcpo-linux-aarch64` — Linux ARM64
- `mcpo-linux-musl` — Linux (静态链接，兼容所有发行版)
- `mcpo-macos-x86_64` — macOS Intel
- `mcpo-macos-aarch64` — macOS Apple Silicon
- `mcpo-windows-x86_64.exe` — Windows x86_64

或从源码编译：

```bash
cargo build --release
```

### 基本用法

```bash
# 代理一个 stdio MCP 服务器
mcpo --port 8000 --api-key "your-secret" -- uvx mcp-server-time --local-timezone=America/New_York

# 代理 SSE MCP 服务器
mcpo --port 8000 --type sse -- http://127.0.0.1:8001/sse

# 代理 Streamable HTTP MCP 服务器
mcpo --port 8000 --type streamable-http -- http://127.0.0.1:8002/mcp
```

启动后访问：

- API 文档：`http://localhost:8000/docs`
- OpenAPI 3.0 规范：`http://localhost:8000/<server>/openapi.json`
- Swagger UI：`http://localhost:8000/<server>/docs`
- 健康检查：`http://localhost:8000/health`

### 配置文件模式

支持 Claude Desktop 格式的多服务器配置：

```bash
mcpo --config /path/to/config.json --hot-reload
```

`config.json`：

```json
{
  "mcpServers": {
    "memory": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-memory"]
    },
    "time": {
      "command": "uvx",
      "args": ["mcp-server-time", "--local-timezone=America/New_York"],
      "disabledTools": ["convert_time"]
    },
    "remote_server": {
      "type": "sse",
      "url": "http://127.0.0.1:8001/sse",
      "headers": {
        "Authorization": "Bearer token"
      }
    },
    "streamable_server": {
      "type": "streamable-http",
      "url": "http://127.0.0.1:8002/mcp"
    }
  }
}
```

每个服务器有独立的 API 路由：

- `http://localhost:8000/memory` — memory 服务器
- `http://localhost:8000/time` — time 服务器
- `http://localhost:8000/remote_server` — 远程 SSE 服务器
- `http://localhost:8000/streamable_server` — Streamable HTTP 服务器

### 热重载

启用 `--hot-reload` 后，修改配置文件会自动重载服务器列表：

```bash
mcpo --config config.json --hot-reload
```

- 新增服务器 → 自动添加路由
- 删除服务器 → 自动卸载路由
- 修改服务器配置 → 自动重连

### API Key 认证

```bash
mcpo --api-key "my-secret-key" -- your_mcp_command
```

客户端请求时需携带 Header：

```http
Authorization: Bearer my-secret-key
```

## CLI 参数

| 参数 | 简写 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `--host` | `-H` | `0.0.0.0` | 监听地址 |
| `--port` | `-p` | `8000` | 监听端口 |
| `--api-key` | `-k` | — | API 认证密钥 |
| `--type` | — | `stdio` | 服务器类型：stdio / sse / streamable-http |
| `--config` | `-c` | — | 配置文件路径 |
| `--name` | `-n` | `MCP OpenAPI Proxy` | 服务器名称 |
| `--description` | `-d` | — | 服务器描述 |
| `--cors-allow-origins` | — | `*` | CORS 允许的来源（逗号分隔） |
| `--path-prefix` | — | `/` | URL 前缀 |
| `--root-path` | — | — | 反向代理根路径 |
| `--header` | — | — | SSE/HTTP 请求头（JSON 格式） |
| `--hot-reload` | — | false | 启用配置文件热重载 |
| `--log-level` | — | `info` | 日志级别 |
| `--strict-auth` | — | false | API Key 保护所有端点 |

## OpenAPI 3.0

每个 MCP 服务器自动生成 OpenAPI 3.0 JSON 规范：

```http
GET /{server}/openapi.json
```

返回格式：

```json
{
  "openapi": "3.0.3",
  "info": {
    "title": "time MCP Server",
    "version": "1.0.0"
  },
  "paths": {
    "/time/get_current_time": {
      "post": {
        "operationId": "time_get_current_time",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/get_current_time_params"
              }
            }
          }
        }
      }
    }
  }
}
```

每个工具的 JSON Schema 输入参数来自 MCP 协议中的 `inputSchema`。

## 配置文件 JSON Schema

配置文件遵循 `config.schema.json` 中定义的 JSON Schema 规范，详见 [config.schema.json](config.schema.json)。

支持三种服务器类型：

- **stdio**：子进程通信，需指定 `command` 和可选的 `args`、`env`
- **sse**：Server-Sent Events，需指定 `type: "sse"` 和 `url`
- **streamable-http**：Streamable HTTP，需指定 `type: "streamable-http"` 和 `url`

## 项目结构

```text
src/
├── main.rs               # CLI 入口 (clap)
├── server.rs             # actix-web HTTP 服务器 + 动态端点
├── config.rs             # 配置文件加载与校验
├── connection.rs         # 连接管理器
├── openapi.rs            # 动态生成 OpenAPI 3.0 JSON
├── auth.rs               # API Key 认证中间件
├── watcher.rs            # 配置文件热重载
└── mcp/
    ├── mod.rs
    ├── types.rs          # JSON-RPC 2.0 / MCP 协议类型
    ├── client.rs         # MCP 客户端封装
    ├── stdio.rs          # stdio 传输层
    ├── sse.rs            # SSE 传输层
    └── streamable_http.rs # Streamable HTTP 传输层
```

## 从源码构建

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆仓库
git clone https://github.com/xiaofeiTM233/mcpo-rs.git
cd mcpo-rs

# 构建
cargo build --release

# 二进制位置
./target/release/mcpo --help
```

### 交叉编译

```bash
# Linux x86_64 (静态链接)
cargo build --release --target x86_64-unknown-linux-musl

# Linux ARM64
cargo build --release --target aarch64-unknown-linux-gnu

# macOS (在 macOS 上)
cargo build --release --target aarch64-apple-darwin

# Windows (在 Windows 上或交叉编译)
cargo build --release --target x86_64-pc-windows-msvc
```

## 说明

本 README 文档由 AI 辅助生成。如有问题，请提交 Issue 或[与我联系](https://github.com/xiaofeiTM233)！
