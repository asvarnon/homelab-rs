# homelab-rs

A Rust workspace for an MCP server that exposes homelab infrastructure as AI-callable tools.

## Architecture

Two-crate workspace:

- **`homelab-core`** — HTTP client, config loading, auth, and all tool functions. No MCP dependency. Testable in isolation via `cargo test` and `examples/`.
- **`homelab-mcp`** — Thin adapter that wires `homelab-core` tool functions to the MCP protocol over HTTP. Uses `rmcp` with `#[tool]` macros. Deployed to a VM behind a Cloudflare Tunnel; all inbound requests are validated against a Cloudflare Access JWT before reaching the tool handlers.

The boundary rule: `homelab-core` returns domain types (`Vec<NodeSummary>`, `HomelabError`). `homelab-mcp` converts those to protocol types (`CallToolResult`, `McpError`). MCP types never enter `homelab-core`.

> **The Discord bot has moved.** `homelab-discord` was extracted to its own repository — now **[Husk](https://github.com/asvarnon/husk)** (`ghcr.io/asvarnon/husk`). It shared no code with this workspace and its `context-forge`/`rusqlite` dependency conflicted with this graph, so it lives standalone. A future public/bot-facing MCP binary may be added here for the bot to call over the wire.

## Tools

| Tool | Backend | Description |
|---|---|---|
| `scan_nodes` | Proxmox | Node CPU, memory, uptime across the cluster |
| `scan_cluster` | Proxmox | All nodes, VMs, and LXCs in one call |
| `get_dhcp_leases` | OPNsense | Active DHCP leases with IP, MAC, hostname, VLAN, and time remaining |
| `osrs_market_lookup` | OSRS Wiki Prices API | Item metadata plus latest observed high/low GE trades |
| `osrs_market_history` | OSRS Wiki Prices API | Aggregate price/volume history for one item and lookback |
| `osrs_market_activity` | OSRS Wiki Prices API | Selected item activity from a bulk 5-minute or hourly snapshot |

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

For local dev (`PROFILE=local`), the binary binds to `http://127.0.0.1:8787/mcp`. Add it to your client:

```bash
claude mcp add --transport http -s user homelab-mcp-local http://127.0.0.1:8787/mcp
```

For the production instance (see Deployment below), connect via the public tunnel URL using a Cloudflare Access service token:

```bash
claude mcp add --transport http \
  -H "CF-Access-Client-Id: <client-id>" \
  -H "CF-Access-Client-Secret: <client-secret>" \
  -s user \
  homelab-mcp https://<your-tunnel-hostname>/mcp
```

The `-s user` flag registers the server globally across all Claude Code sessions, not just inside this repo.

## Deployment

`homelab-mcp` deploys to a Linux VM via a tag-triggered GitHub Actions build and a pull-based deploy script.

### Releasing

Tag with the `homelab-mcp-v*` prefix — the `homelab-mcp-release.yml` workflow builds a release binary and attaches it to the GitHub Release:

```bash
git tag homelab-mcp-v0.2.0
git push origin homelab-mcp-v0.2.0
```

### Deploying to the VM

The deploy script assumes the following are already in place on the VM:
- `/opt/mcp-homelab/` directory exists with `deploy-mcp.sh` copied into it
- A `systemd` service unit (`mcp-homelab.service`) exists with `ExecStart=/opt/mcp-homelab/current`
- The `.env` file exists at `/opt/mcp-homelab/.env` (see below)

The script manages everything else: it creates `/opt/mcp-homelab/releases/<tag>/`, downloads the binary, and creates or updates the `current` symlink on each deploy. It does not bootstrap the directory structure or systemd unit from scratch.

SSH into the VM and run from `/opt/mcp-homelab`:

```bash
sudo ./deploy-mcp.sh homelab-mcp-v0.2.0
```

It atomically swaps the `current` symlink, restarts the service, health-checks, and auto-rolls-back on failure.

### Runtime configuration (VM)

The server loads configuration from a `.env` file at `/opt/mcp-homelab/.env` at startup via `dotenvy` — these are **not** system environment variables. The file should be mode 400, owned by the service user.

Required entries:

| Var | Description |
|---|---|
| `CF_CERTS_URL` | Cloudflare Access JWKS endpoint — `https://<team>.cloudflareaccess.com/cdn-cgi/access/certs` |
| `CF_AUD` | Access application AUD tag (Zero Trust → Applications → Additional settings → Token) |
| `PROFILE` | Set to `production` to enforce CF JWT validation; `local` skips it |
| `MCP_PUBLIC_HOST` | Public tunnel hostname (e.g. `your-server.your-domain.com`) — added to rmcp's Host-header allowlist |
| `PROXMOX_TOKEN_ID` | Proxmox API token ID |
| `PROXMOX_TOKEN_SECRET` | Proxmox API token secret |
| `OPNSENSE_API_KEY` | OPNsense API key |
| `OPNSENSE_API_SECRET` | OPNsense API secret |

### Auth

All inbound traffic goes through Cloudflare Tunnel → Cloudflare Access (outer gate) → CF JWT validation middleware (inner check). The server binds to `127.0.0.1:8787` only; the tunnel handles external exposure. Set `PROFILE=local` to bypass JWT validation for local dev.

---

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

## Discord Bot — moved to [Husk](https://github.com/asvarnon/husk)

The friend-group Discord bot that used to live in this workspace (`homelab-discord`) was extracted to its own repository, **[Husk](https://github.com/asvarnon/husk)** — an Ollama-backed Discord bot with long-term memory (context-forge), web search, and a configurable persona. It's published as `ghcr.io/asvarnon/husk`. It shared no code with this workspace, so it lives standalone; see that repo for usage, configuration, and deployment.

---

## OPNsense API Notes

- Auth: API key as username, API secret as password (HTTP Basic)
- The user associated with the API key must have ACL access to the endpoints you call
- All search endpoints return `{ total, rowCount, current, rows: [...] }` — handled by the generic `SearchResponse<T>` wrapper
- DHCP lease `expire` timestamps are converted to `"Xh Ym remaining"` at deserialization time
