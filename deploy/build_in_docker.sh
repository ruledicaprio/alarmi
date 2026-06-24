#!/usr/bin/env bash
# Build fully-static (musl) Linux binaries AND frontend — all inside Docker.
# No Rust, musl-tools, or Node.js needed on the host.
#
# Run from anywhere:  bash deploy/build_in_docker.sh
set -euo pipefail
cd "$(dirname "$0")/.."   # repo root

# ── 1. Backend (static MUSL binaries) ───────────────────────────────────────
docker run --rm -v "$PWD":/src -w /src rust:slim bash -euc '
  rustup target add x86_64-unknown-linux-musl
  apt-get update -qq && apt-get install -y -qq musl-tools file >/dev/null
  cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller -p bht-neteco-poller
  echo "==> static binaries:"
  for bin in bht-api bht-poller neteco-poller; do
    out=$(file target/x86_64-unknown-linux-musl/release/$bin)
    echo "  $bin: $out"
    echo "$out" | grep -qE "(statically|static-pie) linked" \
      && echo "  $bin OK" || { echo "  $bin FAIL (not statically linked)"; exit 1; }
  done
'

# ── 2. Frontend ──────────────────────────────────────────────────────────────
docker run --rm -v "$PWD/web":/web -w /web node:lts-slim \
  sh -c "npm ci && npm run build"
echo "==> frontend dist:"
ls -lh web/dist/
