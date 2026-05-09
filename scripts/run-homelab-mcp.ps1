# Starts homelab-mcp as a Windows process for pi running from WSL.
#
# Why this exists:
# - pi runs inside WSL.
# - homelab-mcp.exe is a Windows binary.
# - WSL environment variables are visible in /proc, but Windows Rust std::env
#   does not reliably see them when the .exe is launched directly from WSL.
# - This wrapper sets Windows process env vars before starting homelab-mcp.exe.
#
# Optional local secrets file, not committed:
#   scripts/run-homelab-mcp.local.ps1
#
# Example local file:
#   $env:PROXMOX_TOKEN_ID = "user@pam!token-name"
#   $env:PROXMOX_TOKEN_SECRET = "secret"

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$LocalEnv = Join-Path $ScriptDir "run-homelab-mcp.local.ps1"

if (Test-Path $LocalEnv) {
    . $LocalEnv
}

if (-not $env:HOMELAB_CONFIG) {
    $env:HOMELAB_CONFIG = Join-Path $RepoRoot "config.toml"
}

$Binary = Join-Path $env:USERPROFILE ".cargo\bin\homelab-mcp.exe"

if (-not (Test-Path $Binary)) {
    throw "homelab-mcp.exe not found at $Binary. Run: cargo install --path crates/homelab-mcp --force"
}

& $Binary
