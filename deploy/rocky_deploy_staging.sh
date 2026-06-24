#!/usr/bin/env bash
# ============================================================
# BHT — Rocky 9 STAGING deploy script
# Same host as prod, isolated to /opt/bht-staging, port 8081
# Usage: bash rocky_deploy_staging.sh <tarfile>
# ============================================================
set -euo pipefail

TAR="${1:?Usage: $0 <tarfile.tar.gz>}"
BHT_DIR="/opt/bht-staging"
WEB_DIST="$BHT_DIR/web/dist"
CONFIG_SRC="$BHT_DIR/../bht/config/staging"   # sibling of prod config

echo "==> [STAGING] Deploying from $TAR"

# ---- 1. Extract
python3 -c "
import tarfile, warnings
warnings.filterwarnings('ignore')
tarfile.open('$TAR').extractall()
"

# ---- 2. Verify
[[ -f ~/bht-api ]] || { echo "ERROR: bht-api not found after extract"; exit 1; }
[[ -d ~/dist    ]] || { echo "ERROR: dist/ not found after extract"; exit 1; }
echo "    binary: $(ls -lh ~/bht-api | awk '{print $5, $9}')"
echo "    dist files: $(find ~/dist -type f | wc -l)"

# ---- 3. Stop staging service
echo "==> Stopping bht-api-staging"
sudo systemctl stop bht-api-staging 2>/dev/null || true
sleep 1

# ---- 4. Deploy binary
echo "==> Deploying binary to $BHT_DIR"
sudo mkdir -p "$BHT_DIR"
[[ -f $BHT_DIR/bht-api ]] && sudo cp $BHT_DIR/bht-api $BHT_DIR/bht-api.bak
sudo cp ~/bht-api "$BHT_DIR/bht-api"
sudo chmod +x "$BHT_DIR/bht-api"
sudo chown bht:bht "$BHT_DIR/bht-api"

# ---- 5. Deploy web dist
echo "==> Deploying web dist to $WEB_DIST"
sudo mkdir -p "$BHT_DIR/web"
sudo rm -rf "$WEB_DIST"
sudo mv ~/dist "$WEB_DIST"
sudo chown -R bht:bht "$BHT_DIR/web"

# ---- 6. Ensure staging config dir is linked
sudo mkdir -p "$BHT_DIR/config"
if [[ ! -f "$BHT_DIR/config/api.toml" ]]; then
    sudo cp /opt/bht/config/staging/api.toml "$BHT_DIR/config/api.toml"
    sudo cp /opt/bht/config/staging/poller.toml "$BHT_DIR/config/poller.toml"
    echo "    Copied staging config files to $BHT_DIR/config/"
fi

# ---- 7. Start staging service
echo "==> Starting bht-api-staging"
sudo systemctl start bht-api-staging
sleep 2

# ---- 8. Verify
sudo systemctl is-active bht-api-staging || {
    echo "ERROR: staging service not active"
    sudo journalctl -u bht-api-staging -n 30 --no-pager
    exit 1
}

echo "==> Health check (port 8081)"
curl -sf localhost:8081/api/health || {
    echo "ERROR: staging health check failed"
    sudo journalctl -u bht-api-staging -n 20 --no-pager
    exit 1
}

echo ""
echo "==> [STAGING] Deploy complete. Running on port 8081."
