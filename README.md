# homelab-rs

A Rust MCP server that exposes homelab infrastructure as AI-callable tools. Built as a typed Rust replacement for the original Python `mcp-homelab`.

## Architecture

Two-crate workspace:

- **`homelab-core`** — HTTP client, config loading, auth, and all tool functions. No MCP dependency. Testable in isolation via `cargo test` and `examples/`.
- **`homelab-mcp`** — Thin adapter that wires `homelab-core` tool functions to the MCP protocol over stdio. Uses `rmcp` with `#[tool]` macros.

The boundary rule: `homelab-core` returns domain types (`Vec<NodeSummary>`, `HomelabError`). `homelab-mcp` converts those to protocol types (`CallToolResult`, `McpError`). MCP types never enter `homelab-core`.

## Tools

| Tool | Backend | Description |
|---|---|---|
| `scan_nodes` | Proxmox | Node CPU, memory, uptime across the cluster |
| `scan_cluster` | Proxmox | All nodes, VMs, and LXCs in one call |
| `get_dhcp_leases` | OPNsense | Active DHCP leases with IP, MAC, hostname, VLAN, and time remaining |

## Config

Config lives in a TOML file. Path defaults to `config.toml` in the working directory, or override with `HOMELAB_CONFIG` env var.

```toml
[[endpoints]]
name = "proxmox"
url  = "https://<proxmox-ip>:8006"
tls_insecure = true
[endpoints.auth]
type       = "api-token"
id_env     = "PROXMOX_TOKEN_ID"
secret_env = "PROXMOX_TOKEN_SECRET"

[[endpoints]]
name = "opnsense"
url  = "https://<opnsense-ip>/api/"
[endpoints.auth]
type     = "basic"
user_env = "OPNSENSE_API_KEY"
pass_env = "OPNSENSE_API_SECRET"
```

Secret values are never in the config file — only the names of env vars that hold them.

### Auth types

| Type | Usage |
|---|---|
| `api-token` | Proxmox — sends `Authorization: PVEAPIToken=<id>=<secret>` |
| `basic` | OPNsense — standard HTTP Basic auth via `reqwest::basic_auth` |
| `bearer` | Bearer token in `Authorization` header |
| `none` | No auth (local services) |

## Setup

### 1. Create your config

```toml
# config.toml
[[endpoints]]
name = "proxmox"
url  = "https://<your-proxmox-ip>:8006"
tls_insecure = true
[endpoints.auth]
type       = "api-token"
id_env     = "PROXMOX_TOKEN_ID"
secret_env = "PROXMOX_TOKEN_SECRET"
```

### 2. Set env vars

```powershell
# Windows — set as User env vars so they persist across sessions
[System.Environment]::SetEnvironmentVariable("PROXMOX_TOKEN_ID", "user@pam!token", "User")
[System.Environment]::SetEnvironmentVariable("PROXMOX_TOKEN_SECRET", "your-secret", "User")
[System.Environment]::SetEnvironmentVariable("OPNSENSE_API_KEY", "your-key", "User")
[System.Environment]::SetEnvironmentVariable("OPNSENSE_API_SECRET", "your-secret", "User")
[System.Environment]::SetEnvironmentVariable("HOMELAB_CONFIG", "C:\path\to\config.toml", "User")
```

### 3. Build and install

```powershell
cargo install --path crates/homelab-mcp
```

### 4. Wire into your MCP client

Add `homelab-mcp` as an MCP server in your client of choice. The binary speaks MCP JSON-RPC over stdio with no additional arguments required.

## Development

### Running examples

Each tool has a corresponding example for running and stepping through without going through the MCP layer:

```powershell
cargo run --example scan_nodes -p homelab-core
cargo run --example opnsense_leases -p homelab-core
```

### Reinstalling after changes

```powershell
cargo install --path crates/homelab-mcp
```

## OPNsense API Notes

- Auth: API key as username, API secret as password (HTTP Basic)
- The user associated with the API key must have ACL access to the endpoints you call
- All search endpoints return `{ total, rowCount, current, rows: [...] }` — handled by the generic `SearchResponse<T>` wrapper
- DHCP lease `expire` timestamps are converted to `"Xh Ym remaining"` at deserialization time
