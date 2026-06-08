#!/usr/bin/env bash
# Deploys a tagged homelab-mcp release binary to this host using the
# releases/ + symlink pattern, with health-check and automatic rollback.
#
# Usage: ./deploy-mcp.sh <release-tag>
#   e.g. ./deploy-mcp.sh homelab-mcp-v0.2.0
#
# Run this ON the target VM (e.g. mcp-homelab), as a user that can
# sudo systemctl restart the service.

set -euo pipefail

TAG="${1:?Usage: $0 <release-tag>}"
REPO="asvarnon/homelab-rs"
ASSET_NAME="homelab-mcp"
INSTALL_ROOT="/opt/mcp-homelab"
RELEASES_DIR="$INSTALL_ROOT/releases"
CURRENT_LINK="$INSTALL_ROOT/current"
SERVICE="mcp-homelab.service"
HEALTH_URL="http://127.0.0.1:8787/mcp"

RELEASE_DIR="$RELEASES_DIR/$TAG"
BINARY_PATH="$RELEASE_DIR/$ASSET_NAME"

echo "==> Looking up release asset for $TAG"
DOWNLOAD_URL=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/tags/$TAG" \
  | grep -o "\"browser_download_url\": *\"[^\"]*$ASSET_NAME[^\"]*\"" \
  | head -n1 \
  | cut -d '"' -f 4)

if [[ -z "$DOWNLOAD_URL" ]]; then
  echo "Could not find asset '$ASSET_NAME' on release '$TAG' in $REPO" >&2
  exit 1
fi

echo "==> Downloading $DOWNLOAD_URL"
mkdir -p "$RELEASE_DIR"
curl -fsSL "$DOWNLOAD_URL" -o "$BINARY_PATH"
chmod +x "$BINARY_PATH"

# Remember what's currently live so we can roll back to it on failure.
PREVIOUS_TARGET=""
if [[ -L "$CURRENT_LINK" ]]; then
  PREVIOUS_TARGET="$(readlink "$CURRENT_LINK")"
fi

echo "==> Pointing $CURRENT_LINK -> $BINARY_PATH"
ln -sfn "$BINARY_PATH" "$CURRENT_LINK"

echo "==> Restarting $SERVICE"
sudo systemctl restart "$SERVICE"

echo "==> Health-checking $HEALTH_URL"
for _ in $(seq 1 10); do
  HTTP_CODE="$(curl -s -o /dev/null -w '%{http_code}' "$HEALTH_URL" || true)"
  if [[ "$HTTP_CODE" != "000" ]]; then
    echo "==> $TAG is up (HTTP $HTTP_CODE from $HEALTH_URL)."
    exit 0
  fi
  sleep 2
done

echo "!! $TAG never came up — rolling back" >&2

if [[ -n "$PREVIOUS_TARGET" ]]; then
  ln -sfn "$PREVIOUS_TARGET" "$CURRENT_LINK"
  sudo systemctl restart "$SERVICE"
  echo "==> Rolled back $CURRENT_LINK -> $PREVIOUS_TARGET" >&2
else
  echo "!! No previous release recorded — manual recovery required" >&2
fi

exit 1
