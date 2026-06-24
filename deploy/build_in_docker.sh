#!/usr/bin/env bash
# Build fully-static (musl) Linux binaries INSIDE Docker, on a machine that has
# Docker (your home PC). No Rust/WSL/musl setup needed on the host, and the
# output has NO glibc dependency -> runs on the Rocky 9 LXC as-is.
#
# Run from anywhere:  bash deploy/build_in_docker.sh
set -euo pipefail
cd "$(dirname "$0")/.."   # repo root

docker run --rm -v "$PWD":/src -w /src rust:slim bash -euc '
  rustup target add x86_64-unknown-linux-musl
  apt-get update -qq && apt-get install -y -qq musl-tools >/dev/null
  cargo build --release --target x86_64-unknown-linux-musl -p bht-api -p bht-poller -p neteco-poller
'
echo "==> static binaries (ldd should say: not a dynamic executable):"
ls -la target/x86_64-unknown-linux-musl/release/bht-api \
       target/x86_64-unknown-linux-musl/release/bht-poller \
       target/x86_64-unknown-linux-musl/release/neteco-poller
