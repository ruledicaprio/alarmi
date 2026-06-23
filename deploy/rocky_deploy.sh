#!/usr/bin/env bash
# ============================================================
# BHT — Rocky 9 deploy script
# Run on Rocky after curling bht-upgrade-*.tar.gz to ~
# Usage: bash rocky_deploy.sh <tarfile>
# Example: bash rocky_deploy.sh bht-upgrade-map.tar.gz
# ============================================================
set -euo pipefail

TAR="${1:?Usage: $0 <tarfile.tar.gz>}"
BHT_DIR="/opt/bht"
WEB_DIST="$BHT_DIR/web/dist"

# ---- 1. Extract
echo "==> Extracting $TAR"
python3 -c "
import tarfile, warnings
warnings.filterwarnings('ignore')
tarfile.open('$TAR').extractall()
"

# ---- 2. Verify extracted artifacts
[[ -f ~/bht-api ]] || { echo "ERROR: bht-api not found after extract"; exit 1; }
[[ -d ~/dist    ]] || { echo "ERROR: dist/ not found after extract"; exit 1; }
echo "    binary: $(ls -lh ~/bht-api | awk '{print $5, $9}')"
echo "    dist files: $(find ~/dist -type f | wc -l)"

# ---- 3. Stop service
echo "==> Stopping bht-api"
sudo systemctl stop bht-api
sleep 1

# ---- 4. Deploy binary (backup current)
echo "==> Deploying binary"
[[ -f $BHT_DIR/bht-api ]] && sudo cp $BHT_DIR/bht-api $BHT_DIR/bht-api.bak
sudo cp ~/bht-api $BHT_DIR/bht-api
sudo chmod +x $BHT_DIR/bht-api
sudo chown bht:bht $BHT_DIR/bht-api

# ---- 5. Deploy web dist
echo "==> Deploying web dist to $WEB_DIST"
sudo mkdir -p $BHT_DIR/web
sudo rm -rf $WEB_DIST
sudo mv ~/dist $WEB_DIST
sudo chown -R bht:bht $BHT_DIR/web

# ---- 6. Start service
echo "==> Starting bht-api"
sudo systemctl start bht-api
sleep 2

# ---- 7. Verify
echo "==> Service status"
sudo systemctl is-active bht-api || { echo "ERROR: service not active"; sudo journalctl -u bht-api -n 30 --no-pager; exit 1; }

echo "==> Health check"
curl -sf localhost:8080/api/health || { echo "ERROR: health check failed"; sudo journalctl -u bht-api -n 20 --no-pager; exit 1; }
echo ""

echo "==> Map endpoint (first site)"
curl -s localhost:8080/api/map/sites | python3 -m json.tool 2>/dev/null | head -20 \
    || echo "WARNING: /api/map/sites returned non-JSON (check journal)"

echo ""
echo "==> Deploy complete."
