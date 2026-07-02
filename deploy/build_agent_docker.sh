#!/usr/bin/env bash
# Build the bht-alarm-agent Docker image and save it for air-gapped transfer to docker-host.
# Run on the workstation (has internet access for npm install during docker build).
# Transfer pattern: python3 -m http.server 8000 here → curl on docker-host → docker load
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

IMAGE_NAME="bht-alarm-agent"
IMAGE_TAG="$(date +%Y%m%d-%H%M)"
FULL_TAG="${IMAGE_NAME}:${IMAGE_TAG}"
OUTPUT="${HOME}/${IMAGE_NAME}-${IMAGE_TAG}.tar.gz"

echo "[build-agent] Building Docker image: $FULL_TAG"
docker build -t "$FULL_TAG" -t "${IMAGE_NAME}:latest" "$REPO_ROOT/agent"

echo "[build-agent] Saving image → $OUTPUT"
docker save "$FULL_TAG" | gzip > "$OUTPUT"

echo "[build-agent] Done."
du -sh "$OUTPUT"
sha256sum "$OUTPUT"

echo
echo "━━━ Transfer & deploy on docker-host ━━━"
echo
echo "  # 1. On this PC — serve the file:"
echo "  cd ~ && python3 -m http.server 8000"
echo
echo "  # 2. On docker-host — fetch, load, and run:"
BASENAME="$(basename "$OUTPUT")"
echo "  curl -O http://192.168.82.205:8000/${BASENAME}"
echo "  docker load < ${BASENAME}"
echo "  mkdir -p /opt/bht-agent"
echo "  # Copy and fill in .env (copy from agent/.env.example, adjust paths):"
echo "  cp alarmi-repo/agent/.env.example /opt/bht-agent/.env && nano /opt/bht-agent/.env"
echo "  docker rm -f bht-alarm-agent 2>/dev/null || true"
echo "  docker run -d --name bht-alarm-agent --restart always \\"
echo "    --network host \\"
echo "    --env-file /opt/bht-agent/.env \\"
echo "    ${IMAGE_NAME}:latest"
echo
echo "  # --network host is required: lets the container reach llama-server on localhost:8080-8082"
echo
echo "━━━ Qdrant (one-time, if not yet on docker-host) ━━━"
echo
echo "  # On this PC:"
echo "  docker pull qdrant/qdrant"
echo "  docker save qdrant/qdrant | gzip > ~/qdrant-latest.tar.gz"
echo "  # Then same transfer → docker load pattern, then:"
echo "  docker run -d --name qdrant --restart always \\"
echo "    --network host \\"
echo "    -v qdrant_storage:/qdrant/storage \\"
echo "    qdrant/qdrant"
echo "  # --network host exposes Qdrant on localhost:6333 of docker-host"
