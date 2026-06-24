# Build & Pack — bht-alarm (Docker, fully static)

All compilation happens inside Docker — no Rust, musl-tools, or Node.js needed on the host.
Run every command from the repo root (`~/alarmi-repo`).

---

## Step 1 — Build everything

```bash
bash deploy/build_in_docker.sh
```

This script runs two Docker containers in sequence:
1. **rust:slim** — compiles `bht-api`, `bht-poller`, `neteco-poller` as static MUSL binaries
2. **node:lts-slim** — runs `npm ci && npm run build` → outputs `web/dist/`

To include `bht-loader`, add `-p bht-loader` to the `cargo build` line inside `build_in_docker.sh`.

---

## Step 2 — Verify static linking

The script already checks this and exits on failure. To re-check manually:

```bash
for bin in bht-api bht-poller neteco-poller; do
  file target/x86_64-unknown-linux-musl/release/$bin | grep -q "statically linked" \
    && echo "$bin OK" || { echo "$bin FAIL"; exit 1; }
done
```

---

## Step 3 — Pack tarball

**Full pack (frontend + all binaries):**
```bash
tar czf ~/bht-upgrade.tar.gz \
  -C target/x86_64-unknown-linux-musl/release bht-api bht-poller neteco-poller \
  -C "$PWD/web" dist
```

**Frontend only** (config/query/UI changes, no Rust rebuild):
```bash
tar czf ~/bht-upgrade.tar.gz -C "$PWD/web" dist
```

**Backend only** (Rust changes, frontend unchanged):
```bash
tar czf ~/bht-upgrade.tar.gz \
  -C target/x86_64-unknown-linux-musl/release bht-api bht-poller neteco-poller
```

---

## Step 4 — Print transfer info

```bash
du -sh ~/bht-upgrade.tar.gz
sha256sum ~/bht-upgrade.tar.gz
echo "scp ~/bht-upgrade.tar.gz root@192.168.108.88:~"
```

Always print the sha256 so it can be verified on Rocky after transfer.

---

## Scope reference

| Scope | Cargo flags | Tarball contents |
|-------|-------------|-----------------|
| All | `-p bht-api -p bht-poller -p bht-neteco-poller` | binaries + dist |
| API + frontend | `-p bht-api` | bht-api + dist |
| Poller only | `-p bht-poller` | bht-poller |
| NetEco poller | `-p bht-neteco-poller` | neteco-poller |
| Loader (utility) | `-p bht-loader` | bht-loader (no service) |
