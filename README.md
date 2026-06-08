# homelab-rs

A Rust workspace with two tools: an MCP server that exposes homelab infrastructure as AI-callable tools, and a Discord bot backed by local LLM inference.

## Architecture

Three-crate workspace:

- **`homelab-core`** — HTTP client, config loading, auth, and all tool functions. No MCP dependency. Testable in isolation via `cargo test` and `examples/`.
- **`homelab-mcp`** — Thin adapter that wires `homelab-core` tool functions to the MCP protocol over HTTP. Uses `rmcp` with `#[tool]` macros. Deployed to a VM behind a Cloudflare Tunnel; all inbound requests are validated against a Cloudflare Access JWT before reaching the tool handlers.
- **`homelab-discord`** — Discord bot. @mention triggers a thread; all replies in that thread share one conversation context backed by Redis. LLM inference via Ollama with SearXNG as a web search tool.

The boundary rule: `homelab-core` returns domain types (`Vec<NodeSummary>`, `HomelabError`). `homelab-mcp` converts those to protocol types (`CallToolResult`, `McpError`). MCP types never enter `homelab-core`.

`homelab-discord` is standalone — it does not depend on `homelab-core`. It manages its own Ollama and Redis connections.

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

SSH into the VM and run the deploy script. It downloads the release binary, atomically swaps the `current` symlink, restarts the service, health-checks, and auto-rolls-back on failure:

```bash
ssh mcpadmin@mcp-homelab
cd /opt/mcp-homelab && sudo ./deploy-mcp.sh homelab-mcp-v0.2.0
```

### Environment variables (VM)

Secrets live in `/opt/mcp-homelab/.env` (mode 400, owned by the service user). Required vars:

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

## Discord Bot

`homelab-discord` is a friend-group Discord bot. @mention it anywhere to start a conversation — the bot creates a thread and all replies in that thread share one context. No @mention needed for follow-up messages inside the thread.

### How it works

- **Trigger:** @mention in any channel → bot creates a thread, loads/saves history in Redis
- **Thread continuity:** subsequent messages in a bot-owned thread are answered without requiring a mention
- **LLM:** Ollama `/api/chat` with tool calling enabled
- **Search:** SearXNG invoked automatically by the LLM when current information is needed
- **Persona:** configured via `PERSONA` env var — multiline system prompt, no restart of the image required, just update `.env` and `docker compose restart discord-bot`

### Deploy

Deployed via Docker Compose on the inference host. All configuration via env vars — no defaults, bot panics at startup if any are missing:

| Var | Description |
|---|---|
| `DISCORD_TOKEN` | Bot token from Discord developer portal |
| `OLLAMA_HOST` | Ollama base URL, e.g. `http://host.docker.internal:11434` |
| `OLLAMA_MODEL` | Model name, e.g. `gemma4:12b-it-q4_K_M` |
| `REDIS_URL` | Redis connection string, e.g. `redis://redis:6379` |
| `SEARXNG_URL` | SearXNG search endpoint, e.g. `http://<lxc-ip>:8888/search` |
| `PERSONA` | Full system prompt (multiline, double-quoted in `.env`) |

The Docker image is built and published via GitHub Actions. The host pulls the image — it does not need Rust installed.

---

## OPNsense API Notes

- Auth: API key as username, API secret as password (HTTP Basic)
- The user associated with the API key must have ACL access to the endpoints you call
- All search endpoints return `{ total, rowCount, current, rows: [...] }` — handled by the generic `SearchResponse<T>` wrapper
- DHCP lease `expire` timestamps are converted to `"Xh Ym remaining"` at deserialization time
